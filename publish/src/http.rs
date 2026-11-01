//! HTTPS к Bot API. Адрес зашит: запросы уходят только на
//! `api.telegram.org`, и только когда программа явно вызывает метод.
//! Токен — в пути запроса, как требует Bot API; в ошибки и журналы
//! он не попадает.

use std::time::Duration;

use serde_json::Value;

use crate::api::{Field, Form, Transport};
use crate::error::Failure;

/// Единственный адрес, к которому обращается программа.
pub const API_HOST: &str = "https://api.telegram.org";

/// Транспорт по HTTPS (rustls, встроенные корневые сертификаты).
pub struct HttpTransport {
    token: String,
    agent: ureq::Agent,
    /// Всегда `API_HOST`; другой адрес — только в тестах (локальный сервер).
    host: &'static str,
}

impl HttpTransport {
    pub fn new(token: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            // Ответы с ошибкой читаем сами: в них `description` и `retry_after`.
            .http_status_as_error(false)
            .user_agent(concat!("svg2tgs/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            token: token.trim().to_string(),
            agent: config.into(),
            host: API_HOST,
        }
    }
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpTransport { token: *** }")
    }
}

/// Основа границы multipart. Если она вдруг встретится в данных,
/// к ней добавляется номер, пока не перестанет встречаться.
const BOUNDARY: &str = "svg2tgs-7f3a9c1e5b2d4086";

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Граница, которой нет ни в одном поле.
pub(crate) fn boundary(form: &Form) -> String {
    (0..)
        .map(|n| {
            if n == 0 {
                BOUNDARY.to_string()
            } else {
                format!("{BOUNDARY}-{n}")
            }
        })
        .find(|b| {
            form.iter().all(|(_, f)| match f {
                Field::Text(v) => !v.contains(b.as_str()),
                Field::File { bytes, .. } => !contains(bytes, b.as_bytes()),
            })
        })
        .expect("граница найдётся")
}

/// Тело `multipart/form-data` с границей `boundary`.
pub(crate) fn multipart(form: &Form, boundary: &str) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, field) in form {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match field {
            Field::Text(v) => {
                body.extend_from_slice(
                    format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(v.as_bytes());
            }
            Field::File { filename, bytes } => {
                let safe: String = filename
                    .chars()
                    .map(|c| if c == '"' || c.is_control() { '_' } else { c })
                    .collect();
                body.extend_from_slice(
                    format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{safe}\"\r\n\
                         Content-Type: application/x-tgsticker\r\n\r\n"
                    )
                    .as_bytes(),
                );
                body.extend_from_slice(bytes);
            }
        }
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

/// Начало текста для сообщения об ошибке: одна строка, до 200 символов.
pub(crate) fn snippet(text: &str) -> String {
    let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = one.chars().take(200).collect();
    if one.chars().count() > 200 {
        out.push('…');
    }
    out
}

/// Разбор ответа Bot API: `{ok, result}` или `{ok: false, error_code, description, parameters}`.
pub(crate) fn parse(v: Value) -> Result<Value, Failure> {
    let odd = || Failure::Unexpected(snippet(&v.to_string()));
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        return v.get("result").cloned().ok_or_else(odd);
    }
    let code = v
        .get("error_code")
        .and_then(Value::as_i64)
        .ok_or_else(odd)?;
    Err(Failure::Api {
        code,
        description: v
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        retry_after: v.pointer("/parameters/retry_after").and_then(Value::as_u64),
    })
}

/// Тело `application/x-www-form-urlencoded` (для запросов без файлов).
pub(crate) fn urlencoded(form: &Form) -> String {
    fn enc(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    }
    form.iter()
        .filter_map(|(k, v)| match v {
            Field::Text(t) => Some(format!("{}={}", enc(k), enc(t))),
            Field::File { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("&")
}

impl Transport for HttpTransport {
    fn call(&self, method: &str, form: Form) -> Result<Value, Failure> {
        let url = format!("{}/bot{}/{method}", self.host, self.token);
        let has_file = form.iter().any(|(_, f)| matches!(f, Field::File { .. }));
        // Файл — multipart, остальное — обычная форма.
        let (content_type, body) = if has_file {
            let b = boundary(&form);
            (
                format!("multipart/form-data; boundary={b}"),
                multipart(&form, &b),
            )
        } else {
            (
                "application/x-www-form-urlencoded".to_string(),
                urlencoded(&form).into_bytes(),
            )
        };
        let resp = self
            .agent
            .post(&url)
            .header("Content-Type", &content_type)
            .send(&body[..])
            // В тексте ошибки ureq есть адрес с токеном — не пропускаем его дальше.
            .map_err(|e| Failure::Network(network_kind(&e).into()))?;
        let status = resp.status().as_u16();
        let text = resp
            .into_body()
            .read_to_string()
            .map_err(|e| Failure::Network(network_kind(&e).into()))?;
        let value: Value = serde_json::from_str(&text).map_err(|_| {
            // Токена в ответе быть не должно, но на всякий случай вырезаем.
            let clean = text.replace(&self.token, "***");
            Failure::Unexpected(format!("HTTP {status}: {}", snippet(&clean)))
        })?;
        parse(value)
    }

    fn download(&self, file_path: &str) -> Result<Vec<u8>, Failure> {
        if !crate::api::file_path_is_safe(file_path) {
            return Err(Failure::Unexpected(format!(
                "путь файла: {}",
                snippet(file_path)
            )));
        }
        let url = format!("{}/file/bot{}/{file_path}", self.host, self.token);
        let resp = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| Failure::Network(network_kind(&e).into()))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(Failure::Unexpected(format!("HTTP {status}: файл стикера")));
        }
        resp.into_body()
            .with_config()
            .limit(crate::api::DOWNLOAD_MAX as u64 + 1)
            .read_to_vec()
            .map_err(|e| Failure::Network(network_kind(&e).into()))
    }
}

/// Вид сетевой ошибки без подробностей (в них может быть адрес с токеном).
fn network_kind(e: &ureq::Error) -> &'static str {
    match e {
        ureq::Error::Timeout(_) => "timeout",
        ureq::Error::HostNotFound => "host_not_found",
        ureq::Error::ConnectionFailed => "connection_failed",
        ureq::Error::Io(_) => "io",
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => "tls",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_has_fields_and_file() {
        let form: Form = vec![
            ("user_id".into(), Field::Text("42".into())),
            (
                "sticker".into(),
                Field::File {
                    filename: "a\"b.tgs".into(),
                    bytes: vec![0x1f, 0x8b, 0, 1],
                },
            ),
        ];
        let body = multipart(&form, BOUNDARY);
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"user_id\"\r\n\r\n42\r\n"));
        assert!(text.contains("filename=\"a_b.tgs\""));
        assert!(text.contains("application/x-tgsticker"));
        assert!(text.ends_with(&format!("--{BOUNDARY}--\r\n")));
        let pos = body.windows(4).position(|w| w == [0x1f, 0x8b, 0, 1]);
        assert!(pos.is_some());
    }

    #[test]
    fn boundary_avoids_data() {
        let form: Form = vec![(
            "sticker".into(),
            Field::File {
                filename: "x.tgs".into(),
                bytes: format!("..{BOUNDARY}..").into_bytes(),
            },
        )];
        let b = boundary(&form);
        assert_ne!(b, BOUNDARY);
        let Field::File { bytes, .. } = &form[0].1 else {
            unreachable!()
        };
        assert!(!contains(bytes, b.as_bytes()));
    }

    #[test]
    fn responses_are_parsed() {
        let ok = serde_json::json!({"ok": true, "result": {"id": 1}});
        assert_eq!(parse(ok).unwrap()["id"], 1);
        let limited = serde_json::json!({
            "ok": false, "error_code": 429,
            "description": "Too Many Requests: retry after 7",
            "parameters": {"retry_after": 7}
        });
        assert_eq!(
            parse(limited),
            Err(Failure::Api {
                code: 429,
                description: "Too Many Requests: retry after 7".into(),
                retry_after: Some(7)
            })
        );
        assert!(
            matches!(parse(serde_json::json!({"x": 1})), Err(Failure::Unexpected(d)) if d.contains("\"x\""))
        );
    }

    #[test]
    fn text_forms_are_urlencoded() {
        let form: Form = vec![
            ("name".into(), Field::Text("a_by_b".into())),
            (
                "stickers".into(),
                Field::Text(r#"[{"emoji_list":["❤️"]}]"#.into()),
            ),
        ];
        assert_eq!(
            urlencoded(&form),
            "name=a_by_b&stickers=%5B%7B%22emoji_list%22%3A%5B%22%E2%9D%A4%EF%B8%8F%22%5D%7D%5D"
        );
    }

    #[test]
    fn snippets_are_short_and_single_line() {
        let s = snippet(&format!("<html>\n  <body>{}</body>", "x".repeat(300)));
        assert!(!s.contains('\n'));
        assert!(s.chars().count() <= 201);
        assert!(s.starts_with("<html> <body>"));
    }

    /// Локальный сервер на один запрос: возвращает запрос целиком
    /// (заголовки и тело) и отвечает `status` с телом `body`.
    fn serve_once(status: &str, body: &str) -> (&'static str, std::thread::JoinHandle<Vec<u8>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let host: &'static str =
            Box::leak(format!("http://{}", listener.local_addr().unwrap()).into_boxed_str());
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            // Заголовки, затем тело по Content-Length.
            loop {
                let n = sock.read(&mut buf).unwrap();
                req.extend_from_slice(&buf[..n]);
                if let Some(end) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&req[..end]).to_lowercase();
                    let len: usize = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .map_or(0, |v| v.trim().parse().unwrap());
                    while req.len() < end + 4 + len {
                        let n = sock.read(&mut buf).unwrap();
                        req.extend_from_slice(&buf[..n]);
                    }
                    break;
                }
            }
            sock.write_all(reply.as_bytes()).unwrap();
            req
        });
        (host, handle)
    }

    fn local(host: &'static str) -> HttpTransport {
        HttpTransport {
            host,
            ..HttpTransport::new("123:abc")
        }
    }

    #[test]
    fn real_client_sends_form_and_reads_json() {
        let (host, server) =
            serve_once("200 OK", r#"{"ok":true,"result":{"id":5,"username":"b"}}"#);
        let r = local(host).call(
            "getUpdates",
            vec![(
                "allowed_updates".into(),
                Field::Text(r#"["message"]"#.into()),
            )],
        );
        assert_eq!(r.unwrap()["username"], "b");
        let req = String::from_utf8(server.join().unwrap()).unwrap();
        assert!(
            req.starts_with("POST /bot123:abc/getUpdates HTTP/1.1"),
            "{req}"
        );
        assert!(
            req.to_lowercase()
                .contains("content-type: application/x-www-form-urlencoded"),
            "{req}"
        );
        assert!(
            req.ends_with("allowed_updates=%5B%22message%22%5D"),
            "{req}"
        );
    }

    #[test]
    fn real_client_uploads_multipart() {
        let (host, server) = serve_once("200 OK", r#"{"ok":true,"result":{"file_id":"F"}}"#);
        let form: Form = vec![
            ("user_id".into(), Field::Text("7".into())),
            (
                "sticker".into(),
                Field::File {
                    filename: "a.tgs".into(),
                    bytes: vec![0x1f, 0x8b, 8, 0],
                },
            ),
        ];
        assert_eq!(
            local(host).call("uploadStickerFile", form).unwrap()["file_id"],
            "F"
        );
        let req = server.join().unwrap();
        let text = String::from_utf8_lossy(&req).to_lowercase();
        assert!(
            text.contains(&format!(
                "content-type: multipart/form-data; boundary={BOUNDARY}"
            )),
            "{text}"
        );
        assert!(contains(&req, &[0x1f, 0x8b, 8, 0]));
    }

    #[test]
    fn real_client_reports_non_json_with_status() {
        let (host, server) = serve_once(
            "403 Forbidden",
            "<html>\n<body>Blocked by proxy 123:abc</body></html>",
        );
        let err = local(host).call("getMe", vec![]).unwrap_err();
        server.join().unwrap();
        let Failure::Unexpected(detail) = err else {
            panic!("{err:?}")
        };
        assert!(
            detail.starts_with("HTTP 403: <html> <body>Blocked by proxy"),
            "{detail}"
        );
        assert!(!detail.contains("123:abc"), "{detail}");
    }

    #[test]
    fn real_client_reads_api_errors() {
        let (host, server) = serve_once(
            "401 Unauthorized",
            r#"{"ok":false,"error_code":401,"description":"Unauthorized"}"#,
        );
        let err = local(host).call("getMe", vec![]).unwrap_err();
        server.join().unwrap();
        assert!(matches!(err, Failure::Api { code: 401, .. }), "{err:?}");
    }

    #[test]
    fn real_client_downloads_files() {
        let (host, server) = serve_once("200 OK", "TGSDATA");
        let bytes = local(host).download("stickers/file_1.tgs").unwrap();
        assert_eq!(bytes, b"TGSDATA");
        let req = String::from_utf8(server.join().unwrap()).unwrap();
        assert!(
            req.starts_with("GET /file/bot123:abc/stickers/file_1.tgs HTTP/1.1"),
            "{req}"
        );
        // Путь не из Telegram — запроса нет.
        assert!(local(host).download("../x").is_err());
    }

    #[test]
    fn only_telegram_host() {
        assert_eq!(API_HOST, "https://api.telegram.org");
        let t = HttpTransport::new(" 123:abc \n");
        assert_eq!(t.token, "123:abc");
        assert!(!format!("{t:?}").contains("abc"));
    }
}
