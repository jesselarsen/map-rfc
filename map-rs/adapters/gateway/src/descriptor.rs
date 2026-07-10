//! Building a gateway's own descriptor (TASK-016 FR-1). The gateway advertises
//! two capabilities — `map:cap:<namespace>` (the gateway service) and the
//! fronted capability(ies) — with the descriptor **subject set to the gateway's
//! own AgentID** (G-1: a gateway never signs a descriptor naming a foreign
//! subject). The `gateway-of` provenance rides the [`crate::GatewayAdvert`]
//! wrapper because the scaffold `Descriptor` codec models wire fields 1–6/12
//! only (no CapEntry params / ext yet) — the spec assigns
//! `gateway/` params key 1 = gateway-of (discovery §4) for when it does.

use crate::{label_descriptor, GatewayAdvert};
use map_discovery::Descriptor;
use map_identity::AgentId;

/// The gateway service capability URI for a namespace (`namespace` is a valid
/// `gateway/<anchor>`): `map:cap:gateway/<anchor>`.
pub fn gateway_capability(namespace: &str) -> String {
    format!("map:cap:{namespace}")
}

/// FR-1: build the gateway's descriptor advertising the gateway capability plus
/// the fronted capabilities. `agent` is the gateway's own AgentID — it is the
/// descriptor subject (G-1; there is no way to name a foreign subject).
pub fn build_gateway_descriptor(
    agent: AgentId,
    seq: u64,
    namespace: &str,
    fronted_caps: &[String],
    addrs: Vec<String>,
    zones: Vec<u8>,
) -> Descriptor {
    let mut capabilities = vec![gateway_capability(namespace)];
    capabilities.extend(fronted_caps.iter().cloned());
    Descriptor {
        agent,
        seq,
        versions: vec![0x0100],
        suites: vec![1],
        addrs,
        capabilities,
        zones,
    }
}

/// Build the gateway descriptor and wrap it with its `gateway-of` provenance
/// (G-1). The wrapped descriptor's subject is the gateway itself.
pub fn build_labeled_gateway_descriptor(
    agent: AgentId,
    seq: u64,
    namespace: &str,
    fronted_caps: &[String],
    addrs: Vec<String>,
    zones: Vec<u8>,
) -> GatewayAdvert {
    let desc = build_gateway_descriptor(agent, seq, namespace, fronted_caps, addrs, zones);
    label_descriptor(desc, namespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::is_gateway_advert;
    use map_identity::Keypair;

    const FRONTED: &str = "map:cap:inference/text-generation";

    #[test]
    fn gateway_capability_uri() {
        assert_eq!(gateway_capability("gateway/example.com"), "map:cap:gateway/example.com");
    }

    #[test]
    fn descriptor_subject_is_the_gateway_g1() {
        // G-1 / FR-4: the descriptor's subject is ALWAYS the gateway's own
        // AgentID; there is no API path to name a foreign subject.
        let gw = Keypair::from_seed([7u8; 32]).agent_id();
        let desc = build_gateway_descriptor(
            gw.clone(),
            1,
            "gateway/example.com",
            &[FRONTED.to_string()],
            vec!["/ip4/127.0.0.1/tcp/0".to_string()],
            vec![2],
        );
        assert_eq!(desc.agent, gw);
        // Advertises both the gateway service and the fronted capability.
        assert_eq!(desc.capabilities[0], "map:cap:gateway/example.com");
        assert!(desc.capabilities.contains(&FRONTED.to_string()));
    }

    #[test]
    fn labeled_descriptor_carries_provenance() {
        let gw = Keypair::from_seed([7u8; 32]).agent_id();
        let advert = build_labeled_gateway_descriptor(
            gw.clone(),
            1,
            "gateway/example.com",
            &[FRONTED.to_string()],
            vec![],
            vec![2],
        );
        assert_eq!(is_gateway_advert(&advert), Some("gateway/example.com"));
        assert_eq!(advert.inner.agent, gw);
    }
}
