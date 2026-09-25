//! Regression test for Role 7 finding A3 (shard flooding). Discovery §6 places
//! a provider only under the shard `shard(agent)`, but nothing on the query
//! side checks it: an attacker that provides under all 256 sharded keys
//! appears in every shard's shortlist.
//!
//! Asserts the SECURE behaviour and is `#[ignore]`d until the fix lands
//! (accountability-layer plan, P3).

use map_discovery::shard;
use map_identity::Keypair;
use map_mesh::MeshIndex;

const CAP: &str = "map:cap:inference/text-generation";

#[test]
#[ignore = "Role 7 A3 open; fixed in accountability-layer P3"]
fn provider_under_a_foreign_shard_is_not_returned() {
    let attacker = Keypair::from_seed([0xBB; 32]).agent_id();
    let own = shard(&attacker);
    let mut idx = MeshIndex::new();
    for s in 0..=255u8 {
        idx.provide(CAP, s, attacker.clone());
    }
    for s in 0..=255u8 {
        let listed = idx.providers(CAP, s).contains(&attacker);
        assert_eq!(listed, s == own, "shard {s}: attacker listed={listed} (own shard {own})");
    }
}
