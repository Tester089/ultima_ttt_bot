//! Минимальный JSON.
//!
//! Полный парсер тут не нужен: формы входящих сообщений известны и
//! фиксированы, а исходящие мы строим сами. Сканер полей не выделяет
//! память вообще, писатель пишет в переиспользуемый буфер.

/// Читает строковое поле верхнего уровня без аллокаций.
pub fn str_field<'a>(src: &'a str, key: &str) -> Option<&'a str> {
    let val = raw_value(src, key)?;
    let body = val.strip_prefix('"')?;
    let mut end = 0;
    let b = body.as_bytes();
    while end < b.len() {
        if b[end] == b'\\' {
            end += 2;
            continue;
        }
        if b[end] == b'"' {
            return Some(&body[..end]);
        }
        end += 1;
    }
    None
}

/// Читает целочисленное поле верхнего уровня.
pub fn int_field(src: &str, key: &str) -> Option<i64> {
    let val = raw_value(src, key)?;
    let end = val
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(val.len());
    if end == 0 {
        return None;
    }
    val[..end].parse().ok()
}

fn raw_value<'a>(src: &'a str, key: &str) -> Option<&'a str> {
    let mut from = 0;
    loop {
        let idx = src[from..].find('"')? + from;
        let rest = &src[idx + 1..];
        let close = rest.find('"')?;
        let name = &rest[..close];
        let after = rest[close + 1..].trim_start();
        if let Some(v) = after.strip_prefix(':') {
            if name == key {
                return Some(v.trim_start());
            }
            from = idx + 1 + close + 1;
        } else {
            from = idx + 1 + close + 1;
        }
        if from >= src.len() {
            return None;
        }
    }
}

/// Экранирование строки в JSON. Имя приходит от пользователя, поэтому
/// экранируется всегда — в JS-версии имя уходило в `innerHTML` как есть.
pub fn push_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn читает_строки_и_числа() {
        let s = r#"{"t":"move","b":3,"c":7,"code":"K7QM2X"}"#;
        assert_eq!(str_field(s, "t"), Some("move"));
        assert_eq!(int_field(s, "b"), Some(3));
        assert_eq!(int_field(s, "c"), Some(7));
        assert_eq!(str_field(s, "code"), Some("K7QM2X"));
        assert_eq!(str_field(s, "нет"), None);
        assert_eq!(int_field(s, "t"), None);
    }

    #[test]
    fn отрицательные_и_пробелы() {
        let s = r#"{ "a" : -5 , "b" : "x" }"#;
        assert_eq!(int_field(s, "a"), Some(-5));
        assert_eq!(str_field(s, "b"), Some("x"));
    }

    #[test]
    fn не_путает_ключ_со_значением() {
        let s = r#"{"t":"c","c":9}"#;
        assert_eq!(str_field(s, "t"), Some("c"));
        assert_eq!(int_field(s, "c"), Some(9));
    }

    #[test]
    fn мусор_не_валит_разбор() {
        for bad in ["", "{", "не json", r#"{"t":}"#, r#"{"t""#, "[1,2,3]"] {
            let _ = str_field(bad, "t");
            let _ = int_field(bad, "b");
        }
    }

    #[test]
    fn экранирование_закрывает_разметку() {
        let mut out = String::new();
        push_str(&mut out, r#"<img src=x onerror=alert(1)>"#);
        assert!(!out.contains('<'), "{out}");
        assert!(out.contains("\\u003c"));

        let mut out = String::new();
        push_str(&mut out, "кавычка \" и слэш \\");
        assert_eq!(out, r#""кавычка \" и слэш \\""#);

        let mut out = String::new();
        push_str(&mut out, "\u{1}");
        assert_eq!(out, r#""\u0001""#);
    }
}
