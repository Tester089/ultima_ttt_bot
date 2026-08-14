'use strict';

const LINES = [
  [0, 1, 2],
  [3, 4, 5],
  [6, 7, 8],
  [0, 3, 6],
  [1, 4, 7],
  [2, 5, 8],
  [0, 4, 8],
  [2, 4, 6],
];

function winner(arr) {
  for (const [a, b, c] of LINES) {
    if (arr[a] && arr[a] === arr[b] && arr[b] === arr[c]) return arr[a];
  }
  return arr.every(Boolean) ? '-' : null;
}

function createGame() {
  return {
    cells: Array.from({ length: 9 }, () => Array(9).fill(null)),
    boards: Array(9).fill(null),
    turn: 'X',
    forced: null,
    over: null,
    moves: 0,
  };
}

function playable(state, board) {
  if (state.over || state.boards[board]) return false;
  return state.forced === null || state.forced === board;
}

function play(state, board, cell) {
  if (!playable(state, board) || state.cells[board][cell]) {
    return { ok: false, error: 'illegal_move' };
  }

  const next = {
    cells: state.cells.map((row) => row.slice()),
    boards: state.boards.slice(),
    turn: state.turn,
    forced: state.forced,
    over: state.over,
    moves: state.moves + 1,
  };

  next.cells[board][cell] = next.turn;
  const local = winner(next.cells[board]);
  if (local) next.boards[board] = local;

  const global = winner(next.boards);
  if (global) next.over = global;

  next.forced = next.boards[cell] || next.over ? null : cell;
  next.turn = next.turn === 'X' ? 'O' : 'X';

  return { ok: true, state: next };
}

function publicState(state) {
  return {
    cells: state.cells,
    boards: state.boards,
    turn: state.turn,
    forced: state.forced,
    over: state.over,
    moves: state.moves,
  };
}

module.exports = {
  createGame,
  play,
  playable,
  publicState,
};
