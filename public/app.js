(function () {
  const LOG_PREFIX = '[UTTT]';
  const logs = [];
  const debugBox = () => document.getElementById('debugBox');

  function stamp() {
    return new Date().toISOString().slice(11, 23);
  }

  function dlog(...args) {
    const line = [stamp(), ...args.map(stringify)].join(' ');
    logs.push(line);
    if (logs.length > 80) logs.shift();
    try {
      console.log(LOG_PREFIX, ...args);
    } catch (_) {}
    const box = debugBox();
    if (box) box.textContent = logs.slice(-10).join('\n');
  }

  function stringify(v) {
    if (v == null) return String(v);
    if (typeof v === 'string') return v;
    try {
      return JSON.stringify(v);
    } catch (_) {
      return String(v);
    }
  }

  window.addEventListener('error', (e) => dlog('window.error', e.message));
  window.addEventListener('unhandledrejection', (e) => dlog('unhandledrejection', String(e.reason)));

  const tg = window.Telegram?.WebApp;
  if (tg) {
    try {
      tg.ready();
      tg.expand();
      try {
        tg.setHeaderColor('#0f1419');
        tg.setBackgroundColor('#0f1419');
      } catch (_) {}
      dlog('tg ok', { platform: tg.platform, initDataLen: (tg.initData || '').length });
    } catch (e) {
      dlog('tg.ready failed', String(e));
    }
  } else {
    dlog('NO Telegram.WebApp');
  }

  function extractInitDataFromHash() {
    const hash = location.hash || '';
    if (!hash.includes('tgWebAppData=')) return '';
    try {
      const params = new URLSearchParams(hash.replace(/^#/, ''));
      return decodeURIComponent(params.get('tgWebAppData') || '');
    } catch (_) {
      return '';
    }
  }

  function getInitData() {
    if (tg?.initData) return tg.initData;
    return extractInitDataFromHash();
  }

  function pendingInviteCode() {
    const q = new URLSearchParams(location.search).get('inv');
    if (q) return q.toUpperCase();
    const sp = tg?.initDataUnsafe?.start_param || '';
    if (String(sp).startsWith('inv_')) return String(sp).slice(4).toUpperCase();
    if (/^[A-Z0-9]{6}$/i.test(sp)) return String(sp).toUpperCase();
    return null;
  }

  const els = {
    lobby: document.getElementById('lobby'),
    game: document.getElementById('game'),
    me: document.getElementById('me'),
    ratingCard: document.getElementById('ratingCard'),
    lobbyStatus: document.getElementById('lobbyStatus'),
    status: document.getElementById('status'),
    youSide: document.getElementById('youSide'),
    vs: document.getElementById('vs'),
    big: document.getElementById('big'),
    btnQueue: document.getElementById('btnQueue'),
    btnInvite: document.getElementById('btnInvite'),
    btnCancel: document.getElementById('btnCancel'),
    btnCancelInvite: document.getElementById('btnCancelInvite'),
    btnTop: document.getElementById('btnTop'),
    btnShare: document.getElementById('btnShare'),
    btnCopy: document.getElementById('btnCopy'),
    btnResign: document.getElementById('btnResign'),
    btnRematch: document.getElementById('btnRematch'),
    btnLobby: document.getElementById('btnLobby'),
    inviteBox: document.getElementById('inviteBox'),
    inviteCode: document.getElementById('inviteCode'),
    topBox: document.getElementById('topBox'),
    topList: document.getElementById('topList'),
  };

  const cellEls = [];
  const boardEls = [];
  let mySide = null;
  let state = null;
  let finished = false;
  let ws = null;
  let reconnectTimer = null;
  let authFailed = false;
  let welcomeOk = false;
  let connectAttempt = 0;
  let inviteDeepLink = '';
  let inviteShareText = '';
  let autoJoinDone = false;

  for (let b = 0; b < 9; b++) {
    const bd = document.createElement('div');
    bd.className = 'board';
    const ov = document.createElement('div');
    ov.className = 'overlay';
    const cs = [];
    for (let c = 0; c < 9; c++) {
      const el = document.createElement('button');
      el.type = 'button';
      el.className = 'cell';
      el.addEventListener('click', () => {
        if (!ws || finished || !state || mySide !== state.turn) return;
        send({ type: 'move', board: b, cell: c });
      });
      bd.appendChild(el);
      cs.push(el);
    }
    bd.appendChild(ov);
    els.big.appendChild(bd);
    cellEls.push(cs);
    boardEls.push({ bd, ov });
  }

  function playable(b) {
    if (!state || finished || state.boards[b]) return false;
    return state.forced === null || state.forced === b;
  }

  function render() {
    if (!state) return;
    for (let b = 0; b < 9; b++) {
      const { bd, ov } = boardEls[b];
      const live = !finished && mySide === state.turn && playable(b);
      bd.classList.toggle('live', live);
      for (let c = 0; c < 9; c++) {
        const el = cellEls[b][c];
        const v = state.cells[b][c];
        el.textContent = v || '';
        el.classList.toggle('filled', Boolean(v));
        el.classList.toggle('x', v === 'X');
        el.classList.toggle('o', v === 'O');
      }
      if (state.boards[b]) {
        ov.className =
          'overlay show ' +
          (state.boards[b] === '-' ? 'draw' : state.boards[b] === 'X' ? 'x' : 'o');
        ov.textContent = state.boards[b] === '-' ? '' : state.boards[b];
      } else {
        ov.className = 'overlay';
        ov.textContent = '';
      }
    }

    if (finished || state.over) {
      if (state.over === '-') els.status.textContent = 'Ничья';
      else if (state.over === mySide) els.status.textContent = 'Победа!';
      else els.status.textContent = 'Поражение';
      return;
    }

    if (state.turn === mySide) {
      els.status.textContent =
        state.forced === null ? 'Ваш ход — любое поле' : 'Ваш ход — только подсвеченное поле';
    } else {
      els.status.textContent = 'Ход соперника…';
    }
  }

  function setRatingCard(rating, name) {
    if (!rating || rating.guest) {
      els.ratingCard.textContent = 'Гость — рейтинг не считается';
      return;
    }
    const g = rating.games || 0;
    const w = rating.wins || 0;
    const l = rating.losses || 0;
    const d = rating.draws || 0;
    els.ratingCard.textContent = `${name || 'Вы'}: ${rating.display}  ·  ${w}/${l}/${d} (${g} игр)`;
  }

  function showLobby() {
    els.lobby.classList.remove('hidden');
    els.game.classList.add('hidden');
    els.btnQueue.classList.remove('hidden');
    els.btnInvite.classList.remove('hidden');
    els.btnCancel.classList.add('hidden');
    els.btnCancelInvite.classList.add('hidden');
    els.btnQueue.disabled = false;
  }

  function showSearching() {
    els.lobby.classList.remove('hidden');
    els.game.classList.add('hidden');
    els.btnQueue.classList.add('hidden');
    els.btnInvite.classList.add('hidden');
    els.btnCancel.classList.remove('hidden');
    els.inviteBox.classList.add('hidden');
  }

  function showInviteWaiting() {
    els.lobby.classList.remove('hidden');
    els.game.classList.add('hidden');
    els.btnQueue.classList.add('hidden');
    els.btnInvite.classList.add('hidden');
    els.btnCancel.classList.add('hidden');
    els.btnCancelInvite.classList.remove('hidden');
    els.inviteBox.classList.remove('hidden');
  }

  function showGame() {
    els.lobby.classList.add('hidden');
    els.game.classList.remove('hidden');
    els.btnResign.classList.remove('hidden');
    els.btnRematch.classList.add('hidden');
    els.btnLobby.classList.add('hidden');
    els.inviteBox.classList.add('hidden');
  }

  function send(obj) {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      dlog('send SKIP', obj.type);
      return;
    }
    ws.send(JSON.stringify(obj));
    dlog('>>', obj.type);
  }

  function connect() {
    if (authFailed) return;
    connectAttempt += 1;
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(`${proto}//${location.host}/ws`);

    ws.addEventListener('open', () => {
      els.lobbyStatus.textContent = 'Авторизация…';
      send({
        type: 'hello',
        initData: getInitData(),
        debug: {
          hasTg: Boolean(tg),
          platform: tg?.platform || null,
          initDataLen: (tg?.initData || '').length,
          inv: pendingInviteCode(),
          attempt: connectAttempt,
        },
      });
    });

    ws.addEventListener('message', (ev) => {
      let msg;
      try {
        msg = JSON.parse(ev.data);
      } catch (_) {
        return;
      }
      onMessage(msg);
    });

    ws.addEventListener('close', (ev) => {
      dlog('WS close', ev.code, ev.reason);
      if (authFailed) return;
      clearTimeout(reconnectTimer);
      reconnectTimer = setTimeout(connect, welcomeOk ? 1500 : 2000);
      els.lobbyStatus.textContent = 'Переподключение…';
    });
  }

  function onMessage(msg) {
    dlog('<<', msg.type, msg.error || msg.reason || '');
    switch (msg.type) {
      case 'welcome':
        welcomeOk = true;
        authFailed = false;
        els.me.textContent = msg.name + (msg.rating?.display ? ` · ${msg.rating.display}` : '');
        setRatingCard(msg.rating, msg.name);
        els.lobbyStatus.textContent = msg.guest ? 'Гостевой режим' : 'Готов к игре';
        if (!autoJoinDone) {
          const inv = pendingInviteCode();
          if (inv) {
            autoJoinDone = true;
            els.lobbyStatus.textContent = 'Вход по инвайту ' + inv + '…';
            send({ type: 'join_invite', code: inv });
          }
        }
        break;
      case 'queued':
        showSearching();
        els.lobbyStatus.textContent = 'Ищем соперника…';
        break;
      case 'queue_cancelled':
        showLobby();
        els.lobbyStatus.textContent = 'Поиск отменён';
        break;
      case 'invite_created':
        inviteDeepLink = msg.deepLink;
        inviteShareText = msg.shareText;
        els.inviteCode.textContent = msg.code;
        showInviteWaiting();
        els.lobbyStatus.textContent = 'Ждём друга по ссылке…';
        break;
      case 'invite_cancelled':
        showLobby();
        els.lobbyStatus.textContent = 'Инвайт отменён';
        break;
      case 'matched': {
        mySide = msg.side;
        state = msg.state;
        finished = false;
        const myR = msg.ratings?.[mySide]?.display || '—';
        const oppR = msg.ratings?.[mySide === 'X' ? 'O' : 'X']?.display || '—';
        els.youSide.textContent = `Вы ${msg.side} (${myR})`;
        els.vs.textContent = `vs ${msg.opponent} (${oppR}) · ${msg.mode || 'ranked'}`;
        showGame();
        render();
        if (msg.resumed) els.status.textContent = 'Переподключение — партия продолжается';
        if (tg?.HapticFeedback) tg.HapticFeedback.notificationOccurred('success');
        break;
      }
      case 'state':
        state = msg.state;
        render();
        break;
      case 'game_over': {
        state = msg.state;
        finished = true;
        render();
        els.btnResign.classList.add('hidden');
        els.btnRematch.classList.remove('hidden');
        els.btnLobby.classList.remove('hidden');
        if (msg.ratingDelta && mySide) {
          const d = msg.ratingDelta[mySide];
          const sign = d.to - d.from >= 0 ? '+' : '';
          els.status.textContent =
            (state.over === mySide ? 'Победа! ' : state.over === '-' ? 'Ничья. ' : 'Поражение. ') +
            `Рейтинг ${d.from} → ${d.to} (${sign}${Math.round(d.to - d.from)})`;
          if (msg.ratings?.[mySide]) setRatingCard(msg.ratings[mySide], els.me.textContent.split(' · ')[0]);
        }
        break;
      }
      case 'left_room':
        showLobby();
        els.lobbyStatus.textContent = 'Готов к игре';
        break;
      case 'leaderboard':
        els.topBox.classList.remove('hidden');
        els.topList.innerHTML = (msg.rows || [])
          .map(
            (r) =>
              `<li><strong>${r.rank}. ${r.name}</strong> — ${r.display} <span>(${r.wins}/${r.losses}/${r.draws})</span></li>`
          )
          .join('') || '<li>Пока пусто — сыграйте рейтинговую партию</li>';
        break;
      case 'error': {
        const reason = msg.reason || msg.error;
        if (msg.error === 'auth_failed') {
          authFailed = true;
          els.lobbyStatus.textContent = 'Ошибка авторизации: ' + reason;
        } else if (msg.error === 'in_game') {
          els.lobbyStatus.textContent = 'Уже в партии';
        } else if (msg.error === 'host_offline') {
          els.lobbyStatus.textContent = 'Хост инвайта офлайн — пусть откроет игру';
          showLobby();
        } else if (msg.error === 'invite_not_found') {
          els.lobbyStatus.textContent = 'Инвайт не найден или истёк';
          showLobby();
        } else {
          els.lobbyStatus.textContent = 'Ошибка: ' + reason;
        }
        break;
      }
      default:
        break;
    }
  }

  els.btnQueue.addEventListener('click', () => send({ type: 'queue' }));
  els.btnCancel.addEventListener('click', () => send({ type: 'cancel_queue' }));
  els.btnInvite.addEventListener('click', () => send({ type: 'create_invite' }));
  els.btnCancelInvite.addEventListener('click', () => send({ type: 'cancel_invite' }));
  els.btnTop.addEventListener('click', () => {
    send({ type: 'leaderboard', limit: 20 });
    fetch('/api/leaderboard?limit=20')
      .then((r) => r.json())
      .then((data) => onMessage({ type: 'leaderboard', rows: data.rows }))
      .catch(() => {});
  });
  els.btnShare.addEventListener('click', () => {
    if (tg?.openTelegramLink && inviteDeepLink) {
      tg.openTelegramLink(`https://t.me/share/url?url=${encodeURIComponent(inviteDeepLink)}&text=${encodeURIComponent('Сыграем в UTTT!')}`);
      return;
    }
    if (navigator.share && inviteShareText) {
      navigator.share({ text: inviteShareText }).catch(() => {});
      return;
    }
    els.btnCopy.click();
  });
  els.btnCopy.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(inviteDeepLink || inviteShareText);
      els.lobbyStatus.textContent = 'Ссылка скопирована';
    } catch (_) {
      els.lobbyStatus.textContent = inviteDeepLink;
    }
  });
  els.btnResign.addEventListener('click', () => {
    if (confirm('Сдаться?')) send({ type: 'resign' });
  });
  els.btnRematch.addEventListener('click', () => {
    finished = false;
    send({ type: 'rematch' });
    showSearching();
    els.lobbyStatus.textContent = 'Ищем соперника…';
  });
  els.btnLobby.addEventListener('click', () => {
    finished = false;
    state = null;
    mySide = null;
    send({ type: 'leave_room' });
    showLobby();
  });

  setInterval(() => {
    if (welcomeOk && !authFailed) send({ type: 'ping' });
  }, 12000);

  let tries = 0;
  (function waitAndConnect() {
    tries += 1;
    const len = (tg?.initData || '').length || extractInitDataFromHash().length;
    if (len > 0 || tries >= 8) connect();
    else setTimeout(waitAndConnect, 100);
  })();
})();
