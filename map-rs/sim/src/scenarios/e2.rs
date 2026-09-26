//! E2 (eval/HYPOTHESES.md): checkpointed handoff.
//!
//! A task of L Parts is served one Part per time unit. The serving provider
//! fails with hazard λ per Part (lifetime ~ Exp(λ)) and is replaced. Arms:
//! - `restart`: the successor starts from Part 0.
//! - `countersigned` (layer §7): the successor replays from the last
//!   dual-signed checkpoint.
//! - `provider_only` (v1-like): the successor trusts the failed provider's
//!   own claimed progress.
//!
//! A dishonest provider (probability f) that fails claims 50% more progress
//! than it delivered. In real mode, countersigned runs build every
//! checkpoint with `map-transcript` (propose, countersign, verify) and
//! resume through `map_transcript::resume` with prefix-root verification;
//! checkpoint bytes are the real encodings. The C-5 bound is asserted on
//! every failure, and a violation aborts the run.

use crate::config::Params;
use crate::crypto::{task_id, CryptoMode};
use crate::rng::Rng;
use map_core::Usage;
use map_identity::Keypair;
use map_transcript::{
    resume, CheckpointArtifact, CheckpointCore, Chunk, ChunkKind, TranscriptBuilder,
};

pub const METRICS: &[&str] = &[
    "waste_parts",
    "failures",
    "signatures",
    "checkpoint_bytes",
    "overbilled_parts",
    "integrity_failures",
    "unpaid_delivered_parts",
];

/// Exact encoded size of a dual-signed artifact over `core`. Signatures are
/// always 64 bytes, so placeholders give the real length without signing;
/// real mode asserts the equality.
fn checkpoint_len(core: &CheckpointCore) -> Result<usize, String> {
    let art = CheckpointArtifact {
        core_bytes: core.encode().map_err(|e| e.to_string())?,
        sig_requester: Some([0; 64]),
        sig_provider: Some([0; 64]),
    };
    Ok(art.encode().map_err(|e| e.to_string())?.len())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    Restart,
    Countersigned,
    ProviderOnly,
}

fn chunk(seq: u64) -> Chunk {
    Chunk {
        seq,
        kind: ChunkKind::OutputText,
        content: seq.to_le_bytes().to_vec(),
    }
}

struct Real {
    requester: Keypair,
    builder: TranscriptBuilder,
    /// The last dual-signed checkpoint and its prefix length.
    last: Option<(CheckpointArtifact, u64)>,
}

pub fn run(p: &Params, rng: &mut Rng, mode: CryptoMode) -> Result<Vec<f64>, String> {
    let arm = match p.str("arm")? {
        "restart" => Arm::Restart,
        "countersigned" => Arm::Countersigned,
        "provider_only" => Arm::ProviderOnly,
        a => return Err(format!("unknown arm `{a}`")),
    };
    let l = p.u64("L")?;
    let lambda = p.f64("lambda_L")? / l as f64;
    let c = p.u64("c")?; // 0 = never checkpoint (c = ∞)
    let f = p.f64_or("f_dishonest", 0.0)?;
    let tasks = p.u64_or("tasks_per_seed", 10)?;
    let requester = Keypair::from_seed(rng.bytes32());
    let mut totals = [0f64; 7];
    for t in 0..tasks {
        let m = one_task(arm, l, lambda, c, f, rng, mode, &requester, t)?;
        for (a, b) in totals.iter_mut().zip(m) {
            *a += b;
        }
    }
    Ok(totals.iter().map(|x| x / tasks as f64).collect())
}

#[allow(clippy::too_many_arguments)]
fn one_task(
    arm: Arm,
    l: u64,
    lambda: f64,
    c: u64,
    f: f64,
    rng: &mut Rng,
    mode: CryptoMode,
    requester: &Keypair,
    task_n: u64,
) -> Result<[f64; 7], String> {
    let (mut waste, mut failures, mut sigs, mut bytes, mut overbilled, mut integrity, mut unpaid) =
        (0u64, 0u64, 0u64, 0f64, 0u64, 0u64, 0u64);
    let mut start = 0u64; // where the current provider starts
    let mut acked = 0u64; // last countersigned prefix length
    let mut real = (!mode.fast && arm == Arm::Countersigned).then(|| Real {
        requester: requester.clone(),
        builder: TranscriptBuilder::new(),
        last: None,
    });
    let mut tainted = false; // provider-only: a gap was accepted
    loop {
        let provider = Keypair::from_seed(rng.bytes32());
        let lifetime = rng.exp(lambda);
        let dishonest = rng.bernoulli(f);
        let end = if (start as f64 + lifetime) >= l as f64 {
            l
        } else {
            start + lifetime.floor() as u64
        };
        // Checkpoints this provider produces (per-provider cadence, as in
        // map-a2a's ProviderTask).
        let mut provider_acked = start;
        if arm == Arm::Countersigned && c > 0 {
            let n = (end - start) / c;
            for i in 1..=n {
                let through = start + i * c;
                sigs += 2;
                let mut core = CheckpointCore {
                    task: task_id(task_n),
                    requester: requester.agent_id(),
                    provider: provider.agent_id(),
                    through_seq: through - 1,
                    root: [0; 32],
                    usage: Usage {
                        tokens_in: 0,
                        tokens_out: through - start,
                        ttft_ms: 0,
                        wall_ms: through - start,
                    },
                    zone: 1,
                    timestamp_ms: through,
                    voucher_digest: None,
                    resumed_from: None,
                };
                let len = checkpoint_len(&core)?;
                bytes += len as f64;
                if let Some(r) = real.as_mut() {
                    for s in r.builder.leaf_count() as u64..through {
                        r.builder.push(&chunk(s));
                    }
                    core.root = r.builder.root();
                    let mut art =
                        CheckpointArtifact::propose(&provider, &core).map_err(|e| e.to_string())?;
                    art.countersign(&r.requester);
                    art.verify_resumption_point().map_err(|e| e.to_string())?;
                    let enc = art.encode().map_err(|e| e.to_string())?.len();
                    if enc != len {
                        return Err(format!(
                            "signed checkpoint is {enc} bytes, placeholder encoding {len}"
                        ));
                    }
                    r.last = Some((art, through));
                }
                provider_acked = through;
            }
        }
        if end == l {
            sigs += 2; // the final dual-signed receipt
            break;
        }
        failures += 1;
        let delivered = end - start;
        match arm {
            Arm::Restart => {
                waste += end; // everything so far is recomputed
                start = 0;
            }
            Arm::Countersigned => {
                if provider_acked > acked {
                    acked = provider_acked;
                }
                // C-5: the failed provider is billed only its countersigned
                // usage; what it delivered past that is its own loss.
                let billed = provider_acked - start;
                let unpaid_here = end - provider_acked;
                unpaid += unpaid_here;
                if c > 0 && unpaid_here >= c {
                    return Err(format!(
                        "C-5 bound violated: {unpaid_here} unacknowledged Parts with c = {c}"
                    ));
                }
                if billed > delivered {
                    return Err("C-5 bound violated: billed beyond delivered".into());
                }
                waste += end - acked;
                if let Some(r) = real.as_mut() {
                    if let Some((art, through)) = &r.last {
                        let prefix: Vec<Chunk> = (0..*through).map(chunk).collect();
                        let resumed = resume(art, &prefix).map_err(|e| e.to_string())?;
                        if resumed.next_seq != acked {
                            return Err(
                                "resume point disagrees with the countersigned prefix".into()
                            );
                        }
                    }
                    // The successor's transcript continues from the prefix.
                    let mut b = TranscriptBuilder::new();
                    (0..acked).for_each(|s| b.push(&chunk(s)));
                    r.builder = b;
                }
                start = acked;
            }
            Arm::ProviderOnly => {
                sigs += 1; // the provider's signed progress claim
                let claimed = if dishonest {
                    (start + delivered + delivered / 2).min(l)
                } else {
                    end
                };
                if claimed > end {
                    overbilled += claimed - end;
                    tainted = true;
                }
                start = claimed;
            }
        }
    }
    if tainted {
        integrity += 1;
    }
    Ok([
        waste as f64,
        failures as f64,
        sigs as f64,
        bytes,
        overbilled as f64,
        integrity as f64,
        unpaid as f64,
    ])
}
