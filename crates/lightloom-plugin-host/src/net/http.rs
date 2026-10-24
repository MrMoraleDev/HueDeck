// SPDX-License-Identifier: GPL-3.0-or-later
//! A small HTTP/1.1 client behind the plugins' `XMLHttpRequest`.
//!
//! Plain `http://` to local-network hosts only ([`super::policy`]), one
//! request per connection (`Connection: close`), no proxy, no redirects, no
//! cookies. The whole exchange is bounded by the request's timeout; bodies are
//! capped at [`MAX_BODY`]. Failures are a response with status 0 and an
//! `error`, which the JS side turns into `onerror` / `ontimeout`.

use super::policy::resolve_lan;
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
    /// Port (80 by default).
    pub port: u16,
    /// Path and query, at least `/`.
    pub path: String,
}

/// Split an `http://` URL. `https` and other schemes are refused.
pub fn parse_url(url: &str) -> Result<Target, String> {
    let url = url.trim();
    let rest = match url.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => rest,
        Some((scheme, _)) => return Err(format!("{scheme}:// is not supported (plain http:// only)")),
        None => return Err(format!("not an absolute http:// URL: {url}")),
    };
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
        (h.to_string(), port.map_err(|_| "bad port")?.unwrap_or(80))
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|_| "bad port")?),
            None => (authority.to_string(), 80),
        }
    };
    if host.is_empty() {
        return Err("no host in URL".into());
    }
    Ok(Target { host, port, path })
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
    let host = if target.port == 80 {
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

/// Run `req` (blocking, at most its timeout).
pub fn execute(req: &HttpRequest) -> HttpResponse {
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
    let left = |what: &str| -> Result<Duration, HttpResponse> {
        let l = budget.saturating_sub(started.elapsed());
        if l.is_zero() {
            Err(HttpResponse::failed(format!("timed out ({what})"), true))
        } else {
            Ok(l)
        }
    };
    let connect_for = match left("connecting") {
        Ok(l) => l,
        Err(r) => return r,
    };
    let mut stream = match TcpStream::connect_timeout(&addr, connect_for) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            return HttpResponse::failed(format!("connecting to {addr}: timed out"), true)
        }
        Err(e) => return HttpResponse::failed(format!("connecting to {addr}: {e}"), false),
    };
    let _ = stream.set_nodelay(true);
    if let Err(e) = stream
        .set_write_timeout(Some(connect_for))
        .and_then(|()| stream.write_all(&bytes))
    {
        return HttpResponse::failed(format!("sending to {addr}: {e}"), false);
    }
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    let head_only = req.method.eq_ignore_ascii_case("HEAD");
    loop {
        let l = match left("reading") {
            Ok(l) => l,
            Err(r) => return r,
        };
        let _ = stream.set_read_timeout(Some(l));
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
                    return HttpResponse::failed(format!("reading from {addr}: {e}"), false);
                }
                break;
            }
        }
    }
    parse_response(&raw, head_only)
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn urls() {
        assert_eq!(
            parse_url("http://192.168.1.4/json/info").unwrap(),
            Target {
                host: "192.168.1.4".into(),
                port: 80,
                path: "/json/info".into()
            }
        );
        assert_eq!(
            parse_url("http://10.0.0.2:16021?x=1#frag").unwrap(),
            Target {
                host: "10.0.0.2".into(),
                port: 16021,
                path: "/?x=1".into()
            }
        );
        assert_eq!(parse_url("http://[fe80::1]:8080/a").unwrap().host, "fe80::1");
        assert!(parse_url("https://192.168.1.4/").is_err());
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
            timeout: 0,
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
            std::thread::sleep(Duration::from_millis(300));
            String::from_utf8(got).unwrap()
        });
        let t = Instant::now();
        let r = execute(&HttpRequest {
            method: "POST".into(),
            url: format!("http://127.0.0.1:{port}/api"),
            headers: vec![],
            body: Some("hello".into()),
            timeout: 2000,
        });
        assert_eq!((r.status, r.body.as_str()), (200, "{\"ok\":true}"));
        assert!(
            t.elapsed() < Duration::from_millis(250),
            "returns once the body is complete"
        );
        assert!(server.join().unwrap().ends_with("\r\n\r\nhello"));
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
