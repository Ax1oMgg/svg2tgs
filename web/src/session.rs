//! Команды веб-версии: те же действия, что у команд Tauri в установленной
//! программе (`app/src-tauri/src/lib.rs`), и ответы того же вида.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use serde::{Deserialize, Serialize};
use svg2tgs_core::naming::{safe_file_name, tile_names};
use svg2tgs_core::{Grid, Options, TraceInfo, Warning};

/// Команда из фонового потока браузера.
#[derive(Deserialize, Debug)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Встроенный шрифт и версия ядра. Первая команда после загрузки.
    Init,
    /// Конвертировать SVG (данные команды) и запомнить под `id`. Без данных —
    /// уже прочитанный SVG этого файла с новыми настройками.
    Convert { id: String, options: Options },
    /// PNG превью исходника (resvg); `svg` — в координатах результата,
    /// `source` — без трансформации, для кадрирования.
    Preview {
        id: String,
        kind: PreviewKind,
        size: u32,
    },
    /// Готовые клетки как Lottie JSON — для превью через lottie-web.
    Lottie { id: String },
    /// Ширина и высота области просмотра SVG.
    SourceSize { id: String },
    /// Забыть файлы (убраны из списка).
    Forget { ids: Vec<String> },
    /// Настройки из текста (вставка или файл).
    ParseOptions { text: String },
    /// Настройки текстом.
    OptionsJson { options: Options },
    /// Имена файлов результата (без расширения) и подпапки — для записи
    /// в папку: `{"folder": …, "files": [{"id", "names"}]}`.
    Files {
        items: Vec<FileRequest>,
        #[serde(default)]
        folder: Option<String>,
    },
    /// Один готовый TGS: файл или клетка нарезки.
    Tgs { id: String, tile: usize },
    /// Все результаты одним архивом (для скачивания), по желанию — в папке.
    Zip {
        items: Vec<FileRequest>,
        folder: Option<String>,
    },
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PreviewKind {
    Svg,
    Source,
}

/// Что сохранить: файл списка и имя результата без расширения.
#[derive(Deserialize, Debug, Clone)]
pub struct FileRequest {
    pub id: String,
    pub name: String,
}

/// Ответ команды.
#[derive(Debug, PartialEq)]
pub enum Reply {
    Json(String),
    Bytes(Vec<u8>),
    Error(&'static str),
}

/// Один файл и результат его последней конвертации.
struct Item {
    /// Содержимое файла: SVG или картинка (PNG, JPEG, WebP).
    data: Vec<u8>,
    /// SVG для превью исходника: у картинки — она сама внутри SVG.
    svg: Vec<u8>,
    options: Options,
    tiles: Vec<Vec<u8>>,
    grid: Grid,
}

/// Итог конвертации — как `Outcome` в `app/src-tauri/src/lib.rs`.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    id: String,
    tgs_bytes: Option<usize>,
    grid: [u32; 2],
    aspect: f32,
    warnings: Vec<Warning>,
    error: Option<&'static str>,
    /// Что сделала трассировка (только у картинок).
    trace: Option<TraceInfo>,
}

impl Outcome {
    fn failed(id: &str, code: &'static str) -> Self {
        Self {
            id: id.to_string(),
            tgs_bytes: None,
            grid: [1, 1],
            aspect: 1.0,
            warnings: Vec::new(),
            error: Some(code),
            trace: None,
        }
    }
}

/// Подпапка экспорта без запрещённых символов; пустая — без подпапки.
fn safe_folder(folder: Option<&str>) -> Option<String> {
    match folder.map(str::trim) {
        Some(f) if !f.is_empty() => Some(safe_file_name(f)),
        _ => None,
    }
}

/// Самое большое превью (длинная сторона), как в установленной программе.
const MAX_PREVIEW: u32 = 2048;

/// Файлы, добавленные в веб-версии, и их результаты.
#[derive(Default)]
pub struct Session {
    items: HashMap<String, Item>,
}

fn json<T: Serialize>(v: &T) -> Reply {
    Reply::Json(serde_json::to_string(v).expect("ответ сериализуется"))
}

impl Session {
    pub fn run(&mut self, command: Command, data: &[u8]) -> Reply {
        match command {
            Command::Init => {
                svg2tgs_core::use_embedded_fonts(vec![crate::FONT.to_vec()]);
                json(&serde_json::json!({ "coreVersion": svg2tgs_core::VERSION }))
            }
            Command::Convert { id, options } => json(&self.convert(id, options, data)),
            Command::Preview { id, kind, size } => {
                Reply::Bytes(self.preview(&id, kind, size).unwrap_or_default())
            }
            Command::Lottie { id } => match self.lottie(&id) {
                Some(text) => Reply::Json(text),
                None => Reply::Json("null".into()),
            },
            Command::SourceSize { id } => {
                let size = self.items.get(&id).and_then(|item| {
                    let tree = svg2tgs_core::parse_svg(&item.svg).ok()?;
                    let [_, _, w, h] = svg2tgs_core::source_rect(&tree, &item.options);
                    Some([w, h])
                });
                json(&size)
            }
            Command::Forget { ids } => {
                for id in ids {
                    self.items.remove(&id);
                }
                Reply::Json("null".into())
            }
            Command::ParseOptions { text } => match Options::from_json(&text) {
                Ok(p) => json(&serde_json::json!({ "options": p.options, "known": p.known })),
                Err(e) => json(&e),
            },
            Command::OptionsJson { options } => json(&options.to_json()),
            Command::Files { items, folder } => {
                let files: Vec<_> = items
                    .iter()
                    .map(|r| serde_json::json!({ "id": r.id, "names": self.names(r) }))
                    .collect();
                json(
                    &serde_json::json!({ "folder": safe_folder(folder.as_deref()), "files": files }),
                )
            }
            Command::Tgs { id, tile } => {
                match self.items.get(&id).and_then(|i| i.tiles.get(tile)) {
                    Some(tgs) => Reply::Bytes(tgs.clone()),
                    None => Reply::Error("missing"),
                }
            }
            Command::Zip { items, folder } => Reply::Bytes(self.zip(&items, folder.as_deref())),
        }
    }

    fn convert(&mut self, id: String, options: Options, data: &[u8]) -> Outcome {
        let data = if data.is_empty() {
            match self.items.get(&id) {
                Some(prev) => prev.data.clone(),
                None => return Outcome::failed(&id, "read_failed"),
            }
        } else {
            data.to_vec()
        };
        let svg = svg2tgs_trace::source_svg(&data, &options)
            .map(|s| s.into_owned())
            .unwrap_or_default();
        match svg2tgs_trace::convert_any(&data, &options, options.preset.max_bytes()) {
            Ok((out, trace)) => {
                let mut warnings: Vec<Warning> = Vec::new();
                for w in out.tiles.iter().flat_map(|c| &c.warnings) {
                    if !warnings.contains(w) {
                        warnings.push(w.clone());
                    }
                }
                let outcome = Outcome {
                    id: id.clone(),
                    tgs_bytes: out.tiles.iter().map(|c| c.tgs_bytes()).max(),
                    grid: [out.grid.cols, out.grid.rows],
                    aspect: out.aspect,
                    warnings,
                    error: None,
                    trace,
                };
                let tiles = out.tiles.into_iter().map(|c| c.tgs).collect();
                self.items.insert(
                    id,
                    Item {
                        data,
                        svg,
                        options,
                        tiles,
                        grid: out.grid,
                    },
                );
                outcome
            }
            Err(e) => {
                let code = e.code();
                self.items.insert(
                    id.clone(),
                    Item {
                        data,
                        svg,
                        options,
                        tiles: Vec::new(),
                        grid: Grid { cols: 1, rows: 1 },
                    },
                );
                Outcome::failed(&id, code)
            }
        }
    }

    fn preview(&self, id: &str, kind: PreviewKind, size: u32) -> Option<Vec<u8>> {
        let item = self.items.get(id)?;
        let size = size.clamp(16, MAX_PREVIEW);
        let options = match kind {
            // Та же сетка, что у результата (и при подборе по пропорциям).
            PreviewKind::Svg => Options {
                grid: (item.grid.cols * item.grid.rows > 1).then_some(item.grid),
                grid_auto: false,
                ..item.options.clone()
            },
            // Без трансформации, но с той же рамкой рисунка.
            PreviewKind::Source => Options {
                bounds: item.options.bounds,
                crop: None,
                ..Options::default()
            },
        };
        svg2tgs_render::render_svg(&item.svg, size, &options)
            .ok()
            .map(|img| img.to_png())
    }

    /// `{"cols": …, "rows": …, "tiles": [<Lottie JSON>, …]}` — клетки по строкам.
    fn lottie(&self, id: &str) -> Option<String> {
        let item = self.items.get(id)?;
        if item.tiles.is_empty() {
            return None;
        }
        let mut tiles = Vec::with_capacity(item.tiles.len());
        for tgs in &item.tiles {
            let mut text = String::new();
            flate2::read::GzDecoder::new(tgs.as_slice())
                .read_to_string(&mut text)
                .ok()?;
            tiles.push(text);
        }
        Some(format!(
            r#"{{"cols":{},"rows":{},"tiles":[{}]}}"#,
            item.grid.cols,
            item.grid.rows,
            tiles.join(",")
        ))
    }

    /// Имена файлов результата: один или клетки нарезки.
    fn names(&self, r: &FileRequest) -> Vec<String> {
        let n = self.items.get(&r.id).map_or(1, |i| i.tiles.len().max(1));
        tile_names(&safe_file_name(&r.name), n)
    }

    /// Архив без сжатия (TGS уже сжат). Совпадающие имена получают номер,
    /// как в установленной программе: `имя_1`, `имя_2`…
    fn zip(&self, items: &[FileRequest], folder: Option<&str>) -> Vec<u8> {
        let prefix = safe_folder(folder).map_or(String::new(), |f| format!("{f}/"));
        let mut used = HashSet::new();
        let mut files = Vec::new();
        for r in items {
            let Some(item) = self.items.get(&r.id).filter(|i| !i.tiles.is_empty()) else {
                continue;
            };
            let n = item.tiles.len();
            let base = safe_file_name(&r.name);
            let free = |b: &str| tile_names(b, n).iter().all(|name| !used.contains(name));
            let base = if free(&base) {
                base
            } else {
                (1..)
                    .map(|k| format!("{base}_{k}"))
                    .find(|b| free(b))
                    .expect("свободное имя найдётся")
            };
            for (name, tgs) in tile_names(&base, n).into_iter().zip(&item.tiles) {
                files.push((format!("{prefix}{name}.tgs"), tgs.as_slice()));
                used.insert(name);
            }
        }
        crate::zip::store(&files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAR: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#f5b400" d="M12 2l3 7h7l-5.5 4.5L18.5 21 12 16.5 5.5 21l2-7.5L2 9h7z"/></svg>"##;
    const WIDE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 10"><rect width="30" height="10" fill="#3157d5"/></svg>"##;

    fn cmd(s: &str) -> Command {
        serde_json::from_str(s).unwrap()
    }

    fn json_of(r: Reply) -> serde_json::Value {
        match r {
            Reply::Json(s) => serde_json::from_str(&s).unwrap(),
            other => panic!("ожидался JSON, а пришло {other:?}"),
        }
    }

    fn convert(s: &mut Session, id: &str, svg: &[u8], options: &str) -> serde_json::Value {
        let c = cmd(&format!(
            r#"{{"cmd":"convert","id":"{id}","options":{options}}}"#
        ));
        json_of(s.run(c, svg))
    }

    #[test]
    fn pictures_are_traced_and_previewed() {
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../tests/fixtures/raster/pixel-art.png"
        ))
        .unwrap();
        let mut s = Session::default();
        json_of(s.run(Command::Init, &[]));
        let o = convert(&mut s, "p", &png, "{}");
        assert_eq!(o["error"], serde_json::Value::Null);
        assert_eq!(o["trace"]["preset"], "pixel");
        // Превью исходника — картинка, нарисованная resvg (PNG).
        match s.run(
            cmd(r#"{"cmd":"preview","id":"p","kind":"svg","size":64}"#),
            &[],
        ) {
            Reply::Bytes(b) => assert!(b.starts_with(b"\x89PNG")),
            other => panic!("{other:?}"),
        }
        // Размер исходника — размер картинки.
        let size = json_of(s.run(cmd(r#"{"cmd":"source_size","id":"p"}"#), &[]));
        assert_eq!(size, serde_json::json!([128.0, 128.0]));
        // Повтор с другими настройками — по запомненной картинке.
        let again = convert(&mut s, "p", &[], r#"{"trace":{"preset":"drawing"}}"#);
        assert_eq!(again["trace"]["preset"], "drawing");
    }

    #[test]
    fn convert_matches_desktop_outcome() {
        let mut s = Session::default();
        json_of(s.run(Command::Init, &[]));
        let o = convert(&mut s, "a", STAR, r#"{"preset":"sticker"}"#);
        assert_eq!(o["error"], serde_json::Value::Null);
        assert_eq!(o["grid"], serde_json::json!([1, 1]));
        let size = o["tgsBytes"].as_u64().unwrap() as usize;
        // Тот же файл, что даёт ядро напрямую.
        let direct = svg2tgs_core::convert_grid(STAR, &Options::default()).unwrap();
        assert_eq!(size, direct.tiles[0].tgs_bytes());
        // Повтор без данных — по прочитанному SVG.
        let again = convert(&mut s, "a", &[], r#"{"preset":"sticker","padding":0.1}"#);
        assert_eq!(again["error"], serde_json::Value::Null);
        // Незнакомый файл без данных — ошибка чтения.
        assert_eq!(convert(&mut s, "zzz", &[], "{}")["error"], "read_failed");
        assert_eq!(
            convert(&mut s, "b", b"not svg", "{}")["error"],
            "invalid_svg"
        );
    }

    #[test]
    fn previews_are_png_and_lottie_is_json() {
        let mut s = Session::default();
        convert(&mut s, "a", STAR, "{}");
        let Reply::Bytes(png) = s.run(
            cmd(r#"{"cmd":"preview","id":"a","kind":"svg","size":64}"#),
            &[],
        ) else {
            panic!()
        };
        assert_eq!(&png[1..4], b"PNG");
        let l = json_of(s.run(cmd(r#"{"cmd":"lottie","id":"a"}"#), &[]));
        assert_eq!(l["cols"], 1);
        assert_eq!(l["tiles"][0]["w"], 512);
        // Нечего показать — пустой ответ, а не ошибка.
        let Reply::Bytes(none) = s.run(
            cmd(r#"{"cmd":"preview","id":"x","kind":"source","size":64}"#),
            &[],
        ) else {
            panic!()
        };
        assert!(none.is_empty());
        assert_eq!(
            s.run(cmd(r#"{"cmd":"lottie","id":"x"}"#), &[]),
            Reply::Json("null".into())
        );
    }

    #[test]
    fn grid_names_and_zip() {
        let mut s = Session::default();
        let o = convert(
            &mut s,
            "w",
            WIDE,
            r#"{"preset":"emoji","grid":{"cols":3,"rows":1},"grid_auto":false}"#,
        );
        assert_eq!(o["grid"], serde_json::json!([3, 1]));
        convert(&mut s, "a", STAR, "{}");
        let reply = json_of(s.run(
            cmd(r#"{"cmd":"files","items":[{"id":"w","name":"wide"},{"id":"a","name":"a/b"}],"folder":" x:y "}"#),
            &[],
        ));
        assert_eq!(reply["folder"], "x_y");
        let files = &reply["files"];
        assert_eq!(
            files[0]["names"],
            serde_json::json!(["wide_01", "wide_02", "wide_03"])
        );
        assert_eq!(files[1]["names"], serde_json::json!(["a_b"]));
        // Одинаковые имена в архиве получают номер.
        let Reply::Bytes(zip) = s.run(
            cmd(r#"{"cmd":"zip","items":[{"id":"a","name":"x"},{"id":"a","name":"x"},{"id":"w","name":"wide"}],"folder":"tgs"}"#),
            &[],
        ) else {
            panic!()
        };
        let names = crate::zip::names(&zip);
        assert_eq!(
            names,
            [
                "tgs/x.tgs",
                "tgs/x_1.tgs",
                "tgs/wide_01.tgs",
                "tgs/wide_02.tgs",
                "tgs/wide_03.tgs"
            ]
        );
        let Reply::Bytes(tgs) = s.run(cmd(r#"{"cmd":"tgs","id":"w","tile":2}"#), &[]) else {
            panic!()
        };
        assert_eq!(&tgs[..2], &[0x1f, 0x8b], "TGS — это gzip");
        assert_eq!(
            s.run(cmd(r#"{"cmd":"tgs","id":"w","tile":3}"#), &[]),
            Reply::Error("missing")
        );
    }

    #[test]
    fn options_text_roundtrip() {
        let mut s = Session::default();
        let text = json_of(s.run(
            cmd(r#"{"cmd":"options_json","options":{"padding":0.1}}"#),
            &[],
        ));
        let text = text.as_str().unwrap().to_string();
        let parsed = json_of(s.run(Command::ParseOptions { text }, &[]));
        assert!((parsed["options"]["padding"].as_f64().unwrap() - 0.1).abs() < 1e-6);
        let err = json_of(s.run(
            Command::ParseOptions {
                text: "{oops".into(),
            },
            &[],
        ));
        assert_eq!(err["code"], "syntax");
    }

    #[test]
    fn forget_drops_results() {
        let mut s = Session::default();
        convert(&mut s, "a", STAR, "{}");
        s.run(cmd(r#"{"cmd":"forget","ids":["a"]}"#), &[]);
        assert_eq!(
            json_of(s.run(cmd(r#"{"cmd":"source_size","id":"a"}"#), &[])),
            serde_json::Value::Null
        );
    }
}
