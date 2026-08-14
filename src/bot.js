'use strict';

const { Bot, InlineKeyboard, webhookCallback } = require('grammy');

function appUrl(publicUrl, assetVersion, invCode) {
  const base = `${publicUrl.replace(/\/$/, '')}/?v=${assetVersion || '5'}`;
  if (invCode) return `${base}&inv=${encodeURIComponent(invCode)}`;
  return base;
}

function createBot({ token, publicUrl, assetVersion }) {
  const bot = new Bot(token);

  bot.command('start', async (ctx) => {
    const payload = (ctx.match || '').trim();
    console.log('[bot] /start from', ctx.from?.id, ctx.from?.username, 'payload=', payload);

    let inv = null;
    if (payload.startsWith('inv_')) inv = payload.slice(4);
    else if (/^[A-Z0-9]{6}$/i.test(payload)) inv = payload.toUpperCase();

    const url = appUrl(publicUrl, assetVersion, inv);
    const keyboard = new InlineKeyboard().webApp(inv ? 'Принять вызов' : 'Играть UTTT', url);

    if (inv) {
      await ctx.reply(
        `Тебя пригласили сыграть в UTTT.\nКод: ${inv.toUpperCase()}\n\nЖми кнопку — откроется партия с другом.`,
        { reply_markup: keyboard }
      );
      return;
    }

    await ctx.reply(
      'UTTT — ультимативные крестики-нолики.\n\n' +
        '• Быстрый подбор — рейтинговая очередь\n' +
        '• Пригласи друга из Mini App\n' +
        '• Рейтинг Glicko (как на chess.com): у новичков «плавает», потом стабилизируется\n\n' +
        'Жми кнопку и играй.',
      { reply_markup: keyboard }
    );
  });

  bot.command('play', async (ctx) => {
    const url = appUrl(publicUrl, assetVersion);
    await ctx.reply('Открывай мини-приложение:', {
      reply_markup: new InlineKeyboard().webApp('Играть UTTT', url),
    });
  });

  bot.command('top', async (ctx) => {
    await ctx.reply('Топ рейтинга смотри в Mini App (кнопка «Рейтинг») или /play.');
  });

  bot.command('help', async (ctx) => {
    await ctx.reply(
      'Команды: /start /play /help\n' +
        'Инвайт: друг присылает ссылку t.me/…?start=inv_CODE\n' +
        'Рейтинг: Glicko, provisional первые ~10 партий (число с ?).'
    );
  });

  bot.catch((err) => {
    console.error('Bot error:', err.error || err);
  });

  return {
    bot,
    webhookMiddleware: webhookCallback(bot, 'express'),
  };
}

async function setupTelegram({ bot, publicUrl, secretToken, assetVersion }) {
  const base = publicUrl.replace(/\/$/, '');
  const webhookUrl = `${base}/telegram/webhook`;
  const menuUrl = appUrl(publicUrl, assetVersion);

  await bot.api.setWebhook(webhookUrl, {
    secret_token: secretToken,
    drop_pending_updates: true,
  });

  await bot.api.setChatMenuButton({
    menu_button: {
      type: 'web_app',
      text: 'Играть',
      web_app: { url: menuUrl },
    },
  });

  console.log('Webhook set:', webhookUrl);
  console.log('Menu button URL:', menuUrl);
}

module.exports = { createBot, setupTelegram };
