(function () {
  const tg = window.Telegram?.WebApp;
  if (tg) {
    tg.ready();
    tg.expand();
    try {
      tg.setHeaderColor('#0f1419');
      tg.setBackgroundColor('#0f1419');
    } catch (_) {}
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
        ov.className = 'overlay show ' + (state.boards[b] === '-' ? 'draw' : state.boards[b] === 'X' ? 'x' : 'o');
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
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
  }

  function connect() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(`${proto}//${location.host}/ws`);

    ws.addEventListener('open', () => {
      els.lobbyStatus.textContent = 'Авторизация…';
      send({ type: 'hello', initData: tg?.initData || '' });
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

    ws.addEventListener('close', () => {
      els.lobbyStatus.textContent = 'Соединение потеряно, переподключение…';
      clearTimeout(reconnectTimer);
      reconnectTimer = setTimeout(connect, 1200);
    });
  }

  function onMessage(msg) {
    switch (msg.type) {
      case 'welcome':
        els.me.textContent = msg.name;
        els.lobbyStatus.textContent = msg.guest
          ? 'Гостевой режим (открой из Telegram для рейтинга позже)'
          : 'Готов к игре';
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
        if (tg?.HapticFeedback) tg.HapticFeedback.notificationOccurred('success');
        break;
      case 'state':
        state = msg.state;
        render();
        break;
      case 'game_over':
        state = msg.state;
        finished = true;
        render();
        els.btnResign.classList.add('hidden');
        els.btnRematch.classList.remove('hidden');
        els.btnLobby.classList.remove('hidden');
        if (tg?.HapticFeedback) {
          tg.HapticFeedback.notificationOccurred(msg.winnerSide === mySide ? 'success' : 'error');
        }
        break;
      case 'error':
        els.lobbyStatus.textContent = 'Ошибка: ' + msg.error;
        break;
      default:
        break;
    }
  }

  els.btnQueue.addEventListener('click', () => send({ type: 'queue' }));
  els.btnCancel.addEventListener('click', () => send({ type: 'cancel_queue' }));
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
    showLobby();
    els.lobbyStatus.textContent = 'Готов к игре';
  });

  setInterval(() => send({ type: 'ping' }), 25000);
  connect();
})();
