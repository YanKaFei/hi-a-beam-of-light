//! A very small HTTP/1.1 client, deliberately limited to loopback.
//!
//! Ollama is a local service on `http://127.0.0.1:11434`. Pulling in a full HTTP
//! stack (with TLS, connection pools, redirect policies) for a loopback call
//! would add a large dependency and a wide attack surface for no benefit. This
//! client therefore:
//!
//! - refuses anything that is not plain `http://` to a loopback address,
//! - sends exactly one request per connection (`Connection: close`),
//! - understands `Content-Length` and `chunked` bodies.

use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub fn get_json(url: &str, timeout_ms: u64) -> Result<Value, String> {
    request("GET", url, None, timeout_ms)
}

pub fn post_json(url: &str, body: &str, timeout_ms: u64) -> Result<Value, String> {
    request("POST", url, Some(body), timeout_ms)
}

struct Target {
    host: String,
    port: u16,
    path: String,
}

fn parse(url: &str) -> Result<Target, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "only plain http:// loopback URLs are allowed".to_string())?;
    if rest.starts_with("https://") {
        return Err("https is not supported for the local service".into());
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>().map_err(|_| format!("bad port in {url}"))?,
        ),
        None => (authority.to_string(), 80),
    };
    if !is_loopback(&host) {
        return Err(format!("refusing non-loopback host `{host}`"));
    }
    Ok(Target {
        host,
        port,
        path: path.to_string(),
    })
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn request(method: &str, url: &str, body: Option<&str>, timeout_ms: u64) -> Result<Value, String> {
    let target = parse(url)?;
    let addr = format!("{}:{}", target.host.trim_matches(['[', ']']), target.port);
    let sock_addr = addr
        .to_socket_addrs()
        .map_err(|e| format!("resolve {addr}: {e}"))?
        .next()
        .ok_or_else(|| format!("no address for {addr}"))?;

    let timeout = Duration::from_millis(timeout_ms);
    let mut stream = TcpStream::connect_timeout(&sock_addr, timeout)
        .map_err(|_| format!("connect failed: {addr}"))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();

    let mut head = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n",
        target.path, addr
    );
    if let Some(b) = body {
        head.push_str("Content-Type: application/json\r\n");
        head.push_str(&format!("Content-Length: {}\r\n", b.len()));
    }
    head.push_str("\r\n");

    stream
        .write_all(head.as_bytes())
        .map_err(|e| format!("write: {e}"))?;
    if let Some(b) = body {
        stream
            .write_all(b.as_bytes())
            .map_err(|e| format!("write body: {e}"))?;
    }
    stream.flush().ok();

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| format!("read: {e}"))?;
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> Result<Value, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "malformed response: no header terminator".to_string())?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body_bytes = &raw[split + 4..];

    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("bad status line: {status_line}"))?;

    let mut chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim();
        if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        }
        if k == "content-length" {
            content_length = v.parse().ok();
        }
    }

    let body = if chunked {
        dechunk(body_bytes)?
    } else if let Some(len) = content_length {
        body_bytes[..len.min(body_bytes.len())].to_vec()
    } else {
        body_bytes.to_vec()
    };

    if status >= 400 {
        let text = String::from_utf8_lossy(&body).to_string();
        return Err(format!("status {status}: {text}"));
    }

    let text = String::from_utf8_lossy(&body).to_string();
    serde_json::from_str(&text).map_err(|e| format!("invalid json: {e}"))
}

fn dechunk(mut data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let Some(pos) = data.windows(2).position(|w| w == b"\r\n") else {
            return Err("malformed chunk header".into());
        };
        let size_str = String::from_utf8_lossy(&data[..pos]).to_string();
        let size = usize::from_str_radix(size_str.trim(), 16)
            .map_err(|_| format!("bad chunk size: {size_str}"))?;
        data = &data[pos + 2..];
        if size == 0 {
            return Ok(out);
        }
        if data.len() < size {
            return Err("truncated chunk".into());
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size..];
        if data.starts_with(b"\r\n") {
            data = &data[2..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_non_loopback_and_https() {
        assert!(parse("https://127.0.0.1:11434/api/version").is_err());
        assert!(parse("http://example.com/api").is_err());
        assert!(parse("http://127.0.0.1:11434/api/version").is_ok());
        assert!(parse("http://localhost:11434/api/tags").is_ok());
    }

    #[test]
    fn content_length_body_is_parsed() {
        let mut raw = b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}".to_vec();
        raw.push(b'\n');
        let v = parse_response(&raw).unwrap();
        assert_eq!(v["ok"], serde_json::json!(true));
    }

    #[test]
    fn chunked_body_is_decoded() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                    7\r\n{\"a\":1,\r\n5\r\n\"b\":2\r\n1\r\n}\r\n0\r\n\r\n"
            .to_vec();
        let v = parse_response(&raw).unwrap();
        assert_eq!(v["a"], serde_json::json!(1));
        assert_eq!(v["b"], serde_json::json!(2));
    }

    #[test]
    fn error_status_is_an_error() {
        let raw = b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\n{}".to_vec();
        let err = parse_response(&raw).unwrap_err();
        assert!(err.contains("404"));
    }
}
