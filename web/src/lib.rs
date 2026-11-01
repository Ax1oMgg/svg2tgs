//! svg2tgs-web — ядро и превью исходника в WebAssembly для веб-версии.
//!
//! Модуль работает в фоновом потоке браузера (`app/src/web/worker.ts`)
//! и ничего не импортирует: ни сети, ни файлов, только память. Обмен — через
//! память модуля без wasm-bindgen:
//!
//! 1. JS берёт буферы через [`alloc`], кладёт туда команду (JSON) и данные
//!    (например, байты SVG) и вызывает [`call`];
//! 2. ответ лежит в памяти модуля: [`reply_ptr`] и [`reply_len`];
//!    код возврата [`call`] — его вид (JSON, двоичные данные или ошибка);
//! 3. буферы команды и данных освобождает [`dealloc`].
//!
//! Конвертация, нарезка, замечания и превью исходника — тот же код, что
//! в установленной программе (`svg2tgs-core`, `svg2tgs-render::render_svg`).
//! TGS-превью в браузере рисует lottie-web: rlottie сюда не входит.

mod session;
pub mod zip;

pub use session::{Command, Reply, Session};

use std::cell::RefCell;

/// Встроенный шрифт: в браузере шрифтов системы нет (`web/fonts/README.md`).
pub const FONT: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");

thread_local! {
    static SESSION: RefCell<Session> = RefCell::new(Session::default());
    static REPLY: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Код ответа [`call`]: JSON.
pub const REPLY_JSON: u32 = 0;
/// Код ответа [`call`]: двоичные данные (PNG, TGS, ZIP); пустые — «нечего показать».
pub const REPLY_BYTES: u32 = 1;
/// Код ответа [`call`]: ошибка, JSON `{"error": "код"}`.
pub const REPLY_ERROR: u32 = 2;

/// Буфер для команды или данных.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Освобождает буфер из [`alloc`].
///
/// # Safety
/// `ptr` и `len` — ровно те, что были переданы в [`alloc`] и получены из него.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    // SAFETY: буфер выделен в `alloc` с ёмкостью `len.max(1)`.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len.max(1)) });
}

/// Выполняет команду (JSON) с данными; ответ — в [`reply_ptr`] / [`reply_len`].
///
/// # Safety
/// Буферы получены из [`alloc`] и заполнены на указанную длину.
#[no_mangle]
pub unsafe extern "C" fn call(
    cmd: *const u8,
    cmd_len: usize,
    data: *const u8,
    data_len: usize,
) -> u32 {
    // SAFETY: см. контракт функции.
    let (cmd, data) = unsafe {
        (
            std::slice::from_raw_parts(cmd, cmd_len),
            std::slice::from_raw_parts(data, data_len),
        )
    };
    let reply = match serde_json::from_slice::<Command>(cmd) {
        Ok(command) => SESSION.with(|s| s.borrow_mut().run(command, data)),
        Err(_) => Reply::Error("bad_command"),
    };
    let (code, bytes) = match reply {
        Reply::Json(v) => (REPLY_JSON, v.into_bytes()),
        Reply::Bytes(b) => (REPLY_BYTES, b),
        Reply::Error(e) => (
            REPLY_ERROR,
            serde_json::json!({ "error": e }).to_string().into_bytes(),
        ),
    };
    REPLY.with(|r| *r.borrow_mut() = bytes);
    code
}

/// Начало ответа последней команды.
#[no_mangle]
pub extern "C" fn reply_ptr() -> *const u8 {
    REPLY.with(|r| r.borrow().as_ptr())
}

/// Длина ответа последней команды.
#[no_mangle]
pub extern "C" fn reply_len() -> usize {
    REPLY.with(|r| r.borrow().len())
}
