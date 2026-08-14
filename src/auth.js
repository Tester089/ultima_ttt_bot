'use strict';

const crypto = require('crypto');

function log(...args) {
  console.log('[auth]', new Date().toISOString(), ...args);
}

function parseInitData(initData) {
  const params = new URLSearchParams(initData);
  const data = {};
  for (const [key, value] of params.entries()) {
    data[key] = value;
  }
  return data;
}

/**
 * @returns {{ ok: true, profile: object } | { ok: false, reason: string, detail?: object }}
 */
function validateInitData(initData, botToken, maxAgeSec = 86400) {
  const detail = {
    initDataType: typeof initData,
    initDataLen: initData ? String(initData).length : 0,
    hasBotToken: Boolean(botToken),
    botTokenLen: botToken ? String(botToken).length : 0,
    botTokenPrefix: botToken ? String(botToken).slice(0, 10) + '…' : null,
  };

  if (!botToken) {
    log('FAIL no_bot_token', detail);
    return { ok: false, reason: 'no_bot_token', detail };
  }

  if (!initData || typeof initData !== 'string' || !initData.trim()) {
    log('FAIL empty_init_data', detail);
    return { ok: false, reason: 'empty_init_data', detail };
  }

  const data = parseInitData(initData);
  const keys = Object.keys(data).sort();
  detail.keys = keys;
  detail.hasHash = Boolean(data.hash);
  detail.hasUser = Boolean(data.user);
  detail.hasSignature = Boolean(data.signature);
  detail.authDate = data.auth_date || null;
  detail.queryId = data.query_id ? String(data.query_id).slice(0, 12) + '…' : null;

  log('parsed initData keys=', keys.join(','), 'len=', detail.initDataLen);

  const hash = data.hash;
  if (!hash) {
    log('FAIL missing_hash', detail);
    return { ok: false, reason: 'missing_hash', detail };
  }

  // Official algorithm: secret = HMAC_SHA256(key="WebAppData", msg=bot_token)
  // then hash = hex(HMAC_SHA256(key=secret, msg=data_check_string))
  const check = keys
    .filter((k) => k !== 'hash')
    .map((k) => `${k}=${data[k]}`)
    .join('\n');

  detail.checkStringLen = check.length;
  detail.checkStringPreview = check.slice(0, 120).replace(/\n/g, '\\n');

  const secret = crypto.createHmac('sha256', 'WebAppData').update(botToken).digest();
  const calc = crypto.createHmac('sha256', secret).update(check).digest('hex');
  detail.hashRecvPrefix = hash.slice(0, 8);
  detail.hashCalcPrefix = calc.slice(0, 8);
  detail.hashMatch = calc === hash;

  if (calc !== hash) {
    // Try alternate key order (some wrong docs swap args) for diagnostics only
    const altSecret = crypto.createHmac('sha256', botToken).update('WebAppData').digest();
    const altCalc = crypto.createHmac('sha256', altSecret).update(check).digest('hex');
    detail.altHashMatch = altCalc === hash;
    log('FAIL bad_hash', detail);
    return { ok: false, reason: 'bad_hash', detail };
  }

  const authDate = Number(data.auth_date || 0);
  const ageSec = authDate ? Math.floor(Date.now() / 1000) - authDate : null;
  detail.ageSec = ageSec;
  if (authDate && ageSec > maxAgeSec) {
    log('FAIL expired', detail);
    return { ok: false, reason: 'expired', detail };
  }

  let user = null;
  try {
    user = data.user ? JSON.parse(data.user) : null;
  } catch (err) {
    detail.userParseError = String(err.message || err);
    log('FAIL user_parse', detail);
    return { ok: false, reason: 'user_parse', detail };
  }
  if (!user?.id) {
    log('FAIL no_user', detail);
    return { ok: false, reason: 'no_user', detail };
  }

  const profile = {
    id: String(user.id),
    name:
      [user.first_name, user.last_name].filter(Boolean).join(' ') ||
      user.username ||
      `User ${user.id}`,
    username: user.username || null,
  };

  log('OK user=', profile.id, profile.name, 'ageSec=', ageSec);
  return { ok: true, profile, detail };
}

function guestProfile() {
  const id = `guest_${crypto.randomBytes(4).toString('hex')}`;
  return { id, name: `Гость ${id.slice(-4)}`, username: null, guest: true };
}

module.exports = { validateInitData, guestProfile };
