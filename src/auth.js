'use strict';

const crypto = require('crypto');

function parseInitData(initData) {
  const params = new URLSearchParams(initData);
  const data = {};
  for (const [key, value] of params.entries()) {
    data[key] = value;
  }
  return data;
}

function validateInitData(initData, botToken, maxAgeSec = 86400) {
  if (!initData || !botToken) return null;
  const data = parseInitData(initData);
  const hash = data.hash;
  if (!hash) return null;

  const check = Object.keys(data)
    .filter((k) => k !== 'hash')
    .sort()
    .map((k) => `${k}=${data[k]}`)
    .join('\n');

  const secret = crypto.createHmac('sha256', 'WebAppData').update(botToken).digest();
  const calc = crypto.createHmac('sha256', secret).update(check).digest('hex');
  if (calc !== hash) return null;

  const authDate = Number(data.auth_date || 0);
  if (authDate && Date.now() / 1000 - authDate > maxAgeSec) return null;

  let user = null;
  try {
    user = data.user ? JSON.parse(data.user) : null;
  } catch (_) {
    return null;
  }
  if (!user?.id) return null;

  return {
    id: String(user.id),
    name: [user.first_name, user.last_name].filter(Boolean).join(' ') || user.username || `User ${user.id}`,
    username: user.username || null,
  };
}

function guestProfile() {
  const id = `guest_${crypto.randomBytes(4).toString('hex')}`;
  return { id, name: `Гость ${id.slice(-4)}`, username: null, guest: true };
}

module.exports = { validateInitData, guestProfile };
