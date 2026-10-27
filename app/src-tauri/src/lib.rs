//! Rust-часть приложения: команды, которые вызывает интерфейс.
//!
//! Конвертированные файлы хранятся здесь (`AppState`), в интерфейс уходят
//! только итоги (размер, замечания) и картинки превью. Тяжёлая работа
//! выполняется в фоновых потоках, окно не подвисает.

mod files;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::{Deserialize, Serialize};
use svg2tgs_core::{Options, Warning};
use tauri::State;
use tauri_plugin_opener::OpenerExt;

/// Один добавленный файл и результат его последней конвертации.
struct Item {
    svg: Vec<u8>,
    options: Options,
    tgs: Option<Vec<u8>>,
}

#[derive(Default)]
struct AppState {
    items: Mutex<HashMap<String, Arc<Item>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    core_version: &'static str,
}

/// Версии приложения и ядра.
#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        core_version: svg2tgs_core::VERSION,
    }
}

/// Файлы, переданные при запуске (`svg2tgs файл.svg папка` или «Открыть с помощью»).
#[tauri::command]
fn startup_paths() -> Vec<String> {
    std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .filter(|a| std::path::Path::new(a).exists())
        .collect()
}

/// Разворачивает перетащенные пути: папки — в SVG-файлы внутри.
#[tauri::command]
async fn expand_paths(paths: Vec<String>, recursive: bool) -> Vec<files::Entry> {
    tauri::async_runtime::spawn_blocking(move || files::expand(&paths, recursive))
        .await
        .unwrap_or_default()
}

/// Откуда взять SVG.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Source {
    /// Файл на диске.
    Path { path: String },
    /// SVG-код (вставка из буфера).
    Text { text: String },
}

/// Итог конвертации для интерфейса.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    id: String,
    tgs_bytes: Option<usize>,
    warnings: Vec<Warning>,
    /// Код ошибки: `read_failed`, `invalid_svg`, `empty`, `invalid_option`.
    error: Option<String>,
}

/// Конвертирует SVG и запоминает результат под `id`.
#[tauri::command]
async fn convert(
    id: String,
    source: Source,
    options: Options,
    state: State<'_, AppState>,
) -> Result<Outcome, String> {
    let previous = state.items.lock().unwrap().get(&id).cloned();
    let work = move || {
        let svg = match (source, previous) {
            // Файл с диска читается при первом добавлении; при смене
            // настроек используется уже прочитанное содержимое.
            (_, Some(prev)) => Ok(prev.svg.clone()),
            (Source::Path { path }, None) => std::fs::read(&path).map_err(|_| ()),
            (Source::Text { text }, None) => Ok(text.into_bytes()),
        };
        let Ok(svg) = svg else {
            return (None, Outcome::failed(&id, "read_failed"));
        };
        match svg2tgs_core::convert(&svg, &options) {
            Ok(conv) => {
                let outcome = Outcome {
                    id: id.clone(),
                    tgs_bytes: Some(conv.tgs_bytes()),
                    warnings: conv.warnings,
                    error: None,
                };
                let item = Item {
                    svg,
                    options,
                    tgs: Some(conv.tgs),
                };
                (Some(item), outcome)
            }
            Err(e) => {
                let code = match e {
                    svg2tgs_core::Error::InvalidSvg(_) => "invalid_svg",
                    svg2tgs_core::Error::Empty => "empty",
                    svg2tgs_core::Error::InvalidOption(_) => "invalid_option",
                };
                let item = Item {
                    svg,
                    options,
                    tgs: None,
                };
                (Some(item), Outcome::failed(&id, code))
            }
        }
    };
    let (item, outcome) = tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(item) = item {
        state
            .items
            .lock()
            .unwrap()
            .insert(outcome.id.clone(), Arc::new(item));
    }
    Ok(outcome)
}

impl Outcome {
    fn failed(id: &str, code: &str) -> Self {
        Self {
            id: id.to_string(),
            tgs_bytes: None,
            warnings: Vec::new(),
            error: Some(code.to_string()),
        }
    }
}

/// Что показать в превью.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
enum PreviewKind {
    /// Исходный SVG (resvg) — в тех же координатах, что и результат.
    Svg,
    /// Готовый TGS глазами Telegram (rlottie).
    Tgs,
    /// Исходный SVG без трансформации и размещения — для кадрирования.
    Source,
}

/// Картинка превью в виде data-URL PNG; `None`, если рисовать нечего.
#[tauri::command]
async fn preview(
    id: String,
    kind: PreviewKind,
    size: u32,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let Some(item) = state.items.lock().unwrap().get(&id).cloned() else {
        return Ok(None);
    };
    let size = size.clamp(16, 1024);
    tauri::async_runtime::spawn_blocking(move || {
        let image = match kind {
            PreviewKind::Svg => svg2tgs_render::render_svg(&item.svg, size, &item.options).ok(),
            PreviewKind::Source => {
                svg2tgs_render::render_svg(&item.svg, size, &Options::default()).ok()
            }
            PreviewKind::Tgs => item
                .tgs
                .as_ref()
                .and_then(|t| svg2tgs_render::render_tgs(t, size, 0).ok()),
        };
        image.map(|img| {
            let png = base64::engine::general_purpose::STANDARD.encode(img.to_png());
            format!("data:image/png;base64,{png}")
        })
    })
    .await
    .map_err(|e| e.to_string())
}

/// Размер рисунка (ширина и высота области просмотра SVG).
#[tauri::command]
fn source_size(id: String, state: State<'_, AppState>) -> Option<[f32; 2]> {
    let item = state.items.lock().unwrap().get(&id).cloned()?;
    let tree = svg2tgs_core::parse_svg(&item.svg).ok()?;
    Some([tree.size().width(), tree.size().height()])
}

/// Убирает файлы из памяти (удалены из списка).
#[tauri::command]
fn forget(ids: Vec<String>, state: State<'_, AppState>) {
    let mut items = state.items.lock().unwrap();
    for id in ids {
        items.remove(&id);
    }
}

/// Какие результаты лягут на уже существующие файлы.
#[tauri::command]
fn find_existing(requests: Vec<files::SaveRequest>) -> Vec<files::Existing> {
    files::find_existing(&requests)
}

/// Сохраняет готовые TGS: каждый — в свою папку (см. `files::SaveRequest`).
#[tauri::command]
async fn save(
    requests: Vec<files::SaveRequest>,
    conflict: files::Conflict,
    state: State<'_, AppState>,
) -> Result<Vec<files::Saved>, String> {
    let items: Vec<(files::SaveRequest, Option<Vec<u8>>)> = {
        let map = state.items.lock().unwrap();
        requests
            .into_iter()
            .map(|r| {
                let tgs = map.get(&r.id).and_then(|i| i.tgs.clone());
                (r, tgs)
            })
            .collect()
    };
    tauri::async_runtime::spawn_blocking(move || files::save_all(items, conflict))
        .await
        .map_err(|e| e.to_string())
}

/// Открывает папку в проводнике / Finder.
#[tauri::command]
fn open_folder(path: String, app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Открывает страницу проекта в браузере. Адрес зашит: окно не может
/// попросить открыть произвольную ссылку.
#[tauri::command]
fn open_project_page(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url("https://github.com/Ax1oMgg/svg2tgs", None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_info,
            startup_paths,
            expand_paths,
            convert,
            preview,
            forget,
            save,
            find_existing,
            source_size,
            open_folder,
            open_project_page,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить приложение");
}
