//! Выгрузка в Telegram: команды для интерфейса.
//!
//! Правило «без сети, пока не включено» проверяется здесь, в Rust: любая
//! команда, которой нужна сеть, сначала смотрит переключатель из
//! `telegram.json` и без него до транспорта не доходит. Токен бота —
//! в системном хранилище паролей, в файлах его нет.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use svg2tgs_publish::{
    rules, Bot, Cover, Edit, EditCover, HttpTransport, Item, Job, Owner, PublishError, SetKind,
    Source, StickerInfo, Transport,
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
    /// Наборы, созданные или дополненные программой, — подсказки для
    /// «Дописать» (Bot API не умеет отдавать список наборов бота).
    pub sets: Vec<KnownSet>,
}

/// Набор, в который программа уже выгружала.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownSet {
    pub name: String,
    pub title: String,
    pub kind: SetKind,
}

/// Сколько наборов помнить.
const KNOWN_SETS: usize = 30;

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

/// Откуда запрос и что его разрешает.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Обычная работа (выгрузка): только при сохранённом включении.
    Saved,
    /// Подключение бота в открытых «Настройках», где переключатель включён,
    /// но ещё не сохранён «Готово»: проверка токена и поиск /start — явные
    /// действия пользователя в этом окне.
    Settings,
}

/// Состояние для интерфейса — без сети.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub bot: Option<String>,
    /// Как назвать владельца: `@ник` или имя.
    pub owner: Option<String>,
    /// Наборы этого бота, в которые программа уже выгружала (новые — первыми).
    pub sets: Vec<KnownSet>,
}

/// Набор в Telegram — для «Дописать» и окна «Набор».
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SetView {
    pub name: String,
    pub title: String,
    pub count: usize,
    pub kind: Option<SetKind>,
    pub stickers: Vec<StickerInfo>,
}

pub struct Telegram {
    config: Mutex<Config>,
    path: Option<PathBuf>,
    store: Box<dyn TokenStore>,
    /// «Остановить» во время выгрузки.
    pub stop: Arc<AtomicBool>,
    /// Скачанные стикеры наборов (`file_id` → TGS): превью в окне «Набор»
    /// и обложка из стикера. Только в памяти.
    files: Mutex<HashMap<String, Vec<u8>>>,
}

/// Сколько скачанных стикеров держать в памяти (два полных набора эмодзи).
const FILES_CACHED: usize = 400;

/// `file_id` из ответа Telegram: латиница, цифры, `-` и `_`.
fn file_id_is_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
            files: Mutex::new(HashMap::new()),
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

    /// Бот для запросов — только если выгрузка разрешена и есть токен.
    /// `make` создаёт транспорт; без разрешения он не вызывается.
    pub fn bot_with<T: Transport>(
        &self,
        access: Access,
        token: Option<&str>,
        make: impl FnOnce(&str) -> T,
    ) -> Result<Bot<T>, TgError> {
        let enabled = self.config.lock().unwrap().enabled;
        if !enabled && access != Access::Settings {
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
        access: Access,
        token: &str,
        make: impl FnOnce(&str) -> T,
    ) -> Result<Status, TgError> {
        let token = token.trim();
        let me = self.bot_with(access, Some(token), make)?.me()?;
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
        access: Access,
        make: impl FnOnce(&str) -> T,
    ) -> Result<Status, TgError> {
        let owner = self.bot_with(access, None, make)?.find_owner()?;
        Ok(match owner {
            Some(o) => self.update(|c| c.owner = Some(o)),
            None => self.status(),
        })
    }

    /// Запомнить набор после выгрузки (новые — первыми, без повторов).
    pub fn remember(&self, set: KnownSet) -> Status {
        self.update(|c| {
            c.sets.retain(|s| !s.name.eq_ignore_ascii_case(&set.name));
            c.sets.insert(0, set);
            c.sets.truncate(KNOWN_SETS);
        })
    }

    /// Набор по началу адреса (`getStickerSet`) — для «Дописать».
    pub fn set_info<T: Transport>(
        &self,
        stem: &str,
        make: impl FnOnce(&str) -> T,
    ) -> Result<SetView, TgError> {
        let bot_name = self.config().bot.ok_or(TgError::NoToken)?;
        let name = rules::set_name(stem.trim(), &bot_name);
        let set = self
            .bot_with(Access::Saved, None, make)?
            .get_set(&name)
            .map_err(|f| PublishError::from_failure(f, 0))?
            .ok_or(PublishError::SetMissing { name: name.clone() })?;
        Ok(SetView {
            name,
            title: set.title,
            count: set.count,
            kind: set.kind,
            stickers: set.stickers,
        })
    }

    /// Файл стикера (TGS) по `file_id`: из памяти или скачать
    /// (`getFile` и загрузка с `api.telegram.org`).
    pub fn sticker_file<T: Transport>(
        &self,
        file_id: &str,
        make: impl FnOnce(&str) -> T,
    ) -> Result<Vec<u8>, TgError> {
        if !file_id_is_valid(file_id) {
            return Err(PublishError::CoverInvalid.into());
        }
        if let Some(b) = self.files.lock().unwrap().get(file_id) {
            return Ok(b.clone());
        }
        let bytes = self
            .bot_with(Access::Saved, None, make)?
            .download_sticker(file_id)
            .map_err(|f| PublishError::from_failure(f, 0))?;
        let mut files = self.files.lock().unwrap();
        if files.len() >= FILES_CACHED {
            files.clear();
        }
        files.insert(file_id.to_string(), bytes.clone());
        Ok(bytes)
    }

    /// Начало адреса из названия.
    pub fn stem(&self, title: &str) -> String {
        let bot = self.config.lock().unwrap().bot.clone().unwrap_or_default();
        rules::stem_from_title(title, &bot)
    }
}

fn status(c: &Config) -> Status {
    let suffix = c
        .bot
        .as_deref()
        .map(|b| rules::suffix(b).to_ascii_lowercase());
    Status {
        enabled: c.enabled,
        bot: c.bot.clone(),
        owner: c.owner.as_ref().map(owner_label),
        // Только наборы текущего бота: у другого бота они чужие.
        sets: c
            .sets
            .iter()
            .filter(|s| {
                suffix
                    .as_ref()
                    .is_some_and(|x| s.name.to_ascii_lowercase().ends_with(x))
            })
            .cloned()
            .collect(),
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
    /// Дописать в существующий набор: сколько в нём было до первой попытки.
    #[serde(default)]
    pub append_base: Option<usize>,
    pub items: Vec<PublishItem>,
    /// Обложка — стикер задания с этим номером (с 0); `None` — не менять.
    #[serde(default)]
    pub cover: Option<usize>,
}

/// Задание для `publish` из запроса и готовых TGS. `tgs(id, tile)` — файл
/// клетки; `cover_tgs(id, tile)` — та же клетка, ужатая до предела обложки
/// (нужна, только если обычный файл больше 32 КБ).
pub fn build_job_with_cover(
    req: &PublishRequest,
    bot: &str,
    tgs: impl Fn(&str, usize) -> Option<Vec<u8>>,
    cover_tgs: impl Fn(&str, usize) -> Option<Vec<u8>>,
) -> Result<Job, TgError> {
    let mut job = build_job(req, bot, &tgs)?;
    job.cover = match (req.cover, req.kind) {
        (None, _) => None,
        (Some(i), _) if i >= req.items.len() => return Err(PublishError::CoverInvalid.into()),
        (Some(i), SetKind::Emoji) => Some(Cover::Item(i)),
        (Some(i), SetKind::Stickers) => {
            let small = match &job.items[i].source {
                Source::Bytes(b) => b.len() <= rules::THUMB_TGS_MAX,
                Source::Path(_) => false,
            };
            if small {
                Some(Cover::Item(i))
            } else {
                let it = &req.items[i];
                let bytes = cover_tgs(&it.id, it.tile.unwrap_or(0))
                    .ok_or(TgError::NotConverted { index: i })?;
                Some(Cover::Tgs(bytes))
            }
        }
    };
    Ok(job)
}

/// Стикер, у которого меняются эмодзи.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiChange {
    pub file_id: String,
    pub emoji: Vec<String>,
}

/// Правка набора из окна «Набор».
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    /// Полное имя набора.
    pub name: String,
    pub kind: SetKind,
    /// Стикеры (`file_id`) в порядке, в каком набор был открыт.
    pub seen: Vec<String>,
    #[serde(default)]
    pub delete: Vec<String>,
    /// Оставшиеся — в нужном порядке.
    pub order: Vec<String>,
    #[serde(default)]
    pub emoji: Vec<EmojiChange>,
    /// Обложка — стикер набора; `None` — не менять.
    #[serde(default)]
    pub cover: Option<String>,
}

/// Правка для `publish`. У набора стикеров обложка — файл стикера
/// (`file(file_id)`), он должен быть не больше 32 КБ.
pub fn build_edit(
    req: &EditRequest,
    bot: &str,
    file: impl FnOnce(&str) -> Result<Vec<u8>, TgError>,
) -> Result<Edit, TgError> {
    if !rules::name_is_valid(&req.name, bot) {
        return Err(PublishError::NameInvalid {
            name: req.name.clone(),
        }
        .into());
    }
    let cover = match (&req.cover, req.kind) {
        (None, _) => None,
        (Some(id), SetKind::Emoji) => Some(EditCover::Sticker(id.clone())),
        (Some(id), SetKind::Stickers) => Some(EditCover::Tgs(file(id)?)),
    };
    Ok(Edit {
        name: req.name.clone(),
        seen: req.seen.clone(),
        delete: req.delete.clone(),
        order: req.order.clone(),
        emoji: req
            .emoji
            .iter()
            .map(|e| (e.file_id.clone(), e.emoji.clone()))
            .collect(),
        cover,
    })
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
        append: req.append_base,
        cover: None,
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
        let r = t.check_token(Access::Saved, "123:abc", |_| {
            m.fetch_add(1, Ordering::Relaxed);
            Counting(c)
        });
        assert_eq!(r, Err(TgError::Disabled));
        let c = calls.clone();
        assert_eq!(
            t.find_owner(Access::Saved, |_| Counting(c)),
            Err(TgError::Disabled)
        );
        assert_eq!(made.load(Ordering::Relaxed), 0);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn settings_may_connect_before_done_but_publish_may_not() {
        let t = tg();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        // Переключатель включён в окне, «Готово» ещё не нажата.
        let s = t
            .check_token(Access::Settings, "123:abc", |_| Counting(c))
            .unwrap();
        assert!(!s.enabled);
        assert_eq!(s.bot.as_deref(), Some("tea_stickers_bot"));
        let c = calls.clone();
        assert_eq!(
            t.find_owner(Access::Settings, |_| Counting(c))
                .unwrap()
                .owner
                .as_deref(),
            Some("@nickname")
        );
        // Выгрузка — только после сохранённого включения.
        let c = calls.clone();
        assert!(matches!(
            t.bot_with(Access::Saved, None, |_| Counting(c)),
            Err(TgError::Disabled)
        ));
        t.set_enabled(true);
        let c = calls.clone();
        assert!(t.bot_with(Access::Saved, None, |_| Counting(c)).is_ok());
    }

    #[test]
    fn connect_flow() {
        let t = tg();
        t.set_enabled(true);
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        assert_eq!(
            t.find_owner(Access::Saved, |_| Counting(c)),
            Err(TgError::NoToken)
        );
        let c = calls.clone();
        let s = t.check_token(Access::Saved, " 123:abc \n", |tok| {
            assert_eq!(tok, "123:abc");
            Counting(c)
        });
        assert_eq!(
            s,
            Ok(Status {
                enabled: true,
                bot: Some("tea_stickers_bot".into()),
                owner: None,
                sets: vec![]
            })
        );
        assert_eq!(t.store.get().as_deref(), Some("123:abc"));
        let c = calls.clone();
        let s = t.find_owner(Access::Saved, |tok| {
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
        t.check_token(Access::Saved, "123:secret", |_| Counting(c))
            .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("secret"), "{text}");
        let again = Telegram::load(Some(path), Box::new(Memory::default()));
        assert_eq!(again.status().bot.as_deref(), Some("tea_stickers_bot"));
        assert!(again.status().enabled);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn known_sets_are_per_bot_newest_first() {
        let t = tg();
        t.set_enabled(true);
        let c = Arc::new(AtomicUsize::new(0));
        t.check_token(Access::Saved, "1:a", |_| Counting(c))
            .unwrap();
        let set = |n: &str| KnownSet {
            name: n.into(),
            title: n.into(),
            kind: SetKind::Emoji,
        };
        t.remember(set("a_by_tea_stickers_bot"));
        t.remember(set("b_by_other_bot"));
        let s = t.remember(set("A_by_tea_stickers_bot"));
        let names: Vec<_> = s.sets.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, ["A_by_tea_stickers_bot"]);
        assert_eq!(t.config().sets.len(), 2);
    }

    #[test]
    fn job_is_built_from_tiles() {
        let req = PublishRequest {
            kind: SetKind::Emoji,
            title: " Сердечки ".into(),
            stem: "serdechki".into(),
            resume: false,
            append_base: None,
            cover: None,
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

    fn request(kind: SetKind, cover: Option<usize>) -> PublishRequest {
        PublishRequest {
            kind,
            title: "Т".into(),
            stem: "t".into(),
            resume: false,
            append_base: None,
            cover,
            items: (0..2)
                .map(|i| PublishItem {
                    id: format!("{i}"),
                    tile: None,
                    name: format!("f{i}"),
                    emoji: vec!["⭐".into()],
                })
                .collect(),
        }
    }

    #[test]
    fn cover_for_stickers_is_shrunk_only_when_too_big() {
        let small = |_: &str, _: usize| Some(vec![1; 100]);
        let big = |_: &str, _: usize| Some(vec![1; 40 * 1024]);
        let shrunk = |_: &str, _: usize| Some(vec![2; 30 * 1024]);
        let job =
            build_job_with_cover(&request(SetKind::Stickers, Some(1)), "b", small, shrunk).unwrap();
        assert!(matches!(job.cover, Some(Cover::Item(1))));
        let job =
            build_job_with_cover(&request(SetKind::Stickers, Some(1)), "b", big, shrunk).unwrap();
        assert!(matches!(&job.cover, Some(Cover::Tgs(b)) if b.len() == 30 * 1024));
        let job =
            build_job_with_cover(&request(SetKind::Emoji, Some(0)), "b", big, shrunk).unwrap();
        assert!(matches!(job.cover, Some(Cover::Item(0))));
        assert_eq!(
            build_job_with_cover(&request(SetKind::Emoji, Some(2)), "b", big, shrunk).unwrap_err(),
            TgError::Publish(PublishError::CoverInvalid)
        );
    }

    #[test]
    fn edit_takes_cover_file_only_for_stickers() {
        let req = EditRequest {
            name: "a_by_tea_stickers_bot".into(),
            kind: SetKind::Stickers,
            seen: vec!["x".into(), "y".into()],
            delete: vec![],
            order: vec!["y".into(), "x".into()],
            emoji: vec![EmojiChange {
                file_id: "x".into(),
                emoji: vec!["🔥".into()],
            }],
            cover: Some("y".into()),
        };
        let edit = build_edit(&req, "tea_stickers_bot", |id| {
            assert_eq!(id, "y");
            Ok(vec![7])
        })
        .unwrap();
        assert_eq!(edit.cover, Some(EditCover::Tgs(vec![7])));
        assert_eq!(edit.emoji, vec![("x".into(), vec!["🔥".to_string()])]);
        let emoji = EditRequest {
            kind: SetKind::Emoji,
            ..req.clone()
        };
        let edit = build_edit(&emoji, "tea_stickers_bot", |_| panic!("не нужен файл")).unwrap();
        assert_eq!(edit.cover, Some(EditCover::Sticker("y".into())));
        let foreign = EditRequest {
            name: "a_by_other_bot".into(),
            ..req
        };
        assert!(matches!(
            build_edit(&foreign, "tea_stickers_bot", |_| Ok(vec![])),
            Err(TgError::Publish(PublishError::NameInvalid { .. }))
        ));
    }

    #[test]
    fn sticker_files_need_enabled_publishing_and_valid_ids() {
        let t = tg();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        assert_eq!(
            t.sticker_file("abc", |_| Counting(c)),
            Err(TgError::Disabled)
        );
        let c = calls.clone();
        assert_eq!(
            t.sticker_file("../x", |_| Counting(c)),
            Err(TgError::Publish(PublishError::CoverInvalid))
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);
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
