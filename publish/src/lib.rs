//! Выгрузка готовых TGS в набор стикеров или эмодзи Telegram через бота
//! пользователя (Bot API).
//!
//! Сеть — только здесь и только к `api.telegram.org` ([`http::API_HOST`]).
//! Крейт ничего не делает сам: запросы уходят, только когда программа
//! вызывает [`Bot`] по явному действию пользователя. Ошибки — кодами
//! ([`PublishError`]), тексты для людей — в интерфейсе и CLI.

pub mod api;
pub mod error;
pub mod http;
pub mod job;
pub mod rules;

pub use api::{Bot, BotInfo, Field, Form, NewSticker, Owner, Pause, SetInfo, Transport};
pub use error::{Failure, PublishError};
pub use http::HttpTransport;
pub use job::{run, validate, Done, Item, Job, Progress, Source};
pub use rules::SetKind;

/// Бот по токену, с настоящим HTTPS.
pub fn connect(token: &str) -> Bot<HttpTransport> {
    Bot::new(HttpTransport::new(token))
}
