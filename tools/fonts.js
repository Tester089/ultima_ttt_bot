#!/usr/bin/env node
/* Шрифты не хранятся в репозитории: 28 бинарников на 390 КБ в истории
   ни к чему. Скрипт тянет их из npm и раскладывает по assets/fonts.
   Берём только нужные начертания: моно 400, акцидентный 600, текстовый 400+600. */
'use strict';
const fs = require('fs'), path = require('path'), cp = require('child_process');

const THEME_FONTS = {
  linza:     { d: 'ibm-plex-sans', b: 'ibm-plex-sans', m: 'ibm-plex-mono' },
  linzaDark: { d: 'unbounded',     b: 'manrope',       m: 'ibm-plex-mono' },
  kamen:     { d: 'roboto-slab',   b: 'archivo',       m: 'ibm-plex-mono' },
  myata:     { d: 'comfortaa',     b: 'nunito',        m: 'ibm-plex-mono' },
  list:      { d: 'commissioner',  b: 'manrope',       m: 'ibm-plex-mono' },
  grafit:    { d: 'inter-tight',   b: 'inter-tight',   m: 'ibm-plex-mono' },
};
const FAMILY = {
  'ibm-plex-sans': 'IBM Plex Sans', 'ibm-plex-mono': 'IBM Plex Mono',
  manrope: 'Manrope', 'inter-tight': 'Inter Tight', unbounded: 'Unbounded',
  'roboto-slab': 'Roboto Slab', archivo: 'Archivo', comfortaa: 'Comfortaa',
  nunito: 'Nunito', commissioner: 'Commissioner',
};
// Диапазоны нужны, чтобы браузер качал кириллицу и латиницу раздельно.
const RANGE = {
  latin: 'U+0000-00FF,U+0131,U+0152-0153,U+2000-206F,U+20AC,U+2122,U+2212',
  cyrillic: 'U+0301,U+0400-045F,U+0490-0491,U+04B0-04B1,U+2116',
};

const root = path.join(__dirname, '..');
const out = path.join(root, 'assets', 'fonts');
const tmp = path.join(root, '.fonts-tmp');
fs.mkdirSync(out, { recursive: true });
fs.mkdirSync(tmp, { recursive: true });

const families = [...new Set(Object.values(THEME_FONTS).flatMap((f) => [f.d, f.b, f.m]))];
const pkgs = families.map((f) => '@fontsource/' + f).join(' ');
console.log('качаю', families.length, 'семейств…');
cp.execSync(`npm install --no-audit --no-fund --silent --prefix "${tmp}" ${pkgs}`, { stdio: 'inherit' });

let bytes = 0, copied = new Set();
for (const [theme, f] of Object.entries(THEME_FONTS)) {
  const want = [[f.d, 600], [f.b, 400], [f.b, 600], [f.m, 400]];
  let css = '', seen = new Set();
  for (const [fam, wt] of want) {
    for (const sub of ['latin', 'cyrillic']) {
      const key = fam + wt + sub;
      if (seen.has(key)) continue;
      seen.add(key);
      const file = `${fam}-${sub}-${wt}-normal.woff2`;
      const src = path.join(tmp, 'node_modules/@fontsource', fam, 'files', file);
      if (!fs.existsSync(src)) continue;
      if (!copied.has(file)) {
        fs.copyFileSync(src, path.join(out, file));
        bytes += fs.statSync(src).size;
        copied.add(file);
      }
      css += `@font-face{font-family:"${FAMILY[fam]}";font-style:normal;font-weight:${wt};`
           + `font-display:swap;src:url(/f/${file}) format("woff2");unicode-range:${RANGE[sub]}}`;
    }
  }
  css += `:root{--display:"${FAMILY[f.d]}",system-ui,sans-serif;`
       + `--body:"${FAMILY[f.b]}",system-ui,sans-serif;`
       + `--mono:"${FAMILY[f.m]}",ui-monospace,monospace}`;
  fs.writeFileSync(path.join(out, theme + '.css'), css);
}

fs.rmSync(tmp, { recursive: true, force: true });
console.log(`готово: ${copied.size} woff2, ${(bytes / 1024).toFixed(1)} КБ`);
