//! Очередь, комнаты и правила соединения игроков.
//!
//! Горячий путь — ход — не выделяет память вообще: комнаты лежат в слабе
//! со списком свободных слотов, события пишутся в буфер вызывающего,
//! а клиенту уходит один байт хода вместо полного поля из 81 клетки.

use std::collections::{HashMap, VecDeque};

use crate::game::{Game, MoveError, Outcome, Side};
use crate::invites::{Invites, JoinError};

pub type ConnId = u64;
pub type RoomId = u32;

/// Сколько ждём возвращения оборвавшегося игрока, прежде чем засчитать
/// поражение. Мобильная сеть роняет сокет на ровном месте.
pub const GRACE_SECS: u64 = 45;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ranked,
    Invite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    Mate,
    Draw,
    Resign,
    Disconnect,
}

impl EndReason {
    pub fn code(self) -> &'static str {
        match self {
            EndReason::Mate => "mate",
            EndReason::Draw => "draw",
            EndReason::Resign => "resign",
            EndReason::Disconnect => "disconnect",
        }
    }
}

/// Сообщение конкретному соединению.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Queued { size: usize },
    QueueLeft,
    /// Партия началась. Полное состояние здесь передавать не нужно —
    /// поле пустое, клиент строит его сам.
    Matched {
        room: RoomId,
        you: Side,
        opp_name: String,
        opp_rating: i32,
        opp_skin: [u8; 4],
        mode: Mode,
        resumed: bool,
    },
    /// Ход: один байт `board*9+cell`. Остальное клиент выводит сам,
    /// прогоняя те же правила — они детерминированы.
    Moved { mv: u8 },
    Over { result: &'static str, reason: EndReason },
    OppLeft { secs: u64 },
    Invite { code: String },
    Err(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Out {
    pub to: ConnId,
    pub msg: Msg,
}

struct Room {
    game: Game,
    players: [ConnId; 2], // [X, O]
    mode: Mode,
    moves: Vec<u8>,
    finished: bool,
}

struct Conn {
    id: String,
    name: String,
    guest: bool,
    rating: i32,
    skin: [u8; 4],
    room: Option<RoomId>,
    side: Side,
    /// Момент обрыва: `Some(t)` — игрок офлайн, но комната ещё ждёт.
    dropped_at: Option<u64>,
    alive: bool,
}

pub struct Matchmaking {
    conns: HashMap<ConnId, Conn>,
    /// Соответствие «игрок → активное соединение»: второй вход вытесняет первый.
    by_player: HashMap<String, ConnId>,
    queue: VecDeque<ConnId>,
    rooms: Vec<Option<Room>>,
    free_rooms: Vec<RoomId>,
    pub invites: Invites,
}

/// Итог завершённой партии — серверу нужно записать его в хранилище.
#[derive(Debug, Clone)]
pub struct Finished {
    pub room: RoomId,
    pub x: (String, String, bool),
    pub o: (String, String, bool),
    pub score_x: f64,
    pub result: &'static str,
    pub reason: EndReason,
    pub mode: Mode,
    pub moves: Vec<u8>,
}

impl Matchmaking {
    pub fn new(seed: u64) -> Self {
        Matchmaking {
            conns: HashMap::new(),
            by_player: HashMap::new(),
            queue: VecDeque::new(),
            rooms: Vec::new(),
            free_rooms: Vec::new(),
            invites: Invites::new(seed),
        }
    }

    /// Смена скина: применяется к текущему соединению.
    pub fn set_skin(&mut self, conn: ConnId, skin: [u8; 4]) {
        if let Some(c) = self.conns.get_mut(&conn) {
            c.skin = skin;
        }
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }
    pub fn rooms_live(&self) -> usize {
        self.rooms.iter().filter(|r| r.is_some()).count()
    }
    pub fn conns_len(&self) -> usize {
        self.conns.len()
    }

    /// Регистрация соединения. Возвращает вытесненное соединение того же
    /// игрока, если он вошёл со второго устройства.
    pub fn register(
        &mut self,
        conn: ConnId,
        id: &str,
        name: &str,
        guest: bool,
        rating: i32,
        skin: [u8; 4],
        out: &mut Vec<Out>,
    ) -> Option<ConnId> {
        let mut evicted = None;

        if let Some(&old) = self.by_player.get(id) {
            if old != conn {
                // переносим соединение в комнате, а не сдаёмся
                if let Some(oc) = self.conns.get_mut(&old) {
                    oc.alive = false;
                    let room = oc.room;
                    let side = oc.side;
                    if let Some(rid) = room {
                        if let Some(Some(r)) = self.rooms.get_mut(rid as usize) {
                            r.players[side as usize] = conn;
                        }
                    }
                }
                evicted = Some(old);
            }
        }

        let prev = self
            .by_player
            .get(id)
            .and_then(|c| self.conns.remove(c))
            .filter(|_| evicted.is_some());

        let (room, side) = match &prev {
            Some(p) => (p.room, p.side),
            None => (None, Side::X),
        };

        self.conns.insert(
            conn,
            Conn {
                id: id.to_string(),
                name: name.to_string(),
                guest,
                rating,
                skin,
                room,
                side,
                dropped_at: None,
                alive: true,
            },
        );
        self.by_player.insert(id.to_string(), conn);

        // вернулся в живую партию — говорим об этом
        if let Some(rid) = room {
            if let Some(Some(r)) = self.rooms.get(rid as usize) {
                if !r.finished {
                    let opp = r.players[side.other() as usize];
                    let (on, or, os) = self
                        .conns
                        .get(&opp)
                        .map(|c| (c.name.clone(), c.rating, c.skin))
                        .unwrap_or_else(|| ("Соперник".into(), 1200, [0, 0, 1, 0]));
                    out.push(Out {
                        to: conn,
                        msg: Msg::Matched {
                            room: rid,
                            you: side,
                            opp_name: on,
                            opp_rating: or,
                            opp_skin: os,
                            mode: r.mode,
                            resumed: true,
                        },
                    });
                    // и досылаем ходы, чтобы клиент догнал позицию
                    for &m in &r.moves {
                        out.push(Out {
                            to: conn,
                            msg: Msg::Moved { mv: m },
                        });
                    }
                }
            }
        }

        evicted
    }

    fn free_for_queue(&self, conn: ConnId) -> bool {
        match self.conns.get(&conn) {
            None => false,
            Some(c) => match c.room {
                None => true,
                Some(rid) => matches!(self.rooms.get(rid as usize), Some(Some(r)) if r.finished)
                    || matches!(self.rooms.get(rid as usize), Some(None) | None),
            },
        }
    }

    fn detach_room(&mut self, conn: ConnId) {
        if let Some(c) = self.conns.get_mut(&conn) {
            c.room = None;
        }
    }

    pub fn enqueue(&mut self, conn: ConnId, out: &mut Vec<Out>) {
        if !self.conns.contains_key(&conn) {
            return;
        }
        if !self.free_for_queue(conn) {
            out.push(Out {
                to: conn,
                msg: Msg::Err("in_game"),
            });
            return;
        }
        self.detach_room(conn);
        if self.queue.contains(&conn) {
            out.push(Out {
                to: conn,
                msg: Msg::Queued {
                    size: self.queue.len(),
                },
            });
            return;
        }
        self.queue.push_back(conn);
        out.push(Out {
            to: conn,
            msg: Msg::Queued {
                size: self.queue.len(),
            },
        });
        self.try_match(out);
    }

    pub fn dequeue(&mut self, conn: ConnId, out: &mut Vec<Out>) {
        self.queue.retain(|&c| c != conn);
        out.push(Out {
            to: conn,
            msg: Msg::QueueLeft,
        });
    }

    fn alloc_room(&mut self, room: Room) -> RoomId {
        if let Some(id) = self.free_rooms.pop() {
            self.rooms[id as usize] = Some(room);
            id
        } else {
            self.rooms.push(Some(room));
            (self.rooms.len() - 1) as RoomId
        }
    }

    fn try_match(&mut self, out: &mut Vec<Out>) {
        while self.queue.len() >= 2 {
            let a = self.queue.pop_front().unwrap();
            let b = self.queue.pop_front().unwrap();
            let ok_a = self.conns.get(&a).map(|c| c.alive).unwrap_or(false);
            let ok_b = self.conns.get(&b).map(|c| c.alive).unwrap_or(false);
            if !ok_a {
                if ok_b {
                    self.queue.push_front(b);
                }
                continue;
            }
            if !ok_b {
                self.queue.push_front(a);
                continue;
            }
            self.start(a, b, Mode::Ranked, out);
        }
    }

    fn start(&mut self, x: ConnId, o: ConnId, mode: Mode, out: &mut Vec<Out>) -> RoomId {
        let rid = self.alloc_room(Room {
            game: Game::new(),
            players: [x, o],
            mode,
            moves: Vec::with_capacity(48),
            finished: false,
        });

        let (nx, rx, sx) = self
            .conns
            .get(&x)
            .map(|c| (c.name.clone(), c.rating, c.skin))
            .unwrap_or_else(|| (String::new(), 1200, [0, 0, 1, 0]));
        let (no, ro, so) = self
            .conns
            .get(&o)
            .map(|c| (c.name.clone(), c.rating, c.skin))
            .unwrap_or_else(|| (String::new(), 1200, [0, 0, 1, 0]));

        if let Some(c) = self.conns.get_mut(&x) {
            c.room = Some(rid);
            c.side = Side::X;
        }
        if let Some(c) = self.conns.get_mut(&o) {
            c.room = Some(rid);
            c.side = Side::O;
        }

        out.push(Out {
            to: x,
            msg: Msg::Matched {
                room: rid,
                you: Side::X,
                opp_name: no,
                opp_rating: ro,
                opp_skin: so,
                mode,
                resumed: false,
            },
        });
        out.push(Out {
            to: o,
            msg: Msg::Matched {
                room: rid,
                you: Side::O,
                opp_name: nx,
                opp_rating: rx,
                opp_skin: sx,
                mode,
                resumed: false,
            },
        });
        rid
    }

    /// Ход. Возвращает итог, если партия закончилась.
    pub fn play(
        &mut self,
        conn: ConnId,
        board: u8,
        cell: u8,
        out: &mut Vec<Out>,
    ) -> Option<Finished> {
        let (rid, side) = match self.conns.get(&conn) {
            Some(c) => match c.room {
                Some(r) => (r, c.side),
                None => {
                    out.push(Out {
                        to: conn,
                        msg: Msg::Err("no_room"),
                    });
                    return None;
                }
            },
            None => return None,
        };

        let Some(Some(room)) = self.rooms.get_mut(rid as usize) else {
            out.push(Out {
                to: conn,
                msg: Msg::Err("no_room"),
            });
            return None;
        };
        if room.finished {
            out.push(Out {
                to: conn,
                msg: Msg::Err("finished"),
            });
            return None;
        }
        if room.game.turn() != side {
            out.push(Out {
                to: conn,
                msg: Msg::Err("not_your_turn"),
            });
            return None;
        }

        match room.game.play(board, cell) {
            Err(MoveError::BadIndex) => {
                out.push(Out {
                    to: conn,
                    msg: Msg::Err("bad_index"),
                });
                return None;
            }
            Err(MoveError::Illegal) => {
                out.push(Out {
                    to: conn,
                    msg: Msg::Err("illegal"),
                });
                return None;
            }
            Ok(()) => {}
        }

        let mv = board * 9 + cell;
        room.moves.push(mv);
        let x = room.players[0];
        let o = room.players[1];
        let over = room.game.over();

        out.push(Out {
            to: x,
            msg: Msg::Moved { mv },
        });
        out.push(Out {
            to: o,
            msg: Msg::Moved { mv },
        });

        over.map(|res| {
            let reason = match res {
                Outcome::Draw => EndReason::Draw,
                Outcome::Win(_) => EndReason::Mate,
            };
            self.finish(rid, res, reason, out)
        })
    }

    fn finish(
        &mut self,
        rid: RoomId,
        res: Outcome,
        reason: EndReason,
        out: &mut Vec<Out>,
    ) -> Finished {
        let (x, o, mode, moves) = {
            let room = self.rooms[rid as usize].as_mut().unwrap();
            room.finished = true;
            (
                room.players[0],
                room.players[1],
                room.mode,
                std::mem::take(&mut room.moves),
            )
        };

        let result = res.as_str();
        let score_x = match res {
            Outcome::Win(Side::X) => 1.0,
            Outcome::Win(Side::O) => 0.0,
            Outcome::Draw => 0.5,
        };

        for &c in &[x, o] {
            out.push(Out {
                to: c,
                msg: Msg::Over { result, reason },
            });
        }

        let info = |c: ConnId, s: &Self| {
            s.conns
                .get(&c)
                .map(|p| (p.id.clone(), p.name.clone(), p.guest))
                .unwrap_or_else(|| (String::new(), String::new(), true))
        };
        let fx = info(x, self);
        let fo = info(o, self);

        // Игроки освобождаются СРАЗУ: в JS-версии комната держала их ещё
        // 30 секунд и «Найти соперника» отвечало in_game.
        self.detach_room(x);
        self.detach_room(o);
        self.rooms[rid as usize] = None;
        self.free_rooms.push(rid);

        Finished {
            room: rid,
            x: fx,
            o: fo,
            score_x,
            result,
            reason,
            mode,
            moves,
        }
    }

    pub fn resign(&mut self, conn: ConnId, reason: EndReason, out: &mut Vec<Out>) -> Option<Finished> {
        let (rid, side) = match self.conns.get(&conn) {
            Some(c) => (c.room?, c.side),
            None => return None,
        };
        match self.rooms.get(rid as usize) {
            Some(Some(r)) if !r.finished => {}
            _ => return None,
        }
        let res = Outcome::Win(side.other());
        Some(self.finish(rid, res, reason, out))
    }

    /// Обрыв связи: сразу не сдаёмся, даём grace-период.
    pub fn dropped(&mut self, conn: ConnId, now: u64, out: &mut Vec<Out>) {
        let Some(c) = self.conns.get_mut(&conn) else {
            return;
        };
        c.alive = false;
        c.dropped_at = Some(now);
        let room = c.room;
        let side = c.side;
        self.queue.retain(|&q| q != conn);

        let id = c.id.clone();
        self.invites.cancel(&id);

        if let Some(rid) = room {
            if let Some(Some(r)) = self.rooms.get(rid as usize) {
                if !r.finished {
                    out.push(Out {
                        to: r.players[side.other() as usize],
                        msg: Msg::OppLeft { secs: GRACE_SECS },
                    });
                    return;
                }
            }
        }
        // вне партии соединение можно забыть сразу
        self.forget(conn);
    }

    fn forget(&mut self, conn: ConnId) {
        if let Some(c) = self.conns.remove(&conn) {
            if self.by_player.get(&c.id) == Some(&conn) {
                self.by_player.remove(&c.id);
            }
        }
    }

    /// Периодическая уборка: истёкшие grace-периоды.
    pub fn tick(&mut self, now: u64, out: &mut Vec<Out>) -> Vec<Finished> {
        let expired: Vec<ConnId> = self
            .conns
            .iter()
            .filter(|(_, c)| {
                !c.alive && c.dropped_at.map(|t| now.saturating_sub(t) >= GRACE_SECS).unwrap_or(false)
            })
            .map(|(&c, _)| c)
            .collect();

        let mut fin = Vec::new();
        for c in expired {
            if let Some(f) = self.resign(c, EndReason::Disconnect, out) {
                fin.push(f);
            }
            self.forget(c);
        }
        fin
    }

    pub fn create_invite(&mut self, conn: ConnId, now: u64, out: &mut Vec<Out>) {
        let Some(c) = self.conns.get(&conn) else {
            return;
        };
        let id = c.id.clone();
        match self.invites.create(&id, now) {
            Some(inv) => out.push(Out {
                to: conn,
                msg: Msg::Invite { code: inv.code },
            }),
            None => out.push(Out {
                to: conn,
                msg: Msg::Err("invite_failed"),
            }),
        }
    }

    pub fn join_invite(&mut self, conn: ConnId, code: &str, now: u64, out: &mut Vec<Out>) {
        let Some(c) = self.conns.get(&conn) else {
            return;
        };
        let guest_id = c.id.clone();

        if !self.free_for_queue(conn) {
            out.push(Out {
                to: conn,
                msg: Msg::Err("in_game"),
            });
            return;
        }

        let inv = match self.invites.consume(code, &guest_id, now) {
            Ok(i) => i,
            Err(JoinError::NotFound) => {
                out.push(Out {
                    to: conn,
                    msg: Msg::Err("invite_not_found"),
                });
                return;
            }
            Err(JoinError::Expired) => {
                out.push(Out {
                    to: conn,
                    msg: Msg::Err("invite_expired"),
                });
                return;
            }
            Err(JoinError::Self_) => {
                out.push(Out {
                    to: conn,
                    msg: Msg::Err("invite_self"),
                });
                return;
            }
        };

        let Some(&host_conn) = self.by_player.get(&inv.host) else {
            out.push(Out {
                to: conn,
                msg: Msg::Err("host_offline"),
            });
            return;
        };
        if !self.conns.get(&host_conn).map(|c| c.alive).unwrap_or(false) {
            out.push(Out {
                to: conn,
                msg: Msg::Err("host_offline"),
            });
            return;
        }
        if !self.free_for_queue(host_conn) {
            out.push(Out {
                to: conn,
                msg: Msg::Err("host_busy"),
            });
            return;
        }

        self.queue.retain(|&q| q != host_conn && q != conn);
        self.detach_room(host_conn);
        self.detach_room(conn);
        self.start(host_conn, conn, Mode::Invite, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm() -> Matchmaking {
        Matchmaking::new(1)
    }

    fn reg(m: &mut Matchmaking, conn: ConnId, id: &str) {
        let mut out = Vec::new();
        m.register(conn, id, id, false, 1200, [0, 0, 1, 0], &mut out);
    }

    fn pair(m: &mut Matchmaking) -> (ConnId, ConnId, Vec<Out>) {
        reg(m, 1, "a");
        reg(m, 2, "b");
        let mut out = Vec::new();
        m.enqueue(1, &mut out);
        m.enqueue(2, &mut out);
        (1, 2, out)
    }

    fn side_of(out: &[Out], conn: ConnId) -> Side {
        out.iter()
            .find_map(|o| match (&o.msg, o.to == conn) {
                (Msg::Matched { you, .. }, true) => Some(*you),
                _ => None,
            })
            .expect("нет Matched")
    }

    #[test]
    fn двое_получают_разные_стороны() {
        let mut m = mm();
        let (_, _, out) = pair(&mut m);
        assert_ne!(side_of(&out, 1), side_of(&out, 2));
        assert_eq!(m.rooms_live(), 1);
        assert_eq!(m.queue_len(), 0);
    }

    #[test]
    fn ход_рассылается_обоим_одним_байтом() {
        let mut m = mm();
        let (_, _, out) = pair(&mut m);
        let x = if side_of(&out, 1) == Side::X { 1 } else { 2 };
        let mut o2 = Vec::new();
        m.play(x, 4, 4, &mut o2);
        let moved: Vec<_> = o2.iter().filter(|o| matches!(o.msg, Msg::Moved { .. })).collect();
        assert_eq!(moved.len(), 2, "ход ушёл не обоим");
        assert!(matches!(moved[0].msg, Msg::Moved { mv: 40 }));
    }

    #[test]
    fn чужой_ход_отклоняется() {
        let mut m = mm();
        let (_, _, out) = pair(&mut m);
        let o = if side_of(&out, 1) == Side::O { 1 } else { 2 };
        let mut o2 = Vec::new();
        m.play(o, 0, 0, &mut o2);
        assert!(o2.iter().any(|x| x.msg == Msg::Err("not_your_turn")));
    }

    #[test]
    fn мусорный_индекс_не_подвешивает_партию() {
        let mut m = mm();
        let (_, _, out) = pair(&mut m);
        let x = if side_of(&out, 1) == Side::X { 1 } else { 2 };
        let mut o2 = Vec::new();
        m.play(x, 0, 99, &mut o2);
        m.play(x, 99, 0, &mut o2);
        assert_eq!(o2.iter().filter(|e| e.msg == Msg::Err("bad_index")).count(), 2);
        // и обычный ход после этого проходит
        o2.clear();
        m.play(x, 0, 0, &mut o2);
        assert!(o2.iter().any(|e| matches!(e.msg, Msg::Moved { .. })));
    }

    #[test]
    fn нельзя_в_очередь_из_живой_партии() {
        let mut m = mm();
        pair(&mut m);
        let mut out = Vec::new();
        m.enqueue(1, &mut out);
        assert!(out.iter().any(|e| e.msg == Msg::Err("in_game")));
    }

    #[test]
    fn после_партии_очередь_снова_доступна() {
        let mut m = mm();
        pair(&mut m);
        let mut out = Vec::new();
        m.resign(1, EndReason::Resign, &mut out);
        out.clear();
        m.enqueue(1, &mut out);
        assert!(out.iter().any(|e| matches!(e.msg, Msg::Queued { .. })));
        assert!(!out.iter().any(|e| e.msg == Msg::Err("in_game")));
    }

    #[test]
    fn слот_комнаты_переиспользуется() {
        let mut m = mm();
        pair(&mut m);
        let mut out = Vec::new();
        m.resign(1, EndReason::Resign, &mut out);
        assert_eq!(m.rooms_live(), 0);
        out.clear();
        m.enqueue(1, &mut out);
        m.enqueue(2, &mut out);
        assert_eq!(m.rooms_live(), 1);
        assert_eq!(m.rooms.len(), 1, "слаб растёт вместо переиспользования");
    }

    #[test]
    fn сдача_отдаёт_победу_сопернику() {
        let mut m = mm();
        let (_, _, out) = pair(&mut m);
        let x = if side_of(&out, 1) == Side::X { 1 } else { 2 };
        let mut o2 = Vec::new();
        let f = m.resign(x, EndReason::Resign, &mut o2).unwrap();
        assert_eq!(f.result, "O");
        assert_eq!(f.score_x, 0.0);
        assert_eq!(f.reason, EndReason::Resign);
    }

    #[test]
    fn обрыв_даёт_grace_а_не_мгновенное_поражение() {
        let mut m = mm();
        pair(&mut m);
        let mut out = Vec::new();
        m.dropped(1, 1000, &mut out);
        assert!(out.iter().any(|e| matches!(e.msg, Msg::OppLeft { .. })));
        assert_eq!(m.rooms_live(), 1, "комната закрыта сразу");
        // до истечения ничего не происходит
        out.clear();
        assert!(m.tick(1000 + GRACE_SECS - 1, &mut out).is_empty());
        // после — засчитываем
        let fin = m.tick(1000 + GRACE_SECS, &mut out);
        assert_eq!(fin.len(), 1);
        assert_eq!(fin[0].reason, EndReason::Disconnect);
        assert_eq!(m.rooms_live(), 0);
    }

    #[test]
    fn возврат_до_истечения_восстанавливает_партию() {
        let mut m = mm();
        let (_, _, out) = pair(&mut m);
        let x = if side_of(&out, 1) == Side::X { 1 } else { 2 };
        let mut o2 = Vec::new();
        m.play(x, 4, 4, &mut o2);
        m.dropped(1, 100, &mut o2);
        o2.clear();
        // тот же игрок «a» приходит новым соединением
        m.register(77, "a", "a", false, 1200, [0, 0, 1, 0], &mut o2);
        assert!(o2
            .iter()
            .any(|e| matches!(&e.msg, Msg::Matched { resumed: true, .. })));
        // и получает ходы, чтобы догнать позицию
        assert!(o2.iter().any(|e| matches!(e.msg, Msg::Moved { mv: 40 })));
        let mut o3 = Vec::new();
        assert!(m.tick(100 + GRACE_SECS + 10, &mut o3).is_empty(), "сдали вернувшегося");
    }

    #[test]
    fn инвайт_соединяет_двоих() {
        let mut m = mm();
        reg(&mut m, 1, "host");
        reg(&mut m, 2, "guest");
        let mut out = Vec::new();
        m.create_invite(1, 0, &mut out);
        let code = out
            .iter()
            .find_map(|o| match &o.msg {
                Msg::Invite { code } => Some(code.clone()),
                _ => None,
            })
            .unwrap();
        out.clear();
        m.join_invite(2, &code, 1, &mut out);
        assert_eq!(out.iter().filter(|o| matches!(o.msg, Msg::Matched { .. })).count(), 2);
        assert!(out
            .iter()
            .any(|o| matches!(&o.msg, Msg::Matched { mode: Mode::Invite, .. })));
    }

    #[test]
    fn инвайт_себе_и_офлайн_хосту() {
        let mut m = mm();
        reg(&mut m, 1, "host");
        reg(&mut m, 2, "guest");
        let mut out = Vec::new();
        m.create_invite(1, 0, &mut out);
        let code = out
            .iter()
            .find_map(|o| match &o.msg {
                Msg::Invite { code } => Some(code.clone()),
                _ => None,
            })
            .unwrap();
        out.clear();
        m.join_invite(1, &code, 1, &mut out);
        assert!(out.iter().any(|e| e.msg == Msg::Err("invite_self")));

        out.clear();
        m.dropped(1, 1, &mut out);
        out.clear();
        m.join_invite(2, &code, 2, &mut out);
        // инвайт хоста отменён при обрыве
        assert!(out.iter().any(|e| e.msg == Msg::Err("invite_not_found")));
    }

    #[test]
    fn полная_партия_доигрывается_до_результата() {
        let mut m = mm();
        let (_, _, out) = pair(&mut m);
        let x = if side_of(&out, 1) == Side::X { 1 } else { 2 };
        let o = if x == 1 { 2 } else { 1 };
        let mut buf = Vec::new();
        let mut turn_x = true;
        let mut fin = None;
        let mut g = Game::new();
        for _ in 0..81 {
            let mut done = false;
            'outer: for b in 0..9u8 {
                let mask = g.legal_cells(b);
                for c in 0..9u8 {
                    if mask & (1 << c) != 0 {
                        let conn = if turn_x { x } else { o };
                        buf.clear();
                        let f = m.play(conn, b, c, &mut buf);
                        g.play(b, c).unwrap();
                        if f.is_some() {
                            fin = f;
                        }
                        turn_x = !turn_x;
                        done = true;
                        break 'outer;
                    }
                }
            }
            if !done || fin.is_some() {
                break;
            }
        }
        let f = fin.expect("партия не завершилась");
        assert!(["X", "O", "-"].contains(&f.result));
        assert!(!f.moves.is_empty());
        // лог восстанавливается правилами
        assert!(Game::replay(&f.moves).is_ok());
    }
}
