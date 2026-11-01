//! Выгрузка на заглушке Bot API: ведёт себя как Telegram (наборы, файлы,
//! ошибки с теми же текстами), но без сети. Можно подстроить сбой на
//! нужном запросе.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use svg2tgs_publish::{
    apply, run, validate, Bot, Cover, Edit, EditCover, Failure, Field, Form, Item, Job, Progress,
    PublishError, SetKind, Source, Transport,
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
    /// Ответить 429 на этот метод: сколько раз подряд и сколько секунд ждать.
    rate_limit: Option<(&'static str, usize, u64)>,
    updates: Value,
    token_ok: bool,
    webhook: bool,
    /// Свой предел набора вместо 120/200.
    cap: Option<usize>,
    /// Обложки наборов: `tgs:<байт>` или `emoji:<custom_emoji_id>`.
    thumbs: HashMap<String, String>,
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
        if let Some((m, times, secs)) = s.rate_limit {
            if m == method && times > 0 {
                s.rate_limit = Some((m, times - 1, secs));
                return Err(Failure::Api {
                    code: 429,
                    description: format!("Too Many Requests: retry after {secs}"),
                    retry_after: Some(secs),
                });
            }
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
                Some((ty, st)) => {
                    let stickers: Vec<Value> = st
                        .iter()
                        .map(|(id, emoji)| {
                            let mut v = json!({"file_id": id, "file_unique_id": format!("u{id}"),
                                "emoji": emoji[0], "is_animated": true});
                            if ty == "custom_emoji" {
                                v["custom_emoji_id"] = json!(format!("ce{id}"));
                            }
                            v
                        })
                        .collect();
                    Ok(json!({"title": "t", "sticker_type": ty, "stickers": stickers}))
                }
                None => Err(bad("STICKERSET_INVALID")),
            },
            "deleteStickerFromSet" | "setStickerEmojiList" | "setStickerPositionInSet" => {
                let id = text(&form, "sticker");
                let Some(set) = s
                    .sets
                    .values_mut()
                    .find(|set| set.1.iter().any(|x| x.0 == id))
                else {
                    return Err(bad("STICKER_ID_INVALID"));
                };
                let j = set.1.iter().position(|x| x.0 == id).unwrap();
                match method {
                    "deleteStickerFromSet" => {
                        set.1.remove(j);
                    }
                    "setStickerEmojiList" => {
                        let list: Vec<String> =
                            serde_json::from_str(&text(&form, "emoji_list")).unwrap();
                        set.1[j].1 = list;
                    }
                    _ => {
                        let at: usize = text(&form, "position").parse().unwrap();
                        if at >= set.1.len() {
                            return Err(bad("STICKER_POSITION_INVALID"));
                        }
                        let x = set.1.remove(j);
                        set.1.insert(at, x);
                    }
                }
                Ok(json!(true))
            }
            "setStickerSetThumbnail" => {
                assert_eq!(text(&form, "format"), "animated");
                assert_eq!(text(&form, "user_id"), OWNER.to_string());
                let name = text(&form, "name");
                let Some(Field::File { bytes, .. }) = get(&form, "thumbnail") else {
                    return Err(bad("THUMBNAIL_INVALID"));
                };
                if s.sets.get(&name).is_none_or(|set| set.0 != "regular") {
                    return Err(bad("STICKERSET_INVALID"));
                }
                let v = format!("tgs:{}", bytes.len());
                s.thumbs.insert(name, v);
                Ok(json!(true))
            }
            "setCustomEmojiStickerSetThumbnail" => {
                let name = text(&form, "name");
                let id = text(&form, "custom_emoji_id");
                let ok = s.sets.get(&name).is_some_and(|set| {
                    set.0 == "custom_emoji" && set.1.iter().any(|x| format!("ce{}", x.0) == id)
                });
                if !ok {
                    return Err(bad("CUSTOM_EMOJI_INVALID"));
                }
                s.thumbs.insert(name, format!("emoji:{id}"));
                Ok(json!(true))
            }
            "getFile" => Ok(json!({"file_id": text(&form, "file_id"),
                "file_path": format!("stickers/{}.tgs", text(&form, "file_id"))})),
            other => panic!("неожиданный метод {other}"),
        }
    }

    fn download(&self, file_path: &str) -> Result<Vec<u8>, Failure> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("download".into());
        assert!(file_path.starts_with("stickers/"), "{file_path}");
        Ok(TGS.to_vec())
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
        append: None,
        cover: None,
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
fn append_adds_to_existing_set_and_resumes_without_duplicates() {
    let fake = Fake::new();
    let b = bot(&fake);
    let name = format!("serdechki_by_{BOT}");
    run(
        &b,
        BOT,
        OWNER,
        &job(5, SetKind::Emoji),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();

    // Дописать 10: обрыв на загрузке 4-го (getStickerSet, по 2 вызова на стикер).
    let mut more = job(10, SetKind::Emoji);
    more.append = Some(5);
    let calls_before = fake.0.lock().unwrap().calls.len();
    fake.0.lock().unwrap().fail_at =
        Some((calls_before + 1 + 2 * 3 + 1, Failure::Network("io".into())));
    let err = run(&b, BOT, OWNER, &more, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert_eq!(
        err,
        PublishError::Network {
            done: 3,
            detail: "io".into()
        }
    );
    assert_eq!(fake.set(&name).unwrap().len(), 8);

    // «Продолжить» с той же базой: дописываются только 7 оставшихся.
    let done = run(&b, BOT, OWNER, &more, |_| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(done.resumed_from, 3);
    assert_eq!(fake.set(&name).unwrap().len(), 15);
    assert_eq!(fake.calls("createNewStickerSet"), 1);
}

#[test]
fn append_checks_the_target() {
    let fake = Fake::new();
    let b = bot(&fake);
    let mut j = job(3, SetKind::Emoji);
    j.append = Some(0);
    assert!(matches!(
        run(&b, BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)),
        Err(PublishError::SetMissing { .. })
    ));
    run(
        &b,
        BOT,
        OWNER,
        &job(3, SetKind::Stickers),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(matches!(
        run(&b, BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)),
        Err(PublishError::KindMismatch { .. })
    ));
    let mut full = job(118, SetKind::Stickers);
    full.append = Some(3);
    assert_eq!(
        run(&b, BOT, OWNER, &full, |_| {}, &AtomicBool::new(false)),
        Err(PublishError::TooMany {
            count: 121,
            max: 120
        })
    );
    // База больше, чем в наборе, — это другой набор.
    let mut odd = job(2, SetKind::Stickers);
    odd.append = Some(10);
    assert!(matches!(
        run(&b, BOT, OWNER, &odd, |_| {}, &AtomicBool::new(false)),
        Err(PublishError::SetMismatch { .. })
    ));
}

/// Бот, который записывает, сколько он «проспал».
fn sleepy(fake: &Fake) -> (Bot<Fake>, Arc<Mutex<Duration>>) {
    let slept = Arc::new(Mutex::new(Duration::ZERO));
    let w = slept.clone();
    let b = Bot::new(fake.clone()).with_sleep(move |d| *w.lock().unwrap() += d);
    (b, slept)
}

#[test]
fn rate_limit_is_waited_out_with_countdown() {
    let fake = Fake::new().with(|s| s.rate_limit = Some(("addStickerToSet", 1, 7)));
    let (b, slept) = sleepy(&fake);
    let mut seen = Vec::new();
    run(
        &b,
        BOT,
        OWNER,
        &job(3, SetKind::Stickers),
        |p| seen.push(p),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(*slept.lock().unwrap(), Duration::from_secs(7));
    assert_eq!(fake.set(&format!("serdechki_by_{BOT}")).unwrap().len(), 3);
    // Обратный отсчёт 7…1 на втором файле, затем ожидание снято.
    let waits: Vec<_> = seen.iter().filter_map(|p| p.wait).collect();
    assert_eq!(waits, vec![7, 6, 5, 4, 3, 2, 1]);
    let after = seen.iter().position(|p| p.wait == Some(1)).unwrap() + 1;
    assert_eq!(
        seen[after],
        Progress {
            done: 1,
            total: 3,
            wait: None
        }
    );
}

#[test]
fn long_and_repeated_waits_are_not_cut_short() {
    // Telegram раз за разом просит подождать — выгрузка не сдаётся сама.
    let fake = Fake::new().with(|s| s.rate_limit = Some(("uploadStickerFile", 8, 120)));
    let (b, slept) = sleepy(&fake);
    run(
        &b,
        BOT,
        OWNER,
        &job(2, SetKind::Emoji),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(*slept.lock().unwrap(), Duration::from_secs(8 * 120));
    assert_eq!(fake.set(&format!("serdechki_by_{BOT}")).unwrap().len(), 2);
}

#[test]
fn stop_interrupts_the_wait() {
    // Без проверки во время ожидания выгрузка ждала бы все 300 секунд
    // и только потом остановилась бы.
    let fake = Fake::new().with(|s| s.rate_limit = Some(("addStickerToSet", 1, 300)));
    let (b, slept) = sleepy(&fake);
    let stop = AtomicBool::new(false);
    let err = run(
        &b,
        BOT,
        OWNER,
        &job(5, SetKind::Stickers),
        |p| {
            if p.wait == Some(298) {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        },
        &stop,
    )
    .unwrap_err();
    assert_eq!(err, PublishError::Stopped { done: 1 });
    assert!(*slept.lock().unwrap() <= Duration::from_secs(3));
    assert_eq!(fake.set(&format!("serdechki_by_{BOT}")).unwrap().len(), 1);
}

#[test]
fn stop_interrupts_the_wait_before_the_set_exists() {
    let fake = Fake::new().with(|s| s.rate_limit = Some(("uploadStickerFile", 1, 60)));
    let (b, _) = sleepy(&fake);
    let stop = AtomicBool::new(false);
    let err = run(
        &b,
        BOT,
        OWNER,
        &job(5, SetKind::Stickers),
        |p| {
            if p.wait.is_some() {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        },
        &stop,
    )
    .unwrap_err();
    assert_eq!(err, PublishError::Stopped { done: 0 });
    assert!(fake.set(&format!("serdechki_by_{BOT}")).is_none());
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

fn thumb(fake: &Fake, name: &str) -> Option<String> {
    fake.0.lock().unwrap().thumbs.get(name).cloned()
}

#[test]
fn cover_is_set_after_upload() {
    // Набор эмодзи: обложка — третий эмодзи задания.
    let fake = Fake::new();
    let mut j = job(5, SetKind::Emoji);
    j.cover = Some(Cover::Item(2));
    let done = run(&bot(&fake), BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(done.cover_error, None);
    assert_eq!(thumb(&fake, &done.name).as_deref(), Some("emoji:cef3"));

    // Дописали два эмодзи к трём: обложка — второй дописанный, пятый в наборе.
    let fake = Fake::new();
    run(
        &bot(&fake),
        BOT,
        OWNER,
        &job(3, SetKind::Emoji),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut j = job(2, SetKind::Emoji);
    j.append = Some(3);
    j.cover = Some(Cover::Item(1));
    let done = run(&bot(&fake), BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(thumb(&fake, &done.name).as_deref(), Some("emoji:cef5"));

    // Набор стикеров: обложка — отдельный файл TGS.
    let fake = Fake::new();
    let mut j = job(2, SetKind::Stickers);
    j.cover = Some(Cover::Tgs(TGS.to_vec()));
    let done = run(&bot(&fake), BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(thumb(&fake, &done.name).as_deref(), Some("tgs:4"));
}

#[test]
fn rejected_cover_keeps_the_set() {
    let fake = Fake::new();
    let mut j = job(3, SetKind::Stickers);
    j.cover = Some(Cover::Item(1));
    // getStickerSet, upload+create, 2 × (upload+add), затем обложка — отказ.
    let fake = fake.with(|s| s.fail_at = Some((8, Failure::Network("io".into()))));
    let done = run(&bot(&fake), BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(done.count, 3);
    assert_eq!(fake.set(&done.name).unwrap().len(), 3);
    assert!(matches!(
        done.cover_error,
        Some(PublishError::Network { .. })
    ));
}

#[test]
fn bad_covers_fail_before_any_request() {
    let fake = Fake::new();
    let b = bot(&fake);
    let mut j = job(3, SetKind::Emoji);
    j.cover = Some(Cover::Item(3));
    assert_eq!(validate(&j, BOT), Err(PublishError::CoverInvalid));
    j.cover = Some(Cover::Tgs(TGS.to_vec()));
    assert_eq!(validate(&j, BOT), Err(PublishError::CoverInvalid));
    let mut j = job(3, SetKind::Stickers);
    j.items[0].source = Source::Bytes(vec![0x1f; 32 * 1024 + 1]);
    j.cover = Some(Cover::Item(0));
    assert_eq!(
        run(&b, BOT, OWNER, &j, |_| {}, &AtomicBool::new(false)),
        Err(PublishError::ThumbTooBig {
            bytes: 32 * 1024 + 1,
            max: 32 * 1024
        })
    );
    assert!(fake.0.lock().unwrap().calls.is_empty());
}

/// Набор из `n` эмодзи (`f1`…`fn`), созданный выгрузкой.
fn emoji_set(n: usize) -> (Fake, String) {
    let fake = Fake::new();
    let done = run(
        &bot(&fake),
        BOT,
        OWNER,
        &job(n, SetKind::Emoji),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    (fake, done.name)
}

fn ids(fake: &Fake, name: &str) -> Vec<String> {
    fake.set(name).unwrap().into_iter().map(|s| s.0).collect()
}

fn f(list: &[usize]) -> Vec<String> {
    list.iter().map(|i| format!("f{i}")).collect()
}

#[test]
fn edit_deletes_changes_emoji_reorders_and_sets_cover() {
    let (fake, name) = emoji_set(5);
    let edit = Edit {
        name: name.clone(),
        seen: f(&[1, 2, 3, 4, 5]),
        delete: f(&[2]),
        order: f(&[5, 1, 3, 4]),
        emoji: vec![
            ("f3".into(), vec!["🔥".into(), "✨".into()]),
            // Тот же эмодзи — запроса не будет.
            ("f4".into(), vec!["❤️".into()]),
        ],
        cover: Some(EditCover::Sticker("f5".into())),
    };
    let mut seen = Vec::new();
    let done = apply(
        &bot(&fake),
        OWNER,
        SetKind::Emoji,
        &edit,
        |p| seen.push((p.done, p.total)),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(done.count, 4);
    assert_eq!(ids(&fake, &name), f(&[5, 1, 3, 4]));
    assert_eq!(fake.set(&name).unwrap()[2].1, vec!["🔥", "✨"]);
    assert_eq!(thumb(&fake, &name).as_deref(), Some("emoji:cef5"));
    // Удаление, эмодзи, одна перестановка, обложка.
    assert_eq!(seen.last(), Some(&(4, 4)));
    assert_eq!(fake.calls("setStickerEmojiList"), 1);
    assert_eq!(fake.calls("setStickerPositionInSet"), 1);
}

#[test]
fn edit_continues_after_a_break_without_repeating() {
    let (fake, name) = emoji_set(5);
    let edit = Edit {
        name: name.clone(),
        seen: f(&[1, 2, 3, 4, 5]),
        delete: f(&[1, 2]),
        order: f(&[5, 4, 3]),
        ..Edit::default()
    };
    // Обрыв на втором удалении.
    let n = fake.0.lock().unwrap().calls.len();
    let fake = fake.with(|s| s.fail_at = Some((n + 3, Failure::Network("io".into()))));
    let err = apply(
        &bot(&fake),
        OWNER,
        SetKind::Emoji,
        &edit,
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert_eq!(
        err,
        PublishError::Network {
            done: 1,
            detail: "io".into()
        }
    );
    assert_eq!(ids(&fake, &name), f(&[2, 3, 4, 5]));
    // Та же правка ещё раз: первое удаление не повторяется.
    apply(
        &bot(&fake),
        OWNER,
        SetKind::Emoji,
        &edit,
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(ids(&fake, &name), f(&[5, 4, 3]));
    assert_eq!(fake.calls("deleteStickerFromSet"), 3);
}

#[test]
fn edit_refuses_a_set_changed_elsewhere() {
    let (fake, name) = emoji_set(3);
    let edit = Edit {
        name: name.clone(),
        seen: f(&[1, 2]),
        order: f(&[2, 1]),
        ..Edit::default()
    };
    let err = apply(
        &bot(&fake),
        OWNER,
        SetKind::Emoji,
        &edit,
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert_eq!(err, PublishError::SetChanged { name: name.clone() });
    assert_eq!(fake.calls("setStickerPositionInSet"), 0);
    let wrong_kind = apply(
        &bot(&fake),
        OWNER,
        SetKind::Stickers,
        &Edit {
            seen: f(&[1, 2, 3]),
            order: f(&[1, 2, 3]),
            ..edit
        },
        |_| {},
        &AtomicBool::new(false),
    );
    assert_eq!(wrong_kind, Err(PublishError::KindMismatch { name }));
}

#[test]
fn edit_is_stopped_between_requests() {
    let (fake, name) = emoji_set(3);
    let edit = Edit {
        name,
        seen: f(&[1, 2, 3]),
        order: f(&[3, 2, 1]),
        ..Edit::default()
    };
    let stop = AtomicBool::new(true);
    let err = apply(&bot(&fake), OWNER, SetKind::Emoji, &edit, |_| {}, &stop).unwrap_err();
    assert_eq!(err, PublishError::Stopped { done: 0 });
    assert_eq!(fake.calls("setStickerPositionInSet"), 0);
}

#[test]
fn stickers_are_downloaded_by_file_id() {
    let (fake, name) = emoji_set(2);
    let b = bot(&fake);
    let set = b.get_set(&name).unwrap().unwrap();
    assert_eq!(set.stickers[1].file_id, "f2");
    assert_eq!(set.stickers[1].custom_emoji_id.as_deref(), Some("cef2"));
    assert_eq!(set.stickers[1].emoji.as_deref(), Some("❤️"));
    assert_eq!(b.download_sticker("f2").unwrap(), TGS.to_vec());
    assert_eq!(fake.calls("getFile"), 1);
    assert_eq!(fake.calls("download"), 1);
}

#[test]
fn single_sticker_makes_and_extends_a_set() {
    // Набор из одного файла: только создание, без addStickerToSet.
    let fake = Fake::new();
    let done = run(
        &bot(&fake),
        BOT,
        OWNER,
        &job(1, SetKind::Stickers),
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(done.count, 1);
    assert_eq!(fake.calls("createNewStickerSet"), 1);
    assert_eq!(fake.calls("addStickerToSet"), 0);
    // Дописать ещё один.
    let mut one = job(1, SetKind::Stickers);
    one.append = Some(1);
    run(
        &bot(&fake),
        BOT,
        OWNER,
        &one,
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(fake.set(&done.name).unwrap().len(), 2);
    // Повтор того же задания (продолжение) ничего не дописывает.
    run(
        &bot(&fake),
        BOT,
        OWNER,
        &one,
        |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(fake.set(&done.name).unwrap().len(), 2);
}
