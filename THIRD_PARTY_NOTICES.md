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

## Rust-библиотеки

resvg, usvg, tiny-skia (MIT / Apache-2.0), Tauri (MIT / Apache-2.0),
serde, flate2, clap и другие — лицензии MIT и/или Apache-2.0.
Для выгрузки в Telegram: ureq, rustls (MIT / Apache-2.0 / ISC),
ring (Apache-2.0 AND ISC), webpki-roots (CDLA-Permissive-2.0 —
корневые сертификаты Mozilla); хранение токена — keyring
(MIT / Apache-2.0).
Полный список с текстами лицензий будет собираться автоматически
при подготовке установщиков (этап 7).
