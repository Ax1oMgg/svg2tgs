# Установка

Файлы — на странице [Releases](https://github.com/Ax1oMgg/svg2tgs/releases/latest).
Контрольные суммы — в `SHA256SUMS.txt` там же.

## Windows 10/11

`svg2tgs_…_windows_x64-setup.exe`. Права администратора не нужны.

Если в системе нет WebView2 (компонент Windows для окон приложений),
установщик скачает его. Без интернета — `…_windows_x64-offline-setup.exe`,
WebView2 внутри.

Установщик без цифровой подписи, поэтому при первом запуске появится
«Windows защитила ваш компьютер»: «Подробнее» → «Выполнить в любом случае».

## macOS 10.15+

`svg2tgs_…_macos_universal.dmg` — Intel и Apple Silicon. Открыть файл и
перетащить svg2tgs в «Программы».

Программа без подписи Apple, поэтому при первом запуске macOS её не
откроет: «Системные настройки» → «Конфиденциальность и безопасность» →
«Всё равно открыть». На старых версиях — правый клик по программе →
«Открыть».

## Linux

- Debian, Ubuntu — `svg2tgs_…_linux_amd64.deb`:
  `sudo apt install ./svg2tgs_…_linux_amd64.deb`
- любой дистрибутив — `svg2tgs_…_linux_amd64.AppImage`: `chmod +x` и запустить.

## Командная строка

Архив `svg2tgs-cli_…` для своей системы — см. [CLI.md](CLI.md).
