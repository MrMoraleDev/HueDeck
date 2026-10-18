// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `file://` URLs for effect paths, and the filter deciding what an effect page may load.

use std::path::Path;

fn encode_into(out: &mut String, segment: &str) {
    for b in segment.bytes() {
        let keep = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/' | b':');
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
}

/// `file://` URL for an absolute path (Windows drive / UNC / verbatim forms, or Unix).
/// The path is not canonicalised here; callers pass `std::fs::canonicalize` output.
pub fn file_url(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if let Some(unc) = raw.strip_prefix("//?/UNC/") {
        // \\?\UNC\server\share\x → file://server/share/x
        encode_into(&mut out, unc);
    } else if let Some(local) = raw.strip_prefix("//?/") {
        out.push('/');
        encode_into(&mut out, local);
    } else if let Some(unc) = raw.strip_prefix("//") {
        encode_into(&mut out, unc);
    } else {
        if !raw.starts_with('/') {
            out.push('/');
        }
        encode_into(&mut out, &raw);
    }
    out
}

/// What an effect page may load or navigate to. Effects are untrusted HTML, so the browser
/// process cancels everything except files in the effect's own directory (or below it),
/// `data:`, `blob:`, `about:blank` and `about:srcdoc`; `http(s)` and `ws(s)` pass only with
/// `--allow-network`. Without the filter a page could pull other local files into its frames
/// or reach the network by IP address (the DNS block only stops host names).
#[derive(Clone, Debug)]
pub struct UrlFilter {
    /// Components of the effect's directory, lowercase on Windows; empty allows no file.
    dir: Vec<String>,
    allow_network: bool,
}

impl UrlFilter {
    /// The filter for the effect file `effect` (an absolute, canonical path).
    pub fn for_effect(effect: &Path, allow_network: bool) -> Self {
        let dir = effect
            .parent()
            .and_then(|d| path_components(&d.to_string_lossy()))
            .unwrap_or_default();
        Self { dir, allow_network }
    }

    /// Whether `url` (as Chromium canonicalised it) may be loaded.
    pub fn allows(&self, url: &str) -> bool {
        let Some((scheme, rest)) = url.split_once(':') else {
            return false;
        };
        match scheme.to_ascii_lowercase().as_str() {
            "data" | "blob" => true,
            "about" => matches!(rest.split(['?', '#']).next(), Some("blank" | "srcdoc")),
            "http" | "https" | "ws" | "wss" => self.allow_network,
            "file" => self.allows_file(rest),
            _ => false,
        }
    }

    fn allows_file(&self, rest: &str) -> bool {
        if self.dir.is_empty() {
            return false;
        }
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let Some(rest) = rest.strip_prefix("//") else {
            return false;
        };
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let Some(path) = percent_decode(path) else {
            return false;
        };
        let full = if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
            path
        } else {
            match percent_decode(host) {
                Some(host) => format!("//{host}/{path}"),
                None => return false,
            }
        };
        match path_components(&full) {
            Some(parts) => parts.len() > self.dir.len() && parts.starts_with(&self.dir),
            None => false,
        }
    }
}

/// `%XX` decoded; `None` for a malformed escape, invalid UTF-8 or a NUL.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let decoded = String::from_utf8(out).ok()?;
    (!decoded.contains('\0')).then_some(decoded)
}

/// Path components: `\` and `/` both separate, Windows verbatim prefixes are dropped, a UNC
/// path starts with a `//` marker, `.` is skipped and `..` refuses the whole path (`None`).
/// Lowercase on Windows, where paths compare case-insensitively.
fn path_components(raw: &str) -> Option<Vec<String>> {
    let s = raw.replace('\\', "/");
    let s = if let Some(unc) = s.strip_prefix("//?/UNC/") {
        format!("//{unc}")
    } else if let Some(local) = s.strip_prefix("//?/") {
        local.to_string()
    } else {
        s
    };
    let mut out = Vec::new();
    if s.starts_with("//") {
        out.push("//".to_string());
    }
    for part in s.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            p if cfg!(windows) => out.push(p.to_lowercase()),
            p => out.push(p.to_string()),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_forms() {
        assert_eq!(
            file_url(Path::new(
                r"\\?\C:\Users\x\Signal-x64\Effects\Static\Solid Color.html"
            )),
            "file:///C:/Users/x/Signal-x64/Effects/Static/Solid%20Color.html"
        );
        assert_eq!(file_url(Path::new(r"C:\a\b#c.html")), "file:///C:/a/b%23c.html");
        assert_eq!(
            file_url(Path::new(r"\\?\UNC\srv\share\e.html")),
            "file://srv/share/e.html"
        );
    }

    #[test]
    fn filter_allows_the_effect_dir_and_inline_schemes_only() {
        let effect = if cfg!(windows) {
            Path::new(r"\\?\C:\Users\x\Effects\My Fx\fx.html")
        } else {
            Path::new("/home/x/Effects/My Fx/fx.html")
        };
        let base = if cfg!(windows) {
            "file:///C:/Users/x/Effects"
        } else {
            "file:///home/x/Effects"
        };
        let f = UrlFilter::for_effect(effect, false);
        for ok in [
            format!("{base}/My%20Fx/fx.html"),
            format!("{base}/My%20Fx/img/a.png?x=1#y"),
            "data:image/png;base64,AAAA".to_string(),
            "blob:null/0b1c-22".to_string(),
            "about:blank".to_string(),
            "about:srcdoc".to_string(),
        ] {
            assert!(f.allows(&ok), "{ok}");
        }
        for denied in [
            format!("{base}/My%20Fx"),
            format!("{base}/My%20Fx2/fx.html"),
            format!("{base}/other.html"),
            format!("{base}/My%20Fx/%2e%2e/other.html"),
            format!("{base}/My%20Fx/a%2F..%2F..%2Fb.html"),
            format!("{base}/My%20Fx/a%00.png"),
            format!("{base}/My%20Fx/a%zz.png"),
            "file://server/share/My%20Fx/fx.html".to_string(),
            "http://142.250.180.14/x.js".to_string(),
            "wss://192.168.1.10:8080/".to_string(),
            "javascript:alert(1)".to_string(),
            "chrome://gpu".to_string(),
            "about:version".to_string(),
            "no-scheme".to_string(),
        ] {
            assert!(!f.allows(&denied), "{denied}");
        }
        assert!(UrlFilter::for_effect(effect, true).allows("https://example.org/a.js"));
        if cfg!(windows) {
            assert!(f.allows("file:///c:/users/X/EFFECTS/my%20fx/A.PNG"));
            assert!(f.allows("file://localhost/C:/Users/x/Effects/My%20Fx/b.png"));
            assert!(!f.allows("file:///D:/Users/x/Effects/My%20Fx/fx.html"));
        }
    }

    // Only Windows paths have UNC forms (and `\` separators for `Path::parent`).
    #[cfg(windows)]
    #[test]
    fn unc_effect_dirs_match_unc_urls_only() {
        let f = UrlFilter::for_effect(Path::new(r"\\?\UNC\srv\share\fx\e.html"), false);
        assert!(f.allows("file://srv/share/fx/e.html"));
        assert!(!f.allows("file:///srv/share/fx/e.html"));
        assert!(!f.allows("file://other/share/fx/e.html"));
    }

    #[test]
    fn unix_form() {
        assert_eq!(
            file_url(Path::new("/tmp/é x.html")),
            "file:///tmp/%C3%A9%20x.html"
        );
    }
}
