//! Сценарий выгрузки: проверить задание, создать набор (или найти, на чём
//! остановились), загрузить файлы по одному, сообщать ход.
//!
//! Продолжение после обрыва опирается на сам набор в Telegram: сколько
//! стикеров в нём уже есть, столько первых файлов задания и выгружено.
//! Поэтому порядок файлов в задании должен быть тем же.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::api::{Bot, NewSticker, Transport};
use crate::error::{Failure, PublishError};
use crate::rules::{self, SetKind};

/// Откуда взять файл TGS.
#[derive(Clone, Debug)]
pub enum Source {
    Path(PathBuf),
    Bytes(Vec<u8>),
}

/// Один стикер задания.
#[derive(Clone, Debug)]
pub struct Item {
    /// Имя файла для Telegram (`heart_01.tgs`).
    pub filename: String,
    pub source: Source,
    /// 1–20 эмодзи.
    pub emoji: Vec<String>,
}

/// Что и куда выгрузить.
#[derive(Clone, Debug)]
pub struct Job {
    pub kind: SetKind,
    /// Полное имя набора, с `_by_<бот>`.
    pub name: String,
    pub title: String,
    pub items: Vec<Item>,
    /// Набор с таким именем уже есть — дописать недостающее
    /// (продолжение после обрыва). Иначе это ошибка «имя занято».
    pub resume: bool,
    /// Дописать в существующий набор: сколько в нём было стикеров до первой
    /// попытки. Всё, что сверх этого, — уже дописанные файлы задания
    /// (так продолжение после обрыва не повторяет их). `None` — новый набор.
    pub append: Option<usize>,
}

/// Ход выгрузки.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Progress {
    /// Сколько стикеров уже в наборе.
    pub done: usize,
    pub total: usize,
}

/// Итог.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Done {
    pub name: String,
    pub link: String,
    pub count: usize,
    /// С какого стикера продолжили (0 — набор создан заново).
    pub resumed_from: usize,
}

/// Проверка задания до любых запросов к Telegram.
pub fn validate(job: &Job, bot_username: &str) -> Result<(), PublishError> {
    if !rules::name_is_valid(&job.name, bot_username) {
        return Err(PublishError::NameInvalid {
            name: job.name.clone(),
        });
    }
    if !rules::title_is_valid(&job.title) {
        return Err(PublishError::TitleInvalid);
    }
    if job.items.is_empty() {
        return Err(PublishError::Nothing);
    }
    let max = job.kind.capacity();
    if job.items.len() > max {
        return Err(PublishError::TooMany {
            count: job.items.len(),
            max,
        });
    }
    for (index, item) in job.items.iter().enumerate() {
        let n = item.emoji.len();
        if n == 0 || n > rules::EMOJI_MAX || item.emoji.iter().any(|e| e.trim().is_empty()) {
            return Err(PublishError::EmojiInvalid { index });
        }
    }
    Ok(())
}

fn read(item: &Item, index: usize) -> Result<Vec<u8>, PublishError> {
    match &item.source {
        Source::Bytes(b) => Ok(b.clone()),
        Source::Path(p) => std::fs::read(p).map_err(|e| PublishError::ReadFailed {
            index,
            detail: e.to_string(),
        }),
    }
}

/// Загрузить файл; отказ Telegram с кодом 400 — это «файл не принят».
fn upload<T: Transport>(
    bot: &Bot<T>,
    owner: i64,
    item: &Item,
    index: usize,
) -> Result<NewSticker, PublishError> {
    let bytes = read(item, index)?;
    match bot.upload(owner, &item.filename, bytes) {
        Ok(file_id) => Ok(NewSticker {
            file_id,
            emoji: item.emoji.clone(),
        }),
        Err(Failure::Api {
            code: 400,
            description,
            ..
        }) => Err(PublishError::FileRejected {
            index,
            detail: description,
        }),
        Err(f) => Err(PublishError::from_failure(f, index)),
    }
}

/// Выгрузить задание. `progress` вызывается после каждого стикера,
/// `stop` проверяется перед каждым следующим файлом.
pub fn run<T: Transport>(
    bot: &Bot<T>,
    bot_username: &str,
    owner: i64,
    job: &Job,
    mut progress: impl FnMut(Progress),
    stop: &AtomicBool,
) -> Result<Done, PublishError> {
    validate(job, bot_username)?;
    let total = job.items.len();
    let existing = bot
        .get_set(&job.name)
        .map_err(|f| PublishError::from_failure(f, 0))?;

    // `from` — с какого файла продолжать; `resumed_from` — сколько уже было в наборе.
    let (from, resumed_from) = if let Some(base) = job.append {
        let name = job.name.clone();
        let Some(set) = existing else {
            return Err(PublishError::SetMissing { name });
        };
        if set.kind.is_some_and(|k| k != job.kind) {
            return Err(PublishError::KindMismatch { name });
        }
        let Some(already) = set.count.checked_sub(base).filter(|&a| a <= total) else {
            return Err(PublishError::SetMismatch {
                name,
                existing: set.count,
                planned: base + total,
            });
        };
        let max = job.kind.capacity();
        if base + total > max {
            return Err(PublishError::TooMany {
                count: base + total,
                max,
            });
        }
        if already > 0 {
            progress(Progress {
                done: already,
                total,
            });
        }
        (already, already)
    } else {
        match existing {
            Some(_) if !job.resume => {
                return Err(PublishError::NameTaken {
                    name: job.name.clone(),
                })
            }
            Some(set) if set.count > total => {
                return Err(PublishError::SetMismatch {
                    name: job.name.clone(),
                    existing: set.count,
                    planned: total,
                })
            }
            Some(set) => {
                progress(Progress {
                    done: set.count,
                    total,
                });
                (set.count, set.count)
            }
            None => {
                if stop.load(Ordering::Relaxed) {
                    return Err(PublishError::Stopped { done: 0 });
                }
                let first = upload(bot, owner, &job.items[0], 0)?;
                bot.create_set(owner, &job.name, &job.title, job.kind, &[first])
                    .map_err(|f| create_error(f, &job.name))?;
                progress(Progress { done: 1, total });
                (1, 0)
            }
        }
    };
    for index in from..total {
        if stop.load(Ordering::Relaxed) {
            return Err(PublishError::Stopped { done: index });
        }
        let sticker = upload(bot, owner, &job.items[index], index)?;
        bot.add_sticker(owner, &job.name, &sticker)
            .map_err(|f| PublishError::from_failure(f, index))?;
        progress(Progress {
            done: index + 1,
            total,
        });
    }
    Ok(Done {
        name: job.name.clone(),
        link: job.kind.link(&job.name),
        count: total,
        resumed_from,
    })
}

/// Отказ `createNewStickerSet`: имя занято или неверно — отдельными кодами.
fn create_error(f: Failure, name: &str) -> PublishError {
    match f {
        Failure::Api { description, .. } if description.contains("already occupied") => {
            PublishError::NameTaken { name: name.into() }
        }
        Failure::Api { description, .. } if description.contains("invalid sticker set name") => {
            PublishError::NameInvalid { name: name.into() }
        }
        other => PublishError::from_failure(other, 0),
    }
}
