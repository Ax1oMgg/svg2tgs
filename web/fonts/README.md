# Шрифт веб-версии

В браузере у программы нет доступа к шрифтам системы, поэтому текст из SVG
переводится в кривые единственным встроенным шрифтом — **Noto Sans Regular**
(лицензия SIL Open Font License 1.1, текст — `OFL.txt`; зарезервированных
имён у Noto нет, подмножество допустимо).

`NotoSans-Regular.ttf` — подмножество Noto Sans 2.015: латиница с расширениями,
греческий, кириллица, комбинируемые знаки, пунктуация, валюты, стрелки
(около 230 КБ, сжатым — около 115 КБ). Остальные письменности в веб-версии
не рисуются, ядро сообщает об этом замечанием `glyphs_missing`.

Как получить файл заново (нужны npm и Python):

```bash
npm pack @expo-google-fonts/noto-sans@0.4.2 && tar xzf expo-google-fonts-noto-sans-0.4.2.tgz
pip install fonttools==4.68.0
pyftsubset package/400Regular/NotoSans_400Regular.ttf \
  --unicodes="U+0020-007E,U+00A0-024F,U+0300-036F,U+0370-03FF,U+0400-052F,U+1E00-1EFF,U+2000-206F,U+20A0-20CF,U+2100-215F,U+2190-21FF,U+2212,U+2215,U+FB01-FB02" \
  --layout-features='*' --name-IDs='*' --name-languages='*' \
  --output-file=NotoSans-Regular.ttf
```
