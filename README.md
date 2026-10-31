# svg2tgs

Настольное приложение, которое превращает SVG в стикеры и эмодзи Telegram
(формат TGS). Windows, macOS и Linux. Работает полностью локально.

Проект «Чайная Аксиома». Автор — Ax1oM.

> Статус: **этап 7 — установщики** для Windows, macOS и Linux собираются
> на GitHub при выпуске версии. Функционал — в [`docs/SPEC.md`](docs/SPEC.md),
> план развития — в [`docs/ROADMAP.md`](docs/ROADMAP.md).

## Установка

Скачайте файл для своей системы на странице
[Releases](https://github.com/Ax1oMgg/svg2tgs/releases).

**Windows 10/11** — `…_windows_x64-setup.exe`. Установка без прав
администратора. Если в системе нет WebView2 (компонент Windows для окон
приложений), установщик скачает его; без интернета берите
`…_windows_x64-offline-setup.exe`, там он уже внутри.
Установщик пока без цифровой подписи, поэтому при первом запуске Windows
покажет «Windows защитила ваш компьютер»: нажмите «Подробнее» →
«Выполнить в любом случае».

**macOS 10.15+** — `…_macos_universal.dmg` (Intel и Apple Silicon).
Откройте файл и перетащите svg2tgs в «Программы». Программа без подписи
Apple, поэтому при первом запуске macOS её не откроет: зайдите в
«Системные настройки» → «Конфиденциальность и безопасность» и нажмите
«Всё равно открыть» (на старых версиях — правый клик по программе →
«Открыть»).

**Linux** — `…_linux_amd64.deb` для Debian и Ubuntu
(`sudo apt install ./svg2tgs_…_linux_amd64.deb`) или `…_linux_amd64.AppImage`
для любого дистрибутива (`chmod +x` и запустить).

Программа не обращается к интернету и ничего не отправляет.

## Командная строка

Архив `svg2tgs-cli_…` для своей системы там же, на странице Releases:

```sh
svg2tgs папка_с_svg -o папка_для_tgs
```
Из исходников: `cargo run --release -p svg2tgs-cli -- …`.
Параметры: `--preset sticker|emoji`, `-r` — вложенные папки, `--on-conflict`,
`--padding`, `--bounds`, `--fit`, `--scale`, `--position`, `--flip-h`, `--flip-v`, `--rotate`, `--aspect`, `--crop`, `--bg`, `--bg-radius`, `--bg-border`, `--outline`, `--optimize`, `--fps`, `--duration`,
`--json` — отчёт для скриптов (подробно — `--help`).

## Структура

```
core/            Rust-библиотека: SVG → Lottie → TGS, проверки Telegram
render/          рендер для превью и тестов: TGS через rlottie, SVG через resvg
  vendor/        исходники rlottie (движок Telegram) — см. README внутри
cli/             командная строка svg2tgs на том же ядре
app/             настольное приложение (Tauri 2 + React + TypeScript)
  src/           интерфейс
  src/locales/   переводы — по файлу на язык (docs/TRANSLATIONS.md)
  src/styles/tokens.css   дизайн-токены (общие с макетом, docs/DESIGN.md)
  src-tauri/     Rust-часть приложения
tests/fixtures/  тестовые SVG с описанием, что каждый проверяет
docs/            функционал, сборка, выпуск, макет
.github/         проверки и сборка установщиков (GitHub Actions)
legacy/          исходный Python-скрипт (для сравнения, не используется)
```

## Сборка из исходников

Подробно — в [`docs/BUILD.md`](docs/BUILD.md), выпуск версии — в
[`docs/RELEASE.md`](docs/RELEASE.md).

```sh
cd app
npm install
npm run tauri dev
```

## Лицензия

MIT — см. [`LICENSE`](LICENSE). Сторонние компоненты —
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
