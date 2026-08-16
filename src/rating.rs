//! Рейтинг Glicko-1.
//!
//! Формулы сверены с эталонным примером Гликмана. Отличие от JS-версии
//! только в калибровке простоя: там `c = 34` разгонял RD так, что ветеран
//! становился «предварительным» уже через девять дней без игр.

pub const START_R: f64 = 1200.0;
pub const START_RD: f64 = 350.0;
pub const MIN_RD: f64 = 45.0;
pub const MAX_RD: f64 = 350.0;
/// Потолок роста RD от простоя: вернувшийся игрок не должен обнуляться до новичка.
pub const IDLE_RD_CAP: f64 = 200.0;
/// Прирост RD за сутки простоя. При c=12 порог `provisional` достигается за ~70 дней.
pub const IDLE_C: f64 = 12.0;

const Q: f64 = 0.0057564627324851142; // ln(10)/400
const PI2: f64 = std::f64::consts::PI * std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rating {
    pub r: f64,
    pub rd: f64,
}

impl Default for Rating {
    fn default() -> Self {
        Rating {
            r: START_R,
            rd: START_RD,
        }
    }
}

#[inline]
fn clamp_rd(rd: f64) -> f64 {
    rd.clamp(MIN_RD, MAX_RD)
}

#[inline]
fn g(rd: f64) -> f64 {
    1.0 / (1.0 + 3.0 * Q * Q * rd * rd / PI2).sqrt()
}

#[inline]
fn expected(r: f64, rj: f64, rdj: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf(-g(rdj) * (r - rj) / 400.0))
}

/// Один результат: `score` = 1 победа, 0.5 ничья, 0 поражение.
pub fn update(me: Rating, opp: Rating, score: f64) -> Rating {
    let gj = g(opp.rd);
    let e = expected(me.r, opp.r, opp.rd);
    let d2 = 1.0 / (Q * Q * gj * gj * e * (1.0 - e));
    let rd2 = me.rd * me.rd;
    let denom = 1.0 / rd2 + 1.0 / d2;

    Rating {
        r: me.r + (Q / denom) * gj * (score - e),
        rd: clamp_rd((1.0 / denom).sqrt()),
    }
}

/// Рост неопределённости за время без игр.
pub fn advance_rd(rd: f64, days_idle: f64) -> f64 {
    if days_idle <= 0.0 {
        return clamp_rd(rd);
    }
    let next = (rd * rd + IDLE_C * IDLE_C * days_idle).sqrt();
    clamp_rd(next.min(IDLE_RD_CAP))
}

/// Рейтинг ещё «плавает»: показываем со знаком вопроса.
pub fn is_provisional(games: u32, rd: f64) -> bool {
    games < 10 || rd > 110.0
}

/// Доверительный интервал для карточки в лобби.
/// Возвращает границы, а не сам RD: игроку понятнее «1210…1284».
pub fn interval(r: f64, rd: f64) -> (i32, i32) {
    let half = 0.9 * rd;
    ((r - half).round() as i32, (r + half).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Эталон Гликмана: r=1500, RD=200 против 1400/30, 1550/100, 1700/300
    /// за один рейтинговый период даёт r≈1464.1, RD≈151.4.
    fn reference_period(r: f64, rd: f64, opps: &[(f64, f64, f64)]) -> (f64, f64) {
        let mut dsum = 0.0;
        let mut num = 0.0;
        for &(rj, rdj, s) in opps {
            let gj = g(rdj);
            let ej = expected(r, rj, rdj);
            dsum += gj * gj * ej * (1.0 - ej);
            num += gj * (s - ej);
        }
        let d2 = 1.0 / (Q * Q * dsum);
        let denom = 1.0 / (rd * rd) + 1.0 / d2;
        (r + (Q / denom) * num, (1.0 / denom).sqrt())
    }

    #[test]
    fn совпадает_с_эталоном_на_одной_партии() {
        let (rr, rrd) = reference_period(1500.0, 200.0, &[(1400.0, 30.0, 1.0)]);
        let got = update(Rating { r: 1500.0, rd: 200.0 }, Rating { r: 1400.0, rd: 30.0 }, 1.0);
        assert!((got.r - rr).abs() < 0.05, "r {} vs {}", got.r, rr);
        assert!((got.rd - rrd).abs() < 0.05, "rd {} vs {}", got.rd, rrd);
        assert!((got.r - 1563.4).abs() < 0.1);
        assert!((got.rd - 175.2).abs() < 0.1);
    }

    #[test]
    fn три_партии_подряд_близки_к_периоду() {
        let mut p = Rating { r: 1500.0, rd: 200.0 };
        for &(rj, rdj, s) in &[(1400.0, 30.0, 1.0), (1550.0, 100.0, 0.0), (1700.0, 300.0, 0.0)] {
            p = update(p, Rating { r: rj, rd: rdj }, s);
        }
        // последовательное обновление ≠ период, но расхождение должно быть малым
        assert!((p.r - 1464.1).abs() < 1.0, "r = {}", p.r);
        assert!((p.rd - 151.4).abs() < 1.0, "rd = {}", p.rd);
    }

    #[test]
    fn победа_поднимает_поражение_опускает() {
        let base = Rating { r: 1200.0, rd: 100.0 };
        let opp = Rating { r: 1200.0, rd: 100.0 };
        assert!(update(base, opp, 1.0).r > 1200.0);
        assert!(update(base, opp, 0.0).r < 1200.0);
    }

    #[test]
    fn ничья_между_равными_почти_не_двигает() {
        let p = Rating { r: 1200.0, rd: 60.0 };
        let d = update(p, p, 0.5).r - 1200.0;
        assert!(d.abs() < 1.0, "сдвиг {d}");
    }

    #[test]
    fn rd_только_убывает_от_игр_и_не_ниже_пола() {
        let mut p = Rating { r: 1200.0, rd: 350.0 };
        for i in 0..40 {
            let n = update(p, Rating { r: 1200.0, rd: 60.0 }, (i % 2) as f64);
            assert!(n.rd <= p.rd + 1e-9, "RD выросла от игры");
            p = n;
        }
        assert!(p.rd >= MIN_RD - 1e-9);
    }

    #[test]
    fn простой_не_превращает_ветерана_в_новичка() {
        assert!(!is_provisional(30, advance_rd(45.0, 30.0)));
        assert!(advance_rd(45.0, 100_000.0) <= IDLE_RD_CAP + 1e-9);
        // порог provisional достигается не раньше двух месяцев
        assert!(!is_provisional(30, advance_rd(45.0, 60.0)));
        assert!(is_provisional(30, advance_rd(45.0, 120.0)));
    }

    #[test]
    fn provisional_пока_мало_партий() {
        assert!(is_provisional(3, 60.0));
        assert!(!is_provisional(10, 100.0));
        assert!(is_provisional(50, 120.0));
    }

    #[test]
    fn интервал_симметричен_и_достижим() {
        let (lo, hi) = interval(1247.0, 45.0);
        // симметрия с точностью до округления
        assert!(((1247 - lo) - (hi - 1247)).abs() <= 1);
        // при поле RD=45 полуширина ≈41, поэтому «±37» из макета недостижим:
        // либо опускать MIN_RD, либо показывать не сам RD
        assert!(hi - 1247 >= 40, "полуширина {}", hi - 1247);
        // а вот у новичка интервал широкий
        let (lo2, hi2) = interval(1200.0, 350.0);
        assert!(hi2 - lo2 > 600, "ширина {}", hi2 - lo2);
    }

    #[test]
    fn новичок_быстро_находит_уровень() {
        // 20 партий против 1600/50 с долей побед 80% должны поднять к 1500+
        let mut p = Rating::default();
        let opp = Rating { r: 1600.0, rd: 50.0 };
        for i in 0..20 {
            p = update(p, opp, if i % 5 == 0 { 0.0 } else { 1.0 });
        }
        assert!(p.r > 1500.0, "r = {}", p.r);
        assert!(p.rd < 110.0, "rd = {}", p.rd);
    }
}
