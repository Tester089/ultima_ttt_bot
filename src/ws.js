'use strict';

const { WebSocketServer } = require('ws');
const { validateInitData, guestProfile } = require('./auth');

function attachWebSocket(server, { matchmaking, botToken, allowGuests }) {
  const wss = new WebSocketServer({ server, path: '/ws' });

  wss.on('connection', (ws) => {
    let playerId = null;

    ws.on('message', (raw) => {
      let msg;
      try {
        msg = JSON.parse(String(raw));
      } catch (_) {
        ws.send(JSON.stringify({ type: 'error', error: 'bad_json' }));
        return;
      }

      try {
        handle(ws, msg);
      } catch (err) {
        console.error('WS handler error:', err);
        ws.send(JSON.stringify({ type: 'error', error: 'internal' }));
      }
    });

    ws.on('close', () => {
      matchmaking.detach(ws);
      playerId = null;
    });

    function handle(socket, msg) {
      if (msg.type === 'hello') {
        let profile = validateInitData(msg.initData || '', botToken);
        if (!profile) {
          if (!allowGuests) {
            socket.send(JSON.stringify({ type: 'error', error: 'auth_failed' }));
            socket.close(4001, 'auth_failed');
            return;
          }
          profile = guestProfile();
        }
        const player = matchmaking.register(socket, profile);
        playerId = player.id;
        socket.send(
          JSON.stringify({
            type: 'welcome',
            playerId: player.id,
            name: player.name,
            guest: Boolean(profile.guest),
          })
        );
        return;
      }

      if (!playerId) {
        socket.send(JSON.stringify({ type: 'error', error: 'say_hello_first' }));
        return;
      }

      switch (msg.type) {
        case 'queue':
          matchmaking.enqueue(playerId);
          break;
        case 'cancel_queue':
          matchmaking.leaveQueue(playerId);
          socket.send(JSON.stringify({ type: 'queue_cancelled' }));
          break;
        case 'move':
          matchmaking.move(playerId, Number(msg.board), Number(msg.cell));
          break;
        case 'resign':
          matchmaking.resign(playerId, 'resign');
          break;
        case 'rematch':
          matchmaking.rematch(playerId);
          break;
        case 'ping':
          socket.send(JSON.stringify({ type: 'pong', t: Date.now() }));
          break;
        default:
          socket.send(JSON.stringify({ type: 'error', error: 'unknown_type' }));
      }
    }
  });

  return wss;
}

module.exports = { attachWebSocket };
