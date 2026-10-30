# Сборка и запуск

## Что нужно установить один раз

### Windows 10/11
1. **Rust**: скачать и запустить `rustup-init.exe` с https://rustup.rs
   (на вопрос про Visual Studio C++ Build Tools — согласиться, установщик
   поставит их сам; нужен компонент «Desktop development with C++»).
2. **Node.js** 22 LTS или новее: https://nodejs.org
3. **WebView2**: в Windows 11 уже есть; в Windows 10 обычно ставится
   вместе с Edge. Если нет — https://developer.microsoft.com/microsoft-edge/webview2/

### macOS
1. `xcode-select --install`
2. Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
3. Node.js 22+: https://nodejs.org

### Linux (Debian/Ubuntu)
```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
  libayatana-appindicator3-dev libssl-dev build-essential
```
Плюс Rust (как для macOS) и Node.js 22+.

## Запуск в режиме разработки

В PowerShell на Windows вместо `npm` пишите `npm.cmd`, если появляется
ошибка «выполнение сценариев отключено в этой системе».

```sh
cd app
npm install
npm run tauri dev
```
Окно приложения откроется само. Правки интерфейса в `app/src`
применяются мгновенно, правки Rust — с пересборкой.

Только интерфейс в обычном браузере, без Rust (тестовые данные):
```sh
cd app
npm run dev
```
и открыть http://127.0.0.1:1420

## Сборка установщиков

```sh
cd app
npm run tauri build
```
Результат — в `target/release/bundle/`.

Офлайн-установщик для Windows (WebView2 внутри):
```sh
npm run tauri -- build --config src-tauri/tauri.windows-offline.conf.json
```

Компилятор C++ из списка выше нужен и для rlottie (рендер превью):
он собирается из исходников в `render/vendor` при первой сборке,
cmake и интернет не нужны.

## Ядро, командная строка и тесты

```sh
cargo test                     # все тесты, включая попиксельные
cargo run -p svg2tgs-cli -- tests/fixtures -o out
```

Попиксельный тест (`render/tests/pixel.rs`) конвертирует каждый SVG
из `tests/fixtures`, рисует результат движком Telegram (rlottie),
эталон — resvg, и сравнивает. При провале картинка различий появится
в `target/pixel-diff/`: эталон | результат | карта различий.

Тесты на форке rlottie из Telegram Desktop (LGPL, в сборку
приложения не входит — см. `render/vendor/README.md`):
```sh
cargo test -p svg2tgs-render --features telegram-rlottie
```

## Проверка на большом наборе SVG

```sh
cargo run --release -p svg2tgs-render --example corpus -- ПАПКА_С_SVG --out отчёт
```
В папке `отчёт`: `summary.txt` со статистикой, `all.csv` по каждому
файлу и картинки различий для самых непохожих.

## Иконка приложения

Два исходника: `app/app-icon.svg` — основная иконка (48 px и больше),
`app/app-icon-small.svg` — упрощённая для 16–32 px (панель задач, заголовок
окна, проводник): линии толще, без узлов, без теней. После правки любого
из них — из корня репозитория (нужны Node и Python с Pillow):
```sh
python tools/icons.py
```
Скрипт собирает набор `app/src-tauri/icons` (png, ico, icns); в `icon.ico`
размеры 16, 24 и 32 берутся из упрощённой иконки, 48, 64 и 256 — из основной.

## Зависимости и безопасность сборки

- Шаги GitHub Actions подключены по хешу коммита, версия — в комментарии
  (`actions/checkout@fbc6f39… # v5.1.0`). Метку версии автор шага может
  перевесить на другой код, хеш — нет. Новый шаг подключать так же.
- Проверка «Уязвимости» (`.github/workflows/audit.yml`) в каждом PR и раз
  в неделю сверяет `Cargo.lock` с базой RustSec, а `app/package-lock.json` —
  с базой npm. Падает на известной уязвимости. Предупреждения («заброшена»,
  «unsound») только пишет в журнал.
- Dependabot (`.github/dependabot.yml`) раз в месяц предлагает PR
  с обновлениями: шаги сборки, Rust, npm — по одному PR на каждую группу.
- Руками, перед выпуском:
  ```sh
  cargo install cargo-audit --locked && cargo audit
  (cd app && npm audit --omit=dev)
  ```
