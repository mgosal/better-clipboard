//! Local ingest API: lets other apps on this Mac push text into Better Clipboard.
//!
//! The server binds to 127.0.0.1 only, requires a bearer token, and rejects any
//! request that looks like it came from a browser (an `Origin` header or a
//! foreign `Host`), so web pages cannot overwrite the clipboard.

use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

pub const DEFAULT_API_PORT: u16 = 47823;
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 128 * 1024;
const MAX_SOURCE_CHARS: usize = 80;
const MAX_LABEL_CHARS: usize = 200;
const RATE_LIMIT_BURST: f64 = 5.0;
const RATE_LIMIT_PER_SECOND: f64 = 5.0;
const TOKEN_FILE: &str = "api-token";
const TOKEN_PREFIX: &str = "bc_";
const BIND_RETRIES: u32 = 10;
const BIND_RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestKind {
    Text,
    OneTimeCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestRequest {
    pub text: String,
    pub source: String,
    pub label: Option<String>,
    pub kind: IngestKind,
    pub notify: bool,
}

/// Places an ingested item on the clipboard and in history, returning the history item id.
pub type IngestHandler = dyn Fn(IngestRequest) -> Result<String, String> + Send + Sync;

pub struct IngestServer {
    server: Arc<tiny_http::Server>,
    worker: Option<JoinHandle<()>>,
}

impl IngestServer {
    pub fn start(
        port: u16,
        token: Arc<RwLock<String>>,
        handler: Box<IngestHandler>,
    ) -> Result<Self> {
        let server = Arc::new(bind_with_retry(port)?);
        let worker_server = Arc::clone(&server);
        let worker = thread::spawn(move || {
            let mut limiter = RateLimiter::new(Instant::now());
            for mut request in worker_server.incoming_requests() {
                let reply = {
                    let token = token.read().map(|token| token.clone()).unwrap_or_default();
                    handle_request(&mut request, &token, port, &mut limiter, handler.as_ref())
                };
                let body = reply.body.to_string();
                let response = tiny_http::Response::from_string(body)
                    .with_status_code(reply.status)
                    .with_header(
                        "Content-Type: application/json"
                            .parse::<tiny_http::Header>()
                            .expect("static header"),
                    );
                let _ = request.respond(response);
            }
        });

        Ok(Self {
            server,
            worker: Some(worker),
        })
    }

    pub fn stop(&mut self) {
        self.server.unblock();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for IngestServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A server stopped moments ago releases its socket asynchronously, so a quick
/// off/on toggle in Settings retries briefly instead of failing.
fn bind_with_retry(port: u16) -> Result<tiny_http::Server> {
    let mut attempts = 0;
    loop {
        match tiny_http::Server::http(("127.0.0.1", port)) {
            Ok(server) => return Ok(server),
            Err(_) if attempts < BIND_RETRIES => {
                attempts += 1;
                thread::sleep(BIND_RETRY_DELAY);
            }
            Err(err) => anyhow::bail!("bind 127.0.0.1:{port}: {err}"),
        }
    }
}

fn handle_request(
    request: &mut tiny_http::Request,
    token: &str,
    port: u16,
    limiter: &mut RateLimiter,
    handler: &IngestHandler,
) -> HttpReply {
    let header = |name: &'static str| {
        request
            .headers()
            .iter()
            .find(|header| header.field.equiv(name))
            .map(|header| header.value.as_str().to_owned())
    };
    let host = header("Host");
    let origin_present = header("Origin").is_some();
    let authorization = header("Authorization");
    let method = request.method().as_str().to_owned();
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_owned();
    let remote_loopback = request
        .remote_addr()
        .is_some_and(|addr| addr.ip().is_loopback());

    let mut body = Vec::new();
    let read_ok = request
        .as_reader()
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .is_ok();

    route(
        &HttpRequestParts {
            method: &method,
            path: &path,
            remote_loopback,
            host: host.as_deref(),
            origin_present,
            authorization: authorization.as_deref(),
            body: if read_ok { &body } else { &[] },
        },
        token,
        port,
        limiter,
        Instant::now(),
        handler,
    )
}

#[derive(Debug)]
pub struct HttpRequestParts<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub remote_loopback: bool,
    pub host: Option<&'a str>,
    pub origin_present: bool,
    pub authorization: Option<&'a str>,
    pub body: &'a [u8],
}

#[derive(Debug, PartialEq)]
pub struct HttpReply {
    pub status: u16,
    pub body: serde_json::Value,
}

fn error(status: u16, message: &str) -> HttpReply {
    HttpReply {
        status,
        body: json!({ "error": message }),
    }
}

pub fn route(
    request: &HttpRequestParts,
    token: &str,
    port: u16,
    limiter: &mut RateLimiter,
    now: Instant,
    handler: &IngestHandler,
) -> HttpReply {
    if !request.remote_loopback {
        return error(403, "only loopback clients are allowed");
    }
    // Browsers always attach Origin to cross-site POSTs and CORS preflights.
    // Local apps don't, so any Origin means a web page is trying to reach us.
    if request.origin_present {
        return error(403, "browser requests are not allowed");
    }
    // Guards against DNS rebinding, where a web page reaches 127.0.0.1 via its own hostname.
    if !request.host.is_some_and(|host| host_is_local(host, port)) {
        return error(403, "unexpected Host header");
    }
    if token.is_empty() || !bearer_matches(request.authorization, token) {
        return error(401, "missing or invalid bearer token");
    }
    if !limiter.allow(now) {
        return error(429, "too many requests");
    }

    match (request.method, request.path) {
        ("GET", "/v1/health") => HttpReply {
            status: 200,
            body: json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }),
        },
        ("POST", "/v1/clip") => {
            if request.body.len() > MAX_BODY_BYTES {
                return error(413, "request body too large");
            }
            let ingest = match parse_clip_body(request.body) {
                Ok(ingest) => ingest,
                Err(reply) => return reply,
            };
            match handler(ingest) {
                Ok(id) => HttpReply {
                    status: 201,
                    body: json!({ "id": id }),
                },
                Err(message) => error(500, &message),
            }
        }
        (_, "/v1/health") | (_, "/v1/clip") => error(405, "method not allowed"),
        _ => error(404, "not found"),
    }
}

#[derive(Deserialize)]
struct ClipBody {
    text: Option<String>,
    source: Option<String>,
    label: Option<String>,
    kind: Option<String>,
    notify: Option<bool>,
}

fn parse_clip_body(body: &[u8]) -> Result<IngestRequest, HttpReply> {
    let body: ClipBody = serde_json::from_slice(body)
        .map_err(|err| error(400, &format!("invalid JSON body: {err}")))?;

    let text = body.text.unwrap_or_default();
    if text.trim().is_empty() {
        return Err(error(400, "`text` is required"));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(error(413, "`text` exceeds 64 KB"));
    }
    let source = clean_field(body.source.as_deref(), MAX_SOURCE_CHARS)
        .ok_or_else(|| error(400, "`source` is required"))?;
    let label = clean_field(body.label.as_deref(), MAX_LABEL_CHARS);
    let kind = match body.kind.as_deref() {
        Some("one-time-code") => IngestKind::OneTimeCode,
        _ => IngestKind::Text,
    };

    Ok(IngestRequest {
        text,
        source,
        label,
        kind,
        notify: body.notify.unwrap_or(false),
    })
}

fn clean_field(value: Option<&str>, max_chars: usize) -> Option<String> {
    let cleaned: String = value?
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max_chars)
        .collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

fn host_is_local(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn bearer_matches(authorization: Option<&str>, token: &str) -> bool {
    let Some(presented) = authorization.and_then(|value| value.strip_prefix("Bearer ")) else {
        return false;
    };
    constant_time_eq(presented.trim().as_bytes(), token.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub struct RateLimiter {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(now: Instant) -> Self {
        Self {
            tokens: RATE_LIMIT_BURST,
            last: now,
        }
    }

    pub fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * RATE_LIMIT_PER_SECOND).min(RATE_LIMIT_BURST);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

fn token_path(data_dir: &Path) -> PathBuf {
    data_dir.join(TOKEN_FILE)
}

pub fn load_or_create_token(data_dir: &Path) -> Result<String> {
    let path = token_path(data_dir);
    if let Ok(existing) = fs::read_to_string(&path) {
        let existing = existing.trim();
        if existing.starts_with(TOKEN_PREFIX) {
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            return Ok(existing.to_owned());
        }
    }
    regenerate_token(data_dir)
}

pub fn regenerate_token(data_dir: &Path) -> Result<String> {
    fs::create_dir_all(data_dir).context("create data directory")?;
    let token = format!(
        "{TOKEN_PREFIX}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let path = token_path(data_dir);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .context("open API token file")?;
    file.write_all(token.as_bytes())
        .context("write API token file")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .context("restrict API token file permissions")?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "bc_test-token";
    const PORT: u16 = 47823;

    fn parts<'a>(method: &'a str, path: &'a str, body: &'a [u8]) -> HttpRequestParts<'a> {
        HttpRequestParts {
            method,
            path,
            remote_loopback: true,
            host: Some("127.0.0.1:47823"),
            origin_present: false,
            authorization: Some("Bearer bc_test-token"),
            body,
        }
    }

    fn accept_all(request: IngestRequest) -> Result<String, String> {
        Ok(format!("{}:{:?}", request.source, request.kind))
    }

    fn run(request: &HttpRequestParts) -> HttpReply {
        let mut limiter = RateLimiter::new(Instant::now());
        route(
            request,
            TOKEN,
            PORT,
            &mut limiter,
            Instant::now(),
            &accept_all,
        )
    }

    #[test]
    fn accepts_valid_clip() {
        let body = br#"{"text":"482913","source":"Mail","label":"Code","kind":"one-time-code","notify":true}"#;
        let reply = run(&parts("POST", "/v1/clip", body));
        assert_eq!(reply.status, 201);
        assert_eq!(reply.body["id"], "Mail:OneTimeCode");
    }

    #[test]
    fn unknown_kind_is_plain_text() {
        let body = br#"{"text":"hello","source":"Tool","kind":"mystery"}"#;
        let reply = run(&parts("POST", "/v1/clip", body));
        assert_eq!(reply.status, 201);
        assert_eq!(reply.body["id"], "Tool:Text");
    }

    #[test]
    fn health_reports_version() {
        let reply = run(&parts("GET", "/v1/health", b""));
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["ok"], true);
        assert_eq!(reply.body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn rejects_missing_or_wrong_token() {
        let mut request = parts("GET", "/v1/health", b"");
        request.authorization = None;
        assert_eq!(run(&request).status, 401);
        request.authorization = Some("Bearer bc_wrong");
        assert_eq!(run(&request).status, 401);
        request.authorization = Some("bc_test-token");
        assert_eq!(run(&request).status, 401);
    }

    #[test]
    fn rejects_browser_and_remote_requests() {
        let mut request = parts("POST", "/v1/clip", br#"{"text":"x","source":"y"}"#);
        request.origin_present = true;
        assert_eq!(run(&request).status, 403);

        let mut request = parts("POST", "/v1/clip", br#"{"text":"x","source":"y"}"#);
        request.remote_loopback = false;
        assert_eq!(run(&request).status, 403);

        let mut request = parts("POST", "/v1/clip", br#"{"text":"x","source":"y"}"#);
        request.host = Some("evil.example:47823");
        assert_eq!(run(&request).status, 403);

        let mut request = parts("OPTIONS", "/v1/clip", b"");
        request.origin_present = true;
        assert_eq!(run(&request).status, 403);
    }

    #[test]
    fn validates_body() {
        assert_eq!(run(&parts("POST", "/v1/clip", b"not json")).status, 400);
        assert_eq!(
            run(&parts("POST", "/v1/clip", br#"{"text":"  ","source":"y"}"#)).status,
            400
        );
        assert_eq!(
            run(&parts("POST", "/v1/clip", br#"{"text":"x"}"#)).status,
            400
        );
        let big = format!(
            r#"{{"text":"{}","source":"y"}}"#,
            "a".repeat(MAX_TEXT_BYTES + 1)
        );
        assert_eq!(run(&parts("POST", "/v1/clip", big.as_bytes())).status, 413);
    }

    #[test]
    fn unknown_routes_and_methods() {
        assert_eq!(run(&parts("GET", "/v1/clip", b"")).status, 405);
        assert_eq!(run(&parts("GET", "/nope", b"")).status, 404);
    }

    #[test]
    fn rate_limits_bursts() {
        let start = Instant::now();
        let mut limiter = RateLimiter::new(start);
        for _ in 0..5 {
            assert!(limiter.allow(start));
        }
        assert!(!limiter.allow(start));
        assert!(limiter.allow(start + Duration::from_millis(250)));
    }

    fn raw_request(port: u16, request: &str) -> String {
        use std::net::TcpStream;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.write_all(request.as_bytes()).expect("send");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read");
        response
    }

    #[test]
    fn serves_requests_over_loopback() {
        let port = 47900 + (std::process::id() % 90) as u16;
        let token = Arc::new(RwLock::new(TOKEN.to_owned()));
        let mut server =
            IngestServer::start(port, Arc::clone(&token), Box::new(accept_all)).expect("start");

        let body = r#"{"text":"482913","source":"Mail","kind":"one-time-code"}"#;
        let ok = raw_request(
            port,
            &format!(
                "POST /v1/clip HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(ok.starts_with("HTTP/1.1 201"), "{ok}");
        assert!(ok.contains(r#"{"id":"Mail:OneTimeCode"}"#), "{ok}");

        let browser = raw_request(
            port,
            &format!(
                "POST /v1/clip HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: https://evil.example\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        );
        assert!(browser.starts_with("HTTP/1.1 403"), "{browser}");
        assert!(
            !browser
                .to_ascii_lowercase()
                .contains("access-control-allow")
        );

        // A regenerated token takes effect without restarting the server.
        *token.write().unwrap() = "bc_rotated".to_owned();
        let stale = raw_request(
            port,
            &format!(
                "GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(stale.starts_with("HTTP/1.1 401"), "{stale}");

        server.stop();
        drop(server);
        let mut restarted =
            IngestServer::start(port, token, Box::new(accept_all)).expect("restart on same port");
        restarted.stop();
    }

    #[test]
    fn token_file_is_private_and_stable() {
        let dir = std::env::temp_dir().join(format!("better-clipboard-token-{}", Uuid::new_v4()));
        let token = load_or_create_token(&dir).expect("create token");
        assert!(token.starts_with(TOKEN_PREFIX));
        assert_eq!(load_or_create_token(&dir).expect("reload token"), token);
        let mode = fs::metadata(token_path(&dir)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_ne!(regenerate_token(&dir).expect("regenerate"), token);
        let _ = fs::remove_dir_all(dir);
    }
}
