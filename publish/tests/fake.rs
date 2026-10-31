//! Выгрузка на заглушке Bot API: ведёт себя как Telegram (наборы, файлы,
//! ошибки с теми же текстами), но без сети. Можно подстроить сбой на
//! нужном запросе.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use svg2tgs_publish::{
    run, validate, Bot, Failure, Field, Form, Item, Job, Progress, PublishError, SetKind, Source,
    Transport,
};

const BOT: &str = "tea_stickers_bot";
const OWNER: i64 = 777;

/// Стикер в наборе: `file_id` и эмодзи.
type Sticker = (String, Vec<String>);
/// Набор: `sticker_type` и стикеры по порядку.
type Set = (String, Vec<Sticker>);

#[derive(Default)]
struct State {
    sets: HashMap<String, Set>,
    files: usize,
    calls: Vec<String>,
    /// Сбой на N-м вызове метода (по общему счёту вызовов).
    fail_at: Option<(usize, Failure)>,
    /// Ответить 429 один раз на этот метод.
    rate_limit: Option<&'static str>,
    updates: Value,
    token_ok: bool,
    webhook: bool,
    /// Свой предел набора вместо 120/200.
    cap: Option<usize>,
}

#[derive(Clone)]
struct Fake(Arc<Mutex<State>>);

impl Fake {
    fn new() -> Self {
        Fake(Arc::new(Mutex::new(State {
            token_ok: true,
            updates: json!([]),
            ..State::default()
        })))
    }
    fn with(self, f: impl FnOnce(&mut State)) -> Self {
        f(&mut self.0.lock().unwrap());
        self
    }
    fn set(&self, name: &str) -> Option<Vec<Sticker>> {
        self.0.lock().unwrap().sets.get(name).map(|s| s.1.clone())
    }
    fn calls(&self, method: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|c| *c == method)
            .count()
    }
}

fn bad(desc: &str) -> Failure {
    Failure::Api {
        code: 400,
        description: format!("Bad Request: {desc}"),
        retry_after: None,
    }
}

fn get<'a>(form: &'a Form, key: &str) -> Option<&'a Field> {
    form.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn text(form: &Form, key: &str) -> String {
    match get(form, key) {
        Some(Field::Text(t)) => t.clone(),
        _ => String::new(),
    }
}

fn sticker(v: &Value) -> Sticker {
    assert_eq!(v["format"], "animated");
    (
        v["sticker"].as_str().unwrap().to_string(),
        v["emoji_list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap().to_string())
            .collect(),
    )
}

impl Transport for Fake {
    fn call(&self, method: &str, form: Form) -> Result<Value, Failure> {
        let mut s = self.0.lock().unwrap();
        s.calls.push(method.to_string());
        let n = s.calls.len();
        if let Some((at, f)) = s.fail_at.clone() {
            if at == n {
                return Err(f);
            }
        }
        if s.rate_limit == Some(method) {
            s.rate_limit = None;
            return Err(Failure::Api {
                code: 429,
                description: "Too Many Requests: retry after 7".into(),
                retry_after: Some(7),
            });
        }
        if !s.token_ok {
            return Err(Failure::Api {
                code: 401,
                description: "Unauthorized".into(),
                retry_after: None,
            });
        }
        match method {
            "getMe" => Ok(json!({"id": 1, "is_bot": true, "username": BOT})),
            "getUpdates" if s.webhook => Err(Failure::Api {
                code: 409,
                description: "Conflict: can't use getUpdates method while webhook is active".into(),
                retry_after: None,
            }),
            "getUpdates" => Ok(s.updates.clone()),
            "uploadStickerFile" => {
                assert_eq!(text(&form, "sticker_format"), "animated");
                assert_eq!(text(&form, "user_id"), OWNER.to_string());
                match get(&form, "sticker") {
                    Some(Field::File { bytes, .. }) if bytes.starts_with(&[0x1f, 0x8b]) => {
                        s.files += 1;
                        Ok(json!({"file_id": format!("f{}", s.files)}))
                    }
                    _ => Err(bad("STICKER_TGS_NOTGS")),
                }
            }
            "createNewStickerSet" => {
                let name = text(&form, "name");
                if !name.ends_with(&format!("_by_{BOT}")) {
                    return Err(bad("invalid sticker set name is specified"));
                }
                if s.sets.contains_key(&name) {
                    return Err(bad("sticker set name is already occupied"));
                }
                let list: Value = serde_json::from_str(&text(&form, "stickers")).unwrap();
                let stickers = list.as_array().unwrap().iter().map(sticker).collect();
                s.sets.insert(name, (text(&form, "sticker_type"), stickers));
                Ok(json!(true))
            }
            "addStickerToSet" => {
                let name = text(&form, "name");
                let item = sticker(&serde_json::from_str(&text(&form, "sticker")).unwrap());
                let cap_override = s.cap;
                let Some(set) = s.sets.get_mut(&name) else {
                    return Err(bad("STICKERSET_INVALID"));
                };
                let cap = cap_override.unwrap_or(if set.0 == "custom_emoji" { 200 } else { 120 });
                if set.1.len() >= cap {
                    return Err(bad("STICKERS_TOO_MUCH"));
                }
                set.1.push(item);
                Ok(json!(true))
            }
            "getStickerSet" => match s.sets.get(&text(&form, "name")) {
                Some((_, st)) => Ok(json!({"title": "t", "stickers": vec![json!({}); st.len()]})),
                None => Err(bad("STICKERSET_INVALID")),
            },
            other => panic!("неожиданный метод {other}"),
        }
    }
}

const TGS: [u8; 4] = [0x1f, 0x8b, 8, 0];

fn job(n: usize, kind: SetKind) -> Job {
    Job {
        kind,
        name: format!("serdechki_by_{BOT}"),
        title: "Сердечки".into(),
        items: (0..n)
            .map(|i| Item {
                filename: format!("heart_{:02}.tgs", i + 1),
                source: Source::Bytes(TGS.to_vec()),
                emoji: vec![if i == 11 { "😍" } else { "❤️" }.to_string()],
            })
            .collect(),
        resume: false,
    }
}

fn bot(fake: &Fake) -> Bot<Fake> {
    Bot::new(fake.clone()).with_sleep(|_| {})
}

#[test]
fn creates_emoji_set_in_order() {
    let fake = Fake::new();
    let mut seen = Vec::new();
    let done = run(
        &bot(&fake),
        BOT,
        OWNER,
        &job(40, SetKind::Emoji),
        |p: Progress| seen.push(p.done),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(done.count, 40);
    assert_eq!(done.resumed_from, 0);
    assert_eq!(
        done.link,
        format!("https://t.me/addemoji/serdechki_by_{BOT}")
    );
    assert_eq!(seen, (1..=40).collect::<Vec<_>>());
    let set = fake.set(&done.name).unwrap();
    let ids: Vec<_> = set.iter().map(|s| s.0.clone()).collect();
    assert_eq!(ids, (1..=40).map(|i| format!("f{i}")).collect::<Vec<_>>());
    assert_eq!(set[11].1, vec!["😍"]);
    assert_eq!(fake.0.lock().unwrap().sets[&done.name].0, "custom_emoji");
    assert_eq!(fake.calls("createNewStickerSet"), 1);
    assert_eq!(fake.calls("addStickerToSet"), 39);
}

#[test]
fn network_break_then_resume_continues_without_duplicates() {
    // getStickerSet, upload+create (2), затем по 2 вызова на стикер:
    // обрыв на загрузке 24-го файла.
    let at = 1 + 2 + 2 * 22 + 1;
    let fake = Fake::new().with(|s| s.fail_at = Some((at, Failure::Network("io".into()))));
    let b = bot(&fake);
    let err = run(
        &b,
        BOT,
        OWNER,
        &job(40, SetKind::Emoji),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert_eq!(
        err,
        PublishError::Network {
            done: 23,
            detail: "io".into()
        }
    );
    assert_eq!(err.done(), Some(23));
    assert_eq!(fake.set(&format!("serdechki_by_{BOT}")).unwrap().len(), 23);

    // Без разрешения дописывать — «имя занято».
    let again = run(
        &b,
        BOT,
        OWNER,
        &job(40, SetKind::Emoji),
        |_| {},
        &AtomicBool::new(false),
    );
    assert!(matches!(again, Err(PublishError::NameTaken { .. })));

    let mut resume = job(40, SetKind::Emoji);
    resume.resume = true;
    let mut first = None;
    let done = run(
        &b,
        BOT,
        OWNER,
        &resume,
        |p| {
            first.get_or_insert(p.done);
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(first, Some(23));
    assert_eq!(done.resumed_from, 23);
    let set = fake.set(&done.name).unwrap();
    assert_eq!(set.len(), 40);
    assert_eq!(set[11].1, vec!["😍"]);
}

#[test]
fn rate_limit_is_waited_out() {
    let fake = Fake::new().with(|s| s.rate_limit = Some("addStickerToSet"));
    let waits = Arc::new(Mutex::new(Vec::new()));
    let w = waits.clone();
    let b = Bot::new(fake.clone()).with_sleep(move |d| w.lock().unwrap().push(d));
    run(
        &b,
        BOT,
        OWNER,
        &job(3, SetKind::Stickers),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(*waits.lock().unwrap(), vec![Duration::from_secs(7)]);
    assert_eq!(fake.set(&format!("serdechki_by_{BOT}")).unwrap().len(), 3);
}

#[test]
fn rejected_file_names_its_index() {
    let fake = Fake::new();
    let mut j = job(5, SetKind::Stickers);
    j.items[3].source = Source::Bytes(b"{\"v\":1}".to_vec());
    let err = run(&bot(&fake), BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(
        matches!(err, PublishError::FileRejected { index: 3, .. }),
        "{err:?}"
    );
}

#[test]
fn stop_is_honoured_between_files() {
    let fake = Fake::new();
    let stop = AtomicBool::new(false);
    let err = run(
        &bot(&fake),
        BOT,
        OWNER,
        &job(10, SetKind::Stickers),
        |p| {
            if p.done == 4 {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        },
        &stop,
    )
    .unwrap_err();
    assert_eq!(err, PublishError::Stopped { done: 4 });
}

#[test]
fn full_set_is_reported() {
    // Telegram сам ответил «набор полон» (например, предел изменился) — код set_full.
    let fake = Fake::new().with(|s| s.cap = Some(5));
    let err = run(
        &bot(&fake),
        BOT,
        OWNER,
        &job(10, SetKind::Stickers),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert_eq!(err, PublishError::SetFull { done: 5 });
}

#[test]
fn bad_jobs_fail_before_any_request() {
    let fake = Fake::new();
    let b = bot(&fake);
    let mut j = job(3, SetKind::Emoji);
    j.items[1].emoji.clear();
    assert_eq!(
        run(&b, BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)),
        Err(PublishError::EmojiInvalid { index: 1 })
    );
    let mut j = job(3, SetKind::Emoji);
    j.name = "serdechki".into();
    assert!(matches!(
        validate(&j, BOT),
        Err(PublishError::NameInvalid { .. })
    ));
    let mut j = job(3, SetKind::Emoji);
    j.title = " ".into();
    assert_eq!(validate(&j, BOT), Err(PublishError::TitleInvalid));
    assert_eq!(
        validate(&job(0, SetKind::Emoji), BOT),
        Err(PublishError::Nothing)
    );
    assert_eq!(
        validate(&job(201, SetKind::Emoji), BOT),
        Err(PublishError::TooMany {
            count: 201,
            max: 200
        })
    );
    assert!(fake.0.lock().unwrap().calls.is_empty());
}

#[test]
fn token_and_owner() {
    let fake = Fake::new().with(|s| s.token_ok = false);
    assert_eq!(bot(&fake).me(), Err(PublishError::InvalidToken));

    let fake = Fake::new().with(|s| {
        s.updates = json!([
            {"update_id": 1, "message": {"text": "/start", "chat": {"type": "private"},
                "from": {"id": 5, "first_name": "Аня", "username": "anya"}}},
            {"update_id": 2, "message": {"text": "/start", "chat": {"type": "group"},
                "from": {"id": 6, "first_name": "Чужой"}}},
            {"update_id": 3, "message": {"text": "привет", "chat": {"type": "private"},
                "from": {"id": 7, "first_name": "Боря"}}},
            {"update_id": 4, "message": {"text": "/start", "chat": {"type": "private"},
                "from": {"id": 777, "first_name": "Григорий"}}}
        ])
    });
    let b = bot(&fake);
    assert_eq!(b.me().unwrap().username, BOT);
    let owner = b.find_owner().unwrap().unwrap();
    assert_eq!(owner.id, 777);
    assert_eq!(owner.username, None);
    assert_eq!(owner.first_name, "Григорий");

    assert_eq!(bot(&Fake::new()).find_owner(), Ok(None));
    let hooked = Fake::new().with(|s| s.webhook = true);
    assert_eq!(bot(&hooked).find_owner(), Err(PublishError::WebhookActive));
}

#[test]
fn errors_serialize_as_codes() {
    let v = serde_json::to_value(PublishError::Network {
        done: 23,
        detail: "io".into(),
    })
    .unwrap();
    assert_eq!(v, json!({"code": "network", "done": 23, "detail": "io"}));
}
