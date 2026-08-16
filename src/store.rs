//! Хранилище игроков и лог партий.
//!
//! Ключевые отличия от JS-версии:
//! * запись игрока плотная — рейтинг в `f32`, счётчики в `u32`, скин в 4 байтах;
//! * `flush` не вызывается синхронно на каждой законченной партии
//!   (в JS это давало 76 мс блокировки event loop при 20k игроков);
//! * лидерборд кэшируется, а не пересортировывает всю карту на каждый клик.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::rating::{self, Rating};

/// Запись игрока. 4 байта на скин — форма, два цвета, эффекты.
#[derive(Clone, Debug)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub r: f32,
    pub rd: f32,
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
    pub streak: u8,
    pub best_streak: u8,
    pub last_at: u64,
    /// Скин: форма, основной цвет, дополнительный, эффекты (битовая маска).
    pub skin: [u8; 4],
    pub sparks: u32,
    /// Значки — 64 достижения в одном слове.
    pub badges: u64,
}

impl Player {
    pub fn new(id: &str, name: &str, now: u64) -> Self {
        Player {
            id: id.to_string(),
            name: name.to_string(),
            r: rating::START_R as f32,
            rd: rating::START_RD as f32,
            games: 0,
            wins: 0,
            losses: 0,
            draws: 0,
            streak: 0,
            best_streak: 0,
            last_at: now,
            skin: [0, 0, 1, 0],
            sparks: 0,
            badges: 0,
        }
    }

    pub fn rating(&self) -> Rating {
        Rating {
            r: self.r as f64,
            rd: self.rd as f64,
        }
    }

    pub fn provisional(&self) -> bool {
        rating::is_provisional(self.games, self.rd as f64)
    }

    /// Что видно игроку: значение и границы доверительного интервала.
    pub fn public(&self) -> (i32, i32, i32, bool) {
        let (lo, hi) = rating::interval(self.r as f64, self.rd as f64);
        (self.r.round() as i32, lo, hi, self.provisional())
    }

    /// Награда за партию. Поражение тоже приносит искру — иначе
    /// проигравший уходит совсем ни с чем.
    fn award_sparks(&mut self, score: f64) {
        let base = if score >= 1.0 {
            5
        } else if score > 0.0 {
            2
        } else {
            1
        };
        self.sparks = self
            .sparks
            .saturating_add(base + u32::from(self.streak.min(20)));
    }
}

/// Результат применения партии к рейтингу.
#[derive(Debug, Clone, Copy)]
pub struct Applied {
    pub before: (i32, i32),
    pub after: (i32, i32),
}

pub struct Store {
    dir: PathBuf,
    players: HashMap<String, Player>,
    dirty: bool,
    lb_cache: Option<(u64, Vec<(String, i32, bool)>)>,
}

impl Store {
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(dir.join("games"))?;
        let mut s = Store {
            dir,
            players: HashMap::new(),
            dirty: false,
            lb_cache: None,
        };
        s.load();
        Ok(s)
    }

    fn players_path(&self) -> PathBuf {
        self.dir.join("players.tsv")
    }

    /// Формат намеренно построчный TSV, а не JSON: разбор без аллокаций
    /// на каждое поле и вдвое меньше байт на игрока.
    fn load(&mut self) {
        let Ok(text) = fs::read_to_string(self.players_path()) else {
            return;
        };
        for line in text.lines() {
            if line.is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 13 {
                continue;
            }
            let p = Player {
                id: f[0].to_string(),
                name: f[1].replace("\\t", "\t"),
                r: f[2].parse().unwrap_or(1200.0),
                rd: f[3].parse().unwrap_or(350.0),
                games: f[4].parse().unwrap_or(0),
                wins: f[5].parse().unwrap_or(0),
                losses: f[6].parse().unwrap_or(0),
                draws: f[7].parse().unwrap_or(0),
                streak: f[8].parse().unwrap_or(0),
                best_streak: f[9].parse().unwrap_or(0),
                last_at: f[10].parse().unwrap_or(0),
                sparks: f[11].parse().unwrap_or(0),
                badges: f[12].parse().unwrap_or(0),
                skin: {
                    let mut s = [0u8, 0, 1, 0];
                    if let Some(v) = f.get(13) {
                        for (i, part) in v.split(',').take(4).enumerate() {
                            s[i] = part.parse().unwrap_or(s[i]);
                        }
                    }
                    s
                },
            };
            self.players.insert(p.id.clone(), p);
        }
    }

    /// Атомарная запись: сначала во временный файл, потом переименование.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let tmp = self.players_path().with_extension("tmp");
        let mut buf = String::with_capacity(self.players.len() * 96);
        for p in self.players.values() {
            buf.push_str(&format!(
                "{}\t{}\t{:.1}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{},{},{},{}\n",
                p.id,
                p.name.replace('\t', "\\t").replace('\n', " "),
                p.r,
                p.rd,
                p.games,
                p.wins,
                p.losses,
                p.draws,
                p.streak,
                p.best_streak,
                p.last_at,
                p.sparks,
                p.badges,
                p.skin[0],
                p.skin[1],
                p.skin[2],
                p.skin[3]
            ));
        }
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(buf.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, self.players_path())?;
        self.dirty = false;
        Ok(())
    }

    pub fn get_or_create(&mut self, id: &str, name: &str, now: u64) -> &mut Player {
        let e = self
            .players
            .entry(id.to_string())
            .or_insert_with(|| Player::new(id, name, now));
        if !name.is_empty() && e.name != name {
            e.name = name.to_string();
        }
        // RD растёт за время простоя — считаем лениво, при обращении
        if now > e.last_at {
            let days = (now - e.last_at) as f64 / 86_400.0;
            if days >= 1.0 {
                e.rd = rating::advance_rd(e.rd as f64, days) as f32;
            }
        }
        e
    }

    pub fn get(&self, id: &str) -> Option<&Player> {
        self.players.get(id)
    }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Player> {
        self.players.get_mut(id)
    }
    pub fn len(&self) -> usize {
        self.players.len()
    }
    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.lb_cache = None;
    }

    /// Применяет результат к обоим игрокам. Гости не рейтингуются.
    pub fn apply_game(
        &mut self,
        id_x: &str,
        name_x: &str,
        guest_x: bool,
        id_o: &str,
        name_o: &str,
        guest_o: bool,
        score_x: f64,
        now: u64,
    ) -> Option<(Applied, Applied)> {
        if guest_x || guest_o || id_x == id_o {
            return None;
        }

        let (rx, ro) = {
            let px = self.get_or_create(id_x, name_x, now);
            let rx = px.rating();
            let po = self.get_or_create(id_o, name_o, now);
            let ro = po.rating();
            (rx, ro)
        };

        // Оба считаются от ИСХОДНЫХ значений — порядок не должен влиять.
        let nx = rating::update(rx, ro, score_x);
        let no = rating::update(ro, rx, 1.0 - score_x);

        let bx = (rx.r.round() as i32, rx.rd.round() as i32);
        let bo = (ro.r.round() as i32, ro.rd.round() as i32);

        {
            let px = self.get_or_create(id_x, name_x, now);
            px.r = nx.r as f32;
            px.rd = nx.rd as f32;
            px.games += 1;
            px.last_at = now;
            if score_x >= 1.0 {
                px.wins += 1;
                px.streak = px.streak.saturating_add(1);
                px.best_streak = px.best_streak.max(px.streak);
            } else if score_x <= 0.0 {
                px.losses += 1;
                px.streak = 0;
            } else {
                px.draws += 1;
            }
            px.award_sparks(score_x);
        }
        {
            let po = self.get_or_create(id_o, name_o, now);
            po.r = no.r as f32;
            po.rd = no.rd as f32;
            po.games += 1;
            po.last_at = now;
            let so = 1.0 - score_x;
            if so >= 1.0 {
                po.wins += 1;
                po.streak = po.streak.saturating_add(1);
                po.best_streak = po.best_streak.max(po.streak);
            } else if so <= 0.0 {
                po.losses += 1;
                po.streak = 0;
            } else {
                po.draws += 1;
            }
            po.award_sparks(so);
        }

        self.mark_dirty();

        Some((
            Applied {
                before: bx,
                after: (nx.r.round() as i32, nx.rd.round() as i32),
            },
            Applied {
                before: bo,
                after: (no.r.round() as i32, no.rd.round() as i32),
            },
        ))
    }

    /// Таблица: имя, рейтинг, признак предварительного. Без telegram id —
    /// в JS-версии он утекал наружу через открытый HTTP-эндпоинт.
    pub fn leaderboard(&mut self, limit: usize, now: u64) -> Vec<(String, i32, bool)> {
        if let Some((at, rows)) = &self.lb_cache {
            if now.saturating_sub(*at) < 10 {
                return rows.iter().take(limit).cloned().collect();
            }
        }
        let mut v: Vec<&Player> = self.players.values().filter(|p| p.games > 0).collect();
        v.sort_by(|a, b| b.r.partial_cmp(&a.r).unwrap_or(std::cmp::Ordering::Equal));
        let rows: Vec<(String, i32, bool)> = v
            .iter()
            .take(50)
            .map(|p| (p.name.clone(), p.r.round() as i32, p.provisional()))
            .collect();
        self.lb_cache = Some((now, rows.clone()));
        rows.into_iter().take(limit).collect()
    }

    /// Дозапись партии в компактный лог. Ходы — байты `board*9+cell`,
    /// партия восстанавливается прогоном через правила.
    pub fn log_game(
        &self,
        day: &str,
        id: &str,
        mode: &str,
        result: &str,
        reason: &str,
        moves: &[u8],
        px: (&str, bool),
        po: (&str, bool),
    ) -> std::io::Result<()> {
        let path = self.dir.join("games").join(format!("{day}.jsonl"));
        let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
        let mv: String = moves.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(",");
        writeln!(
            f,
            r#"{{"v":2,"id":"{id}","mode":"{mode}","res":"{result}","why":"{reason}","x":["{}",{}],"o":["{}",{}],"mv":[{mv}]}}"#,
            px.0, px.1, po.0, po.1
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("uttt-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn рейтинг_переживает_перезапуск() {
        let dir = tmpdir("persist");
        {
            let mut s = Store::open(&dir).unwrap();
            s.apply_game("1", "Аня", false, "2", "Бо", false, 1.0, 100).unwrap();
            s.flush().unwrap();
        }
        let s2 = Store::open(&dir).unwrap();
        let p = s2.get("1").unwrap();
        assert!(p.r > 1200.0, "рейтинг не сохранился: {}", p.r);
        assert_eq!(p.wins, 1);
        assert_eq!(p.name, "Аня");
        assert_eq!(s2.get("2").unwrap().losses, 1);
    }

    #[test]
    fn гость_не_влияет_на_рейтинг() {
        let dir = tmpdir("guest");
        let mut s = Store::open(&dir).unwrap();
        assert!(s
            .apply_game("g_1", "Гость", true, "2", "Бо", false, 1.0, 0)
            .is_none());
        assert!(s.get("2").is_none(), "запись для соперника гостя создана");
    }

    #[test]
    fn обе_стороны_считаются_от_исходных_значений() {
        let dir = tmpdir("sym");
        let mut s = Store::open(&dir).unwrap();
        let (ax, ao) = s.apply_game("1", "A", false, "2", "B", false, 1.0, 0).unwrap();
        assert!(ax.after.0 > ax.before.0);
        assert!(ao.after.0 < ao.before.0);
        // при равных стартовых значениях сдвиги зеркальны
        assert_eq!(ax.after.0 - ax.before.0, ao.before.0 - ao.after.0);
    }

    #[test]
    fn серия_растёт_и_обнуляется() {
        let dir = tmpdir("streak");
        let mut s = Store::open(&dir).unwrap();
        for _ in 0..3 {
            s.apply_game("1", "A", false, "2", "B", false, 1.0, 0);
        }
        assert_eq!(s.get("1").unwrap().streak, 3);
        assert_eq!(s.get("1").unwrap().best_streak, 3);
        s.apply_game("1", "A", false, "2", "B", false, 0.0, 0);
        assert_eq!(s.get("1").unwrap().streak, 0);
        assert_eq!(s.get("1").unwrap().best_streak, 3, "рекорд потерян");
    }

    #[test]
    fn искры_даются_даже_за_поражение() {
        let dir = tmpdir("sparks");
        let mut s = Store::open(&dir).unwrap();
        s.apply_game("1", "A", false, "2", "B", false, 1.0, 0);
        assert!(s.get("2").unwrap().sparks >= 1, "проигравший ушёл ни с чем");
        assert!(s.get("1").unwrap().sparks > s.get("2").unwrap().sparks);
    }

    #[test]
    fn лидерборд_без_telegram_id_и_с_кэшем() {
        let dir = tmpdir("lb");
        let mut s = Store::open(&dir).unwrap();
        s.apply_game("111111111", "Аня", false, "222222222", "Бо", false, 1.0, 0);
        let rows = s.leaderboard(10, 0);
        assert_eq!(rows[0].0, "Аня");
        assert!(!rows.iter().any(|r| r.0.contains("111111111")));
        // кэш отдаёт то же самое в пределах десяти секунд
        assert_eq!(s.leaderboard(10, 5), rows);
    }

    #[test]
    fn имя_с_табуляцией_не_ломает_формат() {
        let dir = tmpdir("tsv");
        {
            let mut s = Store::open(&dir).unwrap();
            s.get_or_create("1", "А\tБ\nВ", 0);
            s.mark_dirty();
            s.flush().unwrap();
        }
        let s2 = Store::open(&dir).unwrap();
        assert!(s2.get("1").is_some(), "запись потеряна");
        assert!(!s2.get("1").unwrap().name.contains('\n'));
    }

    #[test]
    fn партия_пишется_в_лог_и_читается_обратно() {
        let dir = tmpdir("log");
        let s = Store::open(&dir).unwrap();
        let moves: Vec<u8> = vec![40, 4, 36, 0, 3, 30];
        s.log_game("2026-08-16", "abc", "ranked", "X", "mate", &moves, ("1", false), ("2", false))
            .unwrap();
        let text = fs::read_to_string(dir.join("games/2026-08-16.jsonl")).unwrap();
        assert!(text.contains(r#""mv":[40,4,36,0,3,30]"#), "{text}");
        assert!(text.lines().count() == 1);
    }

    #[test]
    fn простой_поднимает_rd_при_чтении() {
        let dir = tmpdir("idle");
        let mut s = Store::open(&dir).unwrap();
        {
            let p = s.get_or_create("1", "A", 0);
            p.rd = 45.0;
            p.games = 30;
        }
        let day = 86_400;
        let p = s.get_or_create("1", "A", day * 40);
        assert!(p.rd > 45.0, "RD не выросла: {}", p.rd);
        assert!(p.rd <= rating::IDLE_RD_CAP as f32 + 0.01);
    }
}
