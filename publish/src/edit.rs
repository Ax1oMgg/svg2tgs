//! Правка набора, созданного ботом: удалить стикеры, поменять эмодзи,
//! переставить, поставить обложку.
//!
//! Правка описывает, каким набор должен стать, а не последовательность
//! запросов: запросы считаются от того, что сейчас лежит в Telegram. Поэтому
//! после обрыва ту же правку можно просто применить ещё раз — сделанное не
//! повторится, а несделанное доделается.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::api::{Bot, Pause, SetInfo, Transport};
use crate::error::PublishError;
use crate::job::{check_thumb, emoji_is_valid, wait_out, Progress};
use crate::rules::SetKind;

/// Обложка при правке набора.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditCover {
    /// Набор эмодзи: эмодзи набора с этим `file_id`.
    Sticker(String),
    /// Набор стикеров: файл TGS (до [`crate::rules::THUMB_TGS_MAX`]).
    Tgs(Vec<u8>),
}

/// Каким должен стать набор.
#[derive(Clone, Debug, Default)]
pub struct Edit {
    /// Полное имя набора, с `_by_<бот>`.
    pub name: String,
    /// Стикеры (`file_id`) в том порядке, в каком их видел пользователь,
    /// когда открыл набор.
    pub seen: Vec<String>,
    /// Какие из них удалить.
    pub delete: Vec<String>,
    /// Оставшиеся — в нужном порядке.
    pub order: Vec<String>,
    /// Новые эмодзи стикеров.
    pub emoji: Vec<(String, Vec<String>)>,
    pub cover: Option<EditCover>,
}

/// Итог правки.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EditDone {
    pub name: String,
    pub link: String,
    /// Сколько стикеров в наборе после правки.
    pub count: usize,
}

/// Проверка правки до любых запросов к Telegram.
pub fn validate_edit(edit: &Edit, kind: SetKind) -> Result<(), PublishError> {
    let kept: Vec<&String> = edit
        .seen
        .iter()
        .filter(|id| !edit.delete.contains(id))
        .collect();
    if kept.is_empty() {
        return Err(PublishError::DeleteAll);
    }
    let same = kept.len() == edit.order.len() && kept.iter().all(|id| edit.order.contains(id));
    if !same || edit.delete.iter().any(|id| !edit.seen.contains(id)) {
        return Err(PublishError::SetChanged {
            name: edit.name.clone(),
        });
    }
    for (id, emoji) in &edit.emoji {
        if !emoji_is_valid(emoji) {
            let index = edit.seen.iter().position(|s| s == id).unwrap_or(0);
            return Err(PublishError::EmojiInvalid { index });
        }
    }
    match (&edit.cover, kind) {
        (None, _) => Ok(()),
        (Some(EditCover::Sticker(id)), SetKind::Emoji) if edit.order.contains(id) => Ok(()),
        (Some(EditCover::Tgs(b)), SetKind::Stickers) => check_thumb(b),
        _ => Err(PublishError::CoverInvalid),
    }
}

/// Перестановки, которые превращают `current` в `desired`: пары
/// (`file_id`, новое место с 0). Стикер вынимается и вставляется на новое
/// место, как делает `setStickerPositionInSet`.
pub fn moves(current: &[String], desired: &[String]) -> Vec<(String, usize)> {
    let mut cur = current.to_vec();
    let mut out = Vec::new();
    for (i, want) in desired.iter().enumerate() {
        if cur.get(i) == Some(want) {
            continue;
        }
        let Some(j) = cur.iter().position(|c| c == want) else {
            continue;
        };
        let id = cur.remove(j);
        cur.insert(i, id.clone());
        out.push((id, i));
    }
    out
}

/// Набор в Telegram сейчас — тот, что видел пользователь, за вычетом уже
/// удалённых этой же правкой (продолжение после обрыва)?
fn matches(set: &SetInfo, edit: &Edit) -> bool {
    let now: Vec<&String> = set.stickers.iter().map(|s| &s.file_id).collect();
    now.iter().all(|id| edit.seen.contains(id))
        && edit
            .seen
            .iter()
            .filter(|id| !now.contains(id))
            .all(|id| edit.delete.contains(id))
}

/// Применить правку. Ход — «сделано N запросов из M», с ожиданием по
/// просьбе Telegram (429), как у выгрузки; `stop` проверяется перед каждым
/// запросом.
pub fn apply<T: Transport>(
    bot: &Bot<T>,
    owner: i64,
    kind: SetKind,
    edit: &Edit,
    mut progress: impl FnMut(Progress),
    stop: &AtomicBool,
) -> Result<EditDone, PublishError> {
    validate_edit(edit, kind)?;
    let name = edit.name.clone();
    let set = bot
        .get_set(&name)
        .map_err(|f| PublishError::from_failure(f, 0))?
        .ok_or(PublishError::SetMissing { name: name.clone() })?;
    if set.kind.is_some_and(|k| k != kind) {
        return Err(PublishError::KindMismatch { name });
    }
    if !matches(&set, edit) {
        return Err(PublishError::SetChanged { name });
    }

    // Что осталось сделать — от того, что лежит в Telegram сейчас.
    let now: Vec<String> = set.stickers.iter().map(|s| s.file_id.clone()).collect();
    let deletes: Vec<&String> = edit.delete.iter().filter(|id| now.contains(id)).collect();
    let after: Vec<String> = now
        .iter()
        .filter(|id| !edit.delete.contains(id))
        .cloned()
        .collect();
    let emoji: Vec<&(String, Vec<String>)> = edit
        .emoji
        .iter()
        .filter(|(id, list)| {
            // `getStickerSet` знает только первый эмодзи: если он другой
            // или эмодзи несколько — заменяем список.
            let first = set
                .stickers
                .iter()
                .find(|s| &s.file_id == id)
                .and_then(|s| s.emoji.as_deref());
            after.contains(id) && (list.len() != 1 || first != Some(list[0].as_str()))
        })
        .collect();
    let moves = moves(&after, &edit.order);
    let total = deletes.len() + emoji.len() + moves.len() + usize::from(edit.cover.is_some());

    let mut done = 0;
    let step = |done: &mut usize,
                progress: &mut dyn FnMut(Progress),
                call: &mut dyn FnMut(Pause) -> Result<(), crate::Failure>|
     -> Result<(), PublishError> {
        if stop.load(Ordering::Relaxed) {
            return Err(PublishError::Stopped { done: *done });
        }
        let mut pause = |s| wait_out(bot, stop, &mut *progress, *done, total, s);
        call(&mut pause).map_err(|f| PublishError::from_failure(f, *done))?;
        *done += 1;
        progress(Progress::at(*done, total));
        Ok(())
    };
    progress(Progress::at(0, total));
    for id in deletes {
        step(&mut done, &mut progress, &mut |p| bot.delete_sticker(id, p))?;
    }
    for (id, list) in emoji {
        step(&mut done, &mut progress, &mut |p| {
            bot.set_emoji(id, list, p)
        })?;
    }
    for (id, at) in &moves {
        step(&mut done, &mut progress, &mut |p| {
            bot.set_position(id, *at, p)
        })?;
    }
    match &edit.cover {
        None => {}
        Some(EditCover::Sticker(id)) => {
            let custom = set
                .stickers
                .iter()
                .find(|s| &s.file_id == id)
                .and_then(|s| s.custom_emoji_id.clone())
                .ok_or(PublishError::CoverInvalid)?;
            step(&mut done, &mut progress, &mut |p| {
                bot.set_emoji_thumbnail(&name, &custom, p)
            })?;
        }
        Some(EditCover::Tgs(bytes)) => {
            step(&mut done, &mut progress, &mut |p| {
                bot.set_sticker_thumbnail(owner, &name, bytes.clone(), p)
            })?;
        }
    }
    Ok(EditDone {
        link: kind.link(&name),
        name,
        count: after.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(s: &str) -> Vec<String> {
        s.chars().map(|c| c.to_string()).collect()
    }

    /// Применить перестановки так, как это делает Telegram.
    fn replay(current: &[String], moves: &[(String, usize)]) -> Vec<String> {
        let mut cur = current.to_vec();
        for (id, at) in moves {
            let j = cur.iter().position(|c| c == id).unwrap();
            let x = cur.remove(j);
            cur.insert(*at, x);
        }
        cur
    }

    #[test]
    fn moves_reach_the_order_with_few_requests() {
        for (from, to, most) in [
            ("abcde", "abcde", 0),
            ("abcde", "eabcd", 1),
            ("abcde", "bcdea", 4),
            ("abcde", "edcba", 4),
            ("abcde", "acbde", 1),
        ] {
            let m = moves(&ids(from), &ids(to));
            assert_eq!(replay(&ids(from), &m), ids(to), "{from} → {to}");
            assert!(m.len() <= most, "{from} → {to}: {m:?}");
        }
    }

    #[test]
    fn edits_are_checked() {
        let base = Edit {
            name: "a_by_b".into(),
            seen: ids("abc"),
            order: ids("cab"),
            ..Edit::default()
        };
        assert_eq!(validate_edit(&base, SetKind::Emoji), Ok(()));
        let all = Edit {
            delete: ids("abc"),
            order: vec![],
            ..base.clone()
        };
        assert_eq!(
            validate_edit(&all, SetKind::Emoji),
            Err(PublishError::DeleteAll)
        );
        let lost = Edit {
            order: ids("ab"),
            ..base.clone()
        };
        assert!(matches!(
            validate_edit(&lost, SetKind::Emoji),
            Err(PublishError::SetChanged { .. })
        ));
        let no_emoji = Edit {
            emoji: vec![("b".into(), vec![])],
            ..base.clone()
        };
        assert_eq!(
            validate_edit(&no_emoji, SetKind::Emoji),
            Err(PublishError::EmojiInvalid { index: 1 })
        );
        let tgs_for_emoji = Edit {
            cover: Some(EditCover::Tgs(vec![1])),
            ..base.clone()
        };
        assert_eq!(
            validate_edit(&tgs_for_emoji, SetKind::Emoji),
            Err(PublishError::CoverInvalid)
        );
        let big = Edit {
            cover: Some(EditCover::Tgs(vec![0; 32 * 1024 + 1])),
            ..base.clone()
        };
        assert_eq!(
            validate_edit(&big, SetKind::Stickers),
            Err(PublishError::ThumbTooBig {
                bytes: 32 * 1024 + 1,
                max: 32 * 1024
            })
        );
        let deleted_cover = Edit {
            delete: ids("a"),
            order: ids("cb"),
            cover: Some(EditCover::Sticker("a".into())),
            ..base
        };
        assert_eq!(
            validate_edit(&deleted_cover, SetKind::Emoji),
            Err(PublishError::CoverInvalid)
        );
    }
}
