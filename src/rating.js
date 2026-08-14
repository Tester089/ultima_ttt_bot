'use strict';

/**
 * Glicko-1 (как база Chess.com): рейтинг + RD (неопределённость).
 * «Хитрость»: высокий RD у новичков → рейтинг быстро плавает первые партии,
 * потом стабилизируется; отображаем provisional пока games < 10 или RD > 110.
 */

const Q = Math.log(10) / 400;
const DEFAULT_R = 1200;
const DEFAULT_RD = 350;
const MIN_RD = 45;
const MAX_RD = 350;
const TYPICAL_RD = 50; // «спокойный» RD для формулы c

function g(rd) {
  return 1 / Math.sqrt(1 + (3 * Q * Q * rd * rd) / (Math.PI * Math.PI));
}

function E(r, rOpp, rdOpp) {
  return 1 / (1 + Math.pow(10, (-g(rdOpp) * (r - rOpp)) / 400));
}

function clampRd(rd) {
  return Math.max(MIN_RD, Math.min(MAX_RD, rd));
}

function defaultRating() {
  return { r: DEFAULT_R, rd: DEFAULT_RD, games: 0, wins: 0, losses: 0, draws: 0 };
}

/** Увеличение RD за время без игр (дни) — как у Glicko между рейтинговыми периодами */
function advanceRd(rd, daysIdle) {
  if (!daysIdle || daysIdle <= 0) return clampRd(rd);
  const c = 34; // ~как у шахматных систем: RD растёт в простое
  const next = Math.sqrt(rd * rd + c * c * Math.min(daysIdle, 100));
  return clampRd(next);
}

/**
 * score: 1 win, 0.5 draw, 0 loss (для игрока)
 * @returns {{ r: number, rd: number }}
 */
function updateGlicko(player, opponent, score) {
  const r = player.r;
  const rd = player.rd;
  const rOpp = opponent.r;
  const rdOpp = opponent.rd;

  const gOpp = g(rdOpp);
  const exp = E(r, rOpp, rdOpp);
  const d2Inv = Q * Q * gOpp * gOpp * exp * (1 - exp);
  const d2 = 1 / d2Inv;

  const newR = r + ((Q / (1 / (rd * rd) + 1 / d2)) * gOpp * (score - exp));
  const newRd = Math.sqrt(1 / (1 / (rd * rd) + 1 / d2));

  return {
    r: Math.round(newR * 10) / 10,
    rd: Math.round(clampRd(newRd) * 10) / 10,
  };
}

function isProvisional(rec) {
  return (rec.games || 0) < 10 || (rec.rd || DEFAULT_RD) > 110;
}

function displayRating(rec) {
  const n = Math.round(rec.r || DEFAULT_R);
  return isProvisional(rec) ? `${n}?` : String(n);
}

function publicRating(rec) {
  return {
    r: Math.round(rec.r || DEFAULT_R),
    rd: Math.round(rec.rd || DEFAULT_RD),
    games: rec.games || 0,
    wins: rec.wins || 0,
    losses: rec.losses || 0,
    draws: rec.draws || 0,
    provisional: isProvisional(rec),
    display: displayRating(rec),
  };
}

module.exports = {
  DEFAULT_R,
  DEFAULT_RD,
  defaultRating,
  advanceRd,
  updateGlicko,
  isProvisional,
  displayRating,
  publicRating,
};
