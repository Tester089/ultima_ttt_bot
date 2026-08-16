//! Правила Ultimate Tic-Tac-Toe на битбордах.
//!
//! Всё состояние партии — 40 байт и ни одной аллокации. В JS-версии
//! одна комната стоила около 3 КБ объектов; здесь `Game` целиком
//! помещается в три кэш-линии, а проверка победы — обращение к таблице.

/// Восемь линий как маски по 9 бит (индекс 0 — левый верхний угол).
const LINES: [u16; 8] = [
    0b000_000_111,
    0b000_111_000,
    0b111_000_000,
    0b001_001_001,
    0b010_010_010,
    0b100_100_100,
    0b100_010_001,
    0b001_010_100,
];

/// Таблица «есть ли линия» для всех 512 расположений девяти клеток.
/// Считается на этапе компиляции, в рантайме — один индекс в массив.
const WIN: [bool; 512] = {
    let mut t = [false; 512];
    let mut m = 0usize;
    while m < 512 {
        let mut i = 0;
        while i < 8 {
            let line = LINES[i] as usize;
            if m & line == line {
                t[m] = true;
            }
            i += 1;
        }
        m += 1;
    }
    t
};

/// Полное 9-битное поле.
const FULL: u16 = 0b1_1111_1111;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    X,
    O,
}

impl Side {
    #[inline]
    pub fn other(self) -> Side {
        match self {
            Side::X => Side::O,
            Side::O => Side::X,
        }
    }
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Side::X => "X",
            Side::O => "O",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Win(Side),
    Draw,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Win(s) => s.as_str(),
            Outcome::Draw => "-",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveError {
    BadIndex,
    Illegal,
}

/// Состояние партии. 40 байт.
#[derive(Clone, Copy, Debug)]
pub struct Game {
    /// Клетки X: 81 бит, поле `b` занимает биты `b*9 .. b*9+8`.
    x: u128,
    /// Клетки O.
    o: u128,
    /// Малые поля, выигранные X / O, и сведённые вничью — по 9 бит.
    won_x: u16,
    won_o: u16,
    drawn: u16,
    /// Обязательное поле, 9 = свободный ход.
    forced: u8,
    turn: Side,
    over: Option<Outcome>,
    moves: u8,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub const fn new() -> Self {
        Game {
            x: 0,
            o: 0,
            won_x: 0,
            won_o: 0,
            drawn: 0,
            forced: 9,
            turn: Side::X,
            over: None,
            moves: 0,
        }
    }

    #[inline]
    pub fn turn(&self) -> Side {
        self.turn
    }
    #[inline]
    pub fn over(&self) -> Option<Outcome> {
        self.over
    }
    #[inline]
    pub fn moves(&self) -> u8 {
        self.moves
    }
    /// `None` — свободный ход.
    #[inline]
    pub fn forced(&self) -> Option<u8> {
        if self.forced == 9 {
            None
        } else {
            Some(self.forced)
        }
    }

    /// Занятые клетки малого поля (маска 9 бит).
    #[inline]
    fn occupied(&self, board: u8) -> u16 {
        let sh = board as u32 * 9;
        (((self.x | self.o) >> sh) as u16) & FULL
    }

    #[inline]
    fn board_mask(&self, side: Side, board: u8) -> u16 {
        let sh = board as u32 * 9;
        let bits = match side {
            Side::X => self.x,
            Side::O => self.o,
        };
        ((bits >> sh) as u16) & FULL
    }

    /// Решено ли малое поле — выиграно кем-то или сведено вничью.
    #[inline]
    fn decided(&self) -> u16 {
        self.won_x | self.won_o | self.drawn
    }

    /// Знак в клетке: `None` — пусто.
    pub fn cell(&self, board: u8, cell: u8) -> Option<Side> {
        if board > 8 || cell > 8 {
            return None;
        }
        let bit = 1u128 << (board as u32 * 9 + cell as u32);
        if self.x & bit != 0 {
            Some(Side::X)
        } else if self.o & bit != 0 {
            Some(Side::O)
        } else {
            None
        }
    }

    /// Итог малого поля.
    pub fn board_result(&self, board: u8) -> Option<Outcome> {
        if board > 8 {
            return None;
        }
        let b = 1u16 << board;
        if self.won_x & b != 0 {
            Some(Outcome::Win(Side::X))
        } else if self.won_o & b != 0 {
            Some(Outcome::Win(Side::O))
        } else if self.drawn & b != 0 {
            Some(Outcome::Draw)
        } else {
            None
        }
    }

    /// Можно ли ходить в это малое поле.
    #[inline]
    pub fn playable(&self, board: u8) -> bool {
        if board > 8 || self.over.is_some() {
            return false;
        }
        if self.decided() & (1u16 << board) != 0 {
            return false;
        }
        self.forced == 9 || self.forced == board
    }

    /// Есть ли хоть один легальный ход.
    pub fn has_move(&self) -> bool {
        if self.over.is_some() {
            return false;
        }
        for b in 0..9u8 {
            if self.playable(b) && self.occupied(b) != FULL {
                return true;
            }
        }
        false
    }

    /// Маска легальных клеток малого поля (0, если поле недоступно).
    #[inline]
    pub fn legal_cells(&self, board: u8) -> u16 {
        if !self.playable(board) {
            0
        } else {
            !self.occupied(board) & FULL
        }
    }

    /// Ход. Индексы вне 0..=8 отвергаются до касания состояния —
    /// именно на этом в JS-версии подвешивалась партия.
    pub fn play(&mut self, board: u8, cell: u8) -> Result<(), MoveError> {
        if board > 8 || cell > 8 {
            return Err(MoveError::BadIndex);
        }
        if !self.playable(board) {
            return Err(MoveError::Illegal);
        }
        let bit = 1u128 << (board as u32 * 9 + cell as u32);
        if (self.x | self.o) & bit != 0 {
            return Err(MoveError::Illegal);
        }

        let side = self.turn;
        match side {
            Side::X => self.x |= bit,
            Side::O => self.o |= bit,
        }
        self.moves += 1;

        // Итог малого поля.
        let mine = self.board_mask(side, board);
        let bbit = 1u16 << board;
        if WIN[mine as usize] {
            match side {
                Side::X => self.won_x |= bbit,
                Side::O => self.won_o |= bbit,
            }
        } else if self.occupied(board) == FULL {
            self.drawn |= bbit;
        }

        // Итог большого поля. Ничейные поля НЕ образуют линию: три ничьи
        // подряд — не конец партии. В JS-версии это был живой баг.
        let big = match side {
            Side::X => self.won_x,
            Side::O => self.won_o,
        };
        if WIN[big as usize] {
            self.over = Some(Outcome::Win(side));
        } else if self.decided() == FULL {
            self.over = Some(Outcome::Draw);
        }

        // Куда отправлен соперник.
        self.forced = if self.over.is_some() || self.decided() & (1u16 << cell) != 0 {
            9
        } else {
            cell
        };
        self.turn = side.other();

        // Пат: ходов нет, а результата ещё нет.
        if self.over.is_none() && !self.has_move() {
            self.over = Some(Outcome::Draw);
        }
        Ok(())
    }

    /// Сколько малых полей взято стороной — для значков и статистики.
    pub fn boards_won(&self, side: Side) -> u32 {
        match side {
            Side::X => self.won_x.count_ones(),
            Side::O => self.won_o.count_ones(),
        }
    }

    /// Воспроизведение партии из компактного лога (`board*9+cell`).
    pub fn replay(moves: &[u8]) -> Result<Game, (usize, MoveError)> {
        let mut g = Game::new();
        for (i, &m) in moves.iter().enumerate() {
            if m > 80 {
                return Err((i, MoveError::BadIndex));
            }
            g.play(m / 9, m % 9).map_err(|e| (i, e))?;
        }
        Ok(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legal_moves(g: &Game) -> Vec<(u8, u8)> {
        let mut v = Vec::new();
        for b in 0..9u8 {
            let mask = g.legal_cells(b);
            for c in 0..9u8 {
                if mask & (1 << c) != 0 {
                    v.push((b, c));
                }
            }
        }
        v
    }

    /// Детерминированный генератор — тесты должны быть воспроизводимы.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn размер_состояния_не_разрастается() {
        assert!(
            std::mem::size_of::<Game>() <= 48,
            "Game = {} байт",
            std::mem::size_of::<Game>()
        );
    }

    #[test]
    fn старт_пустой_ход_за_x() {
        let g = Game::new();
        assert_eq!(g.turn(), Side::X);
        assert_eq!(g.forced(), None);
        assert!(g.over().is_none());
        for b in 0..9 {
            assert_eq!(g.legal_cells(b), 0b1_1111_1111);
        }
    }

    #[test]
    fn ход_отправляет_в_поле_с_номером_клетки() {
        let mut g = Game::new();
        g.play(4, 7).unwrap();
        assert_eq!(g.forced(), Some(7));
        assert_eq!(g.turn(), Side::O);
    }

    #[test]
    fn ход_в_решённое_поле_даёт_свободный_ход() {
        let mut g = Game::new();
        // X забирает поле 3 по верхней строке, каждый раз возвращаясь в него
        g.play(3, 0).unwrap(); // O отправлен в поле 0
        g.play(0, 3).unwrap(); // X отправлен обратно в поле 3
        g.play(3, 1).unwrap();
        g.play(1, 3).unwrap();
        g.play(3, 2).unwrap(); // поле 3 взято, O отправлен в поле 2
        assert_eq!(g.board_result(3), Some(Outcome::Win(Side::X)));
        assert_eq!(g.forced(), Some(2));
        // а вот ход в клетку 3 отправляет в уже решённое поле → свободный ход
        g.play(2, 3).unwrap();
        assert_eq!(g.forced(), None);
    }

    #[test]
    fn нельзя_ходить_вне_обязательного_поля() {
        let mut g = Game::new();
        g.play(0, 2).unwrap();
        assert_eq!(g.forced(), Some(2));
        assert_eq!(g.play(5, 0), Err(MoveError::Illegal));
    }

    #[test]
    fn нельзя_ходить_в_занятую_клетку() {
        let mut g = Game::new();
        g.play(0, 0).unwrap(); // соперник отправлен в поле 0
        assert_eq!(g.forced(), Some(0));
        assert_eq!(g.play(0, 0), Err(MoveError::Illegal));
    }

    #[test]
    fn индексы_вне_диапазона_отвергаются() {
        for (b, c) in [(0u8, 9u8), (9, 0), (99, 0), (0, 99), (255, 255)] {
            let mut g = Game::new();
            assert_eq!(g.play(b, c), Err(MoveError::BadIndex), "{b}/{c}");
            assert_eq!(g.moves(), 0, "состояние тронуто при {b}/{c}");
            assert_eq!(g.forced(), None);
        }
    }

    #[test]
    fn три_ничейных_поля_не_образуют_линию() {
        let mut g = Game::new();
        g.drawn = 0b000_000_111;
        // выигрышной линии быть не должно ни у кого
        assert!(!WIN[g.won_x as usize]);
        assert!(!WIN[g.won_o as usize]);
        assert!(g.over().is_none());
    }

    #[test]
    fn линия_x_считается_рядом_с_ничьими() {
        let mut g = Game::new();
        g.won_x = 0b000_000_111;
        g.drawn = 0b000_111_000;
        assert!(WIN[g.won_x as usize]);
    }

    #[test]
    fn партия_всегда_завершается_и_не_виснет() {
        let mut rng = Rng(0x2545F4914F6CDD1D);
        for _ in 0..3000 {
            let mut g = Game::new();
            let mut guard = 0;
            while g.over().is_none() {
                let lm = legal_moves(&g);
                assert!(!lm.is_empty(), "мёртвое состояние без результата");
                let (b, c) = lm[(rng.next() % lm.len() as u64) as usize];
                g.play(b, c).unwrap();
                guard += 1;
                assert!(guard <= 81, "больше 81 хода");
            }
            assert!(g.over().is_some());
        }
    }

    #[test]
    fn партия_воспроизводится_из_компактного_лога() {
        let mut rng = Rng(12345);
        for _ in 0..500 {
            let mut g = Game::new();
            let mut log = Vec::new();
            while g.over().is_none() {
                let lm = legal_moves(&g);
                let (b, c) = lm[(rng.next() % lm.len() as u64) as usize];
                g.play(b, c).unwrap();
                log.push(b * 9 + c);
            }
            let r = Game::replay(&log).expect("реплей не прошёл");
            assert_eq!(r.over(), g.over());
            assert_eq!(r.moves(), g.moves());
            assert_eq!(r.won_x, g.won_x);
            assert_eq!(r.won_o, g.won_o);
            assert_eq!(r.drawn, g.drawn);
        }
    }

    #[test]
    fn реплей_отвергает_мусор() {
        assert!(Game::replay(&[81]).is_err());
        assert!(Game::replay(&[0, 0]).is_err()); // повтор в ту же клетку
    }

    #[test]
    fn клетки_читаются_обратно() {
        let mut g = Game::new();
        g.play(4, 4).unwrap();
        assert_eq!(g.cell(4, 4), Some(Side::X));
        assert_eq!(g.cell(4, 5), None);
        assert_eq!(g.cell(9, 0), None);
    }
}
