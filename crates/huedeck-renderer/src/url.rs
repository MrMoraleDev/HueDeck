// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `file://` URLs for effect paths.

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
    fn unix_form() {
        assert_eq!(
            file_url(Path::new("/tmp/é x.html")),
            "file:///tmp/%C3%A9%20x.html"
        );
    }
}
