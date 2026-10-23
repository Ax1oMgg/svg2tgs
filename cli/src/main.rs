//! svg2tgs — командная строка.
//!
//! Этап 0: разбор аргументов и сбор входных файлов. Сама конвертация
//! подключается вместе с ядром на этапе 1, отчёт и пакетный режим — на этапе 4.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use svg2tgs_core::{Options, Preset};

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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let options = Options {
        preset: cli.preset.into(),
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

    let mut failed = 0;
    for file in &files {
        let result = std::fs::read(file)
            .map_err(|e| e.to_string())
            .and_then(|svg| svg2tgs_core::convert(&svg, &options).map_err(|e| e.to_string()));
        match result {
            Ok(conv) => println!("OK   {} ({} байт)", file.display(), conv.tgs_bytes()),
            Err(e) => {
                eprintln!("FAIL {}: {e}", file.display());
                failed += 1;
            }
        }
    }
    let _ = &cli.output;

    if failed > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}
