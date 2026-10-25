// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom action <url>`: hand a `lightloom://` action URL to the running
//! daemon (`POST /v1/actions` with the API token from the data directory).
//! This is also what the opt-in URL-scheme registration runs when a
//! `lightloom://` link is opened.
//!
//! The URL is checked locally first ([`lightloom_api::actions::parse`]), so a
//! malformed one fails without the daemon. The request is plain HTTP/1.1 on
//! the loopback interface (no HTTP client dependency).

use anyhow::{bail, Context};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

/// How long the daemon may take to answer (an `app/restart` reloads everything).
const TIMEOUT: Duration = Duration::from_secs(60);

/// The API token file in the data directory.
pub const TOKEN_FILE: &str = "api-token";

/// The daemon's answer.
#[derive(Debug, PartialEq)]
pub struct Reply {
    /// HTTP status.
    pub status: u16,
    /// JSON body.
    pub body: serde_json::Value,
}

/// The API token in `data`.
pub fn read_token(data: &Path) -> anyhow::Result<String> {
    let path = data.join(TOKEN_FILE);
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "cannot read the API token {} (is lightloomd set up for this user?)",
            path.display()
        )
    })?;
    Ok(text.trim().to_string())
}

/// The raw HTTP request.
pub fn request_bytes(addr: &str, token: &str, url: &str) -> Vec<u8> {
    let body = serde_json::json!({ "url": url }).to_string();
    format!(
        "POST /v1/actions HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// Parse an HTTP/1.1 response (`Content-Length`, chunked or read-to-close body).
pub fn parse_response(raw: &[u8]) -> anyhow::Result<Reply> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("the daemon's answer has no header end")?;
    let head = std::str::from_utf8(&raw[..split]).context("the daemon's answer headers are not text")?;
    let mut lines = head.split("\r\n");
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .context("the daemon's answer has no status")?;
    let mut chunked = false;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            if k.trim().eq_ignore_ascii_case("transfer-encoding") && v.trim().eq_ignore_ascii_case("chunked")
            {
                chunked = true;
            }
        }
    }
    let mut body = raw[split + 4..].to_vec();
    if chunked {
        let mut out = Vec::new();
        let mut rest = &body[..];
        loop {
            let end = rest
                .windows(2)
                .position(|w| w == b"\r\n")
                .context("bad chunked body")?;
            let size_text = std::str::from_utf8(&rest[..end])?;
            let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
                .context("bad chunk size")?;
            rest = &rest[end + 2..];
            if size == 0 {
                break;
            }
            if rest.len() < size {
                bail!("truncated chunked body");
            }
            out.extend_from_slice(&rest[..size]);
            rest = rest.get(size + 2..).unwrap_or(&[]);
        }
        body = out;
    }
    let body = if body.iter().all(u8::is_ascii_whitespace) {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).context("the daemon's answer is not JSON")?
    };
    Ok(Reply { status, body })
}

/// Send `url` to the daemon at `addr`.
pub fn send(addr: &str, token: &str, url: &str) -> anyhow::Result<Reply> {
    let mut stream = TcpStream::connect(addr)
        .with_context(|| format!("Lightloom is not running (nothing answers on {addr})"))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.write_all(&request_bytes(addr, token, url))?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    parse_response(&raw)
}

/// `lightloom action <url>`: `true` when the daemon carried it out (or dropped
/// an unknown action with a warning, which is printed).
pub fn run(addr: &str, data: &Path, url: &str, json: bool) -> anyhow::Result<bool> {
    let action =
        lightloom_api::actions::parse(url).map_err(|e| anyhow::anyhow!("invalid action URL: {e}"))?;
    let token = read_token(data)?;
    let reply = send(addr, token.trim(), url)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&reply.body)?);
    }
    if reply.status >= 400 {
        let message = reply.body["error"]["message"]
            .as_str()
            .unwrap_or("the daemon refused the action");
        eprintln!(
            "lightloom: {} failed ({}): {message}",
            action.name(),
            reply.status
        );
        return Ok(false);
    }
    if reply.body["dropped"] == true {
        eprintln!(
            "lightloom: action dropped: {}",
            reply.body["warning"].as_str().unwrap_or("unknown action")
        );
    } else if !json {
        println!("{}: done", reply.body["action"].as_str().unwrap_or(action.name()));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn parses_length_and_chunked_answers() {
        let r = parse_response(b"HTTP/1.1 200 OK\r\ncontent-length: 16\r\n\r\n{\"action\":\"x\"}  ").unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body["action"], "x");
        let r = parse_response(
            b"HTTP/1.1 400 Bad Request\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"a\":\r\n2\r\n1}\r\n0\r\n\r\n",
        )
        .unwrap();
        assert_eq!(r.status, 400);
        assert_eq!(r.body["a"], 1);
        assert!(parse_response(b"garbage").is_err());
    }

    #[test]
    fn sends_the_url_with_the_token_to_a_fake_daemon() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = vec![0u8; 4096];
            let mut got = Vec::new();
            // Read until the body arrived.
            while !String::from_utf8_lossy(&got).contains("raiseWindow\"}") {
                let n = s.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
            }
            let body = r#"{"action":"app/raiseWindow","result":{"delivered":1}}"#;
            write!(
                s,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            String::from_utf8(got).unwrap()
        });
        let reply = send(&addr, "secret", "lightloom://app/raiseWindow").unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body["result"]["delivered"], 1);
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/actions HTTP/1.1\r\n"), "{request}");
        assert!(request.contains("Authorization: Bearer secret\r\n"));
        assert!(request.contains(&format!("Host: {addr}\r\n")));
        assert!(request.ends_with(r#"{"url":"lightloom://app/raiseWindow"}"#));
    }

    #[test]
    fn a_malformed_url_fails_before_the_daemon() {
        let dir = std::env::temp_dir().join(format!("lightloom-cli-action-{}", std::process::id()));
        let e = run("127.0.0.1:1", &dir, "lightloom://brightness/500", false).unwrap_err();
        assert!(e.to_string().contains("invalid action URL"), "{e}");
    }
}
