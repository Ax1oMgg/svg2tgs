//! Сценарий выгрузки: проверить задание, создать набор (или найти, на чём
//! остановились), загрузить файлы по одному, сообщать ход.
//!
//! Продолжение после обрыва опирается на сам набор в Telegram: сколько
//! стикеров в нём уже есть, столько первых файлов задания и выгружено.
//! Поэтому порядок файлов в задании должен быть тем же.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

use crate::api::{Bot, NewSticker, Pause, Transport};
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
    /// Обложка набора; `None` — не менять (у нового набора Telegram берёт
    /// первый стикер).
    pub cover: Option<Cover>,
}

/// Обложка набора при выгрузке.
#[derive(Clone, Debug)]
pub enum Cover {
    /// Стикер задания с этим номером (с 0). У набора эмодзи — его эмодзи,
    /// у набора стикеров — его файл (если он не больше
    /// [`rules::THUMB_TGS_MAX`]).
    Item(usize),
    /// Отдельный файл TGS — только для набора стикеров.
    Tgs(Vec<u8>),
}

/// Ход выгрузки.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Progress {
    /// Сколько стикеров уже в наборе.
    pub done: usize,
    pub total: usize,
    /// Telegram попросил подождать (ответ 429): сколько секунд осталось
    /// до повтора. Приходит каждую секунду ожидания; `None` — не ждём.
    pub wait: Option<u64>,
}

impl Progress {
    pub(crate) fn at(done: usize, total: usize) -> Self {
        Self {
            done,
            total,
            wait: None,
        }
    }
}

/// Итог.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Done {
    pub name: String,
    pub link: String,
    pub count: usize,
    /// С какого стикера продолжили (0 — набор создан заново).
    pub resumed_from: usize,
    /// Стикеры выгружены, а обложку Telegram не принял: почему.
    pub cover_error: Option<PublishError>,
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
        if !emoji_is_valid(&item.emoji) {
            return Err(PublishError::EmojiInvalid { index });
        }
    }
    match (&job.cover, job.kind) {
        (None, _) => {}
        (Some(Cover::Item(i)), _) if *i >= job.items.len() => {
            return Err(PublishError::CoverInvalid)
        }
        (Some(Cover::Item(i)), SetKind::Stickers) => {
            if let Source::Bytes(b) = &job.items[*i].source {
                check_thumb(b)?;
            }
        }
        (Some(Cover::Item(_)), SetKind::Emoji) => {}
        (Some(Cover::Tgs(b)), SetKind::Stickers) => check_thumb(b)?,
        (Some(Cover::Tgs(_)), SetKind::Emoji) => return Err(PublishError::CoverInvalid),
    }
    Ok(())
}

/// 1–20 непустых эмодзи.
pub(crate) fn emoji_is_valid(emoji: &[String]) -> bool {
    !emoji.is_empty()
        && emoji.len() <= rules::EMOJI_MAX
        && emoji.iter().all(|e| !e.trim().is_empty())
}

/// Обложка набора стикеров — TGS не больше [`rules::THUMB_TGS_MAX`].
pub(crate) fn check_thumb(tgs: &[u8]) -> Result<(), PublishError> {
    if tgs.len() > rules::THUMB_TGS_MAX {
        return Err(PublishError::ThumbTooBig {
            bytes: tgs.len(),
            max: rules::THUMB_TGS_MAX,
        });
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

/// Шаг ожидания: так часто проверяется «Остановить».
const NAP: Duration = Duration::from_millis(250);

/// Переждать `secs` секунд, о которых попросил Telegram, сообщая, сколько
/// осталось. `false` — пользователь остановил выгрузку.
pub(crate) fn wait_out<T: Transport>(
    bot: &Bot<T>,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
    done: usize,
    total: usize,
    secs: u64,
) -> bool {
    let naps = (Duration::from_secs(1).as_millis() / NAP.as_millis()) as u64;
    for left in (1..=secs).rev() {
        progress(Progress {
            done,
            total,
            wait: Some(left),
        });
        for _ in 0..naps {
            if stop.load(Ordering::Relaxed) {
                return false;
            }
            bot.nap(NAP);
        }
    }
    progress(Progress::at(done, total));
    !stop.load(Ordering::Relaxed)
}

/// Загрузить файл; отказ Telegram с кодом 400 — это «файл не принят».
fn upload<T: Transport>(
    bot: &Bot<T>,
    owner: i64,
    item: &Item,
    index: usize,
    pause: Pause,
) -> Result<NewSticker, PublishError> {
    let bytes = read(item, index)?;
    match bot.upload(owner, &item.filename, bytes, pause) {
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

/// Выгрузить задание. `progress` вызывается после каждого стикера
/// и каждую секунду ожидания, если Telegram попросил подождать.
/// `stop` проверяется перед каждым файлом и во время ожидания; запрос,
/// который уже ушёл, дожидается ответа.
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
            progress(Progress::at(already, total));
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
                progress(Progress::at(set.count, total));
                (set.count, set.count)
            }
            None => {
                if stop.load(Ordering::Relaxed) {
                    return Err(PublishError::Stopped { done: 0 });
                }
                let mut pause = |s| wait_out(bot, stop, &mut progress, 0, total, s);
                let first = upload(bot, owner, &job.items[0], 0, &mut pause)?;
                bot.create_set(owner, &job.name, &job.title, job.kind, &[first], &mut pause)
                    .map_err(|f| create_error(f, &job.name))?;
                progress(Progress::at(1, total));
                (1, 0)
            }
        }
    };
    for index in from..total {
        if stop.load(Ordering::Relaxed) {
            return Err(PublishError::Stopped { done: index });
        }
        let mut pause = |s| wait_out(bot, stop, &mut progress, index, total, s);
        let sticker = upload(bot, owner, &job.items[index], index, &mut pause)?;
        bot.add_sticker(owner, &job.name, &sticker, &mut pause)
            .map_err(|f| PublishError::from_failure(f, index))?;
        progress(Progress::at(index + 1, total));
    }
    // Обложка — после всех стикеров: её отказ не отменяет выгрузку.
    let cover_error = job.cover.as_ref().and_then(|cover| {
        let mut pause = |s| wait_out(bot, stop, &mut progress, total, total, s);
        set_cover(bot, owner, job, cover, &mut pause).err()
    });
    Ok(Done {
        name: job.name.clone(),
        link: job.kind.link(&job.name),
        count: total,
        resumed_from,
        cover_error,
    })
}

/// Поставить обложку из задания.
fn set_cover<T: Transport>(
    bot: &Bot<T>,
    owner: i64,
    job: &Job,
    cover: &Cover,
    pause: Pause,
) -> Result<(), PublishError> {
    let fail = |f| PublishError::from_failure(f, job.items.len());
    match (cover, job.kind) {
        (Cover::Item(i), SetKind::Emoji) => {
            // Место стикера в наборе: дописанные идут после тех, что были.
            let at = job.append.unwrap_or(0) + i;
            let set = bot.get_set(&job.name).map_err(fail)?;
            let id = set
                .and_then(|s| s.stickers.get(at).and_then(|st| st.custom_emoji_id.clone()))
                .ok_or(PublishError::CoverInvalid)?;
            bot.set_emoji_thumbnail(&job.name, &id, pause).map_err(fail)
        }
        (Cover::Item(i), SetKind::Stickers) => {
            let bytes = read(&job.items[*i], *i)?;
            check_thumb(&bytes)?;
            bot.set_sticker_thumbnail(owner, &job.name, bytes, pause)
                .map_err(fail)
        }
        (Cover::Tgs(bytes), SetKind::Stickers) => {
            check_thumb(bytes)?;
            bot.set_sticker_thumbnail(owner, &job.name, bytes.clone(), pause)
                .map_err(fail)
        }
        (Cover::Tgs(_), SetKind::Emoji) => Err(PublishError::CoverInvalid),
    }
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
