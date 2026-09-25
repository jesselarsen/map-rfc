//! End-to-end accountability flow over A2A objects, in memory (binding
//! §5–§8): provider A fails mid-task, the requester resumes on provider B
//! from A's last countersigned checkpoint, and the task finishes with a
//! dual-signed receipt that names what it resumed from.

use map_a2a::flow::{BindingError, ProviderConfig, ProviderTask, RequesterTask};
use map_core::Usage;
use map_identity::Keypair;
use map_meter::voucher::digest_mpp_tempo;
use map_meter::Amount;
use map_transcript::checkpoint::multihash_sha256;
use map_transcript::CheckpointError;
use map_trust::{Outcome, Receipt};
use serde_json::{json, Value};

const CAP: &str = "map:cap:inference/text-generation";
const T0: u64 = 1_790_000_000_000;

fn part(text: &str) -> Value {
    json!({"text": text, "mediaType": "text/plain"})
}

fn usage(n: u64) -> Usage {
    Usage { tokens_in: 20, tokens_out: 5 * n, ttft_ms: 150, wall_ms: 100 * n }
}

fn cfg(zone: u8) -> ProviderConfig {
    ProviderConfig { capability: CAP.into(), zone, checkpoint_every: 2 }
}

struct Keys {
    requester: Keypair,
    a: Keypair,
    b: Keypair,
}

fn keys() -> Keys {
    Keys { requester: Keypair::from_seed([1; 32]), a: Keypair::from_seed([2; 32]), b: Keypair::from_seed([3; 32]) }
}

/// Stream `n` Parts from `prov` to `req`, countersigning each proposal.
fn stream(prov: &mut ProviderTask, req: &mut RequesterTask, texts: &[&str], first_n: u64) {
    for (i, t) in texts.iter().enumerate() {
        let n = first_n + i as u64 + 1;
        let p = part(t);
        let proposal = prov.deliver(&p, usage(n), T0 + n * 100).unwrap();
        req.on_part(&p).unwrap();
        if let Some(bytes) = proposal {
            let countersigned = req.on_checkpoint(&bytes, &usage(n)).unwrap();
            prov.on_countersigned(&countersigned).unwrap();
        }
    }
}

#[test]
fn failover_to_a_second_provider_with_countersigned_checkpoint() {
    let k = keys();
    let mut prov_a = ProviderTask::start(k.a.clone(), k.requester.agent_id(), &[7; 16], "a2a-task-A", cfg(2));
    let mut req_a = RequesterTask::start(k.requester.clone(), k.a.agent_id(), &[7; 16], "a2a-task-A", CAP, 2);
    assert_eq!(prov_a.task_id(), req_a.task_id());

    // A delivers 5 Parts; checkpoints after Parts 2 and 4 (seq 1 and 3).
    stream(&mut prov_a, &mut req_a, &["The ", "quick ", "brown ", "fox ", "jumps"], 0);
    // Layer C-5: A is owed for its last countersigned checkpoint (4 Parts),
    // not for the 5th Part delivered after it.
    assert_eq!(prov_a.owed_usage(), Some(usage(4)));

    // A dies. The requester resumes on B with a fresh A2A task and nonce.
    let (mut req_b, pkg) = req_a.resume_on(k.b.agent_id(), &[8; 16], "a2a-task-B").unwrap();
    assert_eq!(pkg.prefix_parts.len(), 4);
    let mut prov_b = ProviderTask::resume(
        k.b.clone(),
        k.requester.agent_id(),
        &[8; 16],
        "a2a-task-B",
        cfg(2),
        &pkg.checkpoint,
        &pkg.prefix_parts,
    )
    .unwrap();
    assert_eq!(prov_b.next_seq(), 4); // continues the transcript sequence (A2A-24)
    assert_eq!(prov_b.task_id(), req_b.task_id());
    assert_ne!(prov_b.task_id(), prov_a.task_id()); // distinct TaskIDs (A2A-25)

    // B finishes the task with metering bound to a voucher (layer §9.3).
    let voucher = digest_mpp_tempo(&[0xc4; 32], &Amount::new(-6, 5_200), "ref-9", "tempo");
    prov_b.set_voucher_digest(voucher.clone());
    stream(&mut prov_b, &mut req_b, &["over ", "the lazy dog."], 0);

    let proposal = prov_b.propose_receipt(Outcome::Ok, usage(2), T0, T0 + 900).unwrap();
    let countersigned = req_b.on_receipt(&proposal, &usage(2), T0 + 50, T0 + 950).unwrap();
    let receipt: Receipt = prov_b.on_countersigned_receipt(&proposal, &countersigned).unwrap();
    let v = receipt.verify().unwrap();
    assert!(v.dual);
    assert_eq!(v.core.resumed_from, Some(multihash_sha256(&pkg.checkpoint)));
    assert_eq!(v.core.payment_ref, Some(voucher));
    assert_eq!(v.core.transcript_root, Some(req_b.transcript_root()));
    assert_eq!(v.core.zone, Some(2));
}

#[test]
fn requester_refuses_inflated_usage_and_unseen_content() {
    let k = keys();
    let mut prov = ProviderTask::start(k.a.clone(), k.requester.agent_id(), &[7; 16], "t", cfg(2));
    let mut req = RequesterTask::start(k.requester.clone(), k.a.agent_id(), &[7; 16], "t", CAP, 2);
    req.on_part(&part("one")).unwrap();
    prov.deliver(&part("one"), usage(1), T0).unwrap();
    // Usage inflated 3x relative to what the requester observed.
    let inflated = prov.deliver(&part("two"), Usage { tokens_out: 30, ..usage(2) }, T0 + 1).unwrap().unwrap();
    req.on_part(&part("two")).unwrap();
    assert!(matches!(req.on_checkpoint(&inflated, &usage(2)), Err(BindingError::OutsideTolerance)));

    // A root over content the requester never received.
    let mut prov2 = ProviderTask::start(k.a.clone(), k.requester.agent_id(), &[7; 16], "t", cfg(2));
    let mut req2 = RequesterTask::start(k.requester.clone(), k.a.agent_id(), &[7; 16], "t", CAP, 2);
    prov2.deliver(&part("one"), usage(1), T0).unwrap();
    let proposal = prov2.deliver(&part("SECRETLY DIFFERENT"), usage(2), T0 + 1).unwrap().unwrap();
    req2.on_part(&part("one")).unwrap();
    req2.on_part(&part("two")).unwrap();
    assert!(matches!(
        req2.on_checkpoint(&proposal, &usage(2)),
        Err(BindingError::Checkpoint(CheckpointError::RootMismatch))
    ));
}

#[test]
fn successor_cannot_widen_the_zone_or_accept_a_tampered_prefix() {
    let k = keys();
    let mut prov_a = ProviderTask::start(k.a.clone(), k.requester.agent_id(), &[7; 16], "t", cfg(1));
    let mut req_a = RequesterTask::start(k.requester.clone(), k.a.agent_id(), &[7; 16], "t", CAP, 1);
    stream(&mut prov_a, &mut req_a, &["a", "b"], 0);
    let (_, pkg) = req_a.resume_on(k.b.agent_id(), &[8; 16], "t2").unwrap();

    // Z-3: the checkpoint says zone 1; a successor configured for zone 2
    // would widen it.
    let widened = ProviderTask::resume(k.b.clone(), k.requester.agent_id(), &[8; 16], "t2", cfg(2), &pkg.checkpoint, &pkg.prefix_parts);
    assert!(matches!(widened, Err(BindingError::ZoneWidened)));

    // §7.4 step 2: a prefix that does not match the countersigned root.
    let mut tampered = pkg.prefix_parts.clone();
    tampered[1] = part("B");
    let bad = ProviderTask::resume(k.b.clone(), k.requester.agent_id(), &[8; 16], "t2", cfg(1), &pkg.checkpoint, &tampered);
    assert!(matches!(bad, Err(BindingError::Checkpoint(CheckpointError::RootMismatch))));

    // A checkpoint countersigned for a different requester is refused.
    let stranger = Keypair::from_seed([9; 32]).agent_id();
    let wrong = ProviderTask::resume(k.b.clone(), stranger, &[8; 16], "t2", cfg(1), &pkg.checkpoint, &pkg.prefix_parts);
    assert!(matches!(wrong, Err(BindingError::Mismatch(_))));
}

#[test]
fn receipt_with_a_different_transcript_is_refused() {
    let k = keys();
    let mut prov = ProviderTask::start(k.a.clone(), k.requester.agent_id(), &[7; 16], "t", cfg(2));
    let mut req = RequesterTask::start(k.requester.clone(), k.a.agent_id(), &[7; 16], "t", CAP, 2);
    stream(&mut prov, &mut req, &["x", "y"], 0);
    req.on_part(&part("z-only-the-requester-saw")).unwrap();
    let proposal = prov.propose_receipt(Outcome::Ok, usage(2), T0, T0 + 200).unwrap();
    assert!(matches!(req.on_receipt(&proposal, &usage(2), T0, T0 + 200), Err(BindingError::Mismatch("transcript root"))));
}
