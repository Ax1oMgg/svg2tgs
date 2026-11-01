//! Rust-часть приложения: команды, которые вызывает интерфейс.
//!
//! Конвертированные файлы хранятся здесь (`AppState`), в интерфейс уходят
//! только итоги (размер, замечания) и картинки превью. Тяжёлая работа
//! выполняется в фоновых потоках, окно не подвисает.

mod files;
mod telegram;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::{Deserialize, Serialize};
use svg2tgs_core::{Grid, Options, Warning};
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

/// Один добавленный файл и результат его последней конвертации.
struct Item {
    svg: Vec<u8>,
    options: Options,
    /// Готовые TGS: один файл или клетки нарезки по строкам.
    tiles: Vec<Vec<u8>>,
    /// Столбцы и строки нарезки (1 × 1 — без нарезки).
    grid: Grid,
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
    /// Размер файла; при нарезке — самой большой клетки.
    tgs_bytes: Option<usize>,
    /// Сетка нарезки `[столбцы, строки]`; `[1, 1]` — один файл.
    grid: [u32; 2],
    /// Соотношение сторон рисунка после трансформаций (ширина / высота).
    aspect: f32,
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
        match svg2tgs_core::convert_grid(&svg, &options) {
            Ok(out) => {
                // Замечания клеток — без повторов.
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
                };
                let item = Item {
                    svg,
                    options,
                    tiles: out.tiles.into_iter().map(|c| c.tgs).collect(),
                    grid: out.grid,
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
                    tiles: Vec::new(),
                    grid: Grid { cols: 1, rows: 1 },
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
            grid: [1, 1],
            aspect: 1.0,
            warnings: Vec::new(),
            error: Some(code.to_string()),
        }
    }
}

/// Что показать в превью.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
enum PreviewKind {
    /// Исходный SVG (resvg) — в тех же координатах, что и результат
    /// (при нарезке — весь холст).
    Svg,
    /// Готовый TGS глазами Telegram (rlottie); при нарезке — клетки,
    /// сложенные вплотную.
    Tgs,
    /// Исходный SVG без трансформации и размещения — для кадрирования.
    Source,
}

/// Картинка превью в виде data-URL PNG; `None`, если рисовать нечего.
/// Длинная сторона картинки — `size`.
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
    // До 2048 — для увеличения в «Крупно».
    let size = size.clamp(16, 2048);
    tauri::async_runtime::spawn_blocking(move || {
        let image = match kind {
            PreviewKind::Svg => {
                // Та же сетка, что у результата (и при подборе по пропорциям).
                let options = Options {
                    grid: (item.grid.cols * item.grid.rows > 1).then_some(item.grid),
                    grid_auto: false,
                    ..item.options.clone()
                };
                svg2tgs_render::render_svg(&item.svg, size, &options).ok()
            }
            PreviewKind::Source => {
                // Без трансформации, но с той же рамкой рисунка.
                let plain = Options {
                    bounds: item.options.bounds,
                    crop: None,
                    ..Options::default()
                };
                svg2tgs_render::render_svg(&item.svg, size, &plain).ok()
            }
            PreviewKind::Tgs => match item.tiles.len() {
                0 => None,
                1 => svg2tgs_render::render_tgs(&item.tiles[0], size, 0).ok(),
                _ => {
                    let side = item.grid.cols.max(item.grid.rows);
                    let cell = (size / side).max(16);
                    svg2tgs_render::render_tgs_grid(&item.tiles, item.grid.cols, cell).ok()
                }
            },
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
    let [_, _, w, h] = svg2tgs_core::source_rect(&tree, &item.options);
    Some([w, h])
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
fn find_existing(
    requests: Vec<files::SaveRequest>,
    state: State<'_, AppState>,
) -> Vec<files::Existing> {
    let counts: Vec<usize> = {
        let map = state.items.lock().unwrap();
        requests
            .iter()
            .map(|r| map.get(&r.id).map_or(1, |i| i.tiles.len().max(1)))
            .collect()
    };
    files::find_existing(&requests, &counts)
}

/// Сохраняет готовые TGS: каждый — в свою папку (см. `files::SaveRequest`).
#[tauri::command]
async fn save(
    requests: Vec<files::SaveRequest>,
    conflict: files::Conflict,
    state: State<'_, AppState>,
) -> Result<Vec<files::Saved>, String> {
    let items: Vec<(files::SaveRequest, Vec<Vec<u8>>)> = {
        let map = state.items.lock().unwrap();
        requests
            .into_iter()
            .map(|r| {
                let tiles = map.get(&r.id).map(|i| i.tiles.clone()).unwrap_or_default();
                (r, tiles)
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

/// Прочитанные из текста настройки.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ParsedOptions {
    options: Options,
    /// Сколько полей текста узнано.
    known: usize,
}

/// Настройки из текста (вставка или файл) — разбирает ядро.
#[tauri::command]
fn parse_options(text: String) -> Result<ParsedOptions, svg2tgs_core::OptionsError> {
    svg2tgs_core::Options::from_json(&text).map(|p| ParsedOptions {
        options: p.options,
        known: p.known,
    })
}

/// Настройки текстом — для копирования и файла.
#[tauri::command]
fn options_json(options: Options) -> String {
    options.to_json()
}

/// Самый большой файл настроек, который стоит читать.
const MAX_OPTIONS_FILE: u64 = 1024 * 1024;

/// Выбрать файл настроек и прочитать его текст. Диалог открывается здесь,
/// в Rust: окно не передаёт пути к файлам и не может прочитать что угодно.
#[tauri::command]
async fn import_options_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = picked.into_path().map_err(|e| e.to_string())?;
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if size > MAX_OPTIONS_FILE {
            return Err("too_large".into());
        }
        std::fs::read_to_string(&path)
            .map(Some)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Сохранить настройки в файл, выбранный в диалоге (тоже здесь, в Rust).
/// `false` — пользователь отменил.
#[tauri::command]
async fn export_options_file(
    options: Options,
    name: String,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .set_file_name(&name)
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = picked.into_path().map_err(|e| e.to_string())?;
        std::fs::write(path, options.to_json()).map_err(|e| e.to_string())?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Ссылки из «О программе».
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
enum Link {
    Project,
    Issues,
    Telegram,
    Email,
    /// @BotFather — подсказка, где создать бота для выгрузки.
    Botfather,
}

/// Открывает ссылку в браузере (почту — в почтовой программе). Адреса
/// зашиты: окно не может попросить открыть произвольную ссылку.
#[tauri::command]
fn open_link(link: Link, app: tauri::AppHandle) -> Result<(), String> {
    let url = match link {
        Link::Project => "https://github.com/Ax1oMgg/svg2tgs",
        Link::Issues => "https://github.com/Ax1oMgg/svg2tgs/issues",
        Link::Telegram => "https://t.me/chajnaya_Axioma",
        Link::Email => "mailto:chajnayaaxioma@gmail.com",
        Link::Botfather => "https://t.me/BotFather",
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/* ------------------------------ Выгрузка в Telegram ----------------------------- */

use telegram::{Access, KnownSet, PublishRequest, SetView, Status, Telegram, TgError};

/// Ошибка фонового потока — как «прочий отказ».
fn join_error(e: impl ToString) -> TgError {
    TgError::Publish(svg2tgs_publish::PublishError::Telegram {
        status: 0,
        detail: e.to_string(),
    })
}

/// Состояние выгрузки — без сети.
#[tauri::command]
fn tg_status(tg: State<'_, Telegram>) -> Status {
    tg.status()
}

/// Переключатель в «Настройках».
#[tauri::command]
fn tg_set_enabled(enabled: bool, tg: State<'_, Telegram>) -> Status {
    tg.set_enabled(enabled)
}

/// Откуда пришёл запрос на подключение: `settings` — из открытых «Настроек»
/// с включённым (ещё не сохранённым) переключателем.
fn access(settings: bool) -> Access {
    if settings {
        Access::Settings
    } else {
        Access::Saved
    }
}

/// Проверить токен бота и запомнить его в системном хранилище.
#[tauri::command]
async fn tg_check_token(
    token: String,
    settings: bool,
    app: tauri::AppHandle,
) -> Result<Status, TgError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Telegram>()
            .check_token(access(settings), &token, telegram::http)
    })
    .await
    .map_err(join_error)?
}

/// Найти владельца по /start.
#[tauri::command]
async fn tg_find_owner(settings: bool, app: tauri::AppHandle) -> Result<Status, TgError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Telegram>()
            .find_owner(access(settings), telegram::http)
    })
    .await
    .map_err(join_error)?
}

/// Забыть бота и токен.
#[tauri::command]
fn tg_disconnect(tg: State<'_, Telegram>) -> Status {
    tg.disconnect()
}

/// Начало адреса набора из названия.
#[tauri::command]
fn tg_stem(title: String, tg: State<'_, Telegram>) -> String {
    tg.stem(&title)
}

/// Набор этого бота по началу адреса — для «Дописать».
#[tauri::command]
async fn tg_set_info(stem: String, app: tauri::AppHandle) -> Result<SetView, TgError> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Telegram>().set_info(&stem, telegram::http)
    })
    .await
    .map_err(join_error)?
}

/// Выгрузить готовые TGS; ход — через `on_progress`.
#[tauri::command]
async fn tg_publish(
    request: PublishRequest,
    on_progress: tauri::ipc::Channel<svg2tgs_publish::Progress>,
    app: tauri::AppHandle,
) -> Result<svg2tgs_publish::Done, TgError> {
    tauri::async_runtime::spawn_blocking(move || {
        let tg = app.state::<Telegram>();
        let config = tg.config();
        let bot_name = config.bot.clone().ok_or(TgError::NoToken)?;
        let owner = config
            .owner
            .clone()
            .ok_or(TgError::Publish(svg2tgs_publish::PublishError::NoOwner))?;
        let items = app.state::<AppState>().items.lock().unwrap().clone();
        let job = telegram::build_job(&request, &bot_name, |id, tile| {
            items.get(id).and_then(|i| i.tiles.get(tile).cloned())
        })?;
        let bot = tg.bot_with(Access::Saved, None, telegram::http)?;
        telegram::reset_stop(&tg.stop);
        let done = svg2tgs_publish::run(
            &bot,
            &bot_name,
            owner.id,
            &job,
            |p| {
                let _ = on_progress.send(p);
            },
            &tg.stop,
        )?;
        tg.remember(KnownSet {
            name: done.name.clone(),
            title: job.title.clone(),
            kind: job.kind,
        });
        Ok(done)
    })
    .await
    .map_err(join_error)?
}

/// «Остановить»: выгрузка закончится после текущего файла.
#[tauri::command]
fn tg_stop(tg: State<'_, Telegram>) {
    tg.stop.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Открыть в браузере набор или бота. Адрес собирается здесь из проверенного
/// имени: окно не может попросить открыть произвольную ссылку.
#[tauri::command]
fn tg_open(
    kind: Option<svg2tgs_publish::SetKind>,
    name: Option<String>,
    tg: State<'_, Telegram>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let bot = tg.config().bot.ok_or("no_bot")?;
    let url = match (kind, name) {
        (Some(kind), Some(name)) => telegram::set_link(kind, &name, &bot).ok_or("bad_name")?,
        _ if bot.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => {
            format!("https://t.me/{bot}")
        }
        _ => return Err("bad_name".into()),
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let path = app
                .path()
                .app_config_dir()
                .ok()
                .map(|d| d.join("telegram.json"));
            app.manage(Telegram::load(path, Box::new(telegram::Keyring)));
            Ok(())
        })
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
            open_link,
            parse_options,
            options_json,
            import_options_file,
            export_options_file,
            tg_status,
            tg_set_enabled,
            tg_check_token,
            tg_find_owner,
            tg_disconnect,
            tg_stem,
            tg_set_info,
            tg_publish,
            tg_stop,
            tg_open,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить приложение");
}
