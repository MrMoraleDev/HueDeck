// SPDX-License-Identifier: GPL-3.0-or-later
//! Which addresses a plugin may reach: the local network only
//! (docs/host-api/network-lightloom.md, "Privacy and safety rules").

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

/// Whether `ip` is on the local network: loopback, private (RFC 1918,
/// `fc00::/7`), link-local, local multicast (`224.0.0.0/24`, `239.0.0.0/8`,
/// `ff02::/16`) or broadcast. Everything else (the internet, CGNAT,
/// unspecified addresses) is refused.
pub fn lan_allowed(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_allowed(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => v4_allowed(v4),
            None => v6_allowed(v6),
        },
    }
}

fn v4_allowed(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || (o[0] == 224 && o[1] == 0 && o[2] == 0)
        || o[0] == 239
}

fn v6_allowed(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    ip.is_loopback()
        || (s[0] & 0xfe00) == 0xfc00 // unique local
        || (s[0] & 0xffc0) == 0xfe80 // link-local
        || s[0] == 0xff02 // link-local multicast
}

/// Whether `ip` is a multicast or broadcast destination (sends to it need
/// `SO_BROADCAST`, replies come from other addresses).
pub fn is_group(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_multicast() || v4.is_broadcast() || v4.octets()[3] == 255,
        IpAddr::V6(v6) => v6.is_multicast(),
    }
}

/// Resolve `host` (an address literal or a name) to the first local-network
/// address, IPv4 first. Names go through the system resolver (which answers
/// `*.local` names through mDNS on current Windows and most Linux systems);
/// the address returned is the one to connect to, so a name cannot resolve
/// to one address for the check and another for the connection.
pub fn resolve_lan(host: &str, port: u16) -> Result<SocketAddr, String> {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return Err("no host".into());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if lan_allowed(ip) {
            Ok(SocketAddr::new(ip, port))
        } else {
            Err(format!("{ip} is not on the local network"))
        };
    }
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {host}: {e}"))?
        .collect();
    let mut lan: Vec<SocketAddr> = addrs.iter().copied().filter(|a| lan_allowed(a.ip())).collect();
    lan.sort_by_key(|a| !a.is_ipv4());
    lan.into_iter().next().ok_or_else(|| {
        if addrs.is_empty() {
            format!("cannot resolve {host}")
        } else {
            format!("{host} is not on the local network")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> bool {
        lan_allowed(s.parse().unwrap())
    }

    #[test]
    fn local_addresses_only() {
        for a in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.9",
            "172.31.255.255",
            "192.168.1.20",
            "169.254.3.4",
            "255.255.255.255",
            "224.0.0.251",
            "239.255.255.250",
            "::1",
            "fe80::1",
            "fd00::5",
            "ff02::fb",
            "::ffff:192.168.0.1",
        ] {
            assert!(ok(a), "{a} is local");
        }
        for a in [
            "8.8.8.8",
            "1.1.1.1",
            "172.32.0.1",
            "100.64.0.1",
            "0.0.0.0",
            "224.0.1.1",
            "2001:4860::8888",
            "ff05::1",
            "::",
            "::ffff:8.8.8.8",
        ] {
            assert!(!ok(a), "{a} is not local");
        }
    }

    #[test]
    fn literals_are_checked_without_a_lookup() {
        assert_eq!(
            resolve_lan("192.168.1.5", 80).unwrap(),
            "192.168.1.5:80".parse().unwrap()
        );
        assert!(resolve_lan("8.8.4.4", 80).is_err());
        assert_eq!(resolve_lan("[::1]", 8).unwrap(), "[::1]:8".parse().unwrap());
        assert!(resolve_lan("", 1).is_err());
        assert!(resolve_lan("localhost", 9).unwrap().ip().is_loopback());
    }

    #[test]
    fn groups() {
        assert!(is_group("255.255.255.255".parse().unwrap()));
        assert!(is_group("239.255.255.250".parse().unwrap()));
        assert!(is_group("192.168.1.255".parse().unwrap()));
        assert!(!is_group("192.168.1.4".parse().unwrap()));
    }
}
