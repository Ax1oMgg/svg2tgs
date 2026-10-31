# Участие в проекте

Язык issue, PR, комментариев и коммитов — русский или английский.
Идентификаторы в коде — на английском.

## Виды вклада

- **Ошибка** — [issue](../../issues/new/choose) с версией программы,
  системой и SVG, на котором воспроизводится проблема.
- **Перевод** — один JSON-файл, см. [`docs/TRANSLATIONS.md`](docs/TRANSLATIONS.md).
- **Идея** — issue. Планы: [`docs/ROADMAP.md`](docs/ROADMAP.md).
- **Код** — сначала issue с описанием изменения.

## Требования

Полный список — [`CLAUDE.md`](CLAUDE.md).

- Приложение не обращается к сети: без телеметрии, обновлений и CDN.
- SVG разбирается только через `usvg`.
- Ядро возвращает коды замечаний, тексты — в интерфейсе и CLI.
- Требования Telegram — по официальной документации, закреплены тестами.
- Изменение конвертера — с попиксельным тестом (`render/tests/pixel.rs`).
- Интерфейс — только переменные из `app/src/styles/tokens.css`.

## Pull request

1. Ветка от `main` в форке.
2. Проверки (сборка — [`docs/BUILD.md`](docs/BUILD.md)):

   ```sh
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cd app && npm run build
   ```

3. Изменение поведения — запись в [`docs/SPEC.md`](docs/SPEC.md).
4. Шаблон PR заполнен, проверки на GitHub зелёные.

Вклад принимается на условиях лицензии [MIT](LICENSE).
