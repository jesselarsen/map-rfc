//! Regression test for Role 7 finding A3 (shard flooding). Discovery places a
//! provider only under the shard `shard(agent)`. Before P3 nothing on the query
//! side checked that, so an attacker providing under all 256 sharded keys
//! appeared in every shard's shortlist. Layer rule D-6 makes the querier
//! discard records found under a foreign shard.

use map_discovery::shard;
use map_identity::Keypair;
use map_mesh::MeshIndex;

const CAP: &str = "map:cap:inference/text-generation";

#[test]
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
