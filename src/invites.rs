//! Приглашения по коду.
//!
//! В JS-версии коллизия кода молча передавала чужой инвайт другому хосту:
//! `byCode.set(c, inv)` перезаписывал запись, а `byHost` старого владельца
//! продолжал на неё указывать. Здесь при занятом коде идёт повтор.

use std::collections::HashMap;

/// Алфавит без похожих глифов: нет `I`, `O`, `0`, `1`.
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const CODE_LEN: usize = 6;
pub const TTL_SECS: u64 = 15 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub code: String,
    pub host: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinError {
    NotFound,
    Expired,
    Self_,
}

/// Простой детерминированный ГПСЧ: криптостойкость тут не нужна,
/// код живёт 15 минут и защищён проверкой хоста.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

pub struct Invites {
    by_code: HashMap<String, Invite>,
    by_host: HashMap<String, String>,
    rng: Rng,
}

impl Invites {
    pub fn new(seed: u64) -> Self {
        Invites {
            by_code: HashMap::new(),
            by_host: HashMap::new(),
            rng: Rng::new(seed),
        }
    }

    fn gen_code(&mut self) -> String {
        let mut s = String::with_capacity(CODE_LEN);
        for _ in 0..CODE_LEN {
            let n = self.rng.next() as usize % ALPHABET.len();
            s.push(ALPHABET[n] as char);
        }
        s
    }

    /// Убирает протухшее. Вызывается лениво при каждом обращении.
    pub fn expire(&mut self, now: u64) {
        let dead: Vec<String> = self
            .by_code
            .iter()
            .filter(|(_, v)| v.expires_at <= now)
            .map(|(k, _)| k.clone())
            .collect();
        for code in dead {
            if let Some(inv) = self.by_code.remove(&code) {
                if self.by_host.get(&inv.host) == Some(&code) {
                    self.by_host.remove(&inv.host);
                }
            }
        }
    }

    /// Новый инвайт хоста. Старый его инвайт аннулируется.
    pub fn create(&mut self, host: &str, now: u64) -> Option<Invite> {
        self.expire(now);
        self.cancel(host);

        let mut code = self.gen_code();
        let mut tries = 0;
        while self.by_code.contains_key(&code) {
            if tries >= 8 {
                return None;
            }
            code = self.gen_code();
            tries += 1;
        }

        let inv = Invite {
            code: code.clone(),
            host: host.to_string(),
            expires_at: now + TTL_SECS,
        };
        self.by_code.insert(code.clone(), inv.clone());
        self.by_host.insert(host.to_string(), code);
        Some(inv)
    }

    pub fn cancel(&mut self, host: &str) {
        if let Some(code) = self.by_host.remove(host) {
            self.by_code.remove(&code);
        }
    }

    /// Забрать инвайт: одноразово.
    pub fn consume(&mut self, code: &str, guest: &str, now: u64) -> Result<Invite, JoinError> {
        self.expire(now);
        let code = code.to_ascii_uppercase();
        if code.len() != CODE_LEN || !code.bytes().all(|b| ALPHABET.contains(&b)) {
            return Err(JoinError::NotFound);
        }
        let inv = self.by_code.get(&code).cloned().ok_or(JoinError::NotFound)?;
        if inv.expires_at <= now {
            return Err(JoinError::Expired);
        }
        if inv.host == guest {
            return Err(JoinError::Self_);
        }
        self.by_code.remove(&code);
        if self.by_host.get(&inv.host) == Some(&code) {
            self.by_host.remove(&inv.host);
        }
        Ok(inv)
    }

    pub fn len(&self) -> usize {
        self.by_code.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_code.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn код_из_разрешённых_символов() {
        let mut inv = Invites::new(42);
        for i in 0..500 {
            let c = inv.create(&format!("h{i}"), 0).unwrap().code;
            assert_eq!(c.len(), CODE_LEN);
            assert!(
                c.bytes().all(|b| ALPHABET.contains(&b)),
                "запрещённый символ в {c}"
            );
            assert!(!c.contains('I') && !c.contains('O') && !c.contains('0') && !c.contains('1'));
        }
    }

    #[test]
    fn коллизия_не_уводит_чужой_инвайт() {
        let mut inv = Invites::new(7);
        let a = inv.create("host_a", 0).unwrap();
        // подсовываем занятый код напрямую
        let b = inv.create("host_b", 0).unwrap();
        assert_ne!(a.code, b.code);
        // инвайт первого хоста цел
        assert_eq!(inv.by_code.get(&a.code).unwrap().host, "host_a");
    }

    #[test]
    fn инвайт_одноразовый() {
        let mut inv = Invites::new(1);
        let a = inv.create("h", 0).unwrap();
        assert!(inv.consume(&a.code, "g", 1).is_ok());
        assert_eq!(inv.consume(&a.code, "g", 1), Err(JoinError::NotFound));
    }

    #[test]
    fn себе_нельзя() {
        let mut inv = Invites::new(2);
        let a = inv.create("h", 0).unwrap();
        assert_eq!(inv.consume(&a.code, "h", 1), Err(JoinError::Self_));
        // и инвайт при этом не сгорел
        assert!(inv.consume(&a.code, "other", 1).is_ok());
    }

    #[test]
    fn истёкший_не_находится() {
        let mut inv = Invites::new(3);
        let a = inv.create("h", 1000).unwrap();
        assert_eq!(
            inv.consume(&a.code, "g", 1000 + TTL_SECS + 1),
            Err(JoinError::NotFound)
        );
        assert!(inv.is_empty(), "протухшее не убрано");
    }

    #[test]
    fn новый_инвайт_гасит_старый() {
        let mut inv = Invites::new(4);
        let a = inv.create("h", 0).unwrap();
        let b = inv.create("h", 0).unwrap();
        assert_eq!(inv.consume(&a.code, "g", 1), Err(JoinError::NotFound));
        assert!(inv.consume(&b.code, "g", 1).is_ok());
        assert_eq!(inv.len(), 0);
    }

    #[test]
    fn мусорный_код_отвергается_без_поиска() {
        let mut inv = Invites::new(5);
        for bad in ["", "abc", "AAAAAAA", "AAAA0A", "<script>", "ЙЦУКЕН"] {
            assert_eq!(inv.consume(bad, "g", 0), Err(JoinError::NotFound), "{bad}");
        }
    }

    #[test]
    fn регистр_кода_не_важен() {
        let mut inv = Invites::new(6);
        let a = inv.create("h", 0).unwrap();
        assert!(inv.consume(&a.code.to_lowercase(), "g", 1).is_ok());
    }

    #[test]
    fn отмена_чистит_обе_карты() {
        let mut inv = Invites::new(8);
        let a = inv.create("h", 0).unwrap();
        inv.cancel("h");
        assert!(inv.is_empty());
        assert_eq!(inv.consume(&a.code, "g", 1), Err(JoinError::NotFound));
    }
}
