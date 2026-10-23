# svg2tgs

Настольное приложение, которое превращает SVG в стикеры и эмодзи Telegram
(формат TGS). Windows, macOS и Linux. Работает полностью локально.

> Статус: **этап 1 — работает конвертация** (ядро и командная строка).
> Интерфейс приложения — этап 5. Функционал — в [`docs/SPEC.md`](docs/SPEC.md).

## Командная строка

```sh
cargo run --release -p svg2tgs-cli -- папка_с_svg -o папка_для_tgs
```
Параметры: `--preset sticker|emoji`, `-r` — искать во вложенных папках.

## Структура

```
core/            Rust-библиотека: SVG → Lottie → TGS, проверки Telegram
cli/             командная строка svg2tgs на том же ядре
app/             настольное приложение (Tauri 2 + React + TypeScript)
  src/           интерфейс
  src/styles/tokens.css   дизайн-токены (синхронизируются с Figma)
  src-tauri/     Rust-часть приложения
tests/fixtures/  тестовые SVG с описанием, что каждый проверяет
docs/            функционал, сборка
legacy/          исходный Python-скрипт (для сравнения, не используется)
```

## Быстрый старт

Подробно — в [`docs/BUILD.md`](docs/BUILD.md).

```sh
cd app
npm install
npm run tauri dev
```

## Лицензия

MIT — см. [`LICENSE`](LICENSE).
