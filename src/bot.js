'use strict';

const { Bot, InlineKeyboard, webhookCallback } = require('grammy');

function createBot({ token, publicUrl }) {
  const bot = new Bot(token);
  const appUrl = `${publicUrl.replace(/\/$/, '')}/?v=3`;

  const playKeyboard = new InlineKeyboard().webApp('Играть UTTT', appUrl);

  bot.command('start', async (ctx) => {
    console.log('[bot] /start from', ctx.from?.id, ctx.from?.username);
    await ctx.reply(
      'UTTT — ультимативные крестики-нолики.\n\n' +
        'Поле 3×3 из малых полей. Побеждает тот, кто соберёт линию на большом поле.\n\n' +
        'Жми кнопку — быстрый подбор соперника онлайн.',
      { reply_markup: playKeyboard }
    );
  });

  bot.command('play', async (ctx) => {
    console.log('[bot] /play from', ctx.from?.id, ctx.from?.username);
    await ctx.reply('Открывай мини-приложение и вставай в очередь:', {
      reply_markup: playKeyboard,
    });
  });

  bot.command('help', async (ctx) => {
    await ctx.reply(
      'Правила кратко:\n' +
        '• Ходи в клетках малого поля 3×3\n' +
        '• Соперник обязан играть в том большом поле, куда ты отправил ход\n' +
        '• Если поле занято или выиграно — можно выбрать любое\n' +
        '• Победа: линия из 3 малых полей на большом поле\n\n' +
        'Команды: /start /play /help'
    );
  });

  bot.on('message', async (ctx, next) => {
    console.log('[bot] message', ctx.from?.id, ctx.message?.text || ctx.message?.web_app_data);
    return next();
  });

  bot.catch((err) => {
    console.error('Bot error:', err.error || err);
  });

  return {
    bot,
    webhookMiddleware: webhookCallback(bot, 'express'),
  };
}

async function setupTelegram({ bot, publicUrl, secretToken }) {
  const base = publicUrl.replace(/\/$/, '');
  const webhookUrl = `${base}/telegram/webhook`;
  const appUrl = `${base}/?v=3`;

  await bot.api.setWebhook(webhookUrl, {
    secret_token: secretToken,
    drop_pending_updates: true,
  });

  await bot.api.setChatMenuButton({
    menu_button: {
      type: 'web_app',
      text: 'Играть',
      web_app: { url: appUrl },
    },
  });

  console.log('Webhook set:', webhookUrl);
  console.log('Menu button URL:', appUrl);
}

module.exports = { createBot, setupTelegram };
