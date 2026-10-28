//! svg2tgs — командная строка.
//!
//! Конвертирует SVG-файлы и папки в .tgs и печатает замечания по-русски
//! или, с `--json`, отчёт для скриптов. Файлы обрабатываются параллельно,
//! вывод — в порядке входных файлов.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};

mod messages;

use clap::{Parser, ValueEnum};
use serde::Serialize;
use svg2tgs_core::{
    Background, Bounds, Crop, Fit, Optimization, Options, Outline, Preset, Severity, Warning,
    MAX_OUTLINE,
};

#[derive(Parser, Debug)]
#[command(
    name = "svg2tgs",
    version,
    about = "Конвертация SVG в стикеры и эмодзи Telegram (TGS)",
    after_help = "Примеры:\n  \
        svg2tgs icon.svg\n  \
        svg2tgs ./svg -r -o ./tgs --preset emoji\n  \
        svg2tgs ./svg --json > report.json\n\n\
        Коды выхода: 0 — всё сконвертировано, 1 — нечего обрабатывать, 2 — часть файлов с ошибкой."
)]
struct Cli {
    /// SVG-файлы или папки с ними.
    #[arg(required = true)]
    inputs: Vec<PathBuf>,

    /// Папка для результатов.
    #[arg(short, long, default_value = "tgs")]
    output: PathBuf,

    /// Для чего готовим файл.
    #[arg(long, value_enum, default_value_t = PresetArg::Sticker)]
    preset: PresetArg,

    /// Искать SVG во вложенных папках.
    #[arg(short, long)]
    recursive: bool,

    /// Если файл с таким именем уже есть (или два SVG дают одно имя):
    /// добавить номер, заменить или пропустить.
    #[arg(long, value_enum, default_value_t = Conflict::Number)]
    on_conflict: Conflict,

    /// Отступ от края канваса, % стороны (0–20).
    #[arg(long, default_value_t = 0.0, value_parser = parse_padding)]
    padding: f32,

    /// Как вписать рисунок: целиком, заполнить с обрезкой, по ширине,
    /// по высоте или с масштабом `--scale`.
    #[arg(long, value_enum, default_value_t = FitArg::Contain)]
    fit: FitArg,

    /// Рамка рисунка: container — вся область SVG, content — по краю рисунка.
    #[arg(long, value_enum, default_value_t = BoundsArg::Container)]
    bounds: BoundsArg,

    /// Масштаб для `--fit free`, % (100 — целиком).
    #[arg(long, default_value_t = 100.0)]
    scale: f32,

    /// Положение в рабочей области X,Y в % (50,50 — по центру).
    #[arg(long, value_parser = parse_pair, default_value = "50,50")]
    position: (f32, f32),

    /// Отразить по горизонтали.
    #[arg(long)]
    flip_h: bool,

    /// Отразить по вертикали.
    #[arg(long)]
    flip_v: bool,

    /// Поворот по часовой стрелке, градусы.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    rotate: f32,

    /// Пропорции: ширина относительно высоты, % (100 — без искажения).
    #[arg(long, default_value_t = 100.0)]
    aspect: f32,

    /// Обрезать края, % стороны: сверху,справа,снизу,слева (например 0,10,0,10).
    #[arg(long, value_parser = parse_crop)]
    crop: Option<Crop>,

    /// Цвет фона, например #ffffff. По умолчанию прозрачный.
    #[arg(long)]
    bg: Option<String>,

    /// Скругление углов фона, пиксели канваса 512.
    #[arg(long, default_value_t = 0.0)]
    bg_radius: f32,

    /// Рамка фона внутри канваса: толщина и цвет, например `12:#000000`.
    #[arg(long, value_parser = parse_outline)]
    bg_border: Option<Outline>,

    /// Обводка вокруг рисунка: толщина в пикселях канваса и цвет,
    /// например `12` (белая) или `12:#000000`.
    #[arg(long, value_parser = parse_outline)]
    outline: Option<Outline>,

    /// Оптимизация размера: auto — уложиться в лимит Telegram,
    /// lossless — без упрощения, число 0–6 — знаков после запятой.
    #[arg(long, default_value = "auto", value_parser = parse_optimize)]
    optimize: Optimization,

    /// Кадров в секунду (Telegram требует 60).
    #[arg(long)]
    fps: Option<u32>,

    /// Длительность, секунды (Telegram разрешает до 3).
    #[arg(long)]
    duration: Option<f32>,

    /// Вывести отчёт в JSON вместо текста (для скриптов).
    #[arg(long)]
    json: bool,

    /// Сколько файлов обрабатывать одновременно (по умолчанию — по числу ядер).
    #[arg(short = 'j', long)]
    jobs: Option<usize>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum FitArg {
    Contain,
    Cover,
    Width,
    Height,
    Free,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum BoundsArg {
    Container,
    Content,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum PresetArg {
    Sticker,
    Emoji,
}

#[derive(Copy, Clone, Debug, PartialEq, ValueEnum)]
enum Conflict {
    Number,
    Overwrite,
    Skip,
}

fn parse_padding(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .trim_end_matches('%')
        .parse()
        .map_err(|_| "ожидается число".to_string())?;
    if !(0.0..=20.0).contains(&v) {
        return Err("отступ должен быть от 0 до 20 %".into());
    }
    Ok(v / 100.0)
}

fn parse_numbers(s: &str, n: usize) -> Result<Vec<f32>, String> {
    let v: Result<Vec<f32>, _> = s
        .split(',')
        .map(|p| p.trim().trim_end_matches('%').parse::<f32>())
        .collect();
    match v {
        Ok(v) if v.len() == n => Ok(v),
        _ => Err(format!("ожидается {n} числа через запятую")),
    }
}

fn parse_pair(s: &str) -> Result<(f32, f32), String> {
    let v = parse_numbers(s, 2)?;
    Ok((v[0], v[1]))
}

fn parse_crop(s: &str) -> Result<Crop, String> {
    let v = parse_numbers(s, 4)?;
    if v.iter().any(|x| !(0.0..=95.0).contains(x)) {
        return Err("обрезка — от 0 до 95 % с каждой стороны".into());
    }
    Ok(Crop {
        top: v[0] / 100.0,
        right: v[1] / 100.0,
        bottom: v[2] / 100.0,
        left: v[3] / 100.0,
    })
}

fn parse_outline(s: &str) -> Result<Outline, String> {
    let (width, color) = match s.split_once(':') {
        Some((w, c)) => (w, c.to_string()),
        None => (s, Outline::default().color),
    };
    let width: f32 = width
        .trim_end_matches("px")
        .parse()
        .map_err(|_| "ожидается толщина, например 12 или 12:#ffffff".to_string())?;
    if !(0.0..=MAX_OUTLINE).contains(&width) {
        return Err(format!("толщина обводки должна быть от 0 до {MAX_OUTLINE}"));
    }
    Ok(Outline { width, color })
}

fn parse_optimize(s: &str) -> Result<Optimization, String> {
    match s {
        "auto" => Ok(Optimization::Auto),
        "lossless" => Ok(Optimization::Lossless),
        n => match n.parse::<u8>() {
            Ok(d) if d <= 6 => Ok(Optimization::Manual(d)),
            _ => Err("ожидается auto, lossless или число 0–6".into()),
        },
    }
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

fn collect_dir(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if recursive {
                collect_dir(&path, recursive, out)?;
            }
        } else if is_svg(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Входные пути → список SVG без повторов, в порядке аргументов.
fn collect_inputs(inputs: &[PathBuf], recursive: bool) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            collect_dir(input, recursive, &mut files)?;
        } else {
            files.push(input.clone());
        }
    }
    let mut seen = HashSet::new();
    files.retain(|f| seen.insert(f.clone()));
    Ok(files)
}

/// Куда писать каждый файл. Имена распределяются заранее, до параллельной
/// работы: два `icon.svg` из разных папок не затрут друг друга.
fn plan_outputs(files: &[PathBuf], out_dir: &Path, conflict: Conflict) -> Vec<Option<PathBuf>> {
    let mut taken: HashSet<PathBuf> = HashSet::new();
    files
        .iter()
        .map(|src| {
            let stem = src
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let mut dst = out_dir.join(format!("{stem}.tgs"));
            let busy = |p: &PathBuf, taken: &HashSet<PathBuf>| taken.contains(p) || p.exists();
            if busy(&dst, &taken) {
                match conflict {
                    // Внутри одного запуска «заменить» затёрло бы только что
                    // записанный файл — такие совпадения всегда нумеруются.
                    Conflict::Overwrite if !taken.contains(&dst) => {}
                    Conflict::Skip if !taken.contains(&dst) => return None,
                    _ => {
                        let mut n = 1;
                        while busy(&dst, &taken) {
                            dst = out_dir.join(format!("{stem}_{n}.tgs"));
                            n += 1;
                        }
                    }
                }
            }
            taken.insert(dst.clone());
            Some(dst)
        })
        .collect()
}

/// Итог по одному файлу (он же — запись JSON-отчёта).
#[derive(Serialize)]
struct Report {
    input: String,
    output: Option<String>,
    /// `ok`, `warning`, `skipped` или `error`.
    status: &'static str,
    tgs_bytes: Option<usize>,
    warnings: Vec<Warning>,
    /// Код ошибки: `read_failed`, `invalid_svg`, `empty`, `invalid_option`, `write_failed`.
    error: Option<&'static str>,
    #[serde(skip)]
    error_text: Option<String>,
}

fn process(src: &Path, dst: Option<&Path>, options: &Options) -> Report {
    let mut report = Report {
        input: src.display().to_string(),
        output: dst.map(|d| d.display().to_string()),
        status: "error",
        tgs_bytes: None,
        warnings: Vec::new(),
        error: None,
        error_text: None,
    };
    let Some(dst) = dst else {
        report.status = "skipped";
        return report;
    };
    let svg = match std::fs::read(src) {
        Ok(s) => s,
        Err(e) => {
            report.error = Some("read_failed");
            report.error_text = Some(format!("не удалось прочитать файл: {e}"));
            return report;
        }
    };
    let conv = match svg2tgs_core::convert(&svg, options) {
        Ok(c) => c,
        Err(e) => {
            report.error = Some(match e {
                svg2tgs_core::Error::InvalidSvg(_) => "invalid_svg",
                svg2tgs_core::Error::Empty => "empty",
                svg2tgs_core::Error::InvalidOption(_) => "invalid_option",
            });
            report.error_text = Some(messages::error(&e));
            return report;
        }
    };
    if let Err(e) = std::fs::write(dst, &conv.tgs) {
        report.error = Some("write_failed");
        report.error_text = Some(format!("не удалось записать результат: {e}"));
        return report;
    }
    let warn = conv
        .warnings
        .iter()
        .any(|w| w.code.severity() == Severity::Warning);
    report.status = if warn { "warning" } else { "ok" };
    report.tgs_bytes = Some(conv.tgs_bytes());
    report.warnings = conv.warnings;
    report
}

/// Консоль Windows по умолчанию не в UTF-8: без этого кириллица
/// и значки ✓ ⚠ ✗ в старом cmd.exe превращаются в «кракозябры».
#[cfg(windows)]
fn enable_utf8_console() {
    extern "system" {
        fn SetConsoleOutputCP(code_page: u32) -> i32;
    }
    // SAFETY: функция kernel32 без указателей; 65001 — кодовая страница UTF-8.
    unsafe {
        SetConsoleOutputCP(65001);
    }
}

#[cfg(not(windows))]
fn enable_utf8_console() {}

fn main() -> ExitCode {
    enable_utf8_console();
    let cli = Cli::parse();
    let options = Options {
        preset: match cli.preset {
            PresetArg::Sticker => Preset::Sticker,
            PresetArg::Emoji => Preset::Emoji,
        },
        padding: cli.padding,
        fit: match cli.fit {
            FitArg::Contain => Fit::Contain,
            FitArg::Cover => Fit::Cover,
            FitArg::Width => Fit::Width,
            FitArg::Height => Fit::Height,
            FitArg::Free => Fit::Free,
        },
        bounds: match cli.bounds {
            BoundsArg::Container => Bounds::Container,
            BoundsArg::Content => Bounds::Content,
        },
        scale: cli.scale / 100.0,
        position_x: cli.position.0 / 100.0,
        position_y: cli.position.1 / 100.0,
        flip_h: cli.flip_h,
        flip_v: cli.flip_v,
        rotation: cli.rotate,
        aspect: cli.aspect / 100.0,
        crop: cli.crop.unwrap_or_default(),
        background: cli
            .bg
            .clone()
            .map_or(Background::Transparent, Background::Solid),
        background_radius: cli.bg_radius.max(0.0),
        background_border: cli.bg_border.clone(),
        outline: cli.outline.clone(),
        optimization: cli.optimize,
        fps: cli.fps,
        duration_secs: cli.duration,
        ..Options::default()
    };

    let files = match collect_inputs(&cli.inputs, cli.recursive) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Не удалось прочитать входные пути: {e}");
            return ExitCode::from(1);
        }
    };
    if files.is_empty() {
        eprintln!("SVG-файлы не найдены.");
        return ExitCode::from(1);
    }
    if let Err(e) = std::fs::create_dir_all(&cli.output) {
        eprintln!("Не удалось создать папку {}: {e}", cli.output.display());
        return ExitCode::from(1);
    }

    let outputs = plan_outputs(&files, &cli.output, cli.on_conflict);
    let threads = cli
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()))
        .clamp(1, files.len());
    let next = AtomicUsize::new(0);
    let mut reports: Vec<Option<Report>> = (0..files.len()).map(|_| None).collect();
    let slots: Vec<std::sync::Mutex<&mut Option<Report>>> =
        reports.iter_mut().map(std::sync::Mutex::new).collect();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= files.len() {
                    break;
                }
                let r = process(&files[i], outputs[i].as_deref(), &options);
                **slots[i].lock().unwrap() = Some(r);
            });
        }
    });
    drop(slots);
    let reports: Vec<Report> = reports
        .into_iter()
        .map(|r| r.expect("каждый файл обработан"))
        .collect();

    let count = |s: &str| reports.iter().filter(|r| r.status == s).count();
    let failed = count("error");

    if cli.json {
        match serde_json::to_string_pretty(&reports) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("Не удалось записать отчёт: {e}"),
        }
    } else {
        print_human(&reports, &options);
    }

    if failed > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_human(reports: &[Report], options: &Options) {
    let limit = options.preset.max_bytes();
    for r in reports {
        match r.status {
            "ok" | "warning" => {
                let mark = if r.status == "ok" { "✓" } else { "⚠" };
                println!(
                    "{mark} {} → {} ({:.1} КБ)",
                    r.input,
                    r.output.as_deref().unwrap_or(""),
                    r.tgs_bytes.unwrap_or(0) as f64 / 1024.0
                );
                for w in &r.warnings {
                    println!("    · {}", messages::warning(w));
                }
            }
            "skipped" => println!("– {}: уже есть, пропущен", r.input),
            _ => eprintln!(
                "✗ {}: {}",
                r.input,
                r.error_text.as_deref().unwrap_or("ошибка")
            ),
        }
    }
    let count = |s: &str| reports.iter().filter(|r| r.status == s).count();
    let mut summary = format!(
        "\nГотово: {} без замечаний, {} с замечаниями",
        count("ok"),
        count("warning")
    );
    if count("skipped") > 0 {
        summary += &format!(", {} пропущено", count("skipped"));
    }
    summary += &format!(", {} с ошибкой.", count("error"));
    println!("{summary}");
    let over = reports
        .iter()
        .filter(|r| r.tgs_bytes.is_some_and(|b| b > limit))
        .count();
    if over > 0 {
        println!(
            "Больше лимита Telegram ({} КБ): {over}. Упростите рисунок или уберите мелкие детали.",
            limit / 1024
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_names_from_different_folders_get_numbers() {
        let out = std::env::temp_dir().join(format!("svg2tgs-cli-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        let files = [PathBuf::from("a/icon.svg"), PathBuf::from("b/icon.svg")];
        for c in [Conflict::Number, Conflict::Overwrite, Conflict::Skip] {
            let plan = plan_outputs(&files, &out, c);
            assert_eq!(plan[0].as_ref().unwrap(), &out.join("icon.tgs"));
            assert_eq!(plan[1].as_ref().unwrap(), &out.join("icon_1.tgs"), "{c:?}");
        }
    }

    #[test]
    fn existing_files_follow_the_rule() {
        let out = std::env::temp_dir().join(format!("svg2tgs-cli-exist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("star.tgs"), b"old").unwrap();
        let files = [PathBuf::from("star.svg")];
        assert_eq!(
            plan_outputs(&files, &out, Conflict::Number)[0]
                .as_ref()
                .unwrap(),
            &out.join("star_1.tgs")
        );
        assert_eq!(
            plan_outputs(&files, &out, Conflict::Overwrite)[0]
                .as_ref()
                .unwrap(),
            &out.join("star.tgs")
        );
        assert!(plan_outputs(&files, &out, Conflict::Skip)[0].is_none());
    }
}
