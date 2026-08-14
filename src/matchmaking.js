'use strict';

const crypto = require('crypto');
const { createGame, play, publicState } = require('./game');
const { Invites } = require('./invites');

const RECONNECT_GRACE_MS = 20_000;

function uid() {
  return crypto.randomBytes(8).toString('hex');
}

function mlog(...args) {
  console.log('[mm]', new Date().toISOString(), ...args);
}

class Matchmaking {
  /**
   * @param {{ store: import('./store').Store, botUsername?: string }} opts
   */
  constructor(opts = {}) {
    this.store = opts.store || null;
    this.botUsername = opts.botUsername || 'ultima_ttt_bot';
    this.invites = new Invites();
    /** @type {Map<string, any>} */
    this.players = new Map();
    /** @type {string[]} */
    this.queue = [];
    /** @type {Map<string, any>} */
    this.rooms = new Map();
  }

  ratingOf(player) {
    if (!this.store || player.guest) {
      return { display: '—', provisional: true, r: null, guest: true };
    }
    return this.store.profile(player.id, player.name);
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
      guest: Boolean(profile.guest),
      ws: null,
      roomId: null,
      side: null,
      queued: false,
      disconnectTimer: null,
    };

    player.name = profile.name || player.name;
    player.guest = Boolean(profile.guest);
    player.ws = ws;
    player.queued = false;
    this.players.set(id, player);
    if (this.store && !player.guest) this.store.getOrCreate(id, player.name);
    mlog('register', id, player.name, 'room=', player.roomId, 'players=', this.players.size);

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
          mode: room.mode,
          ratings: this.roomRatings(room),
          resumed: true,
        });
      } else {
        this.releasePlayerFromRoom(player);
      }
    }

    return player;
  }

  roomRatings(room) {
    const out = {};
    for (const side of ['X', 'O']) {
      const p = this.players.get(room.players[side]);
      out[side] = p ? this.ratingOf(p) : null;
    }
    return out;
  }

  detach(ws) {
    for (const [id, player] of this.players) {
      if (player.ws !== ws) continue;

      player.ws = null;
      this.leaveQueue(id);
      this.invites.cancel(id);
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

  ensureFreeForQueue(player) {
    if (!player?.roomId) return { ok: true };
    const room = this.rooms.get(player.roomId);
    if (!room) {
      this.releasePlayerFromRoom(player);
      return { ok: true };
    }
    if (room.finished) {
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
    return { ok: true, left: true };
  }

  enqueue(playerId) {
    const player = this.players.get(playerId);
    if (!player) return { ok: false, error: 'not_registered' };
    const free = this.ensureFreeForQueue(player);
    if (!free.ok) return free;

    this.invites.cancel(playerId);
    this.leaveQueue(playerId);
    player.queued = true;
    this.queue.push(playerId);
    mlog('enqueue', playerId, 'queueLen=', this.queue.length);
    this.send(playerId, { type: 'queued', position: this.queue.length });
    this.tryMatch();
    return { ok: true };
  }

  tryMatch() {
    while (this.queue.length >= 2) {
      const a = this.queue.shift();
      const b = this.queue.shift();
      const pa = this.players.get(a);
      const pb = this.players.get(b);
      if (!pa?.ws || pa.ws.readyState !== 1) {
        if (pb) this.queue.unshift(b);
        continue;
      }
      if (!pb?.ws || pb.ws.readyState !== 1) {
        this.queue.unshift(a);
        continue;
      }
      this.createRoom(pa, pb, 'ranked');
    }
  }

  createInvite(hostId) {
    const host = this.players.get(hostId);
    if (!host) return { ok: false, error: 'not_registered' };
    const free = this.ensureFreeForQueue(host);
    if (!free.ok) return free;
    this.leaveQueue(hostId);

    const inv = this.invites.create(hostId);
    const deepLink = `https://t.me/${this.botUsername}?start=inv_${inv.code}`;
    const shareText = `Сыграем в UTTT? ${deepLink}`;
    mlog('invite created', inv.code, 'host=', hostId);
    this.send(hostId, {
      type: 'invite_created',
      code: inv.code,
      deepLink,
      shareText,
      expiresAt: inv.expiresAt,
    });
    return { ok: true, code: inv.code, deepLink, shareText, expiresAt: inv.expiresAt };
  }

  cancelInvite(hostId) {
    const ok = this.invites.cancel(hostId);
    if (ok) this.send(hostId, { type: 'invite_cancelled' });
    return { ok };
  }

  joinInvite(guestId, code) {
    const guest = this.players.get(guestId);
    if (!guest) return { ok: false, error: 'not_registered' };
    const free = this.ensureFreeForQueue(guest);
    if (!free.ok) return free;

    const inv = this.invites.get(code);
    if (!inv) return { ok: false, error: 'invite_not_found' };
    if (inv.hostId === guestId) return { ok: false, error: 'invite_self' };

    const host = this.players.get(inv.hostId);
    if (!host?.ws || host.ws.readyState !== 1) {
      return { ok: false, error: 'host_offline' };
    }
    const hostFree = this.ensureFreeForQueue(host);
    if (!hostFree.ok) return { ok: false, error: 'host_busy' };

    this.invites.consume(code);
    this.leaveQueue(guestId);
    this.leaveQueue(inv.hostId);
    this.createRoom(host, guest, 'invite');
    return { ok: true };
  }

  createRoom(pa, pb, mode = 'ranked') {
    const roomId = uid();
    const swap = Math.random() < 0.5;
    const x = swap ? pa : pb;
    const o = swap ? pb : pa;
    mlog('createRoom', roomId, mode, 'X=', x.id, x.name, 'O=', o.id, o.name);

    const room = {
      id: roomId,
      mode,
      players: { X: x.id, O: o.id },
      names: { X: x.name, O: o.name },
      guests: { X: Boolean(x.guest), O: Boolean(o.guest) },
      game: createGame(),
      moves: [], // packed board*9+cell
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

    const ratings = this.roomRatings(room);
    const base = {
      type: 'matched',
      roomId,
      mode,
      state: publicState(room.game),
      ratings,
    };
    this.send(x.id, { ...base, side: 'X', you: x.name, opponent: o.name });
    this.send(o.id, { ...base, side: 'O', you: o.name, opponent: x.name });
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
    room.moves.push(board * 9 + cell);

    this.broadcastRoom(room, {
      type: 'state',
      state: publicState(room.game),
      lastMove: { board, cell, by: player.side },
    });

    if (room.game.over) {
      this.finishRoom(room, {
        type: 'game_over',
        result: room.game.over,
        winnerSide: room.game.over === '-' ? null : room.game.over,
        winnerName: room.game.over === '-' ? null : room.names[room.game.over],
        reason: 'mate',
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

    let ratingUpdate = { rated: false };
    if (this.store) {
      let scoreX = 0.5;
      if (room.game.over === 'X') scoreX = 1;
      else if (room.game.over === 'O') scoreX = 0;

      ratingUpdate = this.store.applyRatedGame({
        idX: room.players.X,
        idO: room.players.O,
        nameX: room.names.X,
        nameO: room.names.O,
        scoreX,
        guestX: room.guests.X,
        guestO: room.guests.O,
      });
    }

    const fullPayload = {
      ...payload,
      ratings: ratingUpdate.rated ? ratingUpdate.after : this.roomRatings(room),
      ratingDelta: ratingUpdate.rated
        ? {
            X: {
              from: Math.round(ratingUpdate.before.X.r),
              to: ratingUpdate.after.X.r,
            },
            O: {
              from: Math.round(ratingUpdate.before.O.r),
              to: ratingUpdate.after.O.r,
            },
          }
        : null,
    };

    this.broadcastRoom(room, fullPayload);
    this.releaseRoomBindings(room);
    mlog('finishRoom', room.id, 'moves=', room.moves.length, 'rated=', ratingUpdate.rated);

    // Компактная запись для обучения ботов — async, не в hot path ходов
    if (this.store) {
      const record = {
        v: 1,
        id: room.id,
        ts: new Date().toISOString(),
        mode: room.mode,
        result: room.game.over,
        reason: payload.reason || 'mate',
        // moves: int 0..80 = board*9+cell — восстанавливается без полного state
        moves: room.moves,
        pl: {
          X: { id: room.players.X, name: room.names.X, guest: room.guests.X },
          O: { id: room.players.O, name: room.names.O, guest: room.guests.O },
        },
        rating: ratingUpdate.rated
          ? { before: ratingUpdate.before, after: ratingUpdate.after }
          : null,
      };
      setImmediate(() => {
        this.store.appendGameRecord(record);
      });
    }

    setTimeout(() => {
      this.rooms.delete(room.id);
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
