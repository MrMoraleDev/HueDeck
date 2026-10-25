// SPDX-License-Identifier: GPL-3.0-or-later
//! A small HTTP/1.1 client behind the plugins' `XMLHttpRequest`.
//!
//! `http://` and `https://` to local-network hosts only ([`super::policy`]),
//! one request per connection (`Connection: close`), no proxy, no redirects,
//! no cookies. The whole exchange is bounded by the request's timeout; bodies
//! are capped at [`MAX_BODY`]. Failures are a response with status 0 and an
//! `error`, which the JS side turns into `onerror` / `ontimeout`. HTTPS
//! certificates are pinned, not checked against public CAs ([`super::tls`]).

use super::policy::resolve_lan;
use super::tls::{self, PinStore, TlsPolicy, Verdict};
use rustls::pki_types::ServerName;
use rustls::{ClientConnection, StreamOwned};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// Largest response (and request) body.
pub const MAX_BODY: usize = 4 << 20;
/// Default timeout of a request that sets none.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(3000);
/// Longest timeout a request may ask for.
pub const MAX_TIMEOUT: Duration = Duration::from_millis(10_000);
/// Longest header section accepted.
const MAX_HEAD: usize = 64 << 10;

/// What `XMLHttpRequest.send()` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequest {
    /// `GET`, `POST`, `PUT`, `DELETE`, ...
    pub method: String,
    /// `http://host[:port]/path?query`.
    pub url: String,
    /// Request headers set by the plugin.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Body (text).
    #[serde(default)]
    pub body: Option<String>,
    /// Milliseconds; 0 = [`DEFAULT_TIMEOUT`].
    #[serde(default)]
    pub timeout: u64,
    /// `https`: the certificate's SHA-256 (Lightloom extension `xhr.tlsPin`).
    #[serde(default, rename = "tlsPin")]
    pub tls_pin: Option<String>,
    /// `https`: the certificate's expected common name (`xhr.tlsName`).
    #[serde(default, rename = "tlsName")]
    pub tls_name: Option<String>,
}

/// What came back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpResponse {
    /// HTTP status; 0 when the request failed.
    pub status: u16,
    /// Reason phrase.
    #[serde(rename = "statusText")]
    pub status_text: String,
    /// Response headers, names lowercase.
    pub headers: Vec<(String, String)>,
    /// Body as text (UTF-8, lossy).
    pub body: String,
    /// Why the request failed (status 0).
    pub error: Option<String>,
    /// The failure was the timeout.
    #[serde(rename = "timedOut")]
    pub timed_out: bool,
    /// `https`: SHA-256 of the server's certificate (also when it was refused).
    #[serde(rename = "tlsFingerprint", skip_serializing_if = "Option::is_none", default)]
    pub tls_fingerprint: Option<String>,
}

impl HttpResponse {
    fn failed(error: impl Into<String>, timed_out: bool) -> Self {
        Self {
            error: Some(error.into()),
            timed_out,
            ..Self::default()
        }
    }
}

/// The parts of a URL the client needs.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    /// Host as written (no brackets).
    pub host: String,
    /// Port (80 or 443 by default).
    pub port: u16,
    /// Path and query, at least `/`.
    pub path: String,
    /// `https://`.
    pub tls: bool,
}

impl Target {
    fn default_port(&self) -> u16 {
        if self.tls {
            443
        } else {
            80
        }
    }
}

/// Split an `http://` or `https://` URL. Other schemes are refused.
pub fn parse_url(url: &str) -> Result<Target, String> {
    let url = url.trim();
    let (rest, tls) = match url.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => (rest, false),
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => (rest, true),
        Some((scheme, _)) => {
            return Err(format!(
                "{scheme}:// is not supported (http:// and https:// only)"
            ))
        }
        None => return Err(format!("not an absolute http:// URL: {url}")),
    };
    let default_port = if tls { 443 } else { 80 };
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let path = path.split('#').next().unwrap_or("/");
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if authority.contains('@') {
        return Err("credentials in URLs are not supported".into());
    }
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']').ok_or("bad IPv6 host")?;
        let port = after.strip_prefix(':').map(str::parse::<u16>).transpose();
        (
            h.to_string(),
            port.map_err(|_| "bad port")?.unwrap_or(default_port),
        )
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|_| "bad port")?),
            None => (authority.to_string(), default_port),
        }
    };
    if host.is_empty() {
        return Err("no host in URL".into());
    }
    Ok(Target {
        host,
        port,
        path,
        tls,
    })
}

fn timeout_of(req: &HttpRequest) -> Duration {
    if req.timeout == 0 {
        DEFAULT_TIMEOUT
    } else {
        Duration::from_millis(req.timeout).min(MAX_TIMEOUT)
    }
}

fn valid_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_graphic() && b != b':')
}

/// The request bytes for `req` to `target` (host header as written).
pub fn encode_request(req: &HttpRequest, target: &Target) -> Result<Vec<u8>, String> {
    let method = req.method.trim().to_ascii_uppercase();
    if !valid_token(&method) {
        return Err(format!("bad method {method:?}"));
    }
    let host = if target.host.contains(':') {
        format!("[{}]", target.host)
    } else {
        target.host.clone()
    };
    let host = if target.port == target.default_port() {
        host
    } else {
        format!("{host}:{}", target.port)
    };
    let mut out = format!(
        "{method} {} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n",
        target.path
    );
    let mut agent = false;
    for (name, value) in &req.headers {
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "host" | "connection" | "content-length" | "transfer-encoding" | "keep-alive"
        ) {
            continue;
        }
        if !valid_token(name) || value.contains(['\r', '\n']) {
            return Err(format!("bad header {name:?}"));
        }
        agent |= lower == "user-agent";
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    if !agent {
        out.push_str("User-Agent: Lightloom\r\n");
    }
    let body = req.body.as_deref().unwrap_or("");
    if body.len() > MAX_BODY {
        return Err("request body too large".into());
    }
    if !body.is_empty() || !matches!(method.as_str(), "GET" | "HEAD" | "DELETE" | "OPTIONS") {
        out.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(body.as_bytes());
    Ok(bytes)
}

/// The connection of one request: plain TCP or TLS over it.
enum Conn {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Conn {
    fn tcp(&self) -> &TcpStream {
        match self {
            Conn::Plain(s) => s,
            Conn::Tls(t) => &t.sock,
        }
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(t) => t.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(t) => t.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(t) => t.flush(),
        }
    }
}

/// Run `req` (blocking, at most its timeout) without a pin store: `https`
/// needs `tlsPin` or `tlsName`.
pub fn execute(req: &HttpRequest) -> HttpResponse {
    execute_with(req, None)
}

/// Run `req` (blocking, at most its timeout); `pins` holds the
/// trust-on-first-use pins of `https` hosts.
pub fn execute_with(req: &HttpRequest, pins: Option<&PinStore>) -> HttpResponse {
    let started = Instant::now();
    let budget = timeout_of(req);
    let target = match parse_url(&req.url) {
        Ok(t) => t,
        Err(e) => return HttpResponse::failed(e, false),
    };
    let bytes = match encode_request(req, &target) {
        Ok(b) => b,
        Err(e) => return HttpResponse::failed(e, false),
    };
    let addr = match resolve_lan(&target.host, target.port) {
        Ok(a) => a,
        Err(e) => return HttpResponse::failed(e, false),
    };
    let left = |what: &str| -> Result<Duration, Box<HttpResponse>> {
        let l = budget.saturating_sub(started.elapsed());
        if l.is_zero() {
            Err(Box::new(HttpResponse::failed(
                format!("timed out ({what})"),
                true,
            )))
        } else {
            Ok(l)
        }
    };
    let connect_for = match left("connecting") {
        Ok(l) => l,
        Err(r) => return *r,
    };
    let tcp = match TcpStream::connect_timeout(&addr, connect_for) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            return HttpResponse::failed(format!("connecting to {addr}: timed out"), true)
        }
        Err(e) => return HttpResponse::failed(format!("connecting to {addr}: {e}"), false),
    };
    let _ = tcp.set_nodelay(true);
    // TLS: the policy of net/tls.rs; the verdict and the pin come back through `outcome`.
    let mut tls_outcome = None;
    let mut stream = if target.tls {
        let policy = TlsPolicy {
            pin: req.tls_pin.as_deref().and_then(tls::normalise_pin),
            name: req
                .tls_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from),
            stored: pins.and_then(|p| p.get(&target.host, target.port)),
            can_remember: pins.is_some(),
        };
        if req.tls_pin.as_deref().is_some_and(|p| !p.trim().is_empty()) && policy.pin.is_none() {
            return HttpResponse::failed("tlsPin is not a SHA-256 in hex", false);
        }
        let (config, outcome) = match tls::client_config(policy) {
            Ok(c) => c,
            Err(e) => return HttpResponse::failed(e, false),
        };
        let name = match ServerName::try_from(target.host.clone()) {
            Ok(n) => n,
            Err(e) => return HttpResponse::failed(format!("TLS server name: {e}"), false),
        };
        let conn = match ClientConnection::new(config, name) {
            Ok(c) => c,
            Err(e) => return HttpResponse::failed(format!("TLS: {e}"), false),
        };
        tls_outcome = Some(outcome);
        Conn::Tls(Box::new(StreamOwned::new(conn, tcp)))
    } else {
        Conn::Plain(tcp)
    };
    let outcome = || {
        tls_outcome
            .as_ref()
            .and_then(|o| o.lock().unwrap_or_else(|e| e.into_inner()).clone())
    };
    // A refused certificate reads better than the I/O error it causes.
    let fail = |e: std::io::Error, what: &str| -> HttpResponse {
        let seen = outcome();
        let mut r = match &seen {
            Some((_, Verdict::Reject(why))) => HttpResponse::failed(format!("TLS: {why}"), false),
            _ => HttpResponse::failed(format!("{what} {addr}: {e}"), false),
        };
        r.tls_fingerprint = seen.map(|(fp, _)| fp);
        r
    };
    // The TLS handshake runs inside the first write.
    let _ = stream.tcp().set_read_timeout(Some(connect_for));
    if let Err(e) = stream
        .tcp()
        .set_write_timeout(Some(connect_for))
        .and_then(|()| stream.write_all(&bytes))
        .and_then(|()| stream.flush())
    {
        if matches!(
            e.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ) {
            return HttpResponse::failed(format!("sending to {addr}: timed out"), true);
        }
        return fail(e, "sending to");
    }
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    let head_only = req.method.eq_ignore_ascii_case("HEAD");
    loop {
        let l = match left("reading") {
            Ok(l) => l,
            Err(r) => return *r,
        };
        let _ = stream.tcp().set_read_timeout(Some(l));
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > MAX_BODY + MAX_HEAD {
                    return HttpResponse::failed("response too large", false);
                }
                if let Some(done) = complete(&raw, head_only) {
                    raw.truncate(done);
                    break;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return HttpResponse::failed("timed out (reading)", true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                if raw.is_empty() {
                    return fail(e, "reading from");
                }
                // TLS peers that close without close_notify: what came is the response.
                break;
            }
        }
    }
    let mut r = parse_response(&raw, head_only);
    if let Some((fp, verdict)) = outcome() {
        if let (Verdict::Accept { remember: true }, Some(p), true) = (&verdict, pins, r.status != 0) {
            p.set(&target.host, target.port, &fp);
        }
        r.tls_fingerprint = Some(fp);
    }
    r
}

fn head_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// The length of a complete response in `raw`, when it is complete before EOF.
fn complete(raw: &[u8], head_only: bool) -> Option<usize> {
    let end = head_end(raw)?;
    let (status, headers) = parse_head(&raw[..end]).ok()?;
    if head_only || status == 204 || status == 304 || (100..200).contains(&status) {
        return Some(end);
    }
    if header(&headers, "transfer-encoding").is_some_and(|t| t.to_ascii_lowercase().contains("chunked")) {
        return dechunk(&raw[end..]).map(|(_, used)| end + used);
    }
    let len: usize = header(&headers, "content-length")?.trim().parse().ok()?;
    (raw.len() >= end + len).then_some(end + len)
}

fn parse_head(head: &[u8]) -> Result<(u16, Vec<(String, String)>), String> {
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/") {
        return Err(format!("not an HTTP response: {status_line:?}"));
    }
    let status: u16 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("bad status line {status_line:?}"))?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Ok((status, headers))
}

/// Decode a chunked body: (body, bytes used) once the final chunk arrived.
fn dechunk(mut data: &[u8]) -> Option<(Vec<u8>, usize)> {
    let total = data.len();
    let mut body = Vec::new();
    loop {
        let line_end = data.windows(2).position(|w| w == b"\r\n")?;
        let size_text = String::from_utf8_lossy(&data[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        data = &data[line_end + 2..];
        if size == 0 {
            // Trailers end with an empty line.
            let end = data.windows(2).position(|w| w == b"\r\n")?;
            let rest = &data[end + 2..];
            return Some((body, total - rest.len()));
        }
        if data.len() < size + 2 || body.len() + size > MAX_BODY {
            return None;
        }
        body.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}

/// Parse a whole response (`raw` read up to EOF or completion).
pub fn parse_response(raw: &[u8], head_only: bool) -> HttpResponse {
    let Some(end) = head_end(raw) else {
        return HttpResponse::failed(
            if raw.is_empty() {
                "empty response"
            } else {
                "truncated response header"
            },
            false,
        );
    };
    let (status, headers) = match parse_head(&raw[..end]) {
        Ok(h) => h,
        Err(e) => return HttpResponse::failed(e, false),
    };
    let text = String::from_utf8_lossy(&raw[..end]);
    let status_text = text
        .split("\r\n")
        .next()
        .and_then(|l| l.splitn(3, ' ').nth(2))
        .unwrap_or_default()
        .to_string();
    let rest = &raw[end..];
    let body = if head_only {
        Vec::new()
    } else if header(&headers, "transfer-encoding")
        .is_some_and(|t| t.to_ascii_lowercase().contains("chunked"))
    {
        match dechunk(rest) {
            Some((b, _)) => b,
            None => return HttpResponse::failed("truncated chunked body", false),
        }
    } else if let Some(len) = header(&headers, "content-length").and_then(|l| l.trim().parse::<usize>().ok())
    {
        if rest.len() < len {
            return HttpResponse::failed("truncated body", false);
        }
        rest[..len].to_vec()
    } else {
        rest.to_vec()
    };
    HttpResponse {
        status,
        status_text,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
        error: None,
        timed_out: false,
        tls_fingerprint: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Arc;

    #[test]
    fn urls() {
        assert_eq!(
            parse_url("http://192.168.1.4/json/info").unwrap(),
            Target {
                host: "192.168.1.4".into(),
                port: 80,
                path: "/json/info".into(),
                tls: false,
            }
        );
        assert_eq!(
            parse_url("http://10.0.0.2:16021?x=1#frag").unwrap(),
            Target {
                host: "10.0.0.2".into(),
                port: 16021,
                path: "/?x=1".into(),
                tls: false,
            }
        );
        assert_eq!(parse_url("http://[fe80::1]:8080/a").unwrap().host, "fe80::1");
        let secure = parse_url("HTTPS://192.168.1.4/clip/v2").unwrap();
        assert_eq!((secure.port, secure.tls), (443, true));
        assert_eq!(parse_url("https://192.168.1.4:8443/").unwrap().port, 8443);
        assert!(parse_url("ftp://192.168.1.4/").is_err());
        assert!(parse_url("192.168.1.4/").is_err());
        assert!(parse_url("http://user:pw@10.0.0.1/").is_err());
        assert!(parse_url("http://10.0.0.1:99999/").is_err());
    }

    #[test]
    fn request_bytes() {
        let req = HttpRequest {
            method: "put".into(),
            url: "http://10.0.0.2:16021/api/v1/t/effects".into(),
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Host".into(), "evil".into()),
            ],
            body: Some("{\"a\":1}".into()),
            ..HttpRequest::default()
        };
        let t = parse_url(&req.url).unwrap();
        let bytes = String::from_utf8(encode_request(&req, &t).unwrap()).unwrap();
        assert_eq!(
            bytes,
            "PUT /api/v1/t/effects HTTP/1.1\r\nHost: 10.0.0.2:16021\r\nConnection: close\r\n\
             Content-Type: application/json\r\nUser-Agent: Lightloom\r\nContent-Length: 7\r\n\r\n{\"a\":1}"
        );
        let bad = HttpRequest {
            method: "GET".into(),
            headers: vec![("X".into(), "a\r\nInjected: 1".into())],
            ..req
        };
        assert!(encode_request(&bad, &t).is_err());
    }

    #[test]
    fn responses() {
        let r = parse_response(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-A: b\r\n\r\nhiEXTRA",
            false,
        );
        assert_eq!(
            (r.status, r.status_text.as_str(), r.body.as_str()),
            (200, "OK", "hi")
        );
        assert_eq!(header(&r.headers, "x-a"), Some("b"));
        let c = parse_response(
            b"HTTP/1.1 201 Created\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5;x=y\r\npedia\r\n0\r\n\r\n",
            false,
        );
        assert_eq!((c.status, c.body.as_str()), (201, "Wikipedia"));
        let eof = parse_response(b"HTTP/1.0 403 Forbidden\r\n\r\nnope", false);
        assert_eq!((eof.status, eof.body.as_str()), (403, "nope"));
        assert_eq!(parse_response(b"garbage", false).status, 0);
        assert_eq!(complete(b"HTTP/1.1 204 No Content\r\n\r\n", false), Some(27));
    }

    #[test]
    fn talks_to_a_local_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        // The server keeps the connection open until the client has returned,
        // so a client that waited for the close instead of the Content-Length
        // could only end by its timeout (and fail the asserts), however slow
        // the machine is.
        let (returned_tx, returned_rx) = std::sync::mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 1024];
            while head_end(&got).is_none_or(|e| got.len() < e + 5) {
                let n = s.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
            }
            // Keep-alive style answer: complete by Content-Length, connection left open.
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}")
                .unwrap();
            let open_until_returned = returned_rx.recv_timeout(Duration::from_secs(60)).is_ok();
            (String::from_utf8(got).unwrap(), open_until_returned)
        });
        let r = execute(&HttpRequest {
            method: "POST".into(),
            url: format!("http://127.0.0.1:{port}/api"),
            headers: vec![],
            body: Some("hello".into()),
            timeout: MAX_TIMEOUT.as_millis() as u64,
            ..HttpRequest::default()
        });
        returned_tx.send(()).unwrap();
        assert!(
            !r.timed_out && r.error.is_none(),
            "returns once the body is complete: {r:?}"
        );
        assert_eq!((r.status, r.body.as_str()), (200, "{\"ok\":true}"));
        let (got, open_until_returned) = server.join().unwrap();
        assert!(open_until_returned, "the server held the connection open");
        assert!(got.ends_with("\r\n\r\nhello"));
    }

    #[test]
    fn https_host_header_omits_443() {
        let req = HttpRequest {
            method: "GET".into(),
            url: "https://192.168.1.20/clip/v2/resource/light".into(),
            ..HttpRequest::default()
        };
        let t = parse_url(&req.url).unwrap();
        let bytes = String::from_utf8(encode_request(&req, &t).unwrap()).unwrap();
        assert!(bytes.starts_with("GET /clip/v2/resource/light HTTP/1.1\r\nHost: 192.168.1.20\r\n"));
    }

    /// A loopback HTTPS server with the test certificate `name` (testdata/tls):
    /// answers each connection with `body` and counts them.
    fn https_server(name: &str, body: &'static str) -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/tls");
        let cert = std::fs::read(dir.join(format!("{name}.crt.der"))).unwrap();
        let key = std::fs::read(dir.join(format!("{name}.key.der"))).unwrap();
        let config =
            rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(cert)],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
                )
                .unwrap();
        let config = Arc::new(config);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = Arc::clone(&served);
        std::thread::spawn(move || {
            for tcp in listener.incoming().flatten() {
                let _ = tcp.set_read_timeout(Some(Duration::from_secs(5)));
                let Ok(conn) = rustls::ServerConnection::new(Arc::clone(&config)) else {
                    continue;
                };
                let mut s = rustls::StreamOwned::new(conn, tcp);
                let mut got = Vec::new();
                let mut buf = [0u8; 1024];
                while head_end(&got).is_none() {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => got.extend_from_slice(&buf[..n]),
                    }
                }
                if head_end(&got).is_none() {
                    continue; // the client refused the certificate
                }
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.flush();
                s.conn.send_close_notify();
                let _ = s.flush();
            }
        });
        (port, served)
    }

    fn get(url: String, pin: Option<String>, name: Option<&str>, pins: Option<&PinStore>) -> HttpResponse {
        execute_with(
            &HttpRequest {
                method: "GET".into(),
                url,
                timeout: MAX_TIMEOUT.as_millis() as u64,
                tls_pin: pin,
                tls_name: name.map(String::from),
                ..HttpRequest::default()
            },
            pins,
        )
    }

    #[test]
    fn https_with_the_bridge_name_a_pin_or_trust_on_first_use() {
        use crate::service::{MemorySettings, PluginSettings};
        use std::sync::atomic::Ordering;
        let bridge_pin = tls::fingerprint(include_bytes!("../../testdata/tls/bridge.crt.der"));
        let other_pin = tls::fingerprint(include_bytes!("../../testdata/tls/other.crt.der"));
        let (port, served) = https_server("bridge", "{\"ok\":true}");
        let url = format!("https://127.0.0.1:{port}/auth/v1");

        // The Hue bridge id as the expected common name.
        let r = get(url.clone(), None, Some("001788FFFE1A2B3C"), None);
        assert_eq!((r.status, r.body.as_str()), (200, "{\"ok\":true}"), "{r:?}");
        assert_eq!(r.tls_fingerprint.as_deref(), Some(bridge_pin.as_str()));
        // A wrong name, no store: refused before any request is sent.
        let r = get(url.clone(), None, Some("001788fffe000000"), None);
        assert_eq!(r.status, 0);
        assert!(
            r.error.as_deref().unwrap().contains("not \"001788fffe000000\""),
            "{r:?}"
        );
        assert_eq!(r.tls_fingerprint.as_deref(), Some(bridge_pin.as_str()));
        // An explicit pin: right, then wrong.
        assert_eq!(
            get(url.clone(), Some(bridge_pin.to_uppercase()), None, None).status,
            200
        );
        let r = get(
            url.clone(),
            Some(other_pin.clone()),
            Some("001788fffe1a2b3c"),
            None,
        );
        assert_eq!(r.status, 0, "an explicit pin decides alone");
        // Neither, and no store: refused.
        let r = get(url.clone(), None, None, None);
        assert_eq!(r.status, 0);
        assert!(r.error.unwrap().contains("no pin"));
        assert_eq!(served.load(Ordering::SeqCst), 2);

        // Trust on first use with a store: remembered in the plugin's settings.
        let settings = Arc::new(MemorySettings::default());
        let pins = PinStore::new("Test", settings.clone());
        assert_eq!(get(url.clone(), None, None, Some(&pins)).status, 200);
        let key = PinStore::key("127.0.0.1", port);
        assert_eq!(
            settings.get("Test", tls::PIN_SETTING_ID, &key),
            Some(bridge_pin.clone())
        );
        assert_eq!(get(url.clone(), None, None, Some(&pins)).status, 200);
        // Another certificate on the same host and port is refused.
        settings.set("Test", tls::PIN_SETTING_ID, &key, Some(&other_pin));
        let r = get(url.clone(), None, None, Some(&pins));
        assert_eq!(r.status, 0);
        assert!(r.error.unwrap().contains("pinned"));
        // ... unless it carries the expected name, which re-pins it.
        assert_eq!(
            get(url.clone(), None, Some("001788fffe1a2b3c"), Some(&pins)).status,
            200
        );
        assert_eq!(settings.get("Test", tls::PIN_SETTING_ID, &key), Some(bridge_pin));
        assert_eq!(served.load(Ordering::SeqCst), 5);
        // A malformed pin is an error, not "no pin".
        assert_eq!(get(url, Some("nope".into()), None, Some(&pins)).status, 0);
    }

    #[test]
    fn https_refuses_a_certificate_without_the_name() {
        let (port, served) = https_server("other", "x");
        let r = get(
            format!("https://127.0.0.1:{port}/"),
            None,
            Some("001788fffe1a2b3c"),
            None,
        );
        assert_eq!(r.status, 0);
        assert!(r.error.unwrap().contains("not-a-bridge"));
        assert_eq!(served.load(std::sync::atomic::Ordering::SeqCst), 0);
        let r = get("https://8.8.8.8/".into(), None, Some("x"), None);
        assert!(
            r.error.unwrap().contains("local network"),
            "LAN only for https too"
        );
    }

    #[test]
    fn refuses_the_internet_and_times_out() {
        let r = execute(&HttpRequest {
            method: "GET".into(),
            url: "http://8.8.8.8/".into(),
            ..HttpRequest::default()
        });
        assert_eq!(r.status, 0);
        assert!(r.error.unwrap().contains("local network"));
        // A server that never answers: the timeout ends the request.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let t = Instant::now();
        let r = execute(&HttpRequest {
            method: "GET".into(),
            url: format!("http://127.0.0.1:{port}/"),
            timeout: 200,
            ..HttpRequest::default()
        });
        assert!(r.timed_out, "{r:?}");
        assert!(t.elapsed() < Duration::from_secs(2));
        drop(listener);
    }
}
