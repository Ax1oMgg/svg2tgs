//! Методы Bot API, которые нужны для наборов. Сам обмен с Telegram — через
//! [`Transport`]: в программе это HTTPS к `api.telegram.org`
//! ([`crate::HttpTransport`]), в тестах — заглушка без сети.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{Failure, PublishError};
use crate::rules::SetKind;

/// Поле запроса: текст или файл.
#[derive(Clone, Debug)]
pub enum Field {
    Text(String),
    File { filename: String, bytes: Vec<u8> },
}

/// Запрос к методу: поля формы по порядку.
pub type Form = Vec<(String, Field)>;

/// Вызов метода Bot API. Возвращает поле `result` ответа или отказ.
pub trait Transport {
    fn call(&self, method: &str, form: Form) -> Result<Value, Failure>;

    /// Скачать файл по `file_path` из ответа `getFile`.
    fn download(&self, file_path: &str) -> Result<Vec<u8>, Failure> {
        Err(Failure::Unexpected(format!(
            "скачивание недоступно: {file_path}"
        )))
    }
}

/// Бот, через которого создаются наборы.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BotInfo {
    pub id: i64,
    pub username: String,
}

/// Человек, которому будут принадлежать наборы (написал боту /start).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct Owner {
    pub id: i64,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub first_name: String,
}

/// Набор в Telegram: для продолжения выгрузки и для правки набора.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetInfo {
    pub title: String,
    pub count: usize,
    /// Тип набора (`sticker_type`); `None` — Telegram не сообщил.
    pub kind: Option<SetKind>,
    /// Стикеры по порядку.
    pub stickers: Vec<StickerInfo>,
}

/// Стикер набора.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerInfo {
    /// Идентификатор файла: по нему стикер двигают, правят и удаляют.
    pub file_id: String,
    /// Постоянный идентификатор (у `file_id` он может меняться).
    pub file_unique_id: String,
    /// Эмодзи стикера. `getStickerSet` отдаёт только первый из списка.
    pub emoji: Option<String>,
    /// Для набора эмодзи — идентификатор эмодзи (нужен для обложки).
    pub custom_emoji_id: Option<String>,
    /// Анимированный (TGS); другие форматы программа не создаёт.
    pub is_animated: bool,
}

impl StickerInfo {
    fn from_json(v: &Value) -> Self {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        Self {
            file_id: s("file_id").unwrap_or_default(),
            file_unique_id: s("file_unique_id").unwrap_or_default(),
            emoji: s("emoji"),
            custom_emoji_id: s("custom_emoji_id"),
            is_animated: v.get("is_animated").and_then(Value::as_bool) == Some(true),
        }
    }
}

/// Самый большой файл, который программа скачивает (стикер — до 64 КБ).
pub const DOWNLOAD_MAX: usize = 1024 * 1024;

/// Стикер для `createNewStickerSet` и `addStickerToSet`.
#[derive(Clone, Debug)]
pub struct NewSticker {
    pub file_id: String,
    pub emoji: Vec<String>,
}

impl NewSticker {
    fn json(&self) -> Value {
        serde_json::json!({
            "sticker": self.file_id,
            "format": "animated",
            "emoji_list": self.emoji,
        })
    }
}

/// Сколько раз повторять запрос, если Telegram попросил подождать (429).
/// Только для служебных запросов; запросы выгрузки ждут сколько угодно,
/// но через [`Pause`], которую можно прервать.
const RATE_LIMIT_RETRIES: usize = 5;

/// Что делать, когда Telegram просит подождать `secs` секунд (ответ 429):
/// переждать и вернуть `true` — запрос повторится; `false` — прервать,
/// запрос вернёт [`Failure::Stopped`].
pub type Pause<'a> = &'a mut dyn FnMut(u64) -> bool;

/// Клиент Bot API поверх транспорта.
pub struct Bot<T: Transport> {
    transport: T,
    /// Ожидание перед повтором; в тестах — без настоящего сна.
    sleep: Box<dyn Fn(Duration) + Send + Sync>,
}

fn text(v: impl ToString) -> Field {
    Field::Text(v.to_string())
}

impl<T: Transport> Bot<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            sleep: Box::new(std::thread::sleep),
        }
    }

    /// Заменить ожидание при повторах (для тестов).
    pub fn with_sleep(mut self, sleep: impl Fn(Duration) + Send + Sync + 'static) -> Self {
        self.sleep = Box::new(sleep);
        self
    }

    /// Подождать (в тестах — без настоящего сна).
    pub fn nap(&self, d: Duration) {
        (self.sleep)(d)
    }

    /// Вызов с повтором, пока Telegram просит подождать, а `pause` согласна ждать.
    fn call_paced(&self, method: &str, form: Form, pause: Pause) -> Result<Value, Failure> {
        loop {
            match self.transport.call(method, form.clone()) {
                Err(Failure::Api {
                    retry_after: Some(secs),
                    ..
                }) => {
                    if !pause(secs.max(1)) {
                        return Err(Failure::Stopped);
                    }
                }
                other => return other,
            }
        }
    }

    /// Вызов с повтором, если Telegram просит подождать.
    fn call(&self, method: &str, form: Form) -> Result<Value, Failure> {
        let mut tries = 0;
        loop {
            match self.transport.call(method, form.clone()) {
                Err(Failure::Api {
                    retry_after: Some(secs),
                    ..
                }) if tries < RATE_LIMIT_RETRIES => {
                    tries += 1;
                    (self.sleep)(Duration::from_secs(secs.max(1)));
                }
                other => return other,
            }
        }
    }

    /// `getMe`: проверка токена и имя бота.
    pub fn me(&self) -> Result<BotInfo, PublishError> {
        let v = self
            .call("getMe", vec![])
            .map_err(PublishError::from_token)?;
        serde_json::from_value(v.clone()).map_err(|e| {
            PublishError::unexpected(format!(
                "getMe: {e}: {}",
                crate::http::snippet(&v.to_string())
            ))
        })
    }

    /// `getUpdates`: последний, кто написал боту /start.
    pub fn find_owner(&self) -> Result<Option<Owner>, PublishError> {
        let v = self
            .call(
                "getUpdates",
                vec![("allowed_updates".into(), text(r#"["message"]"#))],
            )
            .map_err(PublishError::from_updates)?;
        let updates = v.as_array().cloned().unwrap_or_default();
        let owner = updates.iter().rev().find_map(|u| {
            let msg = u.get("message")?;
            let txt = msg.get("text")?.as_str()?;
            let is_private = msg.pointer("/chat/type")?.as_str()? == "private";
            if is_private && (txt == "/start" || txt.starts_with("/start ")) {
                serde_json::from_value::<Owner>(msg.get("from")?.clone()).ok()
            } else {
                None
            }
        });
        Ok(owner)
    }

    /// `uploadStickerFile`: файл TGS на сервер Telegram, возвращает `file_id`.
    pub fn upload(
        &self,
        owner: i64,
        filename: &str,
        bytes: Vec<u8>,
        pause: Pause,
    ) -> Result<String, Failure> {
        let v = self.call_paced(
            "uploadStickerFile",
            vec![
                ("user_id".into(), text(owner)),
                ("sticker_format".into(), text("animated")),
                (
                    "sticker".into(),
                    Field::File {
                        filename: filename.into(),
                        bytes,
                    },
                ),
            ],
            pause,
        )?;
        v.get("file_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                Failure::Unexpected(format!(
                    "uploadStickerFile без file_id: {}",
                    crate::http::snippet(&v.to_string())
                ))
            })
    }

    /// `createNewStickerSet`.
    pub fn create_set(
        &self,
        owner: i64,
        name: &str,
        title: &str,
        kind: SetKind,
        stickers: &[NewSticker],
        pause: Pause,
    ) -> Result<(), Failure> {
        let list: Vec<Value> = stickers.iter().map(NewSticker::json).collect();
        self.call_paced(
            "createNewStickerSet",
            vec![
                ("user_id".into(), text(owner)),
                ("name".into(), text(name)),
                ("title".into(), text(title)),
                ("sticker_type".into(), text(kind.api_type())),
                ("stickers".into(), text(Value::Array(list))),
            ],
            pause,
        )
        .map(drop)
    }

    /// `addStickerToSet`.
    pub fn add_sticker(
        &self,
        owner: i64,
        name: &str,
        sticker: &NewSticker,
        pause: Pause,
    ) -> Result<(), Failure> {
        self.call_paced(
            "addStickerToSet",
            vec![
                ("user_id".into(), text(owner)),
                ("name".into(), text(name)),
                ("sticker".into(), text(sticker.json())),
            ],
            pause,
        )
        .map(drop)
    }

    /// `getStickerSet`: что уже есть в наборе; `None` — набора нет.
    pub fn get_set(&self, name: &str) -> Result<Option<SetInfo>, Failure> {
        match self.call("getStickerSet", vec![("name".into(), text(name))]) {
            Ok(v) => Ok(Some(SetInfo {
                title: v
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                count: v
                    .get("stickers")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
                stickers: v
                    .get("stickers")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(StickerInfo::from_json).collect())
                    .unwrap_or_default(),
                kind: match v.get("sticker_type").and_then(Value::as_str) {
                    Some("regular") => Some(SetKind::Stickers),
                    Some("custom_emoji") => Some(SetKind::Emoji),
                    _ => None,
                },
            })),
            Err(Failure::Api { description, .. }) if description.contains("STICKERSET_INVALID") => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
    /// `setStickerPositionInSet`: передвинуть стикер на место `position` (с 0).
    pub fn set_position(
        &self,
        file_id: &str,
        position: usize,
        pause: Pause,
    ) -> Result<(), Failure> {
        self.call_paced(
            "setStickerPositionInSet",
            vec![
                ("sticker".into(), text(file_id)),
                ("position".into(), text(position)),
            ],
            pause,
        )
        .map(drop)
    }

    /// `setStickerEmojiList`: заменить эмодзи стикера (1–20).
    pub fn set_emoji(&self, file_id: &str, emoji: &[String], pause: Pause) -> Result<(), Failure> {
        self.call_paced(
            "setStickerEmojiList",
            vec![
                ("sticker".into(), text(file_id)),
                ("emoji_list".into(), text(serde_json::json!(emoji))),
            ],
            pause,
        )
        .map(drop)
    }

    /// `deleteStickerFromSet`: убрать стикер из набора (не отменить).
    pub fn delete_sticker(&self, file_id: &str, pause: Pause) -> Result<(), Failure> {
        self.call_paced(
            "deleteStickerFromSet",
            vec![("sticker".into(), text(file_id))],
            pause,
        )
        .map(drop)
    }

    /// `setStickerSetThumbnail`: обложка набора стикеров — файл TGS
    /// (до [`crate::rules::THUMB_TGS_MAX`]).
    pub fn set_sticker_thumbnail(
        &self,
        owner: i64,
        name: &str,
        tgs: Vec<u8>,
        pause: Pause,
    ) -> Result<(), Failure> {
        self.call_paced(
            "setStickerSetThumbnail",
            vec![
                ("name".into(), text(name)),
                ("user_id".into(), text(owner)),
                ("format".into(), text("animated")),
                (
                    "thumbnail".into(),
                    Field::File {
                        filename: "cover.tgs".into(),
                        bytes: tgs,
                    },
                ),
            ],
            pause,
        )
        .map(drop)
    }

    /// `setCustomEmojiStickerSetThumbnail`: обложка набора эмодзи — один из
    /// его эмодзи.
    pub fn set_emoji_thumbnail(
        &self,
        name: &str,
        custom_emoji_id: &str,
        pause: Pause,
    ) -> Result<(), Failure> {
        self.call_paced(
            "setCustomEmojiStickerSetThumbnail",
            vec![
                ("name".into(), text(name)),
                ("custom_emoji_id".into(), text(custom_emoji_id)),
            ],
            pause,
        )
        .map(drop)
    }

    /// `getFile` и скачивание: файл стикера (для превью набора и обложки).
    pub fn download_sticker(&self, file_id: &str) -> Result<Vec<u8>, Failure> {
        let v = self.call("getFile", vec![("file_id".into(), text(file_id))])?;
        let path = v
            .get("file_path")
            .and_then(Value::as_str)
            .filter(|p| file_path_is_safe(p))
            .ok_or_else(|| {
                Failure::Unexpected(format!(
                    "getFile без file_path: {}",
                    crate::http::snippet(&v.to_string())
                ))
            })?;
        let bytes = self.transport.download(path)?;
        if bytes.len() > DOWNLOAD_MAX {
            return Err(Failure::Unexpected(format!(
                "файл {} байт больше {DOWNLOAD_MAX}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }
}

/// `file_path` из ответа Telegram — только обычный относительный путь
/// (`stickers/file_1.tgs`): без `..`, схемы, запроса и пробелов.
pub(crate) fn file_path_is_safe(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 256
        && !p.starts_with('/')
        && p.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
        && p.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::file_path_is_safe;

    #[test]
    fn only_plain_file_paths_are_downloaded() {
        assert!(file_path_is_safe("stickers/file_12.tgs"));
        for bad in [
            "",
            "/etc/passwd",
            "a/../b",
            "../x",
            "a//b",
            "a?b=1",
            "http://x/y",
            "a b",
            "a/./b",
        ] {
            assert!(!file_path_is_safe(bad), "{bad}");
        }
    }
}
