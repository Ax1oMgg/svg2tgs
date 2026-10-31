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

/// Разбор ответа Bot API: `{ok, result}` или `{ok: false, error_code, description, parameters}`.
pub(crate) fn parse(v: Value) -> Result<Value, Failure> {
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        return v.get("result").cloned().ok_or(Failure::Unexpected);
    }
    let code = v
        .get("error_code")
        .and_then(Value::as_i64)
        .ok_or(Failure::Unexpected)?;
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

impl Transport for HttpTransport {
    fn call(&self, method: &str, form: Form) -> Result<Value, Failure> {
        let url = format!("{API_HOST}/bot{}/{method}", self.token);
        let b = boundary(&form);
        let body = multipart(&form, &b);
        let resp = self
            .agent
            .post(&url)
            .header(
                "Content-Type",
                &format!("multipart/form-data; boundary={b}"),
            )
            .send(&body[..])
            // В тексте ошибки ureq есть адрес с токеном — не пропускаем его дальше.
            .map_err(|e| Failure::Network(network_kind(&e).into()))?;
        let text = resp
            .into_body()
            .read_to_string()
            .map_err(|e| Failure::Network(network_kind(&e).into()))?;
        let value: Value = serde_json::from_str(&text).map_err(|_| Failure::Unexpected)?;
        parse(value)
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
        assert_eq!(parse(serde_json::json!({"x": 1})), Err(Failure::Unexpected));
    }

    #[test]
    fn only_telegram_host() {
        assert_eq!(API_HOST, "https://api.telegram.org");
        let t = HttpTransport::new(" 123:abc \n");
        assert_eq!(t.token, "123:abc");
        assert!(!format!("{t:?}").contains("abc"));
    }
}
