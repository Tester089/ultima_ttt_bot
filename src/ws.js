'use strict';

const { WebSocketServer } = require('ws');
const { validateInitData, guestProfile } = require('./auth');

function log(...args) {
  console.log('[ws]', new Date().toISOString(), ...args);
}

function attachWebSocket(server, { matchmaking, botToken, allowGuests }) {
  const wss = new WebSocketServer({ server, path: '/ws' });
  let connSeq = 0;

  wss.on('connection', (ws, req) => {
    const connId = ++connSeq;
    let playerId = null;
    const ip = req.headers['x-forwarded-for'] || req.socket.remoteAddress;
    const ua = req.headers['user-agent'] || '';
    log(`#${connId} CONNECT ip=${ip} ua=${ua.slice(0, 80)}`);

    ws.on('message', (raw) => {
      const text = String(raw);
      log(`#${connId} << rawLen=${text.length} preview=${text.slice(0, 200)}`);
      let msg;
      try {
        msg = JSON.parse(text);
      } catch (err) {
        log(`#${connId} bad_json`, err.message);
        safeSend(ws, { type: 'error', error: 'bad_json', detail: String(err.message) });
        return;
      }

      try {
        handle(ws, msg, connId);
      } catch (err) {
        console.error(`[ws] #${connId} handler error:`, err);
        safeSend(ws, { type: 'error', error: 'internal', detail: String(err.message || err) });
      }
    });

    ws.on('close', (code, reasonBuf) => {
      const reason = String(reasonBuf || '');
      log(`#${connId} CLOSE code=${code} reason=${reason} playerId=${playerId}`);
      matchmaking.detach(ws);
      playerId = null;
    });

    ws.on('error', (err) => {
      log(`#${connId} ERROR`, err.message);
    });

    function handle(socket, msg, cid) {
      log(`#${cid} msg.type=${msg.type} keys=${Object.keys(msg).join(',')}`);

      if (msg.type === 'hello') {
        const initData = msg.initData || '';
        log(
          `#${cid} hello initDataLen=${initData.length} clientDebug=`,
          JSON.stringify(msg.debug || null)
        );

        const result = validateInitData(initData, botToken);
        if (!result.ok) {
          log(`#${cid} AUTH FAIL reason=${result.reason}`, JSON.stringify(result.detail));
          if (!allowGuests) {
            safeSend(socket, {
              type: 'error',
              error: 'auth_failed',
              reason: result.reason,
              detail: {
                initDataLen: result.detail?.initDataLen,
                keys: result.detail?.keys,
                hasHash: result.detail?.hasHash,
                hasUser: result.detail?.hasUser,
                hashMatch: result.detail?.hashMatch,
                ageSec: result.detail?.ageSec,
                hint:
                  result.reason === 'empty_init_data'
                    ? 'Открой Mini App из Telegram-бота. SDK должен быть локальный.'
                    : result.reason === 'bad_hash'
                      ? 'hash не совпал — проверь BOT_TOKEN'
                      : undefined,
              },
            });
            socket.close(4001, result.reason);
            return;
          }
          log(`#${cid} falling back to guest (ALLOW_GUESTS=1)`);
          const profile = guestProfile();
          const player = matchmaking.register(socket, profile);
          playerId = player.id;
          safeSend(socket, {
            type: 'welcome',
            playerId: player.id,
            name: player.name,
            guest: true,
            authReason: result.reason,
          });
          return;
        }

        const player = matchmaking.register(socket, result.profile);
        playerId = player.id;
        log(`#${cid} AUTH OK playerId=${playerId} name=${player.name}`);
        safeSend(socket, {
          type: 'welcome',
          playerId: player.id,
          name: player.name,
          guest: false,
        });
        return;
      }

      if (!playerId) {
        log(`#${cid} reject: say_hello_first`);
        safeSend(socket, { type: 'error', error: 'say_hello_first' });
        return;
      }

      switch (msg.type) {
        case 'queue': {
          const r = matchmaking.enqueue(playerId);
          log(`#${cid} queue player=${playerId} result=`, r);
          break;
        }
        case 'cancel_queue':
          matchmaking.leaveQueue(playerId);
          safeSend(socket, { type: 'queue_cancelled' });
          log(`#${cid} cancel_queue player=${playerId}`);
          break;
        case 'move': {
          const r = matchmaking.move(playerId, Number(msg.board), Number(msg.cell));
          log(`#${cid} move player=${playerId} b=${msg.board} c=${msg.cell} result=`, r);
          if (!r.ok) safeSend(socket, { type: 'error', error: r.error || 'move_failed' });
          break;
        }
        case 'resign':
          log(`#${cid} resign player=${playerId}`);
          matchmaking.resign(playerId, 'resign');
          break;
        case 'rematch': {
          const r = matchmaking.rematch(playerId);
          log(`#${cid} rematch player=${playerId} result=`, r);
          break;
        }
        case 'ping':
          safeSend(socket, { type: 'pong', t: Date.now(), serverQueue: matchmaking.queue.length });
          break;
        case 'client_log':
          log(`#${cid} CLIENT_LOG`, JSON.stringify(msg.payload || msg));
          break;
        default:
          log(`#${cid} unknown_type`, msg.type);
          safeSend(socket, { type: 'error', error: 'unknown_type', got: msg.type });
      }
    }
  });

  setInterval(() => {
    log(
      `stats clients=${wss.clients.size} queue=${matchmaking.queue.length} rooms=${matchmaking.rooms.size}`
    );
  }, 30000);

  return wss;
}

function safeSend(ws, payload) {
  if (ws.readyState !== 1) return;
  const text = JSON.stringify(payload);
  console.log('[ws]', new Date().toISOString(), '>>', text.slice(0, 300));
  ws.send(text);
}

module.exports = { attachWebSocket };
