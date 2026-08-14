'use strict';

const http = require('http');
const path = require('path');
const crypto = require('crypto');
const express = require('express');

const { Matchmaking } = require('./matchmaking');
const { attachWebSocket } = require('./ws');
const { createBot, setupTelegram } = require('./bot');

const PORT = Number(process.env.PORT || 80);
const BOT_TOKEN = process.env.BOT_TOKEN || '';
const PUBLIC_URL = (process.env.PUBLIC_URL || '').replace(/\/$/, '');
const WEBHOOK_SECRET = process.env.WEBHOOK_SECRET || crypto.randomBytes(16).toString('hex');
const ALLOW_GUESTS = process.env.ALLOW_GUESTS === '1';

if (!BOT_TOKEN) {
  console.error('BOT_TOKEN is required');
  process.exit(1);
}

if (!PUBLIC_URL) {
  console.error('PUBLIC_URL is required (https URL of this app)');
  process.exit(1);
}

const app = express();
const server = http.createServer(app);
const matchmaking = new Matchmaking();

app.use(express.json());
app.use(express.static(path.join(__dirname, '..', 'public'), { maxAge: '1h' }));

app.get('/health', (_req, res) => {
  res.json({
    ok: true,
    queue: matchmaking.queue.length,
    rooms: matchmaking.rooms.size,
    players: matchmaking.players.size,
    ts: new Date().toISOString(),
  });
});

app.get('/debug/ping', (_req, res) => {
  res.type('text').send('uttt-ok ' + new Date().toISOString());
});

const { bot, webhookMiddleware } = createBot({ token: BOT_TOKEN, publicUrl: PUBLIC_URL });

app.post('/telegram/webhook', (req, res, next) => {
  const secret = req.get('x-telegram-bot-api-secret-token');
  if (secret !== WEBHOOK_SECRET) {
    res.sendStatus(401);
    return;
  }
  return webhookMiddleware(req, res, next);
});

attachWebSocket(server, {
  matchmaking,
  botToken: BOT_TOKEN,
  allowGuests: ALLOW_GUESTS,
});

server.listen(PORT, async () => {
  console.log(`[boot] ${new Date().toISOString()} UTTT listening on :${PORT}`);
  console.log(`[boot] PUBLIC_URL=${PUBLIC_URL}`);
  console.log(`[boot] ALLOW_GUESTS=${ALLOW_GUESTS}`);
  console.log(`[boot] BOT_TOKEN set=${Boolean(BOT_TOKEN)} len=${BOT_TOKEN.length} prefix=${BOT_TOKEN.slice(0, 10)}…`);
  try {
    await bot.init();
    const me = await bot.api.getMe();
    console.log(`[boot] bot=@${me.username} id=${me.id}`);
    await setupTelegram({ bot, publicUrl: PUBLIC_URL, secretToken: WEBHOOK_SECRET });
    console.log('[boot] Telegram ready');
  } catch (err) {
    console.error('[boot] Telegram setup failed:', err);
  }
});
