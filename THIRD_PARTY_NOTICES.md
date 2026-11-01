# Сторонние компоненты

svg2tgs распространяется под лицензией MIT (`LICENSE`). В программу входят
компоненты других авторов под их лицензиями.

## rlottie (Samsung)

Рендер TGS для превью — `render/vendor/rlottie-samsung`.
Copyright (c) Samsung Electronics Co., Ltd. и авторы, MIT.
Тексты лицензий: `render/vendor/rlottie-samsung/licenses/`.

Внутри rlottie:
- растеризатор FreeType — The FreeType Project License (FTL).
  Portions of this software are copyright © The FreeType Project
  (www.freetype.org). All rights reserved.
- фрагменты Skia — BSD-3-Clause, Copyright (c) 2011 Google Inc.;
- `vinterpolator.cpp` — Mozilla Public License 2.0;
- pixman — MIT;
- RapidJSON — MIT, Copyright (c) 2015 THL A29 Limited, a Tencent company,
  and Milo Yip;
- stb_image — public domain / MIT.

## rlottie (форк Telegram Desktop) — только при сборке с `telegram-rlottie`

`render/vendor/rlottie-telegram`, GNU LGPL-2.1
(`render/vendor/rlottie-telegram/licenses/COPYING.LGPL`). В сборки
по умолчанию не входит. Подробности — `render/vendor/README.md`.

## Значки Lucide

Значки «шестерёнка» (`settings`) и другие контурные значки интерфейса —
из набора Lucide (https://lucide.dev), лицензия ISC.
Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as part
of Feather (MIT). All other copyright (c) for Lucide are held by Lucide
Contributors 2022.

## Шрифт Noto Sans — веб-версия

`web/fonts/NotoSans-Regular.ttf` — подмножество Noto Sans (латиница,
греческий, кириллица). Copyright 2022 The Noto Project Authors
(https://github.com/notofonts/latin-greek-cyrillic), SIL Open Font
License 1.1 — `web/fonts/OFL.txt`. Входит только в веб-версию.

## lottie-web — веб-версия

Превью результата в веб-версии — lottie-web (Airbnb), MIT,
https://github.com/airbnb/lottie-web. Входит только в веб-версию.

## VTracer и visioncortex — трассировка картинок

Крейт `trace` (`trace/src/vectorize.rs`) содержит код, перенесённый
из VTracer (https://github.com/visioncortex/vtracer, `converter.rs`
и `svg.rs`, версия 0.6.4), лицензия MIT (VTracer — MIT OR Apache-2.0):

> The MIT License (MIT)
>
> Copyright (c) 2023 Tsang Hao Fung
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
> THE SOFTWARE.

Сама кластеризация и подгонка кривых — библиотека visioncortex
(https://github.com/visioncortex/visioncortex), MIT OR Apache-2.0,
Copyright (c) Chris Tsang и авторы visioncortex. Чтение картинок — png
(MIT / Apache-2.0), zune-jpeg (MIT / Apache-2.0 / Zlib), image-webp
(MIT / Apache-2.0).

## Rust-библиотеки

resvg, usvg, tiny-skia (MIT / Apache-2.0), Tauri (MIT / Apache-2.0),
serde, flate2, clap и другие — лицензии MIT и/или Apache-2.0.
Для выгрузки в Telegram: ureq, rustls (MIT / Apache-2.0 / ISC),
ring (Apache-2.0 AND ISC), webpki-roots (CDLA-Permissive-2.0 —
корневые сертификаты Mozilla); хранение токена — keyring
(MIT / Apache-2.0).
Полный список с текстами лицензий будет собираться автоматически
при подготовке установщиков (этап 7).
