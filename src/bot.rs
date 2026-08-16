//! Бот Telegram на вебхуке.
//!
//! Хитрость, которая убирает из проекта весь TLS-клиент: Telegram
//! разрешает ответить на обновление **прямо в теле HTTP-ответа**, указав
//! поле `method`. Значит серверу не нужны исходящие запросы, а с ними —
//! ни rustls, ни корневые сертификаты, ни HTTP-клиент.
//!
//! Плата: бот не может писать первым. Для `/start`, `/play` и приглашений
//! этого достаточно — все они приходят от пользователя.

use crate::json;

/// Разобранное обновление: нам нужны только чат, текст и полезная нагрузка `/start`.
#[derive(Debug, PartialEq)]
pub struct Update<'a> {
    pub chat_id: i64,
    pub text: &'a str,
}

pub fn parse(body: &str) -> Option<Update<'_>> {
    // chat_id лежит внутри message.chat.id — сканер найдёт первый "id"
    // после "chat", поэтому ищем по срезу от подстроки "chat".
    let chat_pos = body.find("\"chat\"")?;
    let chat_id = json::int_field(&body[chat_pos..], "id")?;
    let text = json::str_field(body, "text").unwrap_or("");
    Some(Update { chat_id, text })
}

/// Полезная нагрузка `/start inv_KOD` — код приглашения из диплинка.
pub fn start_payload(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("/start")?.trim();
    rest.strip_prefix("inv_").filter(|c| !c.is_empty())
}

/// Ответ ботом в теле вебхука. `None` — команду игнорируем.
pub fn reply(u: &Update, app_url: &str) -> Option<String> {
    let cmd = u.text.split_whitespace().next().unwrap_or("");
    let (text, button) = match cmd {
        "/start" => {
            let url = match start_payload(u.text) {
                Some(code) => format!("{app_url}?inv={code}"),
                None => app_url.to_string(),
            };
            (
                "Ультимативные крестики-нолики.\n\nДевять полей, и каждый ваш ход \
                 решает, где будет ходить соперник. Партия идёт минут пять.",
                Some(("Играть", url)),
            )
        }
        "/play" => (
            "Открывайте поле и жмите «Найти соперника».",
            Some(("Играть", app_url.to_string())),
        ),
        "/help" => (
            "Выигрываете малое поле — забираете его клетку на большом. \
             Три в ряд на большом поле — победа.\n\nГлавное правило: \
             куда вы походили внутри малого поля, в то поле и отправляется соперник.",
            Some(("Играть", app_url.to_string())),
        ),
        _ => return None,
    };

    let mut s = String::with_capacity(320);
    s.push_str(r#"{"method":"sendMessage","chat_id":"#);
    s.push_str(&u.chat_id.to_string());
    s.push_str(r#","text":"#);
    json::push_str(&mut s, text);
    if let Some((label, url)) = button {
        s.push_str(r#","reply_markup":{"inline_keyboard":[[{"text":"#);
        json::push_str(&mut s, label);
        s.push_str(r#","web_app":{"url":"#);
        json::push_str(&mut s, &url);
        s.push_str("}}]]}");
    }
    s.push('}');
    Some(s)
}

/// Сверка секрета вебхука. Заголовок пишется Telegram при `setWebhook`.
pub fn secret_ok(headers: &str, expected: &str) -> bool {
    if expected.is_empty() {
        return true;
    }
    for line in headers.split("\r\n") {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.trim().eq_ignore_ascii_case("x-telegram-bot-api-secret-token") {
            return crate::crypto::eq_const_time(v.trim().as_bytes(), expected.as_bytes());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPD: &str = r#"{"update_id":1,"message":{"message_id":5,"from":{"id":42,"first_name":"Марат"},"chat":{"id":777,"type":"private"},"text":"/start"}}"#;

    #[test]
    fn разбирает_чат_и_текст() {
        let u = parse(UPD).unwrap();
        assert_eq!(u.chat_id, 777, "взят id пользователя вместо чата");
        assert_eq!(u.text, "/start");
    }

    #[test]
    fn мусор_не_валит_разбор() {
        for bad in ["", "{}", "не json", r#"{"message":{}}"#] {
            assert!(parse(bad).is_none() || parse(bad).is_some());
        }
    }

    #[test]
    fn стартовый_ответ_содержит_кнопку_приложения() {
        let u = parse(UPD).unwrap();
        let r = reply(&u, "https://app.test/").unwrap();
        assert!(r.contains(r#""method":"sendMessage""#));
        assert!(r.contains(r#""chat_id":777"#));
        assert!(r.contains("web_app"));
        assert!(r.contains("https://app.test/"));
    }

    #[test]
    fn диплинк_приглашения_доезжает_до_кнопки() {
        assert_eq!(start_payload("/start inv_K7QM2X"), Some("K7QM2X"));
        assert_eq!(start_payload("/start"), None);
        assert_eq!(start_payload("/start inv_"), None);
        assert_eq!(start_payload("/play"), None);

        let u = Update {
            chat_id: 1,
            text: "/start inv_K7QM2X",
        };
        let r = reply(&u, "https://app.test/").unwrap();
        assert!(r.contains("https://app.test/?inv=K7QM2X"), "{r}");
    }

    #[test]
    fn неизвестные_команды_игнорируются() {
        let u = Update {
            chat_id: 1,
            text: "просто текст",
        };
        assert!(reply(&u, "https://x/").is_none());
    }

    #[test]
    fn текст_экранируется() {
        let u = Update {
            chat_id: 1,
            text: "/start",
        };
        let r = reply(&u, "https://x/\"><script>").unwrap();
        assert!(!r.contains("<script>"), "{r}");
        assert!(r.contains("\\u003c"));
    }

    #[test]
    fn секрет_вебхука_сверяется() {
        let h = "POST /telegram/webhook HTTP/1.1\r\nX-Telegram-Bot-Api-Secret-Token: s3cr3t\r\n\r\n";
        assert!(secret_ok(h, "s3cr3t"));
        assert!(!secret_ok(h, "другой"));
        assert!(!secret_ok("POST / HTTP/1.1\r\n\r\n", "s3cr3t"));
        // пустой ожидаемый секрет означает «проверка выключена»
        assert!(secret_ok("POST / HTTP/1.1\r\n\r\n", ""));
    }
}
