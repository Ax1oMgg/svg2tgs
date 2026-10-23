# Тестовые SVG

Каждый файл проверяет конкретный случай, на котором легко ошибиться.
Тесты ядра (этап 1) прогоняют все файлы, а сравнение рендеров (этап 2)
проверяет, что TGS выглядит как исходный SVG.

| Файл | Что проверяет | Ожидание |
|---|---|---|
| `stroke-icon.svg` | Контурная иконка 24×24 с `stroke-width="2"`, `currentColor` | Толщина линии масштабируется вместе с рисунком (~43 px на канвасе 512) |
| `evenodd-ring.svg` | `fill-rule="evenodd"` | Внутренний круг остаётся дыркой |
| `svgo-minified-arcs.svg` | Дуги со слитными флагами (`a10 10 0 100 20`) | Path разбирается без сдвига |
| `nested-opacity.svg` | Прозрачность группы и элемента | Множители перемножаются; перекрытие внутри полупрозрачной группы не темнеет |
| `use-symbol.svg` | `<use>`, `<symbol>`, `href` и `xlink:href` | Все три фигуры на месте |
| `css-classes.svg` | Стили в `<style>`, классы, `#id`, `style=""` | Цвета как в браузере |
| `units-px.svg` | Нет `viewBox`, размеры в `px` | Единицы понимаются, рисунок вписан |
| `linear-gradient.svg` | Линейный градиент, `stop-opacity` | Переносится как градиент Lottie |
| `radial-gradient.svg` | Радиальный градиент, `userSpaceOnUse` | Переносится как градиент Lottie |
| `text.svg` | `<text>` | Текст переведён в кривые, замечание `text_converted_to_paths` |
| `transforms.svg` | Вложенные `translate/rotate/scale/skewX/matrix` | Положение как в браузере |
| `clip-path.svg` | `clipPath` | Обрезка по кругу |
| `filter-blur.svg` | `feGaussianBlur` | Размытие убрано, замечание `filter_removed` для `shadow-shape` |
| `shapes-basic.svg` | `rect rx`, `circle`, `ellipse`, `polygon`, `polyline`, `line` | Все фигуры на месте |
| `dots.svg` | Отрезки нулевой длины с `round`/`square`/`butt` | Точки — залитые круг и квадрат; `butt` ничего не рисует |
| `shared-start.svg` | Подпуть начинается в конце предыдущего (`…Z m0 0 …`) | Подпути не склеиваются в rlottie |
| `pill.svg` | `rect` с `rx` = половине высоты | Ровные полукруги без искажений в rlottie |
| `invalid.svg` | Не SVG | Ошибка `invalid_svg` |
| `empty.svg` | Ничего видимого | Ошибка `empty` |
