'use strict';

const crypto = require('crypto');
const { createGame, play, publicState } = require('./game');

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
    if (existing?.ws && existing.ws !== ws && existing.ws.readyState === 1) {
      mlog('register replace existing ws', id);
      try {
        existing.ws.close(4000, 'replaced');
      } catch (_) {
        /* ignore */
      }
    }

    const player = {
      id,
      name: profile.name || `Player ${id}`,
      ws,
      roomId: existing?.roomId || null,
      side: existing?.side || null,
      queued: false,
    };
    this.players.set(id, player);
    mlog('register', id, player.name, 'room=', player.roomId, 'players=', this.players.size);
    return player;
  }

  detach(ws) {
    for (const [id, player] of this.players) {
      if (player.ws === ws) {
        player.ws = null;
        this.leaveQueue(id);
        if (player.roomId) {
          this.resign(id, 'disconnect');
        }
        // keep player for short reconnect window only if in room — already resigned
        this.players.delete(id);
        return;
      }
    }
  }

  send(playerId, payload) {
    const player = this.players.get(playerId);
    if (!player?.ws || player.ws.readyState !== 1) return;
    player.ws.send(JSON.stringify(payload));
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

  enqueue(playerId) {
    const player = this.players.get(playerId);
    if (!player) {
      mlog('enqueue fail not_registered', playerId);
      return { ok: false, error: 'not_registered' };
    }
    if (player.roomId) {
      mlog('enqueue fail in_game', playerId, player.roomId);
      return { ok: false, error: 'in_game' };
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
      room.finished = true;
      const winnerSide = room.game.over === '-' ? null : room.game.over;
      this.broadcastRoom(room, {
        type: 'game_over',
        result: room.game.over,
        winnerSide,
        winnerName: winnerSide ? room.names[winnerSide] : null,
        state: publicState(room.game),
      });
      this.clearRoomSoon(room.id);
    }

    return { ok: true };
  }

  resign(playerId, reason = 'resign') {
    const player = this.players.get(playerId);
    if (!player?.roomId) return;
    const room = this.rooms.get(player.roomId);
    if (!room || room.finished) return;

    room.finished = true;
    const winnerSide = player.side === 'X' ? 'O' : 'X';
    room.game.over = winnerSide;

    this.broadcastRoom(room, {
      type: 'game_over',
      result: winnerSide,
      winnerSide,
      winnerName: room.names[winnerSide],
      reason,
      state: publicState(room.game),
    });
    this.clearRoomSoon(room.id);
  }

  clearRoomSoon(roomId) {
    setTimeout(() => {
      const room = this.rooms.get(roomId);
      if (!room) return;
      for (const side of ['X', 'O']) {
        const pid = room.players[side];
        const p = this.players.get(pid);
        if (p && p.roomId === roomId) {
          p.roomId = null;
          p.side = null;
        }
      }
      this.rooms.delete(roomId);
    }, 30_000);
  }

  rematch(playerId) {
    const player = this.players.get(playerId);
    if (!player) return { ok: false, error: 'not_registered' };
    if (player.roomId) {
      const room = this.rooms.get(player.roomId);
      if (room && !room.finished) return { ok: false, error: 'in_game' };
      // leave finished room
      if (room) {
        player.roomId = null;
        player.side = null;
      }
    }
    return this.enqueue(playerId);
  }
}

module.exports = { Matchmaking };
