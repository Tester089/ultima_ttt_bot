#!/bin/sh
# Полная сборка: шрифты из npm, сжатие ассетов, генерация таблицы, релиз.
# В рантайме сервер не открывает файлов и ничего не сжимает.
set -e
cd "$(dirname "$0")"

[ -d assets/fonts ] && [ -n "$(ls assets/fonts/*.woff2 2>/dev/null)" ] || node tools/fonts.js

gzip -9 -kf assets/index.html
gzip -9 -kf assets/tg.js
for f in assets/themes/*.json assets/fonts/*.css; do gzip -9 -kf "$f"; done

node tools/gen_assets.js
cargo build --release
ls -la target/release/uttt
