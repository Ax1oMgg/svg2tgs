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

/// Набор в Telegram — столько, сколько нужно для продолжения выгрузки.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetInfo {
    pub title: String,
    pub count: usize,
    /// Тип набора (`sticker_type`); `None` — Telegram не сообщил.
    pub kind: Option<SetKind>,
}

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
const RATE_LIMIT_RETRIES: usize = 5;

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
    pub fn upload(&self, owner: i64, filename: &str, bytes: Vec<u8>) -> Result<String, Failure> {
        let v = self.call(
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
    ) -> Result<(), Failure> {
        let list: Vec<Value> = stickers.iter().map(NewSticker::json).collect();
        self.call(
            "createNewStickerSet",
            vec![
                ("user_id".into(), text(owner)),
                ("name".into(), text(name)),
                ("title".into(), text(title)),
                ("sticker_type".into(), text(kind.api_type())),
                ("stickers".into(), text(Value::Array(list))),
            ],
        )
        .map(drop)
    }

    /// `addStickerToSet`.
    pub fn add_sticker(&self, owner: i64, name: &str, sticker: &NewSticker) -> Result<(), Failure> {
        self.call(
            "addStickerToSet",
            vec![
                ("user_id".into(), text(owner)),
                ("name".into(), text(name)),
                ("sticker".into(), text(sticker.json())),
            ],
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
}
