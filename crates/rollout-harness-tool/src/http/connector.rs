//! Post-DNS IP filter for the SSRF-hardened HTTP tools (RESEARCH Pattern 4).
//!
//! The domain-only allowlist is defeated by DNS rebinding / redirects / direct
//! IP (RESEARCH Anti-Patterns + Pitfall C), so the filter operates on the
//! RESOLVED `IpAddr`: every candidate IP is checked against the blocked ranges
//! below, then (optionally) required to be in the configured egress allowlist,
//! then the chosen IP is PINNED for the connection (defeats rebinding — the
//! second resolution that returns `169.254.169.254` is never connected to).
//! The same filter re-runs on every redirect target (see [`crate::http`]).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Why a resolved IP was rejected (surfaced in the tool error + events).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockReason {
    /// RFC1918 private (`10/8`, `172.16/12`, `192.168/16`).
    Private,
    /// CGNAT shared address space (`100.64/10`).
    Cgnat,
    /// Link-local / cloud IMDS (`169.254/16`, includes `169.254.169.254`).
    LinkLocal,
    /// Loopback (`127/8`, `::1`).
    Loopback,
    /// IPv6 link-local (`fe80::/10`).
    Ipv6LinkLocal,
    /// IPv6 unique-local (`fc00::/7`).
    Ipv6UniqueLocal,
    /// Multicast (v4 or v6).
    Multicast,
    /// Unspecified (`0.0.0.0`, `::`).
    Unspecified,
    /// IPv4-mapped / -compatible IPv6 wrapping a blocked v4 (`::ffff:127.0.0.1`).
    MappedV4,
    /// Resolved IP is not in the configured egress allowlist.
    NotAllowlisted,
}

impl BlockReason {
    /// Stable human-readable reason.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "RFC1918 private address",
            Self::Cgnat => "CGNAT shared address space",
            Self::LinkLocal => "link-local / cloud IMDS (169.254/16)",
            Self::Loopback => "loopback address",
            Self::Ipv6LinkLocal => "IPv6 link-local (fe80::/10)",
            Self::Ipv6UniqueLocal => "IPv6 unique-local (fc00::/7)",
            Self::Multicast => "multicast address",
            Self::Unspecified => "unspecified address",
            Self::MappedV4 => "IPv4-mapped IPv6 wrapping a blocked address",
            Self::NotAllowlisted => "resolved IP not in egress allowlist",
        }
    }
}

/// Reject any IP in a private / link-local / IMDS / loopback / multicast range.
///
/// Returns `Some(reason)` if the IP MUST NOT be connected to. Checks IPv4-mapped
/// and -compatible IPv6 by unwrapping to the inner v4 first (`::ffff:127.0.0.1`).
#[must_use]
pub fn blocked_range(ip: IpAddr) -> Option<BlockReason> {
    match ip {
        IpAddr::V4(v4) => blocked_v4(v4),
        IpAddr::V6(v6) => {
            // Unwrap IPv4-mapped/-compatible IPv6 and apply the v4 filter.
            if let Some(v4) = v6.to_ipv4() {
                if blocked_v4(v4).is_some() {
                    return Some(BlockReason::MappedV4);
                }
            }
            blocked_v6(v6)
        }
    }
}

fn blocked_v4(v4: Ipv4Addr) -> Option<BlockReason> {
    let o = v4.octets();
    if v4.is_loopback() {
        return Some(BlockReason::Loopback); // 127/8
    }
    if v4.is_unspecified() {
        return Some(BlockReason::Unspecified); // 0.0.0.0
    }
    if v4.is_link_local() {
        return Some(BlockReason::LinkLocal); // 169.254/16 (IMDS lives here)
    }
    if v4.is_private() {
        return Some(BlockReason::Private); // 10/8, 172.16/12, 192.168/16
    }
    if v4.is_multicast() || v4.is_broadcast() {
        return Some(BlockReason::Multicast);
    }
    // 100.64.0.0/10 CGNAT (no stable std helper on 1.91).
    if o[0] == 100 && (64..=127).contains(&o[1]) {
        return Some(BlockReason::Cgnat);
    }
    None
}

fn blocked_v6(v6: Ipv6Addr) -> Option<BlockReason> {
    if v6.is_loopback() {
        return Some(BlockReason::Loopback); // ::1
    }
    if v6.is_unspecified() {
        return Some(BlockReason::Unspecified); // ::
    }
    if v6.is_multicast() {
        return Some(BlockReason::Multicast); // ff00::/8
    }
    let seg = v6.segments();
    if (seg[0] & 0xffc0) == 0xfe80 {
        return Some(BlockReason::Ipv6LinkLocal); // fe80::/10
    }
    if (seg[0] & 0xfe00) == 0xfc00 {
        return Some(BlockReason::Ipv6UniqueLocal); // fc00::/7
    }
    None
}

/// Filter a resolved IP: reject blocked ranges, then enforce the allowlist.
///
/// When `allowlist` is empty the allowlist gate is skipped (block-list only —
/// the default posture). When non-empty, the IP must additionally be listed
/// (defends split-horizon DNS, RESEARCH Pattern 4 step 3).
///
/// `allow_loopback` is a TEST-ONLY escape hatch so the witness suite can point
/// the tools at a `127.0.0.1` mock server; production [`crate::http::EgressConfig`]
/// always sets it `false`, so loopback (SSRF to a local service) stays blocked.
/// It never relaxes the link-local / IMDS / private / CGNAT blocks.
#[must_use]
pub fn filter_ip(ip: IpAddr, allowlist: &[IpAddr], allow_loopback: bool) -> Option<BlockReason> {
    if let Some(reason) = blocked_range(ip) {
        if allow_loopback && reason == BlockReason::Loopback {
            // fall through to the allowlist gate; loopback permitted in tests only
        } else {
            return Some(reason);
        }
    }
    if !allowlist.is_empty() && !allowlist.contains(&ip) {
        return Some(BlockReason::NotAllowlisted);
    }
    None
}

/// Pick the first safe IP from a resolved set (the PINNED address for the
/// connection). Returns the last [`BlockReason`] when none are safe.
///
/// # Errors
/// Returns the rejection reason if every resolved IP is blocked.
pub fn pick_safe_ip(
    ips: &[IpAddr],
    allowlist: &[IpAddr],
    allow_loopback: bool,
) -> Result<IpAddr, BlockReason> {
    let mut last = BlockReason::Unspecified;
    for &ip in ips {
        match filter_ip(ip, allowlist, allow_loopback) {
            None => return Ok(ip),
            Some(r) => last = r,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn ip(s: &str) -> IpAddr {
        IpAddr::from_str(s).unwrap()
    }

    #[test]
    fn imds_is_blocked() {
        assert_eq!(
            blocked_range(ip("169.254.169.254")),
            Some(BlockReason::LinkLocal)
        );
    }

    #[test]
    fn rfc1918_blocked() {
        assert_eq!(blocked_range(ip("10.0.0.1")), Some(BlockReason::Private));
        assert_eq!(blocked_range(ip("192.168.1.1")), Some(BlockReason::Private));
        assert_eq!(blocked_range(ip("172.16.0.1")), Some(BlockReason::Private));
    }

    #[test]
    fn cgnat_blocked() {
        assert_eq!(blocked_range(ip("100.64.0.1")), Some(BlockReason::Cgnat));
    }

    #[test]
    fn ipv6_loopback_and_v4_mapped_blocked() {
        assert_eq!(blocked_range(ip("::1")), Some(BlockReason::Loopback));
        assert_eq!(
            blocked_range(ip("fe80::1")),
            Some(BlockReason::Ipv6LinkLocal)
        );
        assert_eq!(
            blocked_range(ip("::ffff:127.0.0.1")),
            Some(BlockReason::MappedV4)
        );
        assert_eq!(
            blocked_range(ip("::ffff:169.254.169.254")),
            Some(BlockReason::MappedV4)
        );
    }

    #[test]
    fn public_ip_allowed() {
        assert_eq!(blocked_range(ip("93.184.216.34")), None);
        assert_eq!(filter_ip(ip("93.184.216.34"), &[], false), None);
    }

    #[test]
    fn allowlist_gate() {
        let allow = [ip("93.184.216.34")];
        assert_eq!(filter_ip(ip("93.184.216.34"), &allow, false), None);
        assert_eq!(
            filter_ip(ip("8.8.8.8"), &allow, false),
            Some(BlockReason::NotAllowlisted)
        );
    }

    #[test]
    fn loopback_test_escape_never_unblocks_imds() {
        // The test escape hatch permits loopback but NOT link-local/IMDS.
        assert_eq!(filter_ip(ip("127.0.0.1"), &[], true), None);
        assert_eq!(
            filter_ip(ip("169.254.169.254"), &[], true),
            Some(BlockReason::LinkLocal)
        );
    }
}
