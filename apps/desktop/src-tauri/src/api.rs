//! Signed API client.
//!
//! Talks to the F3 transmission service using the exact same signing scheme the
//! server verifies (F1 canonicalisation, epoch keys). No second protocol is
//! invented here.
//!
//! Scope of what this client can claim: a request signed by this device proves
//! *this light seed asked for this*. It does not upgrade any evidence: the
//! server still decides whether a usage event is countable, and E3 stays E3.
//!
//! Networking is deliberately minimal and loopback-friendly: the service is a
//! local process for now, and pulling in a full HTTP stack would add surface for
//! no benefit.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use light_core::identity::RootIdentity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEFAULT_BASE: &str = "http://127.0.0.1:8787";
const TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("无法连接传光服务（{0}）。光点仍可离线使用。")]
    Unreachable(String),
    #[error("服务返回错误 {status}: {code}")]
    Server { status: u16, code: String },
    #[error("响应格式不正确：{0}")]
    Protocol(String),
    #[error("尚未配置传光服务地址")]
    NotConfigured,
}

pub struct ApiClient {
    base: String,
    anon_id: String,
    epoch: u32,
    seed: [u8; 32],
    counter: std::sync::atomic::AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    pub id: String,
    pub token: String,
    pub receive_url: String,
    pub nfc_payload: String,
    pub expires_at_ms: i64,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransmissionView {
    pub id: String,
    pub state: String,
    #[serde(default)]
    pub version: u32,
    /// Present on GET, absent on transition responses.
    #[serde(default)]
    pub expires_at_ms: i64,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeView {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub visibility: String,
    pub created_at_ms: i64,
    pub activated_at_ms: Option<i64>,
    pub transmission_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoView {
    pub id: i64,
    pub at_ms: i64,
    pub message: String,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathsView {
    pub transmitted_nodes: u32,
    pub outgoing: Vec<EdgeView>,
    pub incoming: Vec<EdgeView>,
    pub echoes: Vec<EchoView>,
}

impl ApiClient {
    pub fn new(base: &str, identity: &RootIdentity, epoch: u32) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            anon_id: identity.anon_id(),
            epoch,
            seed: identity.seed(),
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn epoch_key(&self) -> light_core::identity::RootIdentity {
        light_core::identity::RootIdentity::from_seed(self.seed)
    }

    fn certificate_json(&self) -> serde_json::Value {
        let cert = self.epoch_key().epoch_certificate(self.epoch);
        serde_json::json!({
            "epoch": cert.epoch,
            "root_pubkey_hex": cert.root_pubkey_hex,
            "epoch_pubkey_hex": cert.epoch_pubkey_hex,
            "signature_hex": cert.signature_hex,
        })
    }

    /// Build the signature headers for one request.
    fn signed_headers(&self, method: &str, path: &str, body: &str) -> Vec<(String, String)> {
        use ed25519_dalek::Signer;
        let ts = now_ms();
        let n = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let nonce = format!("desktop-{ts}-{n}-{}", std::process::id());

        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let body_hash: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        let canonical = format!("FL1\n{}\n{}\n{}\n{}\n{}", method, path, body_hash, ts, nonce);

        let key = self.epoch_key().epoch_signing_key(self.epoch);
        let signature = key.sign(canonical.as_bytes());
        let sig_hex: String = signature.to_bytes().iter().map(|b| format!("{b:02x}")).collect();

        vec![
            ("x-fl-identity".into(), self.anon_id.clone()),
            ("x-fl-epoch".into(), self.epoch.to_string()),
            ("x-fl-timestamp".into(), ts.to_string()),
            ("x-fl-nonce".into(), nonce),
            ("x-fl-signature".into(), sig_hex),
        ]
    }

    // ---- endpoints -----------------------------------------------------

    pub fn register(&self) -> Result<(), ApiError> {
        let body = serde_json::json!({
            "anon_id": self.anon_id,
            "cert": self.certificate_json(),
        })
        .to_string();
        // Registration is unauthenticated by signature: the certificate is the proof.
        let (status, text) = http_request(&self.base, "POST", "/v1/identities/register", &body, &[])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        Ok(())
    }

    pub fn create_invitation(&self, channel: &str) -> Result<Invitation, ApiError> {
        let body = serde_json::json!({ "channel": channel }).to_string();
        let path = "/v1/transmissions";
        let idem = format!("inv-{}-{}", self.anon_id, now_ms());
        let (status, text) = self.authed("POST", path, &body, &[("idempotency-key", &idem)])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    /// Look up a session by its one-time token (no signature needed).
    pub fn resolve(&self, token: &str) -> Result<serde_json::Value, ApiError> {
        let path = format!("/v1/transmissions/resolve?token={token}");
        let (status, text) = http_request(&self.base, "GET", &path, "", &[])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    pub fn accept(&self, id: &str) -> Result<TransmissionView, ApiError> {
        let path = format!("/v1/transmissions/{id}/accept");
        let idem = format!("ac-{id}-{}", now_ms());
        let (status, text) = self.authed("POST", &path, "", &[("idempotency-key", &idem)])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    pub fn activate(&self, id: &str, usage_event_id: &str) -> Result<serde_json::Value, ApiError> {
        let path = format!("/v1/transmissions/{id}/activate");
        let body = serde_json::json!({ "usage_event_id": usage_event_id }).to_string();
        let idem = format!("act-{id}-{usage_event_id}");
        let (status, text) = self.authed("POST", &path, &body, &[("idempotency-key", &idem)])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    pub fn get_transmission(&self, id: &str) -> Result<TransmissionView, ApiError> {
        let path = format!("/v1/transmissions/{id}");
        let (status, text) = self.authed("GET", &path, "", &[])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    pub fn confirm(&self, id: &str) -> Result<TransmissionView, ApiError> {
        let path = format!("/v1/transmissions/{id}/confirm");
        let idem = format!("cf-{id}-{}", now_ms());
        let (status, text) = self.authed("POST", &path, "", &[("idempotency-key", &idem)])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    pub fn cancel(&self, id: &str) -> Result<TransmissionView, ApiError> {
        let path = format!("/v1/transmissions/{id}/cancel");
        let (status, text) = self.authed("POST", &path, "", &[])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    pub fn light_paths(&self) -> Result<PathsView, ApiError> {
        let (status, text) = self.authed("GET", "/v1/light/paths", "", &[])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    pub fn attest_usage(
        &self,
        event_id: &str,
        family: &str,
        level: &str,
        adapter: &str,
        registry_semver: &str,
        observed_at_ms: i64,
    ) -> Result<serde_json::Value, ApiError> {
        let body = serde_json::json!({
            "event_id": event_id,
            "model_family": family,
            "evidence_level": level,
            "proof_method": "local_sqlite_metadata",
            "adapter_id": adapter,
            "adapter_version": "1.0.0",
            "registry_semver": registry_semver,
            "observed_at_ms": observed_at_ms,
        })
        .to_string();
        let idem = format!("at-{event_id}");
        let (status, text) = self.authed("POST", "/v1/usage/attest", &body, &[("idempotency-key", &idem)])?;
        if status >= 400 {
            return Err(server_error(status, &text));
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Protocol(e.to_string()))
    }

    fn authed(
        &self,
        method: &str,
        path: &str,
        body: &str,
        extra: &[(&str, &str)],
    ) -> Result<(u16, String), ApiError> {
        let mut headers = self.signed_headers(method, path, body);
        for (k, v) in extra {
            headers.push((k.to_string(), v.to_string()));
        }
        http_request(&self.base, method, path, body, &headers)
    }

    /// Open the SSE stream and call `on_event` for each frame.
    ///
    /// Returns when the connection closes; callers reconnect with the last seen
    /// event id, which is what makes delivery at-least-once with client-side
    /// idempotency.
    pub fn stream_events<F>(&self, last_event_id: Option<i64>, mut on_event: F) -> Result<Option<i64>, ApiError>
    where
        F: FnMut(&str, Option<i64>, &str),
    {
        let path = "/v1/events/stream";
        let headers = self.signed_headers("GET", path, "");
        let (mut stream, status) = open_stream(&self.base, path, &headers)?;
        if status >= 400 {
            return Err(ApiError::Server { status, code: "SSE_REJECTED".into() });
        }

        let reader = BufReader::new(stream.try_clone().map_err(|e| ApiError::Unreachable(e.to_string()))?);
        let mut current_event = String::new();
        let mut current_id: Option<i64> = last_event_id;
        let mut last_seen = last_event_id;

        for line in reader.lines() {
            let Ok(line) = line else { break };
            if line.is_empty() {
                current_event.clear();
                continue;
            }
            if let Some(rest) = line.strip_prefix("id: ") {
                current_id = rest.trim().parse().ok();
                last_seen = current_id;
            } else if let Some(rest) = line.strip_prefix("event: ") {
                current_event = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("data: ") {
                if !current_event.is_empty() {
                    on_event(&current_event, current_id, rest);
                }
            }
        }
        let _ = stream.flush();
        Ok(last_seen)
    }
}

// ---------------------------------------------------------------------------
// Minimal HTTP/1.1 over TCP.
// ---------------------------------------------------------------------------

fn split_url(base: &str) -> Result<(String, u16), ApiError> {
    let rest = base
        .strip_prefix("http://")
        .ok_or_else(|| ApiError::NotConfigured)?;
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|_| ApiError::NotConfigured)?),
        None => (rest.to_string(), 80),
    };
    Ok((host, port))
}

fn connect(base: &str) -> Result<TcpStream, ApiError> {
    let (host, port) = split_url(base)?;
    let addr = format!("{host}:{port}");
    let sock = addr
        .to_socket_addrs()
        .map_err(|e| ApiError::Unreachable(format!("{addr}: {e}")))?
        .next()
        .ok_or_else(|| ApiError::Unreachable(addr.clone()))?;
    let stream = TcpStream::connect_timeout(&sock, TIMEOUT)
        .map_err(|_| ApiError::Unreachable(addr.clone()))?;
    stream.set_read_timeout(Some(TIMEOUT)).ok();
    stream.set_write_timeout(Some(TIMEOUT)).ok();
    Ok(stream)
}

fn http_request(
    base: &str,
    method: &str,
    path: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<(u16, String), ApiError> {
    let mut stream = connect(base)?;
    let (host, port) = split_url(base)?;

    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nAccept: application/json\r\n"
    );
    if !body.is_empty() {
        head.push_str("Content-Type: application/json\r\n");
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");

    stream
        .write_all(head.as_bytes())
        .map_err(|e| ApiError::Unreachable(e.to_string()))?;
    if !body.is_empty() {
        stream
            .write_all(body.as_bytes())
            .map_err(|e| ApiError::Unreachable(e.to_string()))?;
    }
    stream.flush().ok();

    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| ApiError::Unreachable(e.to_string()))?;
    parse_http(&raw)
}

fn open_stream(
    base: &str,
    path: &str,
    headers: &[(String, String)],
) -> Result<(TcpStream, u16), ApiError> {
    let mut stream = connect(base)?;
    let (host, port) = split_url(base)?;
    let mut head = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: text/event-stream\r\nConnection: keep-alive\r\n"
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .map_err(|e| ApiError::Unreachable(e.to_string()))?;
    stream.flush().ok();

    // Read only the header block so the body stays available for streaming.
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = stream
            .read(&mut byte)
            .map_err(|e| ApiError::Unreachable(e.to_string()))?;
        if n == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
        if buf.len() > 16 * 1024 {
            return Err(ApiError::Protocol("response headers too large".into()));
        }
    }
    let head = String::from_utf8_lossy(&buf).to_string();
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| ApiError::Protocol("no status line".into()))?;
    Ok((stream, status))
}

fn parse_http(raw: &[u8]) -> Result<(u16, String), ApiError> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| ApiError::Protocol("no header terminator".into()))?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body = &raw[split + 4..];

    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ApiError::Protocol("bad status line".into()))?;

    let mut chunked = false;
    for line in head.lines().skip(1) {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
    }
    let bytes = if chunked { dechunk(body) } else { body.to_vec() };
    Ok((status, String::from_utf8_lossy(&bytes).to_string()))
}

fn dechunk(mut data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(pos) = data.windows(2).position(|w| w == b"\r\n") else {
            return out;
        };
        let Ok(size) = usize::from_str_radix(
            String::from_utf8_lossy(&data[..pos]).trim(),
            16,
        ) else {
            return out;
        };
        data = &data[pos + 2..];
        if size == 0 || data.len() < size {
            return out;
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size..];
        if data.starts_with(b"\r\n") {
            data = &data[2..];
        }
    }
}

fn server_error(status: u16, text: &str) -> ApiError {
    let code = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| "UNKNOWN".into());
    ApiError::Server { status, code }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_non_http_bases() {
        assert!(matches!(split_url("https://example.com"), Err(ApiError::NotConfigured)));
        assert!(split_url("http://127.0.0.1:8787").is_ok());
    }

    #[test]
    fn parses_a_simple_response() {
        let raw = b"HTTP/1.1 201 Created\r\nContent-Length: 11\r\n\r\n{\"ok\":true}";
        let (status, body) = parse_http(raw).unwrap();
        assert_eq!(status, 201);
        assert_eq!(body, "{\"ok\":true}");
    }

    #[test]
    fn parses_a_chunked_response() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        let (status, body) = parse_http(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, "{\"a\":1}");
    }

    #[test]
    fn unreachable_service_is_reported_as_such() {
        let err = http_request("http://127.0.0.1:1", "GET", "/v1/x", "", &[]).unwrap_err();
        assert!(matches!(err, ApiError::Unreachable(_)), "got {err:?}");
    }

    #[test]
    fn signature_headers_are_well_formed() {
        let identity = RootIdentity::from_seed([3u8; 32]);
        let client = ApiClient::new("http://127.0.0.1:8787", &identity, 1);
        let headers = client.signed_headers("POST", "/v1/transmissions", "{}");
        let map: std::collections::HashMap<_, _> = headers.into_iter().collect();
        assert_eq!(map["x-fl-identity"], identity.anon_id());
        assert_eq!(map["x-fl-epoch"], "1");
        assert_eq!(map["x-fl-signature"].len(), 128);
        assert!(!map["x-fl-nonce"].is_empty());
    }
}
