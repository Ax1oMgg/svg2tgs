//! Ошибки выгрузки — кодами с параметрами. Текст для людей делают
//! интерфейс и CLI (как с замечаниями ядра).

use serde::Serialize;

/// Отказ одного запроса к Bot API.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    /// Не удалось связаться с Telegram (нет сети, обрыв, тайм-аут).
    #[error("сеть: {0}")]
    Network(String),
    /// Telegram ответил `ok: false`.
    #[error("Telegram {code}: {description}")]
    Api {
        code: i64,
        description: String,
        /// Сколько секунд подождать (ответ 429).
        retry_after: Option<u64>,
    },
    /// Ответ не похож на ответ Bot API: HTTP-код и начало ответа.
    #[error("непонятный ответ: {0}")]
    Unexpected(String),
    /// Прервано во время ожидания, о котором просил Telegram.
    #[error("остановлено")]
    Stopped,
}

/// Почему выгрузка не удалась.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum PublishError {
    /// Telegram не принял токен бота.
    #[error("invalid_token")]
    InvalidToken,
    /// Владелец не найден: боту ещё не написали /start.
    #[error("no_owner")]
    NoOwner,
    /// У бота включён webhook — `getUpdates` недоступен.
    #[error("webhook_active")]
    WebhookActive,
    /// Неверное имя набора (правила — в `rules`).
    #[error("name_invalid")]
    NameInvalid { name: String },
    /// Имя набора занято.
    #[error("name_taken")]
    NameTaken { name: String },
    /// Неверное название.
    #[error("title_invalid")]
    TitleInvalid,
    /// Нечего выгружать.
    #[error("nothing")]
    Nothing,
    /// Файлов больше, чем помещается в набор.
    #[error("too_many")]
    TooMany { count: usize, max: usize },
    /// У стикера нет эмодзи или их больше 20.
    #[error("emoji_invalid")]
    EmojiInvalid { index: usize },
    /// В наборе уже больше файлов, чем выгружаем: это другой набор.
    #[error("set_mismatch")]
    SetMismatch {
        name: String,
        existing: usize,
        planned: usize,
    },
    /// Набор заполнен.
    #[error("set_full")]
    SetFull { done: usize },
    /// Telegram не принял файл.
    #[error("file_rejected")]
    FileRejected { index: usize, detail: String },
    /// Нет связи. Уже выгруженное осталось в наборе — можно продолжить.
    #[error("network")]
    Network { done: usize, detail: String },
    /// Остановлено пользователем.
    #[error("stopped")]
    Stopped { done: usize },
    /// Прочий отказ Telegram — с его текстом, как есть.
    #[error("telegram")]
    Telegram { status: i64, detail: String },
    /// Не удалось прочитать файл с диска.
    #[error("read_failed")]
    ReadFailed { index: usize, detail: String },
    /// Дописать некуда: набора с таким адресом нет.
    #[error("set_missing")]
    SetMissing { name: String },
    /// Дописываем стикеры в набор эмодзи или наоборот.
    #[error("kind_mismatch")]
    KindMismatch { name: String },
    /// Ответ не похож на ответ Bot API (например, страница прокси вместо
    /// Telegram): HTTP-код и начало ответа.
    #[error("unexpected")]
    Unexpected { detail: String },
}

impl PublishError {
    pub(crate) fn unexpected(detail: impl std::fmt::Display) -> Self {
        Self::Unexpected {
            detail: detail.to_string(),
        }
    }

    /// Общее толкование отказа; `done` — сколько уже выгружено.
    pub fn from_failure(f: Failure, done: usize) -> Self {
        match f {
            Failure::Network(detail) => Self::Network { done, detail },
            Failure::Stopped => Self::Stopped { done },
            Failure::Unexpected(detail) => Self::Unexpected { detail },
            Failure::Api {
                code: 401 | 404, ..
            } => Self::InvalidToken,
            Failure::Api {
                code, description, ..
            } => {
                if description.contains("STICKERS_TOO_MUCH") {
                    Self::SetFull { done }
                } else {
                    Self::Telegram {
                        status: code,
                        detail: description,
                    }
                }
            }
        }
    }

    pub(crate) fn from_token(f: Failure) -> Self {
        Self::from_failure(f, 0)
    }

    pub(crate) fn from_updates(f: Failure) -> Self {
        match f {
            Failure::Api { code: 409, .. } => Self::WebhookActive,
            other => Self::from_failure(other, 0),
        }
    }

    /// Сколько выгружено к моменту ошибки — для продолжения.
    pub fn done(&self) -> Option<usize> {
        match self {
            Self::SetFull { done } | Self::Network { done, .. } | Self::Stopped { done } => {
                Some(*done)
            }
            _ => None,
        }
    }
}
