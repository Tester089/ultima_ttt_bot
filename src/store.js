'use strict';

const fs = require('fs');
const path = require('path');
const { defaultRating, advanceRd, updateGlicko, publicRating } = require('./rating');

function log(...args) {
  console.log('[store]', new Date().toISOString(), ...args);
}

class Store {
  /**
   * @param {string} dataDir
   */
  constructor(dataDir) {
    this.dataDir = dataDir;
    this.playersPath = path.join(dataDir, 'players.json');
    this.gamesDir = path.join(dataDir, 'games');
    /** @type {Map<string, any>} */
    this.players = new Map();
    this._dirty = false;
    this._saveTimer = null;
    this._writeQueue = Promise.resolve();
  }

  async init() {
    fs.mkdirSync(this.dataDir, { recursive: true });
    fs.mkdirSync(this.gamesDir, { recursive: true });
    if (fs.existsSync(this.playersPath)) {
      try {
        const raw = JSON.parse(fs.readFileSync(this.playersPath, 'utf8'));
        for (const [id, rec] of Object.entries(raw.players || raw)) {
          this.players.set(id, rec);
        }
        log('loaded players', this.players.size);
      } catch (err) {
        log('players load fail', err.message);
      }
    }
    // периодический flush
    setInterval(() => this.flushPlayersSync(), 15_000).unref?.();
  }

  getOrCreate(id, name) {
    let rec = this.players.get(id);
    if (!rec) {
      rec = {
        id,
        name: name || id,
        ...defaultRating(),
        updatedAt: Date.now(),
        lastPlayedAt: null,
      };
      this.players.set(id, rec);
      this.markDirty();
    } else if (name && rec.name !== name) {
      rec.name = name;
      this.markDirty();
    }
    return rec;
  }

  touchIdleRd(rec) {
    if (!rec.lastPlayedAt) return;
    const days = (Date.now() - rec.lastPlayedAt) / 86400000;
    if (days >= 1) {
      rec.rd = advanceRd(rec.rd, days);
      rec.updatedAt = Date.now();
      this.markDirty();
    }
  }

  profile(id, name) {
    const rec = this.getOrCreate(id, name);
    this.touchIdleRd(rec);
    return { ...publicRating(rec), name: rec.name, id: rec.id };
  }

  /**
   * Применить результат партии и вернуть дельты для записи.
   * scoreX: 1 / 0.5 / 0 для стороны X
   */
  applyRatedGame({ idX, idO, nameX, nameO, scoreX, guestX, guestO }) {
    if (guestX || guestO) {
      return { rated: false, reason: 'guest' };
    }
    const a = this.getOrCreate(idX, nameX);
    const b = this.getOrCreate(idO, nameO);
    this.touchIdleRd(a);
    this.touchIdleRd(b);

    const before = {
      X: { r: a.r, rd: a.rd, games: a.games },
      O: { r: b.r, rd: b.rd, games: b.games },
    };

    const nextA = updateGlicko(a, b, scoreX);
    const nextB = updateGlicko(b, a, 1 - scoreX);

    a.r = nextA.r;
    a.rd = nextA.rd;
    b.r = nextB.r;
    b.rd = nextB.rd;

    a.games += 1;
    b.games += 1;
    if (scoreX === 1) {
      a.wins += 1;
      b.losses += 1;
    } else if (scoreX === 0) {
      a.losses += 1;
      b.wins += 1;
    } else {
      a.draws += 1;
      b.draws += 1;
    }

    const now = Date.now();
    a.lastPlayedAt = now;
    b.lastPlayedAt = now;
    a.updatedAt = now;
    b.updatedAt = now;
    this.markDirty();
    this.flushPlayersSync();

    return {
      rated: true,
      before,
      after: {
        X: publicRating(a),
        O: publicRating(b),
      },
    };
  }

  leaderboard(limit = 20) {
    return [...this.players.values()]
      .filter((p) => (p.games || 0) > 0)
      .sort((a, b) => b.r - a.r || a.rd - b.rd)
      .slice(0, limit)
      .map((p, i) => ({
        rank: i + 1,
        id: p.id,
        name: p.name,
        ...publicRating(p),
      }));
  }

  markDirty() {
    this._dirty = true;
    if (this._saveTimer) return;
    this._saveTimer = setTimeout(() => {
      this._saveTimer = null;
      this.flushPlayersSync();
    }, 2000);
  }

  flushPlayersSync() {
    if (!this._dirty) return;
    this._dirty = false;
    const obj = { v: 1, savedAt: new Date().toISOString(), players: {} };
    for (const [id, rec] of this.players) obj.players[id] = rec;
    const tmp = this.playersPath + '.tmp';
    try {
      fs.writeFileSync(tmp, JSON.stringify(obj));
      fs.renameSync(tmp, this.playersPath);
    } catch (err) {
      log('flush players fail', err.message);
      this._dirty = true;
    }
  }

  /**
   * Append-only JSONL — один fsync на партию, без записи во время ходов.
   * Формат v1 заточен под обучение: moves = [board*9+cell, ...]
   */
  appendGameRecord(record) {
    const day = (record.ts || new Date().toISOString()).slice(0, 10);
    const file = path.join(this.gamesDir, `${day}.jsonl`);
    const line = JSON.stringify(record) + '\n';
    this._writeQueue = this._writeQueue
      .then(
        () =>
          new Promise((resolve, reject) => {
            fs.appendFile(file, line, (err) => (err ? reject(err) : resolve()));
          })
      )
      .catch((err) => log('append game fail', err.message));
    return this._writeQueue;
  }
}

module.exports = { Store };
