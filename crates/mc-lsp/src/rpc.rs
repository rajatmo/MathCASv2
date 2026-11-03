//! JSON-RPC messages with the protocol's base framing: a header block (`Content-Length: n`, optionally
//! `Content-Type`), an empty line, then `n` bytes of UTF-8 JSON.
use serde_json::Value;
use std::io::{self, BufRead, Write};

/// The largest message accepted (a guard against a corrupt header asking for gigabytes).
pub const MAX_MESSAGE: usize = 64 << 20;

/// One incoming message.
#[derive(Debug)]
pub enum Incoming {
    /// a well-formed JSON value
    Message(Value),
    /// a message whose body is not JSON (answered with a parse error)
    Malformed(String),
}

/// Reads the next message; `None` at the end of the input.
pub fn read_message(r: &mut impl BufRead) -> io::Result<Option<Incoming>> {
    let mut length = None;
    let mut line = String::new();
    let mut any_header = false;
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return if any_header { Err(io::Error::new(io::ErrorKind::UnexpectedEof, "end of input in a header")) } else { Ok(None) };
        }
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() {
            if any_header {
                break;
            }
            // stray blank lines between messages are tolerated
            continue;
        }
        any_header = true;
        if let Some((name, value)) = l.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let n = length.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "a message without Content-Length"))?;
    if n > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("a message of {n} bytes is too long")));
    }
    let mut body = vec![0; n];
    r.read_exact(&mut body)?;
    Ok(Some(match serde_json::from_slice(&body) {
        Ok(v) => Incoming::Message(v),
        Err(e) => Incoming::Malformed(e.to_string()),
    }))
}

/// Writes one message with its header, and flushes.
pub fn write_message(w: &mut impl Write, v: &Value) -> io::Result<()> {
    let body = v.to_string();
    write!(w, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    w.flush()
}

/// Standard error codes.
pub mod code {
    /// the body is not JSON
    pub const PARSE_ERROR: i64 = -32700;
    /// not a valid request object
    pub const INVALID_REQUEST: i64 = -32600;
    /// an unknown method
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// parameters missing or of the wrong shape
    pub const INVALID_PARAMS: i64 = -32602;
    /// the server failed while handling the request
    pub const INTERNAL_ERROR: i64 = -32603;
    /// a request before `initialize`
    pub const SERVER_NOT_INITIALIZED: i64 = -32002;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip_and_errors() {
        let mut buf = vec![];
        write_message(&mut buf, &json!({"jsonrpc": "2.0", "id": 1, "method": "x", "params": {"s": "θ"}})).unwrap();
        write_message(&mut buf, &json!({"jsonrpc": "2.0", "method": "y"})).unwrap();
        buf.extend_from_slice(b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: 3\r\n\r\n{x}");
        let mut r = io::BufReader::new(&buf[..]);
        let Some(Incoming::Message(a)) = read_message(&mut r).unwrap() else { panic!() };
        assert_eq!(a["params"]["s"], "θ");
        let Some(Incoming::Message(b)) = read_message(&mut r).unwrap() else { panic!() };
        assert_eq!(b["method"], "y");
        assert!(matches!(read_message(&mut r).unwrap(), Some(Incoming::Malformed(_))));
        assert!(read_message(&mut r).unwrap().is_none());

        let mut r = io::BufReader::new(&b"Content-Type: x\r\n\r\n{}"[..]);
        assert!(read_message(&mut r).is_err());
        let mut r = io::BufReader::new(&b"Content-Length: 10\r\n\r\n{}"[..]);
        assert!(read_message(&mut r).is_err());
    }
}
