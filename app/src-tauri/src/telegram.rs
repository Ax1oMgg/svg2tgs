//! Выгрузка в Telegram: команды для интерфейса.
//!
//! Правило «без сети, пока не включено» проверяется здесь, в Rust: любая
//! команда, которой нужна сеть, сначала смотрит переключатель из
//! `telegram.json` и без него до транспорта не доходит. Токен бота —
//! в системном хранилище паролей, в файлах его нет.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use svg2tgs_publish::{
    rules, Bot, HttpTransport, Item, Job, Owner, PublishError, SetKind, Source, Transport,
};

/// Что программа помнит о выгрузке (без токена).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Переключатель в «Настройках». Выключен — в сеть ни шагу.
    pub enabled: bool,
    /// Имя бота (`tea_stickers_bot`), если токен проверен.
    pub bot: Option<String>,
    /// Владелец наборов, если он написал боту /start.
    pub owner: Option<Owner>,
}

/// Хранилище токена.
pub trait TokenStore: Send + Sync {
    fn get(&self) -> Option<String>;
    fn set(&self, token: &str) -> Result<(), String>;
    fn delete(&self);
}

/// Системное хранилище паролей: Диспетчер учётных данных Windows,
/// Связка ключей macOS, Secret Service в Linux.
pub struct Keyring;

const SERVICE: &str = "svg2tgs";
const ACCOUNT: &str = "telegram-bot-token";

impl TokenStore for Keyring {
    fn get(&self) -> Option<String> {
        keyring::Entry::new(SERVICE, ACCOUNT)
            .ok()?
            .get_password()
            .ok()
    }
    fn set(&self, token: &str) -> Result<(), String> {
        keyring::Entry::new(SERVICE, ACCOUNT)
            .and_then(|e| e.set_password(token))
            .map_err(|e| e.to_string())
    }
    fn delete(&self) {
        if let Ok(e) = keyring::Entry::new(SERVICE, ACCOUNT) {
            let _ = e.delete_credential();
        }
    }
}

/// Ошибка для интерфейса: свои коды плюс коды выгрузки (`PublishError`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum TgError {
    /// Выгрузка выключена в «Настройках».
    Disabled,
    /// Бот не подключён (нет токена).
    NoToken,
    /// Не удалось сохранить токен в системном хранилище.
    Storage { detail: String },
    /// Файл ещё не сконвертирован (или с ошибкой).
    NotConverted { index: usize },
    #[serde(untagged)]
    Publish(PublishError),
}

impl From<PublishError> for TgError {
    fn from(e: PublishError) -> Self {
        Self::Publish(e)
    }
}

/// Состояние для интерфейса — без сети.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub bot: Option<String>,
    /// Как назвать владельца: `@ник` или имя.
    pub owner: Option<String>,
}

pub struct Telegram {
    config: Mutex<Config>,
    path: Option<PathBuf>,
    store: Box<dyn TokenStore>,
    /// «Остановить» во время выгрузки.
    pub stop: Arc<AtomicBool>,
}

fn owner_label(o: &Owner) -> String {
    match &o.username {
        Some(u) => format!("@{u}"),
        None if !o.first_name.is_empty() => o.first_name.clone(),
        None => o.id.to_string(),
    }
}

impl Telegram {
    /// Состояние из файла `path` (нет файла — всё выключено).
    pub fn load(path: Option<PathBuf>, store: Box<dyn TokenStore>) -> Self {
        let config = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            config: Mutex::new(config),
            path,
            store,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    fn save(&self, c: &Config) {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, serde_json::to_string_pretty(c).unwrap_or_default());
        }
    }

    fn update(&self, f: impl FnOnce(&mut Config)) -> Status {
        let mut c = self.config.lock().unwrap();
        f(&mut c);
        self.save(&c);
        status(&c)
    }

    pub fn status(&self) -> Status {
        status(&self.config.lock().unwrap())
    }

    pub fn config(&self) -> Config {
        self.config.lock().unwrap().clone()
    }

    pub fn set_enabled(&self, enabled: bool) -> Status {
        self.update(|c| c.enabled = enabled)
    }

    /// Забыть бота: токен из хранилища, имя бота и владельца.
    pub fn disconnect(&self) -> Status {
        self.store.delete();
        self.update(|c| {
            c.bot = None;
            c.owner = None;
        })
    }

    /// Бот для запросов — только если выгрузка включена и есть токен.
    /// `make` создаёт транспорт; при выключенной выгрузке он не вызывается.
    pub fn bot_with<T: Transport>(
        &self,
        token: Option<&str>,
        make: impl FnOnce(&str) -> T,
    ) -> Result<Bot<T>, TgError> {
        if !self.config.lock().unwrap().enabled {
            return Err(TgError::Disabled);
        }
        let saved;
        let token = match token {
            Some(t) => t,
            None => {
                saved = self.store.get().ok_or(TgError::NoToken)?;
                saved.as_str()
            }
        };
        Ok(Bot::new(make(token)))
    }

    /// Проверить токен (`getMe`) и запомнить его. Владелец сбрасывается.
    pub fn check_token<T: Transport>(
        &self,
        token: &str,
        make: impl FnOnce(&str) -> T,
    ) -> Result<Status, TgError> {
        let token = token.trim();
        let me = self.bot_with(Some(token), make)?.me()?;
        self.store
            .set(token)
            .map_err(|detail| TgError::Storage { detail })?;
        Ok(self.update(|c| {
            c.bot = Some(me.username);
            c.owner = None;
        }))
    }

    /// Найти владельца по /start (`getUpdates`).
    pub fn find_owner<T: Transport>(
        &self,
        make: impl FnOnce(&str) -> T,
    ) -> Result<Status, TgError> {
        let owner = self.bot_with(None, make)?.find_owner()?;
        Ok(match owner {
            Some(o) => self.update(|c| c.owner = Some(o)),
            None => self.status(),
        })
    }

    /// Начало адреса из названия.
    pub fn stem(&self, title: &str) -> String {
        let bot = self.config.lock().unwrap().bot.clone().unwrap_or_default();
        rules::stem_from_title(title, &bot)
    }
}

fn status(c: &Config) -> Status {
    Status {
        enabled: c.enabled,
        bot: c.bot.clone(),
        owner: c.owner.as_ref().map(owner_label),
    }
}

/// Один стикер из интерфейса: файл списка и, при нарезке, номер клетки.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishItem {
    pub id: String,
    #[serde(default)]
    pub tile: Option<usize>,
    /// Имя файла для Telegram.
    pub name: String,
    pub emoji: Vec<String>,
}

/// Задание выгрузки из интерфейса.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishRequest {
    pub kind: SetKind,
    pub title: String,
    /// Начало адреса (без `_by_<бот>`).
    pub stem: String,
    #[serde(default)]
    pub resume: bool,
    pub items: Vec<PublishItem>,
}

/// Задание для `publish` из запроса и готовых TGS (`tgs(id, tile)`).
pub fn build_job(
    req: &PublishRequest,
    bot: &str,
    tgs: impl Fn(&str, usize) -> Option<Vec<u8>>,
) -> Result<Job, TgError> {
    let items = req
        .items
        .iter()
        .enumerate()
        .map(|(index, it)| {
            let bytes = tgs(&it.id, it.tile.unwrap_or(0)).ok_or(TgError::NotConverted { index })?;
            Ok(Item {
                filename: format!("{}.tgs", it.name),
                source: Source::Bytes(bytes),
                emoji: it.emoji.clone(),
            })
        })
        .collect::<Result<Vec<_>, TgError>>()?;
    Ok(Job {
        kind: req.kind,
        name: rules::set_name(req.stem.trim(), bot),
        title: req.title.trim().to_string(),
        items,
        resume: req.resume,
    })
}

/// Настоящий транспорт.
pub fn http(token: &str) -> HttpTransport {
    HttpTransport::new(token)
}

/// Ссылка на набор — только для имени, которое проходит правила Telegram.
pub fn set_link(kind: SetKind, name: &str, bot: &str) -> Option<String> {
    rules::name_is_valid(name, bot).then(|| kind.link(name))
}

pub fn reset_stop(stop: &AtomicBool) {
    stop.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use svg2tgs_publish::{Failure, Form};

    #[derive(Default)]
    struct Memory(Mutex<Option<String>>);
    impl TokenStore for Memory {
        fn get(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
        fn set(&self, t: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(t.into());
            Ok(())
        }
        fn delete(&self) {
            *self.0.lock().unwrap() = None;
        }
    }

    /// Транспорт, который считает вызовы.
    struct Counting(Arc<AtomicUsize>);
    impl Transport for Counting {
        fn call(&self, method: &str, _: Form) -> Result<serde_json::Value, Failure> {
            self.0.fetch_add(1, Ordering::Relaxed);
            match method {
                "getMe" => Ok(serde_json::json!({"id": 1, "username": "tea_stickers_bot"})),
                "getUpdates" => Ok(serde_json::json!([{"message": {
                    "text": "/start", "chat": {"type": "private"},
                    "from": {"id": 7, "first_name": "Гриша", "username": "nickname"}}}])),
                _ => Err(Failure::Unexpected("test".into())),
            }
        }
    }

    fn tg() -> Telegram {
        Telegram::load(None, Box::new(Memory::default()))
    }

    #[test]
    fn disabled_means_no_requests_at_all() {
        let t = tg();
        let made = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let m = made.clone();
        let c = calls.clone();
        let r = t.check_token("123:abc", |_| {
            m.fetch_add(1, Ordering::Relaxed);
            Counting(c)
        });
        assert_eq!(r, Err(TgError::Disabled));
        let c = calls.clone();
        assert_eq!(t.find_owner(|_| Counting(c)), Err(TgError::Disabled));
        assert_eq!(made.load(Ordering::Relaxed), 0);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn connect_flow() {
        let t = tg();
        t.set_enabled(true);
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        assert_eq!(t.find_owner(|_| Counting(c)), Err(TgError::NoToken));
        let c = calls.clone();
        let s = t.check_token(" 123:abc \n", |tok| {
            assert_eq!(tok, "123:abc");
            Counting(c)
        });
        assert_eq!(
            s,
            Ok(Status {
                enabled: true,
                bot: Some("tea_stickers_bot".into()),
                owner: None
            })
        );
        assert_eq!(t.store.get().as_deref(), Some("123:abc"));
        let c = calls.clone();
        let s = t.find_owner(|tok| {
            assert_eq!(tok, "123:abc");
            Counting(c)
        });
        assert_eq!(s.unwrap().owner.as_deref(), Some("@nickname"));
        assert_eq!(t.stem("Сердечки"), "serdechki");
        let s = t.disconnect();
        assert_eq!(s.bot, None);
        assert_eq!(t.store.get(), None);
        assert!(s.enabled);
    }

    #[test]
    fn config_is_saved_without_token() {
        let dir = std::env::temp_dir().join(format!("svg2tgs-tg-{}", std::process::id()));
        let path = dir.join("telegram.json");
        let t = Telegram::load(Some(path.clone()), Box::new(Memory::default()));
        t.set_enabled(true);
        let c = Arc::new(AtomicUsize::new(0));
        t.check_token("123:secret", |_| Counting(c)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("secret"), "{text}");
        let again = Telegram::load(Some(path), Box::new(Memory::default()));
        assert_eq!(again.status().bot.as_deref(), Some("tea_stickers_bot"));
        assert!(again.status().enabled);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn job_is_built_from_tiles() {
        let req = PublishRequest {
            kind: SetKind::Emoji,
            title: " Сердечки ".into(),
            stem: "serdechki".into(),
            resume: false,
            items: vec![
                PublishItem {
                    id: "a".into(),
                    tile: Some(1),
                    name: "heart_02".into(),
                    emoji: vec!["❤️".into()],
                },
                PublishItem {
                    id: "b".into(),
                    tile: None,
                    name: "x".into(),
                    emoji: vec!["⭐".into()],
                },
            ],
        };
        let job = build_job(&req, "tea_stickers_bot", |id, tile| {
            (id == "a").then(|| vec![tile as u8])
        });
        assert_eq!(job.unwrap_err(), TgError::NotConverted { index: 1 });
        let job = build_job(&req, "tea_stickers_bot", |_, tile| Some(vec![tile as u8])).unwrap();
        assert_eq!(job.name, "serdechki_by_tea_stickers_bot");
        assert_eq!(job.title, "Сердечки");
        assert_eq!(job.items[0].filename, "heart_02.tgs");
        assert!(matches!(&job.items[0].source, Source::Bytes(b) if b == &vec![1]));
    }

    #[test]
    fn errors_have_codes() {
        let v = serde_json::to_value(TgError::Disabled).unwrap();
        assert_eq!(v, serde_json::json!({"code": "disabled"}));
        let v = serde_json::to_value(TgError::Publish(PublishError::SetFull { done: 3 })).unwrap();
        assert_eq!(v, serde_json::json!({"code": "set_full", "done": 3}));
    }

    #[test]
    fn links_only_for_valid_names() {
        assert_eq!(
            set_link(SetKind::Emoji, "a_by_tea_stickers_bot", "tea_stickers_bot").as_deref(),
            Some("https://t.me/addemoji/a_by_tea_stickers_bot")
        );
        assert_eq!(
            set_link(SetKind::Emoji, "a/../../x", "tea_stickers_bot"),
            None
        );
    }
}
