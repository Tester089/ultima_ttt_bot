//! Статика, вшитая в бинарник.
//!
//! Файлы пожаты на этапе сборки и лежат в секции данных. На запрос уходит
//! только запись готового среза: ни `open`, ни `stat`, ни сжатия в рантайме.
//! На урезанном процессоре это снимает всю работу с горячего пути отдачи.
//!
//! Темы вынесены отдельными чанками и качаются по требованию: игроку нужна
//! одна тема, а не все шесть.

pub struct Asset {
    pub path: &'static str,
    pub mime: &'static str,
    pub body: &'static [u8],
    /// `true` — тело уже в gzip. Для woff2 это `false`: шрифт сжат
    /// внутри формата, и повторное сжатие только жгло бы процессор.
    pub gzipped: bool,
    /// Сколько браузеру держать файл в кэше.
    pub max_age: u32,
}

pub const ASSETS: &[Asset] = &[
    Asset {
        path: "/",
        mime: "text/html; charset=utf-8",
        body: include_bytes!("../assets/index.html.gz"),
        gzipped: true,
        max_age: 0,
    },
    Asset {
        path: "/index.html",
        mime: "text/html; charset=utf-8",
        body: include_bytes!("../assets/index.html.gz"),
        gzipped: true,
        max_age: 0,
    },
    Asset {
        path: "/tg.js",
        mime: "application/javascript",
        body: include_bytes!("../assets/tg.js.gz"),
        gzipped: true,
        max_age: 3600,
    },
    Asset {
        path: "/t/grafit.json",
        mime: "application/json",
        body: include_bytes!("../assets/themes/grafit.json.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/t/kamen.json",
        mime: "application/json",
        body: include_bytes!("../assets/themes/kamen.json.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/t/linza.json",
        mime: "application/json",
        body: include_bytes!("../assets/themes/linza.json.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/t/linzaDark.json",
        mime: "application/json",
        body: include_bytes!("../assets/themes/linzaDark.json.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/t/list.json",
        mime: "application/json",
        body: include_bytes!("../assets/themes/list.json.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/t/myata.json",
        mime: "application/json",
        body: include_bytes!("../assets/themes/myata.json.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/f/archivo-latin-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/archivo-latin-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/archivo-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/archivo-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/comfortaa-cyrillic-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/comfortaa-cyrillic-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/comfortaa-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/comfortaa-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/commissioner-cyrillic-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/commissioner-cyrillic-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/commissioner-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/commissioner-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/grafit.css",
        mime: "text/css",
        body: include_bytes!("../assets/fonts/grafit.css.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/f/ibm-plex-mono-cyrillic-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/ibm-plex-mono-cyrillic-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/ibm-plex-mono-latin-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/ibm-plex-mono-latin-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/ibm-plex-sans-cyrillic-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/ibm-plex-sans-cyrillic-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/ibm-plex-sans-cyrillic-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/ibm-plex-sans-cyrillic-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/ibm-plex-sans-latin-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/ibm-plex-sans-latin-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/ibm-plex-sans-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/ibm-plex-sans-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/inter-tight-cyrillic-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/inter-tight-cyrillic-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/inter-tight-cyrillic-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/inter-tight-cyrillic-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/inter-tight-latin-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/inter-tight-latin-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/inter-tight-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/inter-tight-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/kamen.css",
        mime: "text/css",
        body: include_bytes!("../assets/fonts/kamen.css.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/f/linza.css",
        mime: "text/css",
        body: include_bytes!("../assets/fonts/linza.css.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/f/linzaDark.css",
        mime: "text/css",
        body: include_bytes!("../assets/fonts/linzaDark.css.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/f/list.css",
        mime: "text/css",
        body: include_bytes!("../assets/fonts/list.css.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/f/manrope-cyrillic-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/manrope-cyrillic-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/manrope-cyrillic-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/manrope-cyrillic-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/manrope-latin-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/manrope-latin-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/manrope-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/manrope-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/myata.css",
        mime: "text/css",
        body: include_bytes!("../assets/fonts/myata.css.gz"),
        gzipped: true,
        max_age: 31536000,
    },
    Asset {
        path: "/f/nunito-cyrillic-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/nunito-cyrillic-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/nunito-cyrillic-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/nunito-cyrillic-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/nunito-latin-400-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/nunito-latin-400-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/nunito-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/nunito-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/roboto-slab-cyrillic-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/roboto-slab-cyrillic-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/roboto-slab-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/roboto-slab-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/unbounded-cyrillic-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/unbounded-cyrillic-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
    Asset {
        path: "/f/unbounded-latin-600-normal.woff2",
        mime: "font/woff2",
        body: include_bytes!("../assets/fonts/unbounded-latin-600-normal.woff2"),
        gzipped: false,
        max_age: 31536000,
    },
];

pub fn find(path: &str) -> Option<&'static Asset> {
    // Путь очищается от query до вызова; линейный поиск по восьми записям
    // быстрее любой карты и не требует инициализации.
    ASSETS.iter().find(|a| a.path == path)
}

/// Собирает полный ответ. Заголовки пишутся в переданный буфер.
pub fn respond(out: &mut Vec<u8>, a: &Asset) {
    out.clear();
    let cache = if a.max_age == 0 {
        "no-cache".to_string()
    } else {
        format!("public, max-age={}, immutable", a.max_age)
    };
    let enc = if a.gzipped { "Content-Encoding: gzip\r\n" } else { "" };
    out.extend_from_slice(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {}\r\n{}Content-Length: {}\r\n\
             Cache-Control: {}\r\nConnection: close\r\n\r\n",
            a.mime,
            enc,
            a.body.len(),
            cache
        )
        .as_bytes(),
    );
    out.extend_from_slice(a.body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn все_ассеты_на_месте_и_пожаты() {
        for a in ASSETS {
            assert!(!a.body.is_empty(), "{} пуст", a.path);
            if a.gzipped {
                assert_eq!(&a.body[..2], &[0x1f, 0x8b], "{} помечен gzip, но не пожат", a.path);
            } else {
                // woff2 начинается с сигнатуры wOF2
                assert_eq!(&a.body[..4], b"wOF2", "{} не woff2", a.path);
            }
        }
        assert!(find("/").is_some());
        assert!(find("/t/kamen.json").is_some());
        assert!(find("/нет").is_none());
    }

    #[test]
    fn темы_лежат_отдельными_чанками() {
        let themes: Vec<_> = ASSETS.iter().filter(|a| a.path.starts_with("/t/")).collect();
        assert_eq!(themes.len(), 6, "тем должно быть шесть");
        let total: usize = themes.iter().map(|a| a.body.len()).sum();
        // все шесть вместе меньше трёх килобайт — но качается всегда одна
        assert!(total < 3000, "темы весят {total} байт");
        for t in themes {
            assert!(t.body.len() < 600, "{} слишком тяжёлая", t.path);
        }
    }

    #[test]
    fn шрифты_отдаются_без_повторного_сжатия() {
        let fonts: Vec<_> = ASSETS.iter().filter(|a| a.path.ends_with(".woff2")).collect();
        assert!(fonts.len() >= 20, "шрифтов всего {}", fonts.len());
        for f in &fonts {
            assert!(!f.gzipped, "{} жмётся повторно", f.path);
            assert!(f.max_age > 1_000_000, "{} не кэшируется", f.path);
        }
        let mut out = Vec::new();
        respond(&mut out, fonts[0]);
        let head = String::from_utf8_lossy(&out[..200.min(out.len())]).into_owned();
        assert!(!head.contains("Content-Encoding"), "у woff2 лишний Content-Encoding");
        assert!(head.contains("font/woff2"));
    }

    #[test]
    fn у_каждой_темы_есть_свой_набор_шрифтов() {
        for t in ["linza", "linzaDark", "kamen", "myata", "list", "grafit"] {
            assert!(find(&format!("/t/{t}.json")).is_some(), "нет токенов {t}");
            assert!(find(&format!("/f/{t}.css")).is_some(), "нет шрифтов {t}");
        }
    }

    #[test]
    fn индекс_не_кэшируется_а_темы_навсегда() {
        assert_eq!(find("/").unwrap().max_age, 0);
        assert!(find("/t/linza.json").unwrap().max_age > 1_000_000);
    }

    #[test]
    fn ответ_содержит_длину_и_кодировку() {
        let a = find("/t/linza.json").unwrap();
        let mut out = Vec::new();
        respond(&mut out, a);
        let head = String::from_utf8_lossy(&out[..out.len().min(220)]).into_owned();
        assert!(head.starts_with("HTTP/1.1 200 OK"));
        assert!(head.contains("Content-Encoding: gzip"));
        assert!(head.contains(&format!("Content-Length: {}", a.body.len())));
        assert!(head.contains("immutable"));
    }
}
