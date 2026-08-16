//! Ограничитель частоты на соединение.
//!
//! Цена зависит от типа: ход стоит 1, таблица рейтинга — 10, потому что
//! это единственная операция, которая трогает всех игроков сразу.
//! Ведро хранится в двух числах и не требует таймеров.

/// Ёмкость ведра в токенах.
pub const CAPACITY: u32 = 120;
/// За сколько миллисекунд ведро наполняется целиком.
pub const REFILL_MS: u64 = 10_000;

/// Цена сообщения. Неизвестные типы стоят как ход — их всё равно отвергнут.
pub fn cost(kind: &str) -> u32 {
    match kind {
        "top" => 10,
        "invite" => 5,
        "hello" | "queue" | "join" => 3,
        _ => 1,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Bucket {
    /// Токены с точностью до тысячных, чтобы обойтись целочисленной арифметикой.
    milli: u64,
    last_ms: u64,
}

impl Bucket {
    pub fn new(now_ms: u64) -> Self {
        Bucket {
            milli: CAPACITY as u64 * 1000,
            last_ms: now_ms,
        }
    }

    /// Пытается списать цену. `false` — превышение.
    pub fn take(&mut self, cost: u32, now_ms: u64) -> bool {
        let dt = now_ms.saturating_sub(self.last_ms);
        self.last_ms = now_ms;

        if dt > 0 {
            let refill = dt.saturating_mul(CAPACITY as u64 * 1000) / REFILL_MS;
            self.milli = (self.milli + refill).min(CAPACITY as u64 * 1000);
        }

        let need = cost as u64 * 1000;
        if self.milli < need {
            return false;
        }
        self.milli -= need;
        true
    }

    /// Сколько целых токенов доступно — для диагностики.
    pub fn tokens(&self) -> u32 {
        (self.milli / 1000) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn обычная_партия_не_упирается_в_лимит() {
        // человек делает ~40 ходов за партию, самая быстрая — секунд за 40
        let mut b = Bucket::new(0);
        let mut t = 0;
        for _ in 0..40 {
            t += 1000;
            assert!(b.take(cost("move"), t), "ход отвергнут на {t} мс");
        }
    }

    #[test]
    fn блиц_тоже_проходит() {
        // 5 тапов в секунду в течение 20 секунд — быстрее человек не может
        let mut b = Bucket::new(0);
        let mut ok = 0;
        for i in 0..100u64 {
            if b.take(1, i * 200) {
                ok += 1;
            }
        }
        assert_eq!(ok, 100, "заблокирован живой игрок");
    }

    #[test]
    fn шквал_отсекается() {
        let mut b = Bucket::new(0);
        let mut ok = 0;
        for _ in 0..1000 {
            if b.take(1, 0) {
                ok += 1;
            }
        }
        assert_eq!(ok, CAPACITY as usize, "ведро не ограничило поток");
    }

    #[test]
    fn таблица_рейтинга_дороже_хода() {
        assert_eq!(cost("top"), 10);
        assert!(cost("top") > cost("move"));
        let mut b = Bucket::new(0);
        let mut ok = 0;
        for _ in 0..100 {
            if b.take(cost("top"), 0) {
                ok += 1;
            }
        }
        assert_eq!(ok, 12, "дорогую операцию можно звать слишком часто");
    }

    #[test]
    fn ведро_наполняется_со_временем() {
        let mut b = Bucket::new(0);
        while b.take(1, 0) {}
        assert!(!b.take(1, 0));
        // за половину периода возвращается половина ёмкости
        assert!(b.take(1, REFILL_MS / 2));
        assert!(b.tokens() >= CAPACITY / 2 - 2);
        // и не больше ёмкости даже за сутки
        b.take(1, 86_400_000);
        assert!(b.tokens() <= CAPACITY);
    }

    #[test]
    fn часы_назад_не_ломают_счёт() {
        let mut b = Bucket::new(10_000);
        assert!(b.take(1, 5_000));
        assert!(b.tokens() <= CAPACITY);
    }
}
