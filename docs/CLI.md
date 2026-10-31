# Командная строка

Архив `svg2tgs-cli_…` для своей системы — на странице
[Releases](https://github.com/Ax1oMgg/svg2tgs/releases/latest).
Из исходников: `cargo run --release -p svg2tgs-cli -- …`.

```sh
svg2tgs папка_с_svg -o папка_для_tgs
```

| Параметр | Что делает |
|---|---|
| `--preset sticker\|emoji` | стикер или эмодзи |
| `-r` | вложенные папки |
| `--on-conflict` | что делать, если файл уже есть |
| `--padding`, `--bounds`, `--fit`, `--scale`, `--position` | размещение |
| `--flip-h`, `--flip-v`, `--rotate`, `--aspect`, `--crop` | трансформация |
| `--bg`, `--bg-radius`, `--bg-border`, `--no-bg-clip`, `--outline` | фон и обводка |
| `--grid 8x5\|auto` | нарезка на сетку эмодзи |
| `--options файл.json` | настройки из файла (экспорт из приложения) |
| `--optimize`, `--fps`, `--duration` | файл и анимация |
| `-j`, `--jobs` | число потоков |
| `--json` | отчёт для скриптов |

Подробно — `svg2tgs --help`.
