// SPDX-License-Identifier: GPL-3.0-or-later
//! `https://` for the plugins' `XMLHttpRequest`: rustls (ring provider) over
//! the same LAN-only TCP connection as plain HTTP (docs/security.md, "HTTPS
//! from plugins").
//!
//! Devices on a LAN have no certificate a public CA signed (a Hue bridge's
//! is self-signed or signed by Signify's private CA, its common name the
//! bridge id), so the web PKI does not apply. A certificate is accepted when:
//!
//! 1. the request names a pin (`xhr.tlsPin`, SHA-256 of the certificate) and
//!    the certificate matches it, nothing else; or
//! 2. the request names the expected common name (`xhr.tlsName`, e.g. the
//!    Hue bridge id) and the certificate's subject CN is that name; or
//! 3. the certificate matches the pin stored for this host and port in the
//!    plugin's settings (`lightloom-tls` / `<host>:<port>`); or
//! 4. trust on first use: no pin is stored and the request named no
//!    `tlsName`, and a store exists: the certificate is accepted and its pin
//!    stored, so a different certificate later is refused.
//!
//! Everything else fails like an unreachable host (status 0) with the reason
//! in the plugin log. The handshake signature is always checked against the
//! certificate's key (so a pin cannot be replayed without the private key);
//! validity dates and chains are not.

use crate::service::PluginSettings;
use ring::digest;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use std::sync::{Arc, Mutex};

/// Settings id under which certificate pins are stored (key `<host>:<port>`).
pub const PIN_SETTING_ID: &str = "lightloom-tls";

/// Where trust-on-first-use pins live: one plugin's settings.
#[derive(Clone)]
pub struct PinStore {
    plugin: String,
    settings: Arc<dyn PluginSettings>,
}

impl std::fmt::Debug for PinStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinStore")
            .field("plugin", &self.plugin)
            .finish_non_exhaustive()
    }
}

impl PinStore {
    /// The pins of `plugin` (its `Name()`) in `settings`.
    pub fn new(plugin: impl Into<String>, settings: Arc<dyn PluginSettings>) -> Self {
        Self {
            plugin: plugin.into(),
            settings,
        }
    }

    /// The settings key of `host:port`.
    pub fn key(host: &str, port: u16) -> String {
        format!("{}:{port}", host.trim_matches(['[', ']']).to_ascii_lowercase())
    }

    /// The pin stored for `host:port`.
    pub fn get(&self, host: &str, port: u16) -> Option<String> {
        self.settings
            .get(&self.plugin, PIN_SETTING_ID, &Self::key(host, port))
            .and_then(|p| normalise_pin(&p))
    }

    /// Store the pin of `host:port`.
    pub fn set(&self, host: &str, port: u16, pin: &str) {
        self.settings
            .set(&self.plugin, PIN_SETTING_ID, &Self::key(host, port), Some(pin));
    }
}

/// SHA-256 of a DER certificate, lowercase hex: the pin format.
pub fn fingerprint(der: &[u8]) -> String {
    digest::digest(&digest::SHA256, der)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A pin as written by a plugin (hex, any case, `:` separators allowed) in
/// canonical form; `None` when it is not 32 bytes of hex.
pub fn normalise_pin(pin: &str) -> Option<String> {
    let hex: String = pin
        .trim()
        .chars()
        .filter(|c| *c != ':')
        .map(|c| c.to_ascii_lowercase())
        .collect();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

// ------------------------------------------------------------------ DER

/// One DER element: (tag, contents, rest).
fn tlv(data: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *data.first()?;
    let first = *data.get(1)?;
    let (len, head) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 3 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | usize::from(*data.get(2 + i)?);
        }
        (len, 2 + n)
    };
    let body = data.get(head..head + len)?;
    Some((tag, body, &data[head + len..]))
}

/// The subject common name (OID 2.5.4.3) of a DER certificate.
pub fn common_name(der: &[u8]) -> Option<String> {
    const SEQUENCE: u8 = 0x30;
    const SET: u8 = 0x31;
    const CN: [u8; 3] = [0x55, 0x04, 0x03];
    let (tag, cert, _) = tlv(der)?;
    if tag != SEQUENCE {
        return None;
    }
    let (tag, tbs, _) = tlv(cert)?;
    if tag != SEQUENCE {
        return None;
    }
    let mut rest = tbs;
    // [0] version (optional), serial, signature algorithm, issuer, validity, subject.
    let (tag, _, r) = tlv(rest)?;
    rest = if tag == 0xA0 { tlv(r)?.2 } else { r };
    for _ in 0..3 {
        rest = tlv(rest)?.2;
    }
    let (tag, mut subject, _) = tlv(rest)?;
    if tag != SEQUENCE {
        return None;
    }
    while !subject.is_empty() {
        let (tag, mut rdn, r) = tlv(subject)?;
        subject = r;
        if tag != SET {
            continue;
        }
        while !rdn.is_empty() {
            let (_, attr, r) = tlv(rdn)?;
            rdn = r;
            let (tag, oid, value) = tlv(attr)?;
            if tag == 0x06 && oid == CN {
                let (_, text, _) = tlv(value)?;
                return Some(String::from_utf8_lossy(text).into_owned());
            }
        }
    }
    None
}

// ------------------------------------------------------------------ policy

/// What a request asks of the certificate, and what is stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TlsPolicy {
    /// `xhr.tlsPin`, canonical.
    pub pin: Option<String>,
    /// `xhr.tlsName`.
    pub name: Option<String>,
    /// The stored pin of this host and port.
    pub stored: Option<String>,
    /// Trust on first use is possible (there is a store).
    pub can_remember: bool,
}

/// Whether a certificate is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Accepted; `remember`: store its pin.
    Accept {
        /// Store the pin (first use, or a name match whose pin is new).
        remember: bool,
    },
    /// Refused, and why.
    Reject(String),
}

/// The policy of this module's documentation, for a certificate with pin
/// `fp` and common name `cn`.
pub fn decide(fp: &str, cn: Option<&str>, p: &TlsPolicy) -> Verdict {
    if let Some(pin) = &p.pin {
        return if pin == fp {
            Verdict::Accept { remember: false }
        } else {
            Verdict::Reject(format!("certificate {fp} does not match the requested pin"))
        };
    }
    if let Some(name) = &p.name {
        if cn.is_some_and(|cn| cn.trim().eq_ignore_ascii_case(name.trim())) {
            let remember = p.can_remember && p.stored.as_deref() != Some(fp);
            return Verdict::Accept { remember };
        }
    }
    match &p.stored {
        Some(stored) if stored == fp => Verdict::Accept { remember: false },
        Some(_) => Verdict::Reject(format!(
            "certificate {fp} is not the one pinned for this host (it changed: forget the pin to trust it)"
        )),
        None if p.name.is_some() => Verdict::Reject(format!(
            "certificate name {:?} is not {:?}",
            cn.unwrap_or(""),
            p.name.as_deref().unwrap_or("")
        )),
        None if p.can_remember => Verdict::Accept { remember: true },
        None => Verdict::Reject("no pin and no expected name for this certificate".into()),
    }
}

/// What the verifier saw: the certificate's pin and the verdict.
pub type Outcome = Arc<Mutex<Option<(String, Verdict)>>>;

#[derive(Debug)]
struct Verifier {
    policy: TlsPolicy,
    provider: Arc<CryptoProvider>,
    outcome: Outcome,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint(end_entity.as_ref());
        let cn = common_name(end_entity.as_ref());
        let verdict = decide(&fp, cn.as_deref(), &self.policy);
        let accepted = matches!(verdict, Verdict::Accept { .. });
        let why = match &verdict {
            Verdict::Reject(w) => w.clone(),
            Verdict::Accept { .. } => String::new(),
        };
        *self.outcome.lock().unwrap_or_else(|e| e.into_inner()) = Some((fp, verdict));
        if accepted {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(why))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A client configuration that applies `policy`, and where its verdict lands.
pub fn client_config(policy: TlsPolicy) -> Result<(Arc<ClientConfig>, Outcome), String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let outcome: Outcome = Arc::new(Mutex::new(None));
    let verifier = Arc::new(Verifier {
        policy,
        provider: Arc::clone(&provider),
        outcome: Arc::clone(&outcome),
    });
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS setup: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok((Arc::new(config), outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIDGE: &[u8] = include_bytes!("../../testdata/tls/bridge.crt.der");
    const OTHER: &[u8] = include_bytes!("../../testdata/tls/other.crt.der");

    #[test]
    fn common_names_and_fingerprints() {
        assert_eq!(common_name(BRIDGE).as_deref(), Some("001788fffe1a2b3c"));
        assert_eq!(common_name(OTHER).as_deref(), Some("not-a-bridge"));
        assert_eq!(common_name(b"garbage"), None);
        assert_eq!(common_name(&[]), None);
        let fp = fingerprint(BRIDGE);
        assert_eq!(fp.len(), 64);
        assert_ne!(fp, fingerprint(OTHER));
        let colons: String = fp
            .as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap().to_ascii_uppercase())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(normalise_pin(&colons), Some(fp));
        assert_eq!(normalise_pin("abcd"), None);
    }

    #[test]
    fn the_pinning_policy() {
        let fp = "a".repeat(64);
        let other = "b".repeat(64);
        let accept = |remember| Verdict::Accept { remember };
        let p = |pin: Option<&str>, name: Option<&str>, stored: Option<&str>, can_remember| TlsPolicy {
            pin: pin.map(String::from),
            name: name.map(String::from),
            stored: stored.map(String::from),
            can_remember,
        };
        // An explicit pin decides alone.
        assert_eq!(
            decide(&fp, Some("x"), &p(Some(&fp), None, None, true)),
            accept(false)
        );
        assert!(matches!(
            decide(&fp, Some("x"), &p(Some(&other), Some("x"), Some(&fp), true)),
            Verdict::Reject(_)
        ));
        // The expected name (Hue bridge id), any case; a new certificate's pin is remembered.
        assert_eq!(
            decide(
                &fp,
                Some("001788FFFE1A2B3C"),
                &p(None, Some("001788fffe1a2b3c"), None, true)
            ),
            accept(true)
        );
        assert_eq!(
            decide(
                &fp,
                Some("001788fffe1a2b3c"),
                &p(None, Some("001788fffe1a2b3c"), Some(&fp), true)
            ),
            accept(false)
        );
        // A wrong name falls back to the stored pin, never to first use.
        assert_eq!(
            decide(
                &fp,
                Some("evil"),
                &p(None, Some("001788fffe1a2b3c"), Some(&fp), true)
            ),
            accept(false)
        );
        assert!(matches!(
            decide(&fp, Some("evil"), &p(None, Some("001788fffe1a2b3c"), None, true)),
            Verdict::Reject(_)
        ));
        assert!(matches!(
            decide(&fp, None, &p(None, Some("001788fffe1a2b3c"), None, true)),
            Verdict::Reject(_)
        ));
        // Trust on first use, then only that certificate.
        assert_eq!(decide(&fp, None, &p(None, None, None, true)), accept(true));
        assert_eq!(decide(&fp, None, &p(None, None, Some(&fp), true)), accept(false));
        assert!(matches!(
            decide(&other, None, &p(None, None, Some(&fp), true)),
            Verdict::Reject(_)
        ));
        // No store: no first use.
        assert!(matches!(
            decide(&fp, None, &p(None, None, None, false)),
            Verdict::Reject(_)
        ));
    }

    #[test]
    fn pins_live_in_the_plugin_settings() {
        let settings = Arc::new(crate::service::MemorySettings::default());
        let store = PinStore::new("Philips Hue", settings.clone());
        assert_eq!(store.get("192.168.1.20", 443), None);
        store.set("192.168.1.20", 443, &"c".repeat(64));
        assert_eq!(store.get("192.168.1.20", 443), Some("c".repeat(64)));
        assert_eq!(store.get("192.168.1.20", 8443), None);
        assert_eq!(
            settings.get("Philips Hue", PIN_SETTING_ID, "192.168.1.20:443"),
            Some("c".repeat(64))
        );
        assert_eq!(PinStore::key("[FE80::1]", 443), "fe80::1:443");
    }
}
