//! Проверка `initData` мини-приложения Telegram.
//!
//! Алгоритм: `secret = HMAC_SHA256(key="WebAppData", data=bot_token)`,
//! затем сверка `HMAC_SHA256(key=secret, data=data_check_string)` с полем
//! `hash`. Сравнение — за постоянное время.

use crate::crypto::{eq_const_time, hex, hmac_sha256};

/// Потолок на размер `initData`: без него можно заставить сервер считать
/// HMAC от мегабайта на каждое подключение.
pub const MAX_INIT_DATA: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub guest: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    NoBotToken,
    Empty,
    Oversized,
    MissingHash,
    BadHash,
    Expired,
    NoUser,
}

impl AuthError {
    pub fn code(self) -> &'static str {
        match self {
            AuthError::NoBotToken => "no_bot_token",
            AuthError::Empty => "empty_init_data",
            AuthError::Oversized => "oversized_init_data",
            AuthError::MissingHash => "missing_hash",
            AuthError::BadHash => "bad_hash",
            AuthError::Expired => "expired",
            AuthError::NoUser => "no_user",
        }
    }
}

/// Декодирование percent-encoding из query-строки.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hi = (b[i + 1] as char).to_digit(16);
                let lo = (b[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Достаёт строковое или числовое поле верхнего уровня из компактного JSON.
/// Полный парсер здесь не нужен: структура `user` известна и фиксирована.
fn json_field(src: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let start = src.find(&pat)? + pat.len();
    let rest = src[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    if let Some(body) = rest.strip_prefix('"') {
        let mut out = String::new();
        let mut esc = false;
        for ch in body.chars() {
            if esc {
                match ch {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'u' => return Some(out), // \uXXXX не нужен для id/имени
                    c => out.push(c),
                }
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                return Some(out);
            } else {
                out.push(ch);
            }
        }
        None
    } else {
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '-'))
            .unwrap_or(rest.len());
        if end == 0 {
            None
        } else {
            Some(rest[..end].to_string())
        }
    }
}

/// Проверка подписи и извлечение профиля.
/// `now` — текущее время в секундах эпохи, `max_age` — допустимый возраст.
pub fn validate(
    init_data: &str,
    bot_token: &str,
    now: u64,
    max_age: u64,
) -> Result<Profile, AuthError> {
    if bot_token.is_empty() {
        return Err(AuthError::NoBotToken);
    }
    if init_data.len() > MAX_INIT_DATA {
        return Err(AuthError::Oversized);
    }
    if init_data.trim().is_empty() {
        return Err(AuthError::Empty);
    }

    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut got_hash: Option<String> = None;

    for part in init_data.split('&') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = percent_decode(k);
        let val = percent_decode(v);
        if key == "hash" {
            got_hash = Some(val);
        } else {
            pairs.push((key, val));
        }
    }

    let Some(hash) = got_hash else {
        return Err(AuthError::MissingHash);
    };

    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let check: String = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");

    let secret = hmac_sha256(b"WebAppData", bot_token.as_bytes());
    let calc = hex(&hmac_sha256(&secret, check.as_bytes()));

    if !eq_const_time(calc.as_bytes(), hash.as_bytes()) {
        return Err(AuthError::BadHash);
    }

    if max_age > 0 {
        if let Some((_, v)) = pairs.iter().find(|(k, _)| k == "auth_date") {
            if let Ok(ts) = v.parse::<u64>() {
                if now > ts && now - ts > max_age {
                    return Err(AuthError::Expired);
                }
            }
        }
    }

    let user = pairs
        .iter()
        .find(|(k, _)| k == "user")
        .map(|(_, v)| v.as_str())
        .ok_or(AuthError::NoUser)?;

    let id = json_field(user, "id").ok_or(AuthError::NoUser)?;
    let name = json_field(user, "first_name")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Игрок".to_string());

    Ok(Profile {
        id,
        name: clip_name(&name),
        guest: false,
    })
}

/// Имя приходит от пользователя: режем длину, убираем управляющие символы.
pub fn clip_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .take(24)
        .collect();
    let t = cleaned.trim();
    if t.is_empty() {
        "Игрок".to_string()
    } else {
        t.to_string()
    }
}

/// Гость: играет, но в рейтинге не участвует.
pub fn guest(seed: u64) -> Profile {
    Profile {
        id: format!("guest_{seed:x}"),
        name: "Гость".to_string(),
        guest: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{hex, hmac_sha256};

    const TOKEN: &str = "123456:AAHfakefakefakefakefakefakefakefake";

    /// Собирает валидный initData тем же алгоритмом, что и Telegram.
    fn sign(pairs: &[(&str, &str)], token: &str) -> String {
        let mut sorted: Vec<_> = pairs.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        let check: String = sorted
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n");
        let secret = hmac_sha256(b"WebAppData", token.as_bytes());
        let h = hex(&hmac_sha256(&secret, check.as_bytes()));
        let mut qs: Vec<String> = sorted
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencode(v)))
            .collect();
        qs.push(format!("hash={h}"));
        qs.join("&")
    }

    fn urlencode(s: &str) -> String {
        let mut out = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }

    #[test]
    fn валидная_подпись_принимается() {
        let user = r#"{"id":123456789,"first_name":"Марат","username":"m"}"#;
        let data = sign(&[("auth_date", "1700000000"), ("user", user)], TOKEN);
        let p = validate(&data, TOKEN, 1_700_000_100, 86400).unwrap();
        assert_eq!(p.id, "123456789");
        assert_eq!(p.name, "Марат");
        assert!(!p.guest);
    }

    #[test]
    fn чужой_токен_отвергается() {
        let user = r#"{"id":1,"first_name":"A"}"#;
        let data = sign(&[("auth_date", "1700000000"), ("user", user)], TOKEN);
        assert_eq!(
            validate(&data, "999:other", 1_700_000_100, 86400),
            Err(AuthError::BadHash)
        );
    }

    #[test]
    fn подмена_поля_ломает_подпись() {
        let user = r#"{"id":1,"first_name":"A"}"#;
        let data = sign(&[("auth_date", "1700000000"), ("user", user)], TOKEN);
        let tampered = data.replace("1700000000", "1700000001");
        assert_eq!(
            validate(&tampered, TOKEN, 1_700_000_100, 86400),
            Err(AuthError::BadHash)
        );
    }

    #[test]
    fn просроченные_данные_отвергаются() {
        let user = r#"{"id":1,"first_name":"A"}"#;
        let data = sign(&[("auth_date", "1700000000"), ("user", user)], TOKEN);
        assert_eq!(
            validate(&data, TOKEN, 1_700_000_000 + 90_000, 86400),
            Err(AuthError::Expired)
        );
        // с выключенной проверкой возраста — проходит
        assert!(validate(&data, TOKEN, 1_700_000_000 + 90_000, 0).is_ok());
    }

    #[test]
    fn пустые_и_кривые_данные() {
        assert_eq!(validate("", TOKEN, 0, 0), Err(AuthError::Empty));
        assert_eq!(validate("x=1", TOKEN, 0, 0), Err(AuthError::MissingHash));
        assert_eq!(validate("x=1&hash=ab", "", 0, 0), Err(AuthError::NoBotToken));
        let huge = "a=".to_string() + &"x".repeat(MAX_INIT_DATA);
        assert_eq!(validate(&huge, TOKEN, 0, 0), Err(AuthError::Oversized));
    }

    #[test]
    fn без_пользователя_нет_профиля() {
        let data = sign(&[("auth_date", "1700000000")], TOKEN);
        assert_eq!(validate(&data, TOKEN, 1_700_000_100, 0), Err(AuthError::NoUser));
    }

    #[test]
    fn имя_обрезается_и_чистится() {
        assert_eq!(clip_name("  Марат \n"), "Марат");
        assert_eq!(clip_name(""), "Игрок");
        assert_eq!(clip_name("\u{0}\u{1}"), "Игрок");
        assert_eq!(clip_name(&"я".repeat(100)).chars().count(), 24);
    }

    #[test]
    fn имя_с_кавычками_и_разметкой_не_ломает_разбор() {
        let user = r#"{"id":7,"first_name":"<img src=x onerror=alert(1)>"}"#;
        let data = sign(&[("auth_date", "1700000000"), ("user", user)], TOKEN);
        let p = validate(&data, TOKEN, 1_700_000_100, 0).unwrap();
        assert_eq!(p.id, "7");
        assert!(p.name.starts_with("<img"));
    }

    #[test]
    fn гость_помечен_как_гость() {
        let g = guest(0xabc);
        assert!(g.guest);
        assert!(g.id.starts_with("guest_"));
    }
}
