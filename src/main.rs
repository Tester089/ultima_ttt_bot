//! Сервер: HTTP для статики и WebSocket для партий.
//!
//! Устройство выбрано под минимум процессора:
//! * однопоточный рантайм — нет синхронизации и переездов между ядрами;
//! * всё состояние в одной задаче-хабе, соединения общаются каналами,
//!   поэтому нет ни мьютексов, ни блокировок;
//! * статика вшита в бинарник и пожата на сборке — на запрос уходит
//!   только запись готового буфера, без обращений к диску и сжатия;
//! * протокол дельтовый: ход занимает два десятка байт, а не поле из 81 клетки.

mod assets;
mod auth;
mod bot;
mod crypto;
mod game;
mod invites;
mod json;
mod limiter;
mod matchmaking;
mod rating;
mod store;
mod ws;

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use matchmaking::{ConnId, EndReason, Matchmaking, Mode, Msg, Out};
use store::Store;
use ws::{OpCode, Parsed};

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

enum Cmd {
    Join(ConnId, mpsc::UnboundedSender<Vec<u8>>),
    Text(ConnId, String),
    Gone(ConnId),
    Tick,
}

struct Hub {
    ids: HashMap<ConnId, String>,
    buckets: HashMap<ConnId, limiter::Bucket>,
    mm: Matchmaking,
    store: Store,
    sinks: HashMap<ConnId, mpsc::UnboundedSender<Vec<u8>>>,
    bot_token: String,
    allow_guests: bool,
    /// Буферы переиспользуются между сообщениями: ход не аллоцирует.
    out: Vec<Out>,
    buf: String,
    frame: Vec<u8>,
}

impl Hub {
    fn send(&mut self, to: ConnId, payload: &str) {
        self.frame.clear();
        ws::write_frame(&mut self.frame, OpCode::Text, payload.as_bytes());
        if let Some(s) = self.sinks.get(&to) {
            let _ = s.send(self.frame.clone());
        }
    }

    fn flush_events(&mut self) {
        let events = std::mem::take(&mut self.out);
        for e in &events {
            let mut s = std::mem::take(&mut self.buf);
            s.clear();
            serialize(&mut s, &e.msg);
            self.send(e.to, &s);
            self.buf = s;
        }
        self.out = events;
        self.out.clear();
    }

    fn finish(&mut self, f: matchmaking::Finished) {
        let t = now();
        let _ = self.store.apply_game(&f.x.0, &f.x.1, f.x.2, &f.o.0, &f.o.1, f.o.2, f.score_x, t);
        let day = format!("d{}", t / 86_400);
        let _ = self.store.log_game(
            &day,
            &format!("{}-{}", f.room, t),
            if f.mode == Mode::Invite { "invite" } else { "ranked" },
            f.result,
            f.reason.code(),
            &f.moves,
            (&f.x.0, f.x.2),
            (&f.o.0, f.o.2),
        );
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Join(c, tx) => {
                self.sinks.insert(c, tx);
                self.buckets.insert(c, limiter::Bucket::new(now_ms()));
            }
            Cmd::Gone(c) => {
                self.mm.dropped(c, now(), &mut self.out);
                self.flush_events();
                self.sinks.remove(&c);
                self.buckets.remove(&c);
                self.ids.remove(&c);
            }
            Cmd::Tick => {
                let fins = self.mm.tick(now(), &mut self.out);
                self.flush_events();
                for f in fins { self.finish(f); }
                let _ = self.store.flush();
            }
            Cmd::Text(c, text) => self.on_text(c, &text),
        }
    }

    fn on_text(&mut self, c: ConnId, text: &str) {
        let Some(t) = json::str_field(text, "t") else {
            self.send(c, r#"{"t":"err","e":"bad_json"}"#);
            return;
        };

        // Цена зависит от типа: таблица рейтинга дороже хода в десять раз.
        let allowed = self
            .buckets
            .get_mut(&c)
            .map(|b| b.take(limiter::cost(t), now_ms()))
            .unwrap_or(false);
        if !allowed {
            self.send(c, r#"{"t":"err","e":"rate_limited"}"#);
            return;
        }

        match t {
            "hello" => {
                let init = json::str_field(text, "d").unwrap_or("");
                let profile = match auth::validate(init, &self.bot_token, now(), 86_400) {
                    Ok(p) => p,
                    Err(e) => {
                        if self.allow_guests {
                            // Клиент хранит свой токен и присылает его: без этого
                            // перезагрузка страницы теряла партию, потому что гость
                            // получал новый id на каждое соединение.
                            let tok = json::str_field(text, "g").unwrap_or("");
                            let ok = tok.len() >= 8
                                && tok.len() <= 32
                                && tok.bytes().all(|b| b.is_ascii_alphanumeric());
                            if ok {
                                auth::Profile { id: format!("guest_{tok}"), name: "Гость".into(), guest: true }
                            } else {
                                auth::guest(c)
                            }
                        } else {
                            let mut s = String::from(r#"{"t":"err","e":""#);
                            s.push_str(e.code());
                            s.push_str(r#""}"#);
                            self.send(c, &s);
                            return;
                        }
                    }
                };
                let tn = now();
                let (r, lo, hi, prov, skin, sparks, streak, best) = if profile.guest {
                    // Гость не рейтингуется и не хранится: запись в файле ему не нужна.
                    (1200, 1200 - 315, 1200 + 315, true, [0u8, 0, 1, 0], 0u32, 0u8, 0u8)
                } else {
                    let p = self.store.get_or_create(&profile.id, &profile.name, tn);
                    let (r, lo, hi, prov) = p.public();
                    (r, lo, hi, prov, p.skin, p.sparks, p.streak, p.best_streak)
                };
                if let Some(old) = self.mm.register(c, &profile.id, &profile.name, profile.guest, r, skin, &mut self.out) {
                    self.sinks.remove(&old);
                }
                let mut s = String::with_capacity(96);
                s.push_str(r#"{"t":"welcome","n":"#);
                json::push_str(&mut s, &profile.name);
                s.push_str(r#","r":"#); s.push_str(&r.to_string());
                s.push_str(r#","lo":"#); s.push_str(&lo.to_string());
                s.push_str(r#","hi":"#); s.push_str(&hi.to_string());
                s.push_str(r#","p":"#); s.push_str(if prov {"1"} else {"0"});
                s.push_str(r#","sk":["#);
                for (i, v) in skin.iter().enumerate() {
                    if i > 0 { s.push(','); }
                    s.push_str(&v.to_string());
                }
                s.push_str(r#"],"sp":"#); s.push_str(&sparks.to_string());
                s.push_str(r#","st":"#); s.push_str(&streak.to_string());
                s.push_str(r#","bs":"#); s.push_str(&best.to_string());
                s.push_str(r#","g":"#); s.push_str(if profile.guest {"1"} else {"0"});
                s.push('}');
                if !profile.guest {
                    self.ids.insert(c, profile.id.clone());
                }
                self.send(c, &s);
                self.flush_events();
            }
            "queue" => { self.mm.enqueue(c, &mut self.out); self.flush_events(); }
            "cancel" => { self.mm.dequeue(c, &mut self.out); self.flush_events(); }
            "move" => {
                let b = json::int_field(text, "b").unwrap_or(-1);
                let cl = json::int_field(text, "c").unwrap_or(-1);
                if !(0..=8).contains(&b) || !(0..=8).contains(&cl) {
                    self.send(c, r#"{"t":"err","e":"bad_index"}"#);
                    return;
                }
                let fin = self.mm.play(c, b as u8, cl as u8, &mut self.out);
                self.flush_events();
                if let Some(f) = fin { self.finish(f); }
            }
            "resign" => {
                let fin = self.mm.resign(c, EndReason::Resign, &mut self.out);
                self.flush_events();
                if let Some(f) = fin { self.finish(f); }
            }
            "invite" => { self.mm.create_invite(c, now(), &mut self.out); self.flush_events(); }
            "join" => {
                let code = json::str_field(text, "code").unwrap_or("").to_string();
                self.mm.join_invite(c, &code, now(), &mut self.out);
                self.flush_events();
            }
            "top" => {
                let rows = self.store.leaderboard(20, now());
                let mut s = String::with_capacity(rows.len() * 40 + 24);
                s.push_str(r#"{"t":"top","rows":["#);
                for (i, (name, r, prov)) in rows.iter().enumerate() {
                    if i > 0 { s.push(','); }
                    s.push('[');
                    json::push_str(&mut s, name);
                    s.push(','); s.push_str(&r.to_string());
                    s.push(','); s.push_str(if *prov {"1"} else {"0"});
                    s.push(']');
                }
                s.push_str("]}");
                self.send(c, &s);
            }
            "skin" => {
                // скин — четыре байта: форма, два цвета, эффекты
                let mut sk = [0u8; 4];
                for (i, k) in ["s0", "s1", "s2", "s3"].iter().enumerate() {
                    sk[i] = json::int_field(text, k).unwrap_or(0).clamp(0, 255) as u8;
                }
                self.mm.set_skin(c, sk);
                if let Some(id) = self.ids.get(&c).cloned() {
                    if let Some(p) = self.store.get_mut(&id) {
                        p.skin = sk;
                    }
                    self.store.mark_dirty();
                }
                self.send(c, r#"{"t":"skinok"}"#);
            }
            "ping" => self.send(c, r#"{"t":"pong"}"#),
            _ => self.send(c, r#"{"t":"err","e":"unknown"}"#),
        }
    }
}

fn serialize(out: &mut String, m: &Msg) {
    match m {
        Msg::Queued { size } => {
            out.push_str(r#"{"t":"queued","n":"#); out.push_str(&size.to_string()); out.push('}');
        }
        Msg::QueueLeft => out.push_str(r#"{"t":"unqueued"}"#),
        Msg::Matched { you, opp_name, opp_rating, opp_skin, mode, resumed, .. } => {
            out.push_str(r#"{"t":"start","s":""#); out.push_str(you.as_str());
            out.push_str(r#"","o":"#); json::push_str(out, opp_name);
            out.push_str(r#","r":"#); out.push_str(&opp_rating.to_string());
            out.push_str(r#","osk":["#);
            for (i, v) in opp_skin.iter().enumerate() {
                if i > 0 { out.push(','); }
                out.push_str(&v.to_string());
            }
            out.push_str(r#"],"m":""#); out.push_str(if *mode == Mode::Invite {"invite"} else {"ranked"});
            out.push_str(r#"","re":"#); out.push_str(if *resumed {"1"} else {"0"});
            out.push('}');
        }
        // Самое частое сообщение: два десятка байт вместо ~400 в JS-версии.
        Msg::Moved { mv } => {
            out.push_str(r#"{"t":"m","v":"#); out.push_str(&mv.to_string()); out.push('}');
        }
        Msg::Over { result, reason } => {
            out.push_str(r#"{"t":"over","w":""#); out.push_str(result);
            out.push_str(r#"","why":""#); out.push_str(reason.code()); out.push_str(r#""}"#);
        }
        Msg::OppLeft { secs } => {
            out.push_str(r#"{"t":"gone","s":"#); out.push_str(&secs.to_string()); out.push('}');
        }
        Msg::Invite { code } => {
            out.push_str(r#"{"t":"invite","code":""#); out.push_str(code); out.push_str(r#""}"#);
        }
        Msg::Err(e) => {
            out.push_str(r#"{"t":"err","e":""#); out.push_str(e); out.push_str(r#""}"#);
        }
    }
}

#[derive(Clone)]
struct Cfg {
    app_url: String,
    webhook_secret: String,
}

async fn serve_conn(mut sock: TcpStream, id: ConnId, hub: mpsc::UnboundedSender<Cmd>, cfg: Cfg) {
    let _ = sock.set_nodelay(true); // латентность важнее упаковки пакетов

    let mut head = Vec::with_capacity(1024);
    let mut tmp = [0u8; 1024];
    loop {
        let n = match sock.read(&mut tmp).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        head.extend_from_slice(&tmp[..n]);
        if head.windows(4).any(|w| w == b"\r\n\r\n") || head.len() > 8192 { break; }
    }
    let req = String::from_utf8_lossy(&head).into_owned();

    if let Some(resp) = ws::handshake(&req) {
        if sock.write_all(resp.as_bytes()).await.is_err() { return; }
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let _ = hub.send(Cmd::Join(id, tx));

        let (mut rd, mut wr) = sock.into_split();
        let writer = tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                if wr.write_all(&frame).await.is_err() { break; }
            }
        });

        let mut buf: Vec<u8> = Vec::with_capacity(2048);
        let mut chunk = [0u8; 2048];
        'read: loop {
            let n = match rd.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            buf.extend_from_slice(&chunk[..n]);
            loop {
                match ws::parse(&mut buf) {
                    Parsed::Need(_) => break,
                    Parsed::Bad(_) => break 'read,
                    Parsed::Frame { op, start, len, consumed, .. } => {
                        if op == OpCode::Text {
                            let text = String::from_utf8_lossy(&buf[start..start + len]).into_owned();
                            let _ = hub.send(Cmd::Text(id, text));
                        } else if op == OpCode::Close {
                            buf.drain(..consumed);
                            break 'read;
                        }
                        buf.drain(..consumed);
                    }
                }
            }
            if buf.len() > ws::MAX_FRAME * 2 { break; }
        }
        let _ = hub.send(Cmd::Gone(id));
        writer.abort();
        return;
    }

    let method = req.split(' ').next().unwrap_or("");
    let raw_path = req
        .split_once(' ')
        .and_then(|(_, r)| r.split_once(' '))
        .map(|(p, _)| p)
        .unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    // Вебхук бота: отвечаем командой прямо в теле — исходящих запросов
    // и TLS-клиента в проекте нет вовсе.
    if method == "POST" && path == "/telegram/webhook" {
        let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
        let ok = bot::secret_ok(&req, &cfg.webhook_secret);
        let answer = if ok {
            bot::parse(body).and_then(|u| bot::reply(&u, &cfg.app_url))
        } else {
            None
        };
        let resp = match (ok, answer) {
            (false, _) => "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
            (true, None) => "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
            (true, Some(j)) => format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                j.len(),
                j
            ),
        };
        let _ = sock.write_all(resp.as_bytes()).await;
        return;
    }

    let mut buf = Vec::with_capacity(4096);
    if path.starts_with("/health") {
        let body = b"{\"ok\":true}";
        buf.extend_from_slice(
            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes(),
        );
        buf.extend_from_slice(body);
    } else if let Some(a) = assets::find(path) {
        assets::respond(&mut buf, a);
    } else {
        buf.extend_from_slice(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
    let _ = sock.write_all(&buf).await;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::var("PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8080);
    let dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "/data".into());
    let bot_token = std::env::var("BOT_TOKEN").unwrap_or_default();
    let allow_guests = std::env::var("ALLOW_GUESTS").map(|v| v == "1").unwrap_or(false);

    let store = Store::open(&dir)?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Cmd>();

    let mut hub = Hub {
        mm: Matchmaking::new(now() | 1),
        store,
        sinks: HashMap::new(),
        bot_token,
        allow_guests,
        ids: HashMap::new(),
        buckets: HashMap::new(),
        out: Vec::with_capacity(32),
        buf: String::with_capacity(256),
        frame: Vec::with_capacity(256),
    };

    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await { hub.handle(cmd); }
    });

    let ticker = tx.clone();
    tokio::spawn(async move {
        let mut iv = tokio::time::interval(Duration::from_secs(5));
        loop {
            iv.tick().await;
            if ticker.send(Cmd::Tick).is_err() { break; }
        }
    });

    let cfg = Cfg {
        app_url: std::env::var("APP_URL").unwrap_or_else(|_| "https://example.org/".into()),
        webhook_secret: std::env::var("WEBHOOK_SECRET").unwrap_or_default(),
    };

    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("uttt слушает :{port}, данные в {dir}");

    let mut next: ConnId = 1;
    loop {
        let Ok((sock, _)) = listener.accept().await else { continue };
        let id = next;
        next += 1;
        tokio::spawn(serve_conn(sock, id, tx.clone(), cfg.clone()));
    }
}
