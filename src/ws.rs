//! Кодек WebSocket.
//!
//! Разбор и сборка кадров — чистые функции над срезами, без ввода-вывода,
//! поэтому покрываются тестами целиком. Размаскирование идёт словами по
//! 8 байт: на ходах в 12 байт это неважно, но снимает пик при больших
//! кадрах и не даёт злоумышленнику нагрузить процессор мусором.

pub const MAX_FRAME: usize = 16 * 1024;
const MAGIC: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    Cont,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl OpCode {
    fn from(b: u8) -> Option<OpCode> {
        Some(match b {
            0x0 => OpCode::Cont,
            0x1 => OpCode::Text,
            0x2 => OpCode::Binary,
            0x8 => OpCode::Close,
            0x9 => OpCode::Ping,
            0xA => OpCode::Pong,
            _ => return None,
        })
    }
    fn bits(self) -> u8 {
        match self {
            OpCode::Cont => 0x0,
            OpCode::Text => 0x1,
            OpCode::Binary => 0x2,
            OpCode::Close => 0x8,
            OpCode::Ping => 0x9,
            OpCode::Pong => 0xA,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Parsed {
    /// Кадра ещё не хватает — сколько байт минимум нужно дочитать.
    Need(usize),
    Frame {
        op: OpCode,
        fin: bool,
        /// Смещение и длина полезной нагрузки внутри буфера (уже размаскированной).
        start: usize,
        len: usize,
        /// Сколько байт занял весь кадр.
        consumed: usize,
    },
    /// Протокол нарушен — соединение закрывается.
    Bad(&'static str),
}

/// Разбирает первый кадр в буфере, размаскируя нагрузку на месте.
pub fn parse(buf: &mut [u8]) -> Parsed {
    if buf.len() < 2 {
        return Parsed::Need(2 - buf.len());
    }
    let b0 = buf[0];
    let b1 = buf[1];

    if b0 & 0x70 != 0 {
        return Parsed::Bad("rsv_set");
    }
    let Some(op) = OpCode::from(b0 & 0x0F) else {
        return Parsed::Bad("bad_opcode");
    };
    let fin = b0 & 0x80 != 0;
    let masked = b1 & 0x80 != 0;
    if !masked {
        // клиент обязан маскировать (RFC 6455)
        return Parsed::Bad("unmasked");
    }

    let short = (b1 & 0x7F) as usize;
    let (len, mut off) = match short {
        126 => {
            if buf.len() < 4 {
                return Parsed::Need(4 - buf.len());
            }
            (u16::from_be_bytes([buf[2], buf[3]]) as usize, 4)
        }
        127 => {
            if buf.len() < 10 {
                return Parsed::Need(10 - buf.len());
            }
            let mut v = [0u8; 8];
            v.copy_from_slice(&buf[2..10]);
            (u64::from_be_bytes(v) as usize, 10)
        }
        n => (n, 2),
    };

    if len > MAX_FRAME {
        return Parsed::Bad("too_large");
    }
    if matches!(op, OpCode::Close | OpCode::Ping | OpCode::Pong) && (len > 125 || !fin) {
        return Parsed::Bad("bad_control");
    }

    let need = off + 4 + len;
    if buf.len() < need {
        return Parsed::Need(need - buf.len());
    }

    let mask = [buf[off], buf[off + 1], buf[off + 2], buf[off + 3]];
    off += 4;

    unmask(&mut buf[off..off + len], mask);

    Parsed::Frame {
        op,
        fin,
        start: off,
        len,
        consumed: need,
    }
}

/// Размаскирование словами по восемь байт.
fn unmask(data: &mut [u8], mask: [u8; 4]) {
    if mask == [0, 0, 0, 0] {
        return;
    }
    let mut key = [0u8; 8];
    key[..4].copy_from_slice(&mask);
    key[4..].copy_from_slice(&mask);
    let k = u64::from_le_bytes(key);

    let chunks = data.len() / 8;
    let (head, tail) = data.split_at_mut(chunks * 8);
    for c in head.chunks_exact_mut(8) {
        let mut v = [0u8; 8];
        v.copy_from_slice(c);
        let x = u64::from_le_bytes(v) ^ k;
        c.copy_from_slice(&x.to_le_bytes());
    }
    let base = chunks * 8;
    for (i, b) in tail.iter_mut().enumerate() {
        *b ^= mask[(base + i) % 4];
    }
}

/// Собирает кадр сервера (без маски — сервер не маскирует).
pub fn write_frame(out: &mut Vec<u8>, op: OpCode, payload: &[u8]) {
    out.push(0x80 | op.bits());
    let n = payload.len();
    if n < 126 {
        out.push(n as u8);
    } else if n <= u16::MAX as usize {
        out.push(126);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        out.push(127);
        out.extend_from_slice(&(n as u64).to_be_bytes());
    }
    out.extend_from_slice(payload);
}

/// Ответ на рукопожатие. `None` — запрос не является апгрейдом.
pub fn handshake(req: &str) -> Option<String> {
    let mut key = None;
    let mut upgrade = false;
    for line in req.split("\r\n") {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim();
        match k.as_str() {
            "sec-websocket-key" => key = Some(v.to_string()),
            "upgrade" if v.eq_ignore_ascii_case("websocket") => upgrade = true,
            _ => {}
        }
    }
    if !upgrade {
        return None;
    }
    let key = key?;
    let accept = crate::crypto::base64(&crate::crypto::sha1(format!("{key}{MAGIC}").as_bytes()));
    Some(format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_frame(op: OpCode, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
        let mut v = vec![0x80 | op.bits()];
        let n = payload.len();
        if n < 126 {
            v.push(0x80 | n as u8);
        } else {
            v.push(0x80 | 126);
            v.extend_from_slice(&(n as u16).to_be_bytes());
        }
        v.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            v.push(b ^ mask[i % 4]);
        }
        v
    }

    #[test]
    fn разбирает_короткий_текстовый_кадр() {
        let mut f = client_frame(OpCode::Text, br#"{"t":"m","c":40}"#, [1, 2, 3, 4]);
        match parse(&mut f) {
            Parsed::Frame { op, start, len, consumed, fin } => {
                assert_eq!(op, OpCode::Text);
                assert!(fin);
                assert_eq!(&f[start..start + len], br#"{"t":"m","c":40}"#);
                assert_eq!(consumed, f.len());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn размаскирование_совпадает_на_любой_длине() {
        for n in 0..200usize {
            let payload: Vec<u8> = (0..n).map(|i| (i * 7 % 251) as u8).collect();
            let mask = [0xDE, 0xAD, 0xBE, 0xEF];
            let mut f = client_frame(OpCode::Binary, &payload, mask);
            match parse(&mut f) {
                Parsed::Frame { start, len, .. } => {
                    assert_eq!(&f[start..start + len], &payload[..], "длина {n}");
                }
                other => panic!("длина {n}: {other:?}"),
            }
        }
    }

    #[test]
    fn просит_дочитать_при_неполном_кадре() {
        let full = client_frame(OpCode::Text, b"hello world", [9, 9, 9, 9]);
        for cut in 0..full.len() {
            let mut part = full[..cut].to_vec();
            match parse(&mut part) {
                Parsed::Need(n) => assert!(n > 0, "срез {cut}"),
                other => panic!("срез {cut}: {other:?}"),
            }
        }
    }

    #[test]
    fn немаскированный_кадр_отвергается() {
        let mut v = vec![0x81, 0x03, b'a', b'b', b'c'];
        assert_eq!(parse(&mut v), Parsed::Bad("unmasked"));
    }

    #[test]
    fn слишком_большой_кадр_отвергается_до_чтения() {
        let mut v = vec![0x82, 0x80 | 127];
        v.extend_from_slice(&(1_000_000u64).to_be_bytes());
        v.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(parse(&mut v), Parsed::Bad("too_large"));
    }

    #[test]
    fn кривые_опкоды_и_rsv() {
        let mut v = vec![0x8F, 0x80, 0, 0, 0, 0];
        assert_eq!(parse(&mut v), Parsed::Bad("bad_opcode"));
        let mut v = vec![0xC1, 0x80, 0, 0, 0, 0];
        assert_eq!(parse(&mut v), Parsed::Bad("rsv_set"));
    }

    #[test]
    fn управляющий_кадр_не_бывает_длинным() {
        let payload = vec![b'x'; 200];
        let mut f = client_frame(OpCode::Ping, &payload, [1, 1, 1, 1]);
        assert_eq!(parse(&mut f), Parsed::Bad("bad_control"));
    }

    #[test]
    fn несколько_кадров_подряд_разбираются() {
        let a = client_frame(OpCode::Text, b"one", [1, 2, 3, 4]);
        let b = client_frame(OpCode::Text, b"two", [5, 6, 7, 8]);
        let mut buf = a.clone();
        buf.extend_from_slice(&b);

        let mut got = Vec::new();
        loop {
            match parse(&mut buf) {
                Parsed::Frame { start, len, consumed, .. } => {
                    got.push(String::from_utf8(buf[start..start + len].to_vec()).unwrap());
                    buf.drain(..consumed);
                }
                _ => break,
            }
        }
        assert_eq!(got, vec!["one", "two"]);
    }

    #[test]
    fn кадр_сервера_собирается_по_rfc() {
        let mut out = Vec::new();
        write_frame(&mut out, OpCode::Text, b"hi");
        assert_eq!(out, vec![0x81, 0x02, b'h', b'i']);

        let mut out = Vec::new();
        let big = vec![b'z'; 300];
        write_frame(&mut out, OpCode::Text, &big);
        assert_eq!(out[0], 0x81);
        assert_eq!(out[1], 126);
        assert_eq!(u16::from_be_bytes([out[2], out[3]]), 300);
        assert_eq!(out.len(), 4 + 300);
    }

    #[test]
    fn рукопожатие_по_примеру_rfc6455() {
        let req = "GET /ws HTTP/1.1\r\n\
                   Host: x\r\n\
                   Upgrade: websocket\r\n\
                   Connection: Upgrade\r\n\
                   Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n";
        let resp = handshake(req).unwrap();
        assert!(resp.starts_with("HTTP/1.1 101"));
        assert!(resp.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    }

    #[test]
    fn обычный_запрос_не_апгрейдится() {
        assert!(handshake("GET / HTTP/1.1\r\nHost: x\r\n\r\n").is_none());
    }
}
