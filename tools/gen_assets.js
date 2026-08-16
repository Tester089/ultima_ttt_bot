#!/usr/bin/env node
/* Генерирует таблицу вшитых ассетов по содержимому assets/. */
'use strict';
const fs = require('fs'), path = require('path');
const root = path.join(__dirname, '..');
const A = (p, mime, file, gz, age) =>
`    Asset {
        path: "${p}",
        mime: "${mime}",
        body: include_bytes!("../assets/${file}"),
        gzipped: ${gz},
        max_age: ${age},
    },`;

const rows = [
  A('/', 'text/html; charset=utf-8', 'index.html.gz', true, 0),
  A('/index.html', 'text/html; charset=utf-8', 'index.html.gz', true, 0),
  A('/tg.js', 'application/javascript', 'tg.js.gz', true, 3600),
];
for (const f of fs.readdirSync(path.join(root, 'assets/themes')).filter((x) => x.endsWith('.json.gz')))
  rows.push(A('/t/' + f.replace('.gz', ''), 'application/json', 'themes/' + f, true, 31536000));
for (const f of fs.readdirSync(path.join(root, 'assets/fonts')).sort()) {
  if (f.endsWith('.css.gz')) rows.push(A('/f/' + f.replace('.gz', ''), 'text/css', 'fonts/' + f, true, 31536000));
  else if (f.endsWith('.woff2')) rows.push(A('/f/' + f, 'font/woff2', 'fonts/' + f, false, 31536000));
}

const src = fs.readFileSync(path.join(root, 'src/assets.rs'), 'utf8');
const head = src.slice(0, src.indexOf('pub const ASSETS'));
const tail = src.slice(src.indexOf('pub fn find'));
fs.writeFileSync(path.join(root, 'src/assets.rs'),
  head + 'pub const ASSETS: &[Asset] = &[\n' + rows.join('\n') + '\n];\n\n' + tail);
console.log('таблица ассетов:', rows.length, 'записей');
