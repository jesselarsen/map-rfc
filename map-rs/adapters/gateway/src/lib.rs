//! MAP federation gateway adapter (draft-map-ops-00 §5).
//!
//! A gateway re-advertises capabilities it learned upstream. The provenance
//! rules G-1..G-4 keep that honest:
//!
//! * **G-1** — a gateway signs re-advertised descriptors *as itself* and tags
//!   them with a `gateway-of: <namespace>` provenance label. The scaffold
//!   `Descriptor` models wire fields 1–6/12 only and MUST NOT be extended here
//!   (that is spec-side work on optional keys 7–14), so the label lives on a
//!   wrapper [`GatewayAdvert`] instead of inside the wire structure. Because a
//!   `GatewayAdvert` is plain unsigned data until the gateway seals its `inner`
//!   under its own key, there is intentionally no API that re-signs with a
//!   foreign AgentID.
//! * **G-2** — evidence/reputation accrues to the *gateway's* AgentID through
//!   the ordinary dual-signed receipt flow in `map-trust`; that is a property
//!   of the receipt path, not new code here.
//! * **G-3** — [`effective_zone`] tightens zones monotonically (never loosens).
//! * **G-4** — [`valid_namespace`] admits only DNS-anchored or mesh-registered
//!   namespaces.
//!
//! All functions are pure, total (no panics on any input), and O(len(input)).

use map_discovery::Descriptor;
use map_identity::AgentId;

/// The provenance ext-field key a `GatewayAdvert` stands in for (G-1). When the
/// descriptor wire format models optional keys 7–14 (future spec work), this
/// becomes an actual `ext` entry; until then it names the wrapper's semantics.
pub const GATEWAY_OF_EXT: &str = "gateway-of";

/// A descriptor being re-advertised through a gateway, carrying its
/// `gateway-of` provenance namespace out-of-band (G-1). `inner` is left
/// byte-for-byte untouched so the gateway can seal it under its own key.
#[derive(Clone, Debug, PartialEq)]
pub struct GatewayAdvert {
    pub inner: Descriptor,
    pub namespace: String,
}

/// G-1: wrap a descriptor with its gateway-of namespace. The wrap itself is
/// infallible; whether the namespace is well-formed is read back by
/// [`is_gateway_advert`].
pub fn label_descriptor(desc: Descriptor, namespace: &str) -> GatewayAdvert {
    GatewayAdvert {
        inner: desc,
        namespace: namespace.to_string(),
    }
}

/// G-1: read the gateway-of namespace back, iff it is a well-formed namespace.
/// An unlabeled advert (empty / invalid namespace) reads back as `None`.
pub fn is_gateway_advert(advert: &GatewayAdvert) -> Option<&str> {
    if valid_namespace(&advert.namespace) {
        Some(&advert.namespace)
    } else {
        None
    }
}

/// G-3: the zone a gateway must stamp on re-advertised traffic. Zones are
/// tighten-only through a gateway (arch §8.1): the result is
/// `max(upstream_zone, 2)`, or `max(upstream_zone, 1)` for an allowlisted
/// upstream — never below `upstream_zone`. Precondition `upstream_zone <= 3`;
/// out-of-range input is clamped to 3 (and `debug_assert`ed).
pub fn effective_zone(upstream_zone: u8, allowlisted: bool) -> u8 {
    debug_assert!(upstream_zone <= 3, "zone out of range (0..=3)");
    let upstream = upstream_zone.min(3);
    let floor = if allowlisted { 1 } else { 2 };
    upstream.max(floor)
}

/// G-4: a gateway namespace is `gateway/<anchor>` where the anchor is either
/// DNS-anchored (lowercase LDH labels joined by dots, ≥1 dot, ≤253 total) or
/// mesh-registered (a `z…` multibase AgentID). Case is significant: uppercase
/// DNS is rejected rather than silently normalized (§9).
pub fn valid_namespace(ns: &str) -> bool {
    let Some(anchor) = ns.strip_prefix("gateway/") else {
        return false;
    };
    if anchor.is_empty() {
        return false;
    }
    // Mesh form: the whole anchor is a multibase AgentID (`z` + base58btc that
    // decodes to a valid 34-byte `0xED 0x01 || key`). Try this first; a DNS
    // name is never valid base58 (it contains dots), so the two never collide.
    if AgentId::from_multibase(anchor).is_ok() {
        return true;
    }
    valid_dns_name(anchor)
}

/// LDH DNS name: 1–63-char labels of `[a-z0-9-]` (no leading/trailing `-`),
/// at least one dot, ≤253 chars total. IDN is scoped down to punycode-as-LDH.
fn valid_dns_name(s: &str) -> bool {
    if s.len() > 253 {
        return false;
    }
    let mut labels = 0usize;
    for label in s.split('.') {
        labels += 1;
        if !valid_ldh_label(label) {
            return false;
        }
    }
    labels >= 2 // at least one dot
}

fn valid_ldh_label(label: &str) -> bool {
    let b = label.as_bytes();
    if b.is_empty() || b.len() > 63 {
        return false;
    }
    if b[0] == b'-' || b[b.len() - 1] == b'-' {
        return false;
    }
    b.iter()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use map_identity::Keypair;

    fn descriptor(seed: u8) -> Descriptor {
        Descriptor {
            agent: Keypair::from_seed([seed; 32]).agent_id(),
            seq: 1,
            versions: vec![0x0100],
            suites: vec![1],
            addrs: vec!["/ip4/192.0.2.1/udp/4433/quic-v1".to_string()],
            capabilities: vec!["map:cap:inference/text-generation".to_string()],
            zones: vec![2],
        }
    }

    fn agent_multibase() -> String {
        Keypair::from_seed([9u8; 32]).agent_id().to_multibase()
    }

    #[test]
    fn valid_namespace_positive_and_negative() {
        // Positive: DNS-anchored, incl. punycode; mesh-registered AgentID.
        assert!(valid_namespace("gateway/example.com"));
        assert!(valid_namespace("gateway/a.b.example.com"));
        assert!(valid_namespace("gateway/xn--bcher-kva.example.com"));
        assert!(valid_namespace("gateway/host-1.example.com"));
        assert!(valid_namespace(&format!("gateway/{}", agent_multibase())));
        // Negative.
        assert!(!valid_namespace("gateway/")); // empty anchor (§9)
        assert!(!valid_namespace("example.com")); // missing prefix
        assert!(!valid_namespace("gateway/EXAMPLE.com")); // uppercase (§9)
        assert!(!valid_namespace("gateway/nodot")); // no dot
        assert!(!valid_namespace("gateway/-bad.example.com")); // leading hyphen
        assert!(!valid_namespace("gateway/bad-.example.com")); // trailing hyphen
        assert!(!valid_namespace("gateway/under_score.com")); // non-LDH char
        assert!(!valid_namespace("gateway/zNotBase58!!")); // bad mesh + non-DNS
    }

    #[test]
    fn effective_zone_all_eight_combos_are_tighten_only() {
        let expected = [
            // (upstream, allowlisted) -> zone
            ((0, false), 2),
            ((0, true), 1),
            ((1, false), 2),
            ((1, true), 1),
            ((2, false), 2),
            ((2, true), 2),
            ((3, false), 3),
            ((3, true), 3),
        ];
        for ((up, allow), want) in expected {
            let got = effective_zone(up, allow);
            assert_eq!(got, want, "effective_zone({up}, {allow})");
            assert!(got >= up, "zone must never decrease through a gateway");
        }
    }

    #[test]
    fn label_and_read_back_roundtrip() {
        let advert = label_descriptor(descriptor(1), "gateway/example.com");
        assert_eq!(is_gateway_advert(&advert), Some("gateway/example.com"));
        // The wrapped wire descriptor is untouched.
        assert_eq!(advert.inner, descriptor(1));
    }

    #[test]
    fn unlabeled_advert_reads_back_none() {
        // A plain (unlabeled) descriptor carries no valid gateway namespace.
        let plain = GatewayAdvert {
            inner: descriptor(2),
            namespace: String::new(),
        };
        assert_eq!(is_gateway_advert(&plain), None);
    }
}
