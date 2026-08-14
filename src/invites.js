'use strict';

const crypto = require('crypto');

const INVITE_TTL_MS = 15 * 60 * 1000;
const ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

function code(n = 6) {
  const bytes = crypto.randomBytes(n);
  let s = '';
  for (let i = 0; i < n; i++) s += ALPHABET[bytes[i] % ALPHABET.length];
  return s;
}

class Invites {
  constructor() {
    /** @type {Map<string, { code: string, hostId: string, createdAt: number, expiresAt: number }>} */
    this.byCode = new Map();
    /** hostId -> code */
    this.byHost = new Map();
  }

  create(hostId) {
    this.expire();
    const old = this.byHost.get(hostId);
    if (old) this.byCode.delete(old);

    const c = code(6);
    const inv = {
      code: c,
      hostId,
      createdAt: Date.now(),
      expiresAt: Date.now() + INVITE_TTL_MS,
    };
    this.byCode.set(c, inv);
    this.byHost.set(hostId, c);
    return inv;
  }

  get(c) {
    this.expire();
    if (!c) return null;
    return this.byCode.get(String(c).toUpperCase()) || null;
  }

  consume(c) {
    const inv = this.get(c);
    if (!inv) return null;
    this.byCode.delete(inv.code);
    if (this.byHost.get(inv.hostId) === inv.code) this.byHost.delete(inv.hostId);
    return inv;
  }

  cancel(hostId) {
    const c = this.byHost.get(hostId);
    if (!c) return false;
    this.byCode.delete(c);
    this.byHost.delete(hostId);
    return true;
  }

  expire() {
    const now = Date.now();
    for (const [c, inv] of this.byCode) {
      if (inv.expiresAt <= now) {
        this.byCode.delete(c);
        if (this.byHost.get(inv.hostId) === c) this.byHost.delete(inv.hostId);
      }
    }
  }
}

module.exports = { Invites, INVITE_TTL_MS };
