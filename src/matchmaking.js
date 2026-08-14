'use strict';

const crypto = require('crypto');
const { createGame, play, publicState } = require('./game');

const RECONNECT_GRACE_MS = 20_000;

function uid() {
  return crypto.randomBytes(8).toString('hex');
}

function mlog(...args) {
  console.log('[mm]', new Date().toISOString(), ...args);
}

class Matchmaking {
  constructor() {
    /** @type {Map<string, any>} */
    this.players = new Map();
    /** @type {string[]} */
    this.queue = [];
    /** @type {Map<string, any>} */
    this.rooms = new Map();
  }

  register(ws, profile) {
    const id = profile.id;
    const existing = this.players.get(id);

    if (existing?.disconnectTimer) {
      clearTimeout(existing.disconnectTimer);
      existing.disconnectTimer = null;
      mlog('register cancel disconnect timer', id);
    }

    if (existing?.ws && existing.ws !== ws && existing.ws.readyState === 1) {
      mlog('register replace existing ws', id);
      try {
        existing.ws.close(4000, 'replaced');
      } catch (_) {
        /* ignore */
      }
    }

    const player = existing || {
      id,
      name: profile.name || `Player ${id}`,
      ws: null,
      roomId: null,
      side: null,
      queued: false,
      disconnectTimer: null,
    };

    player.name = profile.name || player.name;
    player.ws = ws;
    player.queued = false;
    this.players.set(id, player);
    mlog('register', id, player.name, 'room=', player.roomId, 'players=', this.players.size);

    // Переподключение в живую партию
    if (player.roomId) {
      const room = this.rooms.get(player.roomId);
      if (room && !room.finished) {
        mlog('resume room', player.roomId, 'side=', player.side);
        this.send(id, {
          type: 'matched',
          roomId: room.id,
          side: player.side,
          you: player.name,
          opponent: room.names[player.side === 'X' ? 'O' : 'X'],
          state: publicState(room.game),
          resumed: true,
        });
      } else {
        this.releasePlayerFromRoom(player);
      }
    }

    return player;
  }

  detach(ws) {
    for (const [id, player] of this.players) {
      if (player.ws !== ws) continue;

      player.ws = null;
      this.leaveQueue(id);
      mlog('detach', id, 'room=', player.roomId);

      if (player.roomId) {
        const room = this.rooms.get(player.roomId);
        if (room && !room.finished) {
          if (player.disconnectTimer) clearTimeout(player.disconnectTimer);
          player.disconnectTimer = setTimeout(() => {
            player.disconnectTimer = null;
            const still = this.players.get(id);
            if (!still || still.ws) {
              mlog('disconnect grace aborted (reconnected)', id);
              return;
            }
            mlog('disconnect grace expired → resign', id);
            this.resign(id, 'disconnect');
            this.players.delete(id);
          }, RECONNECT_GRACE_MS);
          return;
        }
      }

      this.players.delete(id);
      return;
    }
  }

  send(playerId, payload) {
    const player = this.players.get(playerId);
    if (!player?.ws || player.ws.readyState !== 1) return;
    try {
      player.ws.send(JSON.stringify(payload));
    } catch (err) {
      mlog('send fail', playerId, err.message);
    }
  }

  broadcastRoom(room, payload, exceptId = null) {
    for (const side of ['X', 'O']) {
      const pid = room.players[side];
      if (pid && pid !== exceptId) this.send(pid, payload);
    }
  }

  leaveQueue(playerId) {
    this.queue = this.queue.filter((id) => id !== playerId);
    const player = this.players.get(playerId);
    if (player) player.queued = false;
  }

  releasePlayerFromRoom(player) {
    if (!player) return;
    player.roomId = null;
    player.side = null;
  }

  releaseRoomBindings(room) {
    for (const side of ['X', 'O']) {
      const pid = room.players[side];
      const p = this.players.get(pid);
      if (p && p.roomId === room.id) this.releasePlayerFromRoom(p);
    }
  }

  /** Сбросить finished/битую комнату, чтобы можно было встать в очередь */
  ensureFreeForQueue(player) {
    if (!player?.roomId) return { ok: true };
    const room = this.rooms.get(player.roomId);
    if (!room) {
      mlog('stale roomId cleared', player.id, player.roomId);
      this.releasePlayerFromRoom(player);
      return { ok: true };
    }
    if (room.finished) {
      mlog('finished room released for queue', player.id, room.id);
      this.releasePlayerFromRoom(player);
      return { ok: true };
    }
    return { ok: false, error: 'in_game', roomId: room.id };
  }

  leaveRoom(playerId) {
    const player = this.players.get(playerId);
    if (!player) return { ok: false, error: 'not_registered' };
    if (!player.roomId) return { ok: true, left: false };

    const room = this.rooms.get(player.roomId);
    if (room && !room.finished) {
      this.resign(playerId, 'leave');
      return { ok: true, left: true, resigned: true };
    }
    this.releasePlayerFromRoom(player);
    mlog('leaveRoom', playerId);
    return { ok: true, left: true };
  }

  enqueue(playerId) {
    const player = this.players.get(playerId);
    if (!player) {
      mlog('enqueue fail not_registered', playerId);
      return { ok: false, error: 'not_registered' };
    }

    const free = this.ensureFreeForQueue(player);
    if (!free.ok) {
      mlog('enqueue fail in_game', playerId, free.roomId);
      return free;
    }

    this.leaveQueue(playerId);
    player.queued = true;
    this.queue.push(playerId);
    mlog('enqueue', playerId, 'queueLen=', this.queue.length, 'queue=', this.queue.slice());
    this.send(playerId, { type: 'queued', position: this.queue.length });
    this.tryMatch();
    return { ok: true };
  }

  tryMatch() {
    mlog('tryMatch queueLen=', this.queue.length);
    while (this.queue.length >= 2) {
      const a = this.queue.shift();
      const b = this.queue.shift();
      const pa = this.players.get(a);
      const pb = this.players.get(b);
      if (!pa?.ws || pa.ws.readyState !== 1) {
        mlog('tryMatch drop stale a', a);
        if (pb) this.queue.unshift(b);
        continue;
      }
      if (!pb?.ws || pb.ws.readyState !== 1) {
        mlog('tryMatch drop stale b', b);
        this.queue.unshift(a);
        continue;
      }
      this.createRoom(pa, pb);
    }
  }

  createRoom(pa, pb) {
    const roomId = uid();
    const swap = Math.random() < 0.5;
    const x = swap ? pa : pb;
    const o = swap ? pb : pa;
    mlog('createRoom', roomId, 'X=', x.id, x.name, 'O=', o.id, o.name);

    const room = {
      id: roomId,
      players: { X: x.id, O: o.id },
      names: { X: x.name, O: o.name },
      game: createGame(),
      createdAt: Date.now(),
      finished: false,
    };

    x.roomId = roomId;
    x.side = 'X';
    x.queued = false;
    o.roomId = roomId;
    o.side = 'O';
    o.queued = false;
    this.rooms.set(roomId, room);

    this.send(x.id, {
      type: 'matched',
      roomId,
      side: 'X',
      you: x.name,
      opponent: o.name,
      state: publicState(room.game),
    });
    this.send(o.id, {
      type: 'matched',
      roomId,
      side: 'O',
      you: o.name,
      opponent: x.name,
      state: publicState(room.game),
    });
  }

  move(playerId, board, cell) {
    const player = this.players.get(playerId);
    if (!player?.roomId) return { ok: false, error: 'not_in_game' };
    const room = this.rooms.get(player.roomId);
    if (!room || room.finished) return { ok: false, error: 'no_room' };
    if (player.side !== room.game.turn) return { ok: false, error: 'not_your_turn' };

    const result = play(room.game, board, cell);
    if (!result.ok) return result;

    room.game = result.state;
    const payload = {
      type: 'state',
      state: publicState(room.game),
      lastMove: { board, cell, by: player.side },
    };
    this.broadcastRoom(room, payload);

    if (room.game.over) {
      this.finishRoom(room, {
        type: 'game_over',
        result: room.game.over,
        winnerSide: room.game.over === '-' ? null : room.game.over,
        winnerName: room.game.over === '-' ? null : room.names[room.game.over],
        state: publicState(room.game),
      });
    }

    return { ok: true };
  }

  resign(playerId, reason = 'resign') {
    const player = this.players.get(playerId);
    if (!player?.roomId) return;
    const room = this.rooms.get(player.roomId);
    if (!room || room.finished) return;

    const winnerSide = player.side === 'X' ? 'O' : 'X';
    room.game.over = winnerSide;
    mlog('resign', playerId, 'reason=', reason, 'winner=', winnerSide);

    this.finishRoom(room, {
      type: 'game_over',
      result: winnerSide,
      winnerSide,
      winnerName: room.names[winnerSide],
      reason,
      state: publicState(room.game),
    });
  }

  finishRoom(room, payload) {
    if (room.finished) return;
    room.finished = true;
    this.broadcastRoom(room, payload);
    // Сразу отпускаем игроков — иначе queue даёт in_game ещё 30с
    this.releaseRoomBindings(room);
    mlog('finishRoom released players', room.id);
    setTimeout(() => {
      this.rooms.delete(room.id);
      mlog('room deleted', room.id);
    }, 5_000);
  }

  rematch(playerId) {
    const player = this.players.get(playerId);
    if (!player) return { ok: false, error: 'not_registered' };
    const free = this.ensureFreeForQueue(player);
    if (!free.ok) return free;
    return this.enqueue(playerId);
  }
}

module.exports = { Matchmaking };
