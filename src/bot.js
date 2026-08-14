'use strict';

const { Bot, InlineKeyboard, webhookCallback } = require('grammy');

function createBot({ token, publicUrl }) {
  const bot = new Bot(token);

  const playKeyboard = new InlineKeyboard().webApp('Играть UTTT', publicUrl);

  bot.command('start', async (ctx) => {
    await ctx.reply(
      'UTTT — ультимативные крестики-нолики.\n\n' +
        'Поле 3×3 из малых полей. Побеждает тот, кто соберёт линию на большом поле.\n\n' +
        'Жми кнопку — быстрый подбор соперника онлайн.',
      { reply_markup: playKeyboard }
    );
  });

  bot.command('play', async (ctx) => {
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

  bot.catch((err) => {
    console.error('Bot error:', err.error || err);
  });

  return {
    bot,
    webhookMiddleware: webhookCallback(bot, 'express'),
  };
}

async function setupTelegram({ bot, publicUrl, secretToken }) {
  const webhookUrl = `${publicUrl.replace(/\/$/, '')}/telegram/webhook`;

  await bot.api.setWebhook(webhookUrl, {
    secret_token: secretToken,
    drop_pending_updates: true,
  });

  await bot.api.setChatMenuButton({
    menu_button: {
      type: 'web_app',
      text: 'Играть',
      web_app: { url: publicUrl },
    },
  });

  console.log('Webhook set:', webhookUrl);
}

module.exports = { createBot, setupTelegram };
