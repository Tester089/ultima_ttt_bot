'use strict';

const http = require('http');
const path = require('path');
const crypto = require('crypto');
const express = require('express');

const { Store } = require('./store');
const { Matchmaking } = require('./matchmaking');
const { attachWebSocket } = require('./ws');
const { createBot, setupTelegram } = require('./bot');

const PORT = Number(process.env.PORT || 80);
const BOT_TOKEN = process.env.BOT_TOKEN || '';
const PUBLIC_URL = (process.env.PUBLIC_URL || '').replace(/\/$/, '');
const WEBHOOK_SECRET = process.env.WEBHOOK_SECRET || crypto.randomBytes(16).toString('hex');
const ALLOW_GUESTS = process.env.ALLOW_GUESTS === '1';
const DATA_DIR = process.env.DATA_DIR || path.join(process.cwd(), 'data');
const ASSET_V = '5';

if (!BOT_TOKEN) {
  console.error('BOT_TOKEN is required');
  process.exit(1);
}

if (!PUBLIC_URL) {
  console.error('PUBLIC_URL is required (https URL of this app)');
  process.exit(1);
}

async function main() {
  const store = new Store(DATA_DIR);
  await store.init();

  const matchmaking = new Matchmaking({ store, botUsername: 'ultima_ttt_bot' });

  const app = express();
  const server = http.createServer(app);

  app.use(express.json());

  app.use((req, res, next) => {
    if (/\.(?:html|js|css)$/i.test(req.path) || req.path === '/' || req.path === '') {
      res.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate, max-age=0');
      res.setHeader('Pragma', 'no-cache');
      res.setHeader('Expires', '0');
    }
    next();
  });

  app.use(
    express.static(path.join(__dirname, '..', 'public'), {
      etag: false,
      lastModified: false,
      setHeaders(res, filePath) {
        if (/\.(?:html|js|css)$/i.test(filePath)) {
          res.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate, max-age=0');
        }
      },
    })
  );

  app.get('/health', (_req, res) => {
    res.json({
      ok: true,
      queue: matchmaking.queue.length,
      rooms: matchmaking.rooms.size,
      players: matchmaking.players.size,
      ratedPlayers: store.players.size,
      ts: new Date().toISOString(),
    });
  });

  app.get('/api/leaderboard', (req, res) => {
    const limit = Math.min(50, Math.max(1, Number(req.query.limit) || 20));
    res.json({ rows: store.leaderboard(limit) });
  });

  app.get('/debug/ping', (_req, res) => {
    res.type('text').send('uttt-ok ' + new Date().toISOString());
  });

  const { bot, webhookMiddleware } = createBot({
    token: BOT_TOKEN,
    publicUrl: PUBLIC_URL,
    assetVersion: ASSET_V,
  });

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
    console.log(`[boot] PUBLIC_URL=${PUBLIC_URL} DATA_DIR=${DATA_DIR}`);
    console.log(`[boot] ALLOW_GUESTS=${ALLOW_GUESTS}`);
    try {
      await bot.init();
      const me = await bot.api.getMe();
      matchmaking.botUsername = me.username || matchmaking.botUsername;
      console.log(`[boot] bot=@${me.username} id=${me.id}`);
      await setupTelegram({
        bot,
        publicUrl: PUBLIC_URL,
        secretToken: WEBHOOK_SECRET,
        assetVersion: ASSET_V,
      });
      console.log('[boot] Telegram ready');
    } catch (err) {
      console.error('[boot] Telegram setup failed:', err);
    }
  });
}

main().catch((err) => {
  console.error('[boot] fatal', err);
  process.exit(1);
});
