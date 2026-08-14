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
    if (box) box.textContent = logs.slice(-12).join('\n');
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

  window.addEventListener('error', (e) => {
    dlog('window.error', e.message, e.filename, e.lineno);
  });
  window.addEventListener('unhandledrejection', (e) => {
    dlog('unhandledrejection', String(e.reason));
  });

  dlog('boot', {
    href: location.href,
    host: location.host,
    proto: location.protocol,
    hasTelegramGlobal: typeof window.Telegram !== 'undefined',
    hasWebApp: Boolean(window.Telegram?.WebApp),
    userAgent: navigator.userAgent.slice(0, 120),
  });

  const tg = window.Telegram?.WebApp;
  if (tg) {
    try {
      tg.ready();
      tg.expand();
      dlog('tg.ready/expand ok', {
        version: tg.version,
        platform: tg.platform,
        colorScheme: tg.colorScheme,
        initDataLen: (tg.initData || '').length,
        hasInitDataUnsafe: Boolean(tg.initDataUnsafe?.user),
        userId: tg.initDataUnsafe?.user?.id || null,
        hashLen: (location.hash || '').length,
      });
      try {
        tg.setHeaderColor('#0f1419');
        tg.setBackgroundColor('#0f1419');
      } catch (e) {
        dlog('setHeaderColor failed', String(e));
      }
    } catch (e) {
      dlog('tg.ready failed', String(e));
    }
  } else {
    dlog('NO Telegram.WebApp — SDK не загрузился или открыто вне Telegram');
  }

  // Fallback: Telegram кладёт данные в hash #tgWebAppData=...
  function extractInitDataFromHash() {
    const hash = location.hash || '';
    if (!hash.includes('tgWebAppData=')) return '';
    try {
      const params = new URLSearchParams(hash.replace(/^#/, ''));
      const raw = params.get('tgWebAppData') || '';
      const decoded = decodeURIComponent(raw);
      dlog('hash tgWebAppData len=', decoded.length);
      return decoded;
    } catch (e) {
      dlog('hash parse fail', String(e));
      return '';
    }
  }

  function getInitData() {
    const fromTg = tg?.initData || '';
    if (fromTg) {
      dlog('initData source=Telegram.WebApp len=', fromTg.length);
      return fromTg;
    }
    const fromHash = extractInitDataFromHash();
    if (fromHash) {
      dlog('initData source=location.hash len=', fromHash.length);
      return fromHash;
    }
    // sessionStorage иногда хранит то, что SDK уже распарсил
    try {
      const raw = sessionStorage.getItem('tgWebAppData') || sessionStorage.getItem('__telegram__initParams');
      dlog('sessionStorage probe', raw ? String(raw).slice(0, 80) : null);
    } catch (e) {
      dlog('sessionStorage fail', String(e));
    }
    dlog('initData EMPTY — открой из @ultima_ttt_bot, не из браузера');
    return '';
  }

  const els = {
    lobby: document.getElementById('lobby'),
    game: document.getElementById('game'),
    me: document.getElementById('me'),
    lobbyStatus: document.getElementById('lobbyStatus'),
    status: document.getElementById('status'),
    youSide: document.getElementById('youSide'),
    vs: document.getElementById('vs'),
    big: document.getElementById('big'),
    btnQueue: document.getElementById('btnQueue'),
    btnCancel: document.getElementById('btnCancel'),
    btnResign: document.getElementById('btnResign'),
    btnRematch: document.getElementById('btnRematch'),
    btnLobby: document.getElementById('btnLobby'),
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
        dlog('click cell', { b, c, mySide, turn: state?.turn, finished });
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

  function showLobby() {
    els.lobby.classList.remove('hidden');
    els.game.classList.add('hidden');
    els.btnQueue.classList.remove('hidden');
    els.btnCancel.classList.add('hidden');
    els.btnQueue.disabled = false;
  }

  function showSearching() {
    els.lobby.classList.remove('hidden');
    els.game.classList.add('hidden');
    els.btnQueue.classList.add('hidden');
    els.btnCancel.classList.remove('hidden');
  }

  function showGame() {
    els.lobby.classList.add('hidden');
    els.game.classList.remove('hidden');
    els.btnResign.classList.remove('hidden');
    els.btnRematch.classList.add('hidden');
    els.btnLobby.classList.add('hidden');
  }

  function send(obj) {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      dlog('send SKIP (ws not open)', obj.type, 'readyState=', ws?.readyState);
      return;
    }
    const text = JSON.stringify(obj);
    dlog('>>', obj.type, 'bytes=', text.length);
    ws.send(text);
  }

  function buildClientDebug() {
    return {
      hasTg: Boolean(tg),
      version: tg?.version || null,
      platform: tg?.platform || null,
      initDataLen: (tg?.initData || '').length,
      unsafeUser: tg?.initDataUnsafe?.user?.id || null,
      hrefHost: location.host,
      hashHasData: (location.hash || '').includes('tgWebAppData'),
      attempt: connectAttempt,
    };
  }

  function connect() {
    if (authFailed) {
      dlog('connect blocked: authFailed');
      return;
    }
    connectAttempt += 1;
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const url = `${proto}//${location.host}/ws`;
    dlog('WS connecting', url, 'attempt=', connectAttempt);
    els.lobbyStatus.textContent = 'Подключение к серверу…';

    try {
      ws = new WebSocket(url);
    } catch (e) {
      dlog('WS construct error', String(e));
      els.lobbyStatus.textContent = 'WS ошибка: ' + e;
      return;
    }

    ws.addEventListener('open', () => {
      dlog('WS open');
      els.lobbyStatus.textContent = 'Авторизация…';
      const initData = getInitData();
      send({
        type: 'hello',
        initData,
        debug: buildClientDebug(),
      });
    });

    ws.addEventListener('message', (ev) => {
      dlog('<< raw', String(ev.data).slice(0, 240));
      let msg;
      try {
        msg = JSON.parse(ev.data);
      } catch (e) {
        dlog('<< bad json', String(e));
        return;
      }
      onMessage(msg);
    });

    ws.addEventListener('close', (ev) => {
      dlog('WS close', { code: ev.code, reason: ev.reason, wasClean: ev.wasClean, welcomeOk, authFailed });
      if (authFailed) {
        els.lobbyStatus.textContent = 'Авторизация не прошла. Смотри лог ниже.';
        return;
      }
      if (welcomeOk && !authFailed) {
        els.lobbyStatus.textContent = 'Соединение потеряно, переподключение…';
        clearTimeout(reconnectTimer);
        reconnectTimer = setTimeout(connect, 1500);
      } else if (!welcomeOk) {
        els.lobbyStatus.textContent = 'Нет связи с сервером, повтор…';
        clearTimeout(reconnectTimer);
        reconnectTimer = setTimeout(connect, 2000);
      }
    });

    ws.addEventListener('error', () => {
      dlog('WS error event');
    });
  }

  function onMessage(msg) {
    dlog('onMessage', msg.type, msg.reason || msg.error || '');
    switch (msg.type) {
      case 'welcome':
        welcomeOk = true;
        authFailed = false;
        els.me.textContent = msg.name;
        els.lobbyStatus.textContent = msg.guest
          ? 'Гостевой режим'
          : 'Готов к игре';
        dlog('welcome OK', { playerId: msg.playerId, guest: msg.guest });
        send({ type: 'client_log', payload: { event: 'welcome_ack', ...buildClientDebug() } });
        break;
      case 'queued':
        showSearching();
        els.lobbyStatus.textContent = 'Ищем соперника…';
        break;
      case 'queue_cancelled':
        showLobby();
        els.lobbyStatus.textContent = 'Поиск отменён';
        break;
      case 'matched':
        mySide = msg.side;
        state = msg.state;
        finished = false;
        els.youSide.textContent = `Вы: ${msg.side}`;
        els.vs.textContent = `vs ${msg.opponent}`;
        showGame();
        render();
        dlog('matched', { side: msg.side, opponent: msg.opponent, roomId: msg.roomId, resumed: msg.resumed });
        if (msg.resumed) els.status.textContent = 'Переподключение — партия продолжается';
        if (tg?.HapticFeedback) tg.HapticFeedback.notificationOccurred('success');
        break;
      case 'state':
        state = msg.state;
        dlog('state', { turn: state.turn, forced: state.forced, moves: state.moves, over: state.over });
        render();
        break;
      case 'game_over':
        state = msg.state;
        finished = true;
        render();
        els.btnResign.classList.add('hidden');
        els.btnRematch.classList.remove('hidden');
        els.btnLobby.classList.remove('hidden');
        dlog('game_over', { result: msg.result, winnerSide: msg.winnerSide, reason: msg.reason });
        if (tg?.HapticFeedback) {
          tg.HapticFeedback.notificationOccurred(msg.winnerSide === mySide ? 'success' : 'error');
        }
        break;
      case 'left_room':
        dlog('left_room', msg);
        showLobby();
        els.lobbyStatus.textContent = 'Готов к игре';
        break;
      case 'error': {
        const reason = msg.reason || msg.error;
        dlog('ERROR from server', msg);
        if (msg.error === 'auth_failed') {
          authFailed = true;
          clearTimeout(reconnectTimer);
          const hint = msg.detail?.hint || '';
          els.lobbyStatus.textContent =
            'Ошибка авторизации: ' + reason + (hint ? ' — ' + hint : '');
        } else if (msg.error === 'in_game') {
          els.lobbyStatus.textContent = 'Уже в партии — открой экран игры или нажми «Ещё раз»';
        } else {
          els.lobbyStatus.textContent = 'Ошибка: ' + reason;
          showLobby();
        }
        break;
      }
      case 'pong':
        dlog('pong', msg);
        break;
      default:
        dlog('unknown msg', msg);
        break;
    }
  }

  els.btnQueue.addEventListener('click', () => {
    dlog('btnQueue');
    send({ type: 'queue' });
  });
  els.btnCancel.addEventListener('click', () => {
    dlog('btnCancel');
    send({ type: 'cancel_queue' });
  });
  els.btnResign.addEventListener('click', () => {
    dlog('btnResign');
    if (confirm('Сдаться?')) send({ type: 'resign' });
  });
  els.btnRematch.addEventListener('click', () => {
    dlog('btnRematch');
    finished = false;
    send({ type: 'rematch' });
    showSearching();
    els.lobbyStatus.textContent = 'Ищем соперника…';
  });
  els.btnLobby.addEventListener('click', () => {
    dlog('btnLobby');
    finished = false;
    state = null;
    mySide = null;
    send({ type: 'leave_room' });
    showLobby();
    els.lobbyStatus.textContent = 'Готов к игре';
  });

  setInterval(() => {
    if (welcomeOk && !authFailed) send({ type: 'ping' });
  }, 12000);

  // Даём SDK время прочитать hash / TelegramWebviewProxy
  let tries = 0;
  function waitAndConnect() {
    tries += 1;
    const len = (tg?.initData || '').length || extractInitDataFromHash().length;
    dlog('waitAndConnect try=', tries, 'initDataLen=', len, 'hasTg=', Boolean(tg));
    if (len > 0 || tries >= 8) {
      connect();
      return;
    }
    setTimeout(waitAndConnect, 100);
  }
  waitAndConnect();
})();
