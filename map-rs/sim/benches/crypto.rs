//! Per-operation costs the evaluation needs (eval/HYPOTHESES.md):
//! - `keygen_shard`: one identity-grinding attempt (keygen + AgentID +
//!   shard), which is E3b's grinding rate;
//! - `receipt_dual`: encode a ReceiptCore, sign as both parties, verify both;
//! - `checkpoint_cycle`: propose + countersign + verify_resumption_point,
//!   the per-checkpoint cost for H5-b;
//! - `transcript_push_root`: one Part appended plus a root computation.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use map_core::{TaskId, Usage};
use map_discovery::shard;
use map_identity::Keypair;
use map_transcript::{CheckpointArtifact, CheckpointCore, Chunk, ChunkKind, TranscriptBuilder};
use map_trust::{sign_receipt, Metrics, Outcome, Receipt, ReceiptCore, ROLE_PROVIDER, ROLE_REQUESTER};

fn benches(c: &mut Criterion) {
    let mut seed = [7u8; 32];
    let mut n = 0u64;
    c.bench_function("keygen_shard", |b| {
        b.iter(|| {
            n += 1;
            seed[..8].copy_from_slice(&n.to_le_bytes());
            black_box(shard(&Keypair::from_seed(seed).agent_id()))
        })
    });
    let req = Keypair::from_seed([1; 32]);
    let prov = Keypair::from_seed([2; 32]);
    let core = ReceiptCore {
        task: TaskId([3; 32]),
        requester: req.agent_id(),
        provider: prov.agent_id(),
        capability: "map:cap:inference/text-generation".into(),
        started_ms: 1_790_000_000_000,
        ended_ms: 1_790_000_060_000,
        outcome: Outcome::Ok,
        metrics: Metrics { tokens_in: 100, tokens_out: 800, ttft_ms: 150, wall_ms: 60_000 },
        rating: None,
        payment_ref: None,
        transcript_root: Some([4; 32]),
        zone: Some(1),
        resumed_from: None,
    };
    c.bench_function("receipt_dual", |b| {
        b.iter(|| {
            let cb = core.encode().unwrap();
            let r = Receipt {
                sig_requester: Some(sign_receipt(&req, ROLE_REQUESTER, &cb)),
                sig_provider: Some(sign_receipt(&prov, ROLE_PROVIDER, &cb)),
                core_bytes: cb,
            };
            black_box(r.verify().unwrap())
        })
    });
    let ck = CheckpointCore {
        task: TaskId([3; 32]),
        requester: req.agent_id(),
        provider: prov.agent_id(),
        through_seq: 799,
        root: [5; 32],
        usage: Usage { tokens_in: 100, tokens_out: 800, ttft_ms: 150, wall_ms: 16_000 },
        zone: 1,
        timestamp_ms: 1_790_000_016_000,
        voucher_digest: None,
        resumed_from: None,
    };
    c.bench_function("checkpoint_cycle", |b| {
        b.iter(|| {
            let mut a = CheckpointArtifact::propose(&prov, &ck).unwrap();
            a.countersign(&req);
            black_box(a.verify_resumption_point().unwrap())
        })
    });
    let mut t = TranscriptBuilder::new();
    let mut seq = 0u64;
    c.bench_function("transcript_push_root", |b| {
        b.iter(|| {
            t.push(&Chunk { seq, kind: ChunkKind::OutputText, content: seq.to_le_bytes().to_vec() });
            seq += 1;
            black_box(t.root())
        })
    });
}

criterion_group!(crypto, benches);
criterion_main!(crypto);
