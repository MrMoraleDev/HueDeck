// SPDX-License-Identifier: GPL-3.0-or-later
//! LibreHardwareMonitor's `data.json` (Options → Remote Web Server, port 8085
//! by default): every sensor it shows, CPU temperatures included, without
//! Lightloom reading a single CPU register.
//!
//! Optional and off unless `[sensors] lhm_url` names it. Only loopback URLs
//! (`http://127.0.0.1:…`, `http://localhost:…`, `http://[::1]:…`) are accepted:
//! the daemon never polls another machine. One short HTTP/1.0 GET per poll
//! with a 500 ms timeout and a 4 MiB cap.
//!
//! The JSON is a tree of `{Text, Value, Min, Max, Children, SensorId?, Type?}`
//! nodes: computer → hardware → (sub-hardware …) → group (`Temperatures`,
//! `Load`, …) → sensor. A sensor is named `"<hardware> <sensor>"` (`"AMD Ryzen
//! 7 5800X Core (Tctl/Tdie)"`, `"NVIDIA GeForce RTX 3060 GPU Core"`). Values
//! are strings with units (`"45.3 °C"`, `"1200 RPM"`, a decimal comma on some
//! locales).

use lightloom_hal::sensors::{SensorBackend, SensorKind, SensorReading};
use lightloom_hal::{HalError, HalResult};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// How long one request may take.
pub const TIMEOUT: Duration = Duration::from_millis(500);
/// Largest reply read.
pub const MAX_REPLY: u64 = 4 << 20;

/// The LibreHardwareMonitor backend.
#[derive(Clone, Debug)]
pub struct LhmJson {
    addr: SocketAddr,
    host: String,
    path: String,
}

/// `http://host:port/path` → `(host, port, path)`; loopback hosts only.
pub fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .trim()
        .strip_prefix("http://")
        .ok_or("only http:// URLs are supported")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/data.json".to_string()),
    };
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, p) = v6.split_once(']').ok_or("bad IPv6 address")?;
        (h.to_string(), p.strip_prefix(':').unwrap_or(""))
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p),
            None => (authority.to_string(), ""),
        }
    };
    let port = if port.is_empty() {
        80
    } else {
        port.parse().map_err(|_| "bad port")?
    };
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if !loopback {
        return Err("only loopback addresses (127.0.0.1, localhost, ::1) are allowed".into());
    }
    Ok((host, port, path))
}

impl LhmJson {
    /// For `url` (see [`parse_url`]).
    pub fn new(url: &str) -> Result<Self, String> {
        let (host, port, path) = parse_url(url)?;
        let addr = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .find(|a| a.ip().is_loopback())
            .ok_or("the host does not resolve to a loopback address")?;
        Ok(Self { addr, host, path })
    }

    fn fetch(&self) -> HalResult<Vec<u8>> {
        let io = |e: std::io::Error| HalError::Backend(format!("LibreHardwareMonitor: {e}"));
        let mut s = TcpStream::connect_timeout(&self.addr, TIMEOUT).map_err(io)?;
        s.set_read_timeout(Some(TIMEOUT)).map_err(io)?;
        s.set_write_timeout(Some(TIMEOUT)).map_err(io)?;
        write!(
            s,
            "GET {} HTTP/1.0\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
            self.path, self.host
        )
        .map_err(io)?;
        let mut reply = Vec::new();
        s.take(MAX_REPLY).read_to_end(&mut reply).map_err(io)?;
        http_body(&reply).map_err(|e| HalError::Backend(format!("LibreHardwareMonitor: {e}")))
    }
}

/// The body of a `200` reply (chunked transfer decoded).
pub fn http_body(reply: &[u8]) -> Result<Vec<u8>, String> {
    let split = reply
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("no HTTP header")?;
    let head = String::from_utf8_lossy(&reply[..split]);
    let status = head.lines().next().unwrap_or_default();
    if status.split_whitespace().nth(1) != Some("200") {
        return Err(format!("HTTP {status:?}"));
    }
    let body = &reply[split + 4..];
    let chunked = head.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    if !chunked {
        return Ok(body.to_vec());
    }
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let eol = rest.windows(2).position(|w| w == b"\r\n").ok_or("bad chunk")?;
        let size_text = String::from_utf8_lossy(&rest[..eol]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| "bad chunk size")?;
        rest = &rest[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if rest.len() < size {
            return Err("truncated chunk".into());
        }
        out.extend_from_slice(&rest[..size]);
        rest = rest.get(size + 2..).unwrap_or_default();
    }
}

/// `"45.3 °C"` / `"1,2 V"` → `(45.3, "°C")`.
pub fn parse_value(text: &str) -> Option<(f64, String)> {
    let t = text.trim();
    let end = t
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || c == ',' || (i == 0 && c == '-')))
        .map_or(t.len(), |(i, _)| i);
    let number = t[..end].replace(',', ".");
    Some((number.parse().ok()?, t[end..].trim().to_string()))
}

fn kind_of(type_name: Option<&str>, group: &str) -> SensorKind {
    let key = type_name.unwrap_or(group).to_ascii_lowercase();
    match key.trim_end_matches('s') {
        "temperature" => SensorKind::Temperature,
        "fan" => SensorKind::Fan,
        "voltage" => SensorKind::Voltage,
        "load" | "control" | "level" => SensorKind::Load,
        "clock" => SensorKind::Clock,
        "power" => SensorKind::Power,
        _ => SensorKind::Other,
    }
}

/// The expected range of a kind, widened to hold `value`.
fn range(kind: SensorKind, value: f64) -> (f64, f64) {
    let max: f64 = match kind {
        SensorKind::Temperature | SensorKind::Load => 100.0,
        SensorKind::Fan => 5000.0,
        SensorKind::Voltage => 2.0,
        SensorKind::Clock => 6000.0,
        SensorKind::Power => 300.0,
        SensorKind::Other => 0.0,
    };
    (0.0_f64.min(value), max.max(value))
}

/// Every sensor of a `data.json` tree.
pub fn parse_tree(json: &Value) -> Vec<SensorReading> {
    let mut out = Vec::new();
    let mut path = Vec::new();
    walk(json, &mut path, &mut out);
    out
}

fn walk<'a>(node: &'a Value, path: &mut Vec<&'a str>, out: &mut Vec<SensorReading>) {
    let text = node.get("Text").and_then(Value::as_str).unwrap_or("");
    let children = node.get("Children").and_then(Value::as_array);
    let leaf = children.is_none_or(|c| c.is_empty());
    // Root, computer, hardware, group, sensor: sensors sit at depth 4 or deeper.
    if leaf && path.len() >= 3 {
        if let Some((value, units)) = node.get("Value").and_then(Value::as_str).and_then(parse_value) {
            let group = path[path.len() - 1];
            let hardware = path[path.len() - 2];
            let kind = kind_of(node.get("Type").and_then(Value::as_str), group);
            let (min, max) = range(kind, value);
            out.push(SensorReading {
                name: format!("{hardware} {text}").trim().to_string(),
                value,
                units,
                min,
                max,
                kind,
            });
        }
        return;
    }
    path.push(text);
    for c in children.into_iter().flatten() {
        walk(c, path, out);
    }
    path.pop();
}

impl SensorBackend for LhmJson {
    fn name(&self) -> &str {
        "lhm"
    }

    fn poll(&mut self) -> HalResult<Vec<SensorReading>> {
        let body = self.fetch()?;
        let json: Value = serde_json::from_slice(&body)
            .map_err(|e| HalError::Backend(format!("LibreHardwareMonitor: bad JSON ({e})")))?;
        Ok(parse_tree(&json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// The shape LibreHardwareMonitor 0.9 serves (trimmed).
    pub(crate) const SAMPLE: &str = r#"{"id":0,"Text":"Sensor","Min":"Min","Value":"Value","Max":"Max","ImageURL":"","Children":[
      {"id":1,"Text":"DESKTOP","Min":"","Value":"","Max":"","ImageURL":"images_icon/computer.png","Children":[
        {"id":2,"Text":"AMD Ryzen 7 5800X","Min":"","Value":"","Max":"","ImageURL":"images_icon/cpu.png","Children":[
          {"id":3,"Text":"Temperatures","Min":"","Value":"","Max":"","ImageURL":"images_icon/temperature.png","Children":[
            {"id":4,"Text":"Core (Tctl/Tdie)","Min":"35,0 °C","Value":"45,3 °C","Max":"80,1 °C","SensorId":"/amdcpu/0/temperature/2","Type":"Temperature","ImageURL":"images/transparent.png","Children":[]}]},
          {"id":5,"Text":"Load","Min":"","Value":"","Max":"","ImageURL":"images_icon/load.png","Children":[
            {"id":6,"Text":"CPU Total","Min":"1.0 %","Value":"12.5 %","Max":"100.0 %","ImageURL":"images/transparent.png","Children":[]}]}]},
        {"id":7,"Text":"ASUS ROG STRIX","Min":"","Value":"","Max":"","ImageURL":"images_icon/mainboard.png","Children":[
          {"id":8,"Text":"Nuvoton NCT6798D","Min":"","Value":"","Max":"","ImageURL":"images_icon/chip.png","Children":[
            {"id":9,"Text":"Fans","Min":"","Value":"","Max":"","ImageURL":"images_icon/fan.png","Children":[
              {"id":10,"Text":"Fan #1","Min":"0 RPM","Value":"1210 RPM","Max":"1300 RPM","ImageURL":"images/transparent.png","Children":[]}]}]}]}]}]}"#;

    #[test]
    fn urls_are_loopback_only() {
        assert_eq!(
            parse_url("http://127.0.0.1:8085/data.json").unwrap(),
            ("127.0.0.1".into(), 8085, "/data.json".into())
        );
        assert_eq!(parse_url("http://localhost:8085").unwrap().2, "/data.json");
        assert_eq!(parse_url("http://[::1]:9000/x").unwrap().1, 9000);
        assert!(parse_url("http://192.168.1.2:8085/data.json").is_err());
        assert!(parse_url("https://127.0.0.1/data.json").is_err());
        assert!(parse_url("http://example.com/data.json").is_err());
    }

    #[test]
    fn values_parse() {
        assert_eq!(parse_value("45,3 °C"), Some((45.3, "°C".into())));
        assert_eq!(parse_value("1210 RPM"), Some((1210.0, "RPM".into())));
        assert_eq!(parse_value("-5.5 dB"), Some((-5.5, "dB".into())));
        assert_eq!(parse_value(""), None);
        assert_eq!(parse_value("Value"), None);
    }

    #[test]
    fn the_tree_becomes_named_readings() {
        let r = parse_tree(&serde_json::from_str(SAMPLE).unwrap());
        let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "AMD Ryzen 7 5800X Core (Tctl/Tdie)",
                "AMD Ryzen 7 5800X CPU Total",
                "Nuvoton NCT6798D Fan #1"
            ]
        );
        assert_eq!(
            (r[0].value, r[0].kind, r[0].units.as_str()),
            (45.3, SensorKind::Temperature, "°C")
        );
        assert_eq!((r[1].kind, r[1].max), (SensorKind::Load, 100.0));
        assert_eq!((r[2].kind, r[2].max), (SensorKind::Fan, 5000.0));
        let alias = crate::sensors::cpu_temperature(&r).unwrap();
        assert_eq!(alias.value, 45.3);
    }

    #[test]
    fn chunked_and_errors() {
        assert_eq!(
            http_body(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n"
            )
            .unwrap(),
            b"abcde"
        );
        assert!(http_body(b"HTTP/1.1 404 Not Found\r\n\r\n").is_err());
        assert!(http_body(b"garbage").is_err());
    }

    #[test]
    fn polls_a_local_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut req = [0u8; 512];
            let n = s.read(&mut req).unwrap();
            assert!(String::from_utf8_lossy(&req[..n]).starts_with("GET /data.json HTTP/1.0"));
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{SAMPLE}"
            )
            .unwrap();
        });
        let mut b = LhmJson::new(&format!("http://127.0.0.1:{port}/data.json")).unwrap();
        assert_eq!(b.poll().unwrap().len(), 3);
        server.join().unwrap();
        // Nothing listens any more: an error, quickly.
        assert!(b.poll().is_err());
    }
}
