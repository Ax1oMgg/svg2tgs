# svg2tgs

Настольное приложение, которое превращает SVG в стикеры и эмодзи Telegram
(формат TGS). Windows, macOS и Linux. Работает полностью локально.

> Статус: **этап 0 — каркас проекта.** Конвертация появится на этапе 1.
> План работ и функционал — в [`docs/SPEC.md`](docs/SPEC.md).

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
