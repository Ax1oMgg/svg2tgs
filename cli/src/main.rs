//! svg2tgs — командная строка.
//!
//! Конвертирует SVG-файлы и папки в .tgs и печатает замечания по-русски.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod messages;

use clap::{Parser, ValueEnum};
use svg2tgs_core::{Background, Fit, Optimization, Options, Preset};

#[derive(Parser, Debug)]
#[command(name = "svg2tgs", version, about = "Конвертация SVG в Telegram TGS")]
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

    /// Отступ от края канваса, % стороны (0–20).
    #[arg(long, default_value_t = 0.0, value_parser = parse_padding)]
    padding: f32,

    /// Как вписать рисунок: целиком или заполнить с обрезкой.
    #[arg(long, value_enum, default_value_t = FitArg::Contain)]
    fit: FitArg,

    /// Цвет фона, например #ffffff. По умолчанию прозрачный.
    #[arg(long)]
    bg: Option<String>,

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
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum FitArg {
    Contain,
    Cover,
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

#[derive(Copy, Clone, Debug, ValueEnum)]
enum PresetArg {
    Sticker,
    Emoji,
}

impl From<PresetArg> for Preset {
    fn from(p: PresetArg) -> Self {
        match p {
            PresetArg::Sticker => Preset::Sticker,
            PresetArg::Emoji => Preset::Emoji,
        }
    }
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

fn collect_dir(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
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

fn collect_inputs(inputs: &[PathBuf], recursive: bool) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_dir() {
            collect_dir(input, recursive, &mut files)?;
        } else {
            files.push(input.clone());
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn convert_one(
    src: &Path,
    out_dir: &Path,
    options: &Options,
) -> Result<(PathBuf, svg2tgs_core::Conversion), String> {
    let svg = std::fs::read(src).map_err(|e| format!("не удалось прочитать файл: {e}"))?;
    let conv = svg2tgs_core::convert(&svg, options).map_err(|e| messages::error(&e))?;
    let name = src.file_stem().unwrap_or_default();
    let dst = out_dir.join(name).with_extension("tgs");
    std::fs::write(&dst, &conv.tgs).map_err(|e| format!("не удалось записать результат: {e}"))?;
    Ok((dst, conv))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let options = Options {
        preset: cli.preset.into(),
        padding: cli.padding,
        fit: match cli.fit {
            FitArg::Contain => Fit::Contain,
            FitArg::Cover => Fit::Cover,
        },
        background: cli
            .bg
            .clone()
            .map_or(Background::Transparent, Background::Solid),
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

    let (mut ok, mut failed, mut with_warnings, mut over_limit) = (0, 0, 0, 0);
    for file in &files {
        match convert_one(file, &cli.output, &options) {
            Ok((dst, conv)) => {
                ok += 1;
                let mark = if conv
                    .warnings
                    .iter()
                    .any(|w| w.code.severity() == svg2tgs_core::Severity::Warning)
                {
                    with_warnings += 1;
                    "⚠"
                } else {
                    "✓"
                };
                if conv.tgs_bytes() > options.preset.max_bytes() {
                    over_limit += 1;
                }
                println!(
                    "{mark} {} → {} ({:.1} КБ)",
                    file.display(),
                    dst.display(),
                    conv.tgs_bytes() as f64 / 1024.0
                );
                for w in &conv.warnings {
                    println!("    · {}", messages::warning(w));
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("✗ {}: {e}", file.display());
            }
        }
    }
    println!("\nГотово: {ok} успешно ({with_warnings} с замечаниями), {failed} с ошибкой.");
    if over_limit > 0 {
        println!(
            "Больше лимита Telegram ({} КБ): {over_limit}. Упростите рисунок или уберите мелкие детали.",
            options.preset.max_bytes() / 1024
        );
    }

    if failed > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}
