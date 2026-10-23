# svg2tgs

Настольное приложение, которое превращает SVG в стикеры и эмодзи Telegram
(формат TGS). Windows, macOS и Linux. Работает полностью локально.

> Статус: **этап 2 — конвертация и попиксельная проверка движком Telegram.**
> Интерфейс приложения — этап 5. Функционал — в [`docs/SPEC.md`](docs/SPEC.md).

## Командная строка

```sh
cargo run --release -p svg2tgs-cli -- папка_с_svg -o папка_для_tgs
```
Параметры: `--preset sticker|emoji`, `-r` — искать во вложенных папках.

## Структура

```
core/            Rust-библиотека: SVG → Lottie → TGS, проверки Telegram
render/          рендер для превью и тестов: TGS через rlottie, SVG через resvg
  vendor/        исходники rlottie (движок Telegram) — см. README внутри
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

MIT — см. [`LICENSE`](LICENSE). Сторонние компоненты —
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
