# Интерфейс в Figma

Файл: [svg2tgs — интерфейс](https://www.figma.com/design/7FVjokiyaMNITyb17Lkzlj)

После первого переноса источник правды по внешнему виду — этот макет
(см. CLAUDE.md). Код повторяет макет, а не наоборот.

## Как вносить правки

1. Правите макет в Figma: цвета и размеры — в переменных, вид элементов —
   в компонентах на странице «Компоненты», раскладку — на странице «Экраны».
2. Пишете Claude, что поменяли (можно ссылкой на фрейм или компонент).
3. Claude читает изменения и переносит их в код отдельным PR:
   переменные → `app/src/styles/tokens.css`, компоненты → `app/src/components`
   и `app/src/App.css`.
4. Если что-то поменялось в коде (новая кнопка, новое состояние), Claude
   добавляет это в макет.

Правила, чтобы перенос был точным:
- Цвет, отступ или радиус меняйте в переменной, а не «сырым» значением на слое.
  Новое значение — новая переменная.
- Новый элемент интерфейса — новый компонент (один компонент Figma —
  один компонент React или CSS-класс).
- Тексты интерфейса правятся в `app/src/i18n.ts`, в макете они для наглядности.

## Переменные

Имена совпадают с CSS-переменными: `группа/имя` в Figma → `--группа-имя`
в `tokens.css` (`color/accent` → `--color-accent`, `space/4` → `--space-4`).
У каждой переменной указан «Code syntax» для веба.

| Коллекция | Что внутри | CSS |
|---|---|---|
| Цвета | `color/*` светлой темы, `shadow/dialog-color` | `:root` |
| Цвета — тёмная тема | те же имена с тёмными значениями | `[data-theme="dark"]` и `prefers-color-scheme: dark` |
| Размеры | `space/*`, `radius/*`, `width/*`, `height/topbar`, `font-size/*`, `font-weight/*`, `font/family` | `:root` |

Стили текста `text/*` собраны из `font-size/*` и `font-weight/*`; стиль эффекта
`shadow/dialog` — это `--shadow-dialog`.

## Компоненты

| Figma | Код |
|---|---|
| TopBar, Logo | `components/TopBar.tsx` |
| EmptyState | `components/EmptyState.tsx` |
| FileList, FileRow, Status | `components/FileList.tsx` (`.file`, `StatusMark`) |
| Preview, Chat, Bubble, Note, SegButton, Segmented | `components/Preview.tsx` |
| SettingsPanel, Field, Slider | `components/SettingsPanel.tsx` |
| SaveDialog | `components/SaveDialog.tsx` |
| Toast, DropOverlay | `App.tsx` (`.toast`, `.drop-overlay`) |
| Button, Checkbox, Radio, Select, TextInput | CSS в `App.css` (`.btn*`, `.check`, `.radio`, `select`, `input`) |
| Фон/шахматка, Фон/чат | фоны `.file__thumb`, `.bg-checker`, `.chat--*` |
| Образец/* | тестовые картинки браузерного режима (`backend.ts`) |

## Отличия макета от приложения

- **Шрифт.** В приложении — системный (Segoe UI на Windows, SF Pro на macOS).
  В Figma его нет в облаке, поэтому в макете Inter: метрики близкие.
  Вес 650 в макете — Semi Bold (600).
- **Тёмная тема — отдельная коллекция.** На бесплатном тарифе Figma у коллекции
  только один режим, поэтому тёмные значения лежат в «Цвета — тёмная тема»
  с теми же именами. На платном тарифе их можно свести в одну коллекцию
  с режимами «Светлая» / «Тёмная».
- **Узоры** (шахматка, точки чата) — картинки-плитки: Figma не умеет
  заливать узором с переменными. Их цвета указаны в описании компонентов.
- Кнопка «×» в строке файла видна только при наведении, в макете на её месте
  пустой слот.
