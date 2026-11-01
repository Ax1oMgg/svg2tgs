//! `svg2tgs publish` — выгрузка готовых .tgs в набор Telegram через бота.
//!
//! Токен бота — из переменной окружения `SVG2TGS_BOT_TOKEN` (аргументы
//! командной строки видны другим программам в списке процессов).
//! В сеть обращается только эта подкоманда и только к `api.telegram.org`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::{Parser, ValueEnum};
use svg2tgs_publish::{rules, Cover, Item, Job, Owner, PublishError, SetKind, Source};

use crate::messages;

/// Переменная окружения с токеном бота.
pub const TOKEN_VAR: &str = "SVG2TGS_BOT_TOKEN";

#[derive(Parser, Debug)]
#[command(
    name = "svg2tgs publish",
    about = "Выгрузка готовых .tgs в набор стикеров или эмодзи Telegram через вашего бота",
    after_help = "Токен бота — в переменной окружения SVG2TGS_BOT_TOKEN (бота создают у @BotFather).\n\
        Владелец наборов — тот, кто последним написал боту /start, или --user-id.\n\n\
        Примеры:\n  \
        svg2tgs publish ./tgs --title \"Сердечки\" --kind emoji --emoji ❤️\n  \
        svg2tgs publish ./tgs --title \"Сердечки\" --kind emoji --emoji ❤️ --resume\n  \
        svg2tgs publish ./new --append --name serdechki --emoji ❤️\n  \
        svg2tgs publish ./tgs --title \"Чай\" --thumbnail 3\n\n\
        Коды выхода: 0 — набор готов, 1 — неверные параметры, 2 — ошибка выгрузки."
)]
pub struct PublishCli {
    /// Файлы .tgs или папки с ними. В наборе — по порядку имён.
    #[arg(required = true)]
    inputs: Vec<PathBuf>,

    /// Название набора (1–64 символа). При --append не нужно.
    #[arg(long, required_unless_present = "append")]
    title: Option<String>,

    /// Адрес набора без окончания `_by_<бот>`; по умолчанию — из названия.
    #[arg(long)]
    name: Option<String>,

    /// Стикеры или эмодзи.
    #[arg(long, value_enum, default_value_t = KindArg::Sticker)]
    kind: KindArg,

    /// Эмодзи для каждого файла: один или несколько через запятую (до 20).
    #[arg(long, default_value = "⭐")]
    emoji: String,

    /// Набор уже есть (выгрузка оборвалась) — дописать недостающее.
    #[arg(long)]
    resume: bool,

    /// Дописать файлы в существующий набор этого бота (адрес — --name).
    #[arg(long, requires = "name", conflicts_with = "resume")]
    append: bool,

    /// Сколько стикеров было в наборе до первой попытки --append — чтобы
    /// после обрыва дописать только оставшееся (программа подскажет число).
    #[arg(long, requires = "append")]
    base: Option<usize>,

    /// Обложка набора: номер файла (с 1) или свой .tgs до 32 КБ (только
    /// для набора стикеров; у набора эмодзи обложка — один из его эмодзи).
    #[arg(long, value_name = "N|ФАЙЛ.tgs")]
    thumbnail: Option<String>,

    /// Числовой id владельца в Telegram, если не хотите писать боту /start.
    #[arg(long)]
    user_id: Option<i64>,

    /// Итог одной строкой JSON (для скриптов).
    #[arg(short, long)]
    json: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum KindArg {
    Sticker,
    Emoji,
}

fn collect(inputs: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    let is_tgs = |p: &Path| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("tgs"));
    let mut out = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(input)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file() && is_tgs(p))
                .collect();
            files.sort();
            out.extend(files);
        } else {
            out.push(input.clone());
        }
    }
    Ok(out)
}

fn split_emoji(s: &str) -> Vec<String> {
    s.split([',', ' '])
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .collect()
}

/// Обложка из `--thumbnail`: номер файла (с 1) или путь к .tgs.
fn cover_arg(arg: &str) -> Result<Cover, String> {
    match arg.trim().parse::<usize>() {
        Ok(0) => Err("номер файла для обложки — с 1".into()),
        Ok(n) => Ok(Cover::Item(n - 1)),
        Err(_) => std::fs::read(arg)
            .map(Cover::Tgs)
            .map_err(|e| format!("не удалось прочитать обложку {arg}: {e}")),
    }
}

fn fail(json: bool, e: &PublishError) -> ExitCode {
    if json {
        println!("{}", serde_json::to_string(e).unwrap_or_default());
    } else {
        eprintln!("✗ {}", messages::publish_error(e));
    }
    ExitCode::from(2)
}

pub fn main(args: impl IntoIterator<Item = std::ffi::OsString>) -> ExitCode {
    let cli = match PublishCli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    let Ok(token) = std::env::var(TOKEN_VAR) else {
        eprintln!("✗ нет токена бота: задайте переменную окружения {TOKEN_VAR}");
        return ExitCode::from(1);
    };
    let files = match collect(&cli.inputs) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("✗ не удалось прочитать папку: {e}");
            return ExitCode::from(1);
        }
    };
    let emoji = split_emoji(&cli.emoji);
    let cover = match cli.thumbnail.as_deref().map(cover_arg).transpose() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("✗ {e}");
            return ExitCode::from(1);
        }
    };

    let bot = svg2tgs_publish::connect(&token);
    let me = match bot.me() {
        Ok(m) => m,
        Err(e) => return fail(cli.json, &e),
    };
    let owner = match cli.user_id {
        Some(id) => Owner {
            id,
            username: None,
            first_name: String::new(),
        },
        None => match bot.find_owner() {
            Ok(Some(o)) => o,
            Ok(None) => return fail(cli.json, &PublishError::NoOwner),
            Err(e) => return fail(cli.json, &e),
        },
    };

    let stem = cli
        .name
        .clone()
        .map(|n| {
            let suffix = rules::suffix(&me.username).to_ascii_lowercase();
            let lower = n.to_ascii_lowercase();
            match lower.strip_suffix(&suffix) {
                Some(_) => n[..n.len() - suffix.len()].to_string(),
                None => n,
            }
        })
        .unwrap_or_else(|| {
            rules::stem_from_title(cli.title.as_deref().unwrap_or(""), &me.username)
        });
    let name = rules::set_name(&stem, &me.username);
    let kind = match cli.kind {
        KindArg::Sticker => SetKind::Stickers,
        KindArg::Emoji => SetKind::Emoji,
    };
    // Дописывание: название и тип — у набора, база — сколько в нём сейчас.
    let (title, kind, append) = if cli.append {
        let set = match bot.get_set(&name) {
            Ok(Some(set)) => set,
            Ok(None) => return fail(cli.json, &PublishError::SetMissing { name }),
            Err(f) => return fail(cli.json, &PublishError::from_failure(f, 0)),
        };
        (
            set.title,
            set.kind.unwrap_or(kind),
            Some(cli.base.unwrap_or(set.count)),
        )
    } else {
        (cli.title.clone().unwrap_or_default(), kind, None)
    };
    let job = Job {
        kind,
        name,
        title,
        items: files
            .iter()
            .map(|p| Item {
                filename: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "sticker.tgs".into()),
                source: Source::Path(p.clone()),
                emoji: emoji.clone(),
            })
            .collect(),
        resume: cli.resume,
        append,
        cover,
    };

    if !cli.json {
        let who = owner
            .username
            .as_ref()
            .map(|u| format!("@{u}"))
            .unwrap_or_else(|| {
                if owner.first_name.is_empty() {
                    owner.id.to_string()
                } else {
                    owner.first_name.clone()
                }
            });
        eprintln!(
            "Бот @{} → аккаунт {who}: «{}» ({}), файлов: {}",
            me.username,
            job.title,
            job.name,
            job.items.len()
        );
    }
    let quiet = cli.json;
    let result = svg2tgs_publish::run(
        &bot,
        &me.username,
        owner.id,
        &job,
        |p| {
            if !quiet {
                // Хвост из пробелов стирает прежнюю, более длинную строку.
                match p.wait {
                    Some(secs) => eprint!(
                        "\r  {} из {} — Telegram просит подождать: {secs} с   ",
                        p.done, p.total
                    ),
                    None => eprint!("\r  {} из {}{:40}", p.done, p.total, ""),
                }
                let _ = std::io::stderr().flush();
            }
        },
        &AtomicBool::new(false),
    );
    if !quiet {
        eprintln!();
    }
    match result {
        Ok(done) => {
            if cli.json {
                println!("{}", serde_json::to_string(&done).unwrap_or_default());
            } else {
                println!("✓ готово: {} шт. — {}", done.count, done.link);
                if let Some(e) = &done.cover_error {
                    eprintln!(
                        "⚠ обложку поставить не удалось: {}",
                        messages::publish_error(e)
                    );
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let code = fail(cli.json, &e);
            if !cli.json && e.done().is_some() {
                match job.append {
                    Some(base) => eprintln!(
                        "  Уже дописанное осталось в наборе: повторите команду с --base {base}."
                    ),
                    None => eprintln!(
                        "  Уже выгруженное осталось в наборе: повторите команду с --resume."
                    ),
                }
            }
            code
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_are_split() {
        assert_eq!(split_emoji("❤️"), vec!["❤️"]);
        assert_eq!(split_emoji("❤️, 😍 ⭐"), vec!["❤️", "😍", "⭐"]);
        assert!(split_emoji(" , ").is_empty());
    }

    #[test]
    fn thumbnail_is_a_number_or_a_file() {
        assert!(matches!(cover_arg("3"), Ok(Cover::Item(2))));
        assert!(cover_arg("0").is_err());
        assert!(cover_arg("нет-такого.tgs").is_err());
        let path = std::env::temp_dir().join(format!("svg2tgs-cover-{}.tgs", std::process::id()));
        std::fs::write(&path, [0x1f, 0x8b]).unwrap();
        let got = cover_arg(path.to_str().unwrap());
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(got, Ok(Cover::Tgs(b)) if b == [0x1f, 0x8b]));
    }

    #[test]
    fn folders_give_tgs_in_name_order() {
        let dir = std::env::temp_dir().join(format!("svg2tgs-pub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["b_02.tgs", "b_01.tgs", "note.txt", "b_10.TGS"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        let got: Vec<_> = collect(std::slice::from_ref(&dir))
            .unwrap()
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(got, ["b_01.tgs", "b_02.tgs", "b_10.TGS"]);
    }
}
