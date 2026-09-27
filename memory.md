# memory.md — MAP (Manolito Agent Protocol) Project Memory

Handoff file for continuing this project in a new context (e.g., Claude
Code). Read this first; it states what exists, what is binding, what is
unverified, and what to do next. Tip: in Claude Code, either rename this to
`CLAUDE.md` or add the line `@memory.md` to your `CLAUDE.md` so it loads
automatically.

## 0. 2026-09 PIVOT — RESUME HERE (supersedes §1–§8 where they conflict)

**Everything from §1 on is historical (v1).** Read it for the protocol decisions
that carry over (deterministic CBOR + COSE, verify the received bytes, one
Ed25519 AgentID, no self-reported trust). Do not read it for goals, submission
path or roadmap.

**What happened.**
- 2026-09-18: eight role-based external reviews (`MAP Review Results/`).
  - ERRATA Track 0 (claim corrections) is done in `a1df777`.
  - The reviews demonstrated real exploits: negative Amount; forgeable dispute
    claims 0/1; receipt farming 0.998/conf 501 in ~150 ms; shard flooding;
    node-ID grinding ~60k IDs/s; a quadratic replay cache. They also found
    ProVerif P4–P6 vacuous.
- 2026-09-24 prior-art review, three findings:
  - The ISE cannot publish the family: RFC 8726 bars the registries.
  - The "who has this file → who can do this task" thesis is pre-empted by
    arXiv:2603.03753.
  - Metering is pre-empted by MPP sessions and x402 batch-settlement. The
    tolerance band is telecom reconciliation practice.
- **Decision (author):** drop the IETF track. Rework MAP into a thin
  **accountability layer** over A2A + an ADS-style DHT + MPP/x402. Publish an
  **arXiv systems paper with a quantitative evaluation**.

**Where things are.**
- **Tag `map-v1-ietf`** (`4107aaa`): the frozen v1 snapshot, including the
  review results and `attic/probes/`. `main` stays there.
- **Branch `accountability-layer`**: all new work.
- **Rationale:** `docs/ADR-0001-thin-accountability-layer.md`.
- **Plan:** `C:\Users\Master\.claude\plans\i-think-an-arxiv-humble-swing.md`
  (outside the repo). Phases:
  - P0 housekeeping
  - P1 spec skeleton in `spec/`
  - P2 prune the Rust code
  - P3 fix the kept layers (A1, A3, A4, A8, R4-02 checkpoint codec, R4-05, formal P2)
  - P4 A2A binding + voucher mapping
  - P5 Go verifier-only
  - P6 `map-sim` harness
  - P7 experiments E1–E5
  - P8 spec final
  - P9 paper
- **Kept (the contribution):**
  1. dual-signed receipts
  2. observer-local trust with the rebuild rule
  3. requester-countersigned checkpoints enabling resume on another provider
  4. metering tied to receipts/checkpoints, mapped onto payer-signed cumulative vouchers
  5. data-sensitivity zones (accountability, not confidentiality)
  6. two-plane discovery with signed A2A Agent Cards as descriptors
- **Dropped:** MAP's own task protocol (replaced by an A2A binding),
  escrow/disputes/credit ledger/x402 method definition, the IETF render
  pipeline, the Manolito framing and name. did:key and multiformats become
  informative.

**Decisions (author):**
| # | Decision | Status | Blocks |
|---|---|---|---|
| D1 | Name | **DECIDED 2026-09-24:** keep the acronym, re-expand it as **MAP = Mutual Attestation Protocol**, drop "Manolito". Wire tags (`"MAPReceipt"`, `"MAPCheckpointAck"`, `map:cap:`) stay byte-stable. | P3 |
| D2 | A2A binding via extension URI + metadata over JSON/HTTP | **DECIDED 2026-09-25** (as recommended) | P4 |
| D3 | libp2p | **DECIDED:** discovery plane only (descriptor publication + STATUS_QUERY). The work plane runs over A2A/HTTPS. | P2 |
| D4 | Go's role | **DECIDED:** FULL second implementation. Go ports the A2A provider *and* requester for the kept layers, so the interop matrix exercises both directions. Budget P4/P5 accordingly. | P5 |
| — | Publishing | **DECIDED:** push `map-v1-ietf` and push `accountability-layer` at each phase gate. `main` is untouched. | — |

**Still open:**
| # | Decision | Blocks |
|---|---|---|
| D5 | Default issuer weighting — **DECIDED 2026-09-27:** unknown issuers = 0 (T-3). The pre-registered D5 picked W2 (α = 0.2); after A8 the author ordered a pre-registered minimax re-test (A9), which selected **W1 (direct agreement)** as the §8.4 default | P3/E1 |
| D6 | TaskID under A2A: `SHA-256(enc(["MAPTaskID", requesterNonce(16B), providerAgentID, a2aTaskId]))` (binding A2A-8) — **DECIDED 2026-09-25** | P4 |
| D7 | arXiv cs.CR primary, cross-listed cs.MA/cs.NI; needs an endorser | P9 |

**P0 status (2026-09-24):**
- Probes preserved and re-run; they reproduce at `a1df777`.
- Ported `#[ignore]` regression tests: `map-trust/tests/attack_farming.rs`,
  `map-core/tests/attack_replay_cache.rs`, `map-mesh/tests/attack_shard_flood.rs`,
  and `map-discovery/tests/attack_grind.rs` (a measurement).
- All 6 regression tests FAIL with `--ignored`, as expected. The normal suites
  are green (Rust, plus Go vet/test).
- Gate for P3: every `attack_*` test passes under `--ignored`.

**P1 status (2026-09-24): DONE.**
- New `spec/` directory:
  - `layer.md`: normative. Rule IDs R-/C-/T-/M-/Z-/D-.
  - `binding-a2a.md`: A2A-*, verified against A2A v1.0.1.
  - `binding-libp2p-discovery.md`: DHT-*.
  - `mapping-payment-rails.md`: RAIL-*, informative.
- v1 drafts, `build/` and `SUBMISSION.md` removed from the branch (they stay
  in the tag). Vectors moved to `spec/test-vectors/`. ERRATA, CHANGELOG, the
  critique and the REVIEW docs moved to `docs/history/`.
- Rulings made while reviewing the drafts (they supersede anything
  contradicting them):
  1. **Zones are the requester's classification** of the counterparty
     (1 allowlist / 2 authenticated mesh / 3 ephemeral-gateway), never a
     provider claim. Z-1: a zone-z task goes only to counterparties
     classified ≤ z. Descriptor key 5 is the zone labels a provider
     *accepts*, not a trust claim.
  2. **Receipt signing order is binding-defined.** Over A2A the provider
     proposes at the terminal state and the requester countersigns. The
     tolerance anchors to the *countersigner's* own observation.
  3. **The Sybil ring is closed by T-3** (unknown issuer = 0), not by R-4.
     R-4 is only the lone-provider guard.
  4. **Checkpoint v2** = `CheckpointArtifact {1 core-bytes, ?2 sig_req, ?3 sig_prov}`,
     signing input `enc(["MAPCheckpoint", role, core-bytes])`, with usage,
     zone and voucher-digest inside the signed core. The v1
     `"MAPCheckpointAck"` input is retired.
  5. ReceiptCore keys 1–11 are unchanged (TV-002 bytes stable). Added: 12
     zone, 13 resumed-from.
  6. `Amount` mantissa is `uint`. Disjoint-path lookups (D-9/DHT-12) and the
     epoch salt (D-8/DHT-13) are **experimental**.
- A2A facts verified at v1.0.1:
  - header `A2A-Extensions` (not `X-`)
  - unified `Part` oneof {text|raw|url|data}
  - `Message.referenceTaskIds`
  - no TIMEOUT state; `REJECTED` produces no receipt
  - checkpoints delivered mid-`WORKING` is implementation-defined, so the
    fallback is a transient `INPUT_REQUIRED`
  - x402 `upto` is single-settlement; only `batch-settlement` fits
    cumulative metering
  - the MPP generic voucher byte form is unverified (RAIL-5 fallback)

**P2 status (2026-09-25): DONE (Rust prune).**
- Workspace is now: map-identity, map-core (+`TaskId`), map-discovery,
  map-transcript (was map-tasks: transcript root + chunks + flow credit),
  map-trust (+`evidence::apply_verified_evidence`, `lifecycle::Tolerance`
  policy struct), map-meter (was map-pay: Amount/Asset/AssetAmount/Usage/
  units/`billed`), map-session, map-mesh, map-net, map-cli (keygen,
  projection demo, TCP interop peer), ollama-provider (backend seam only),
  map-conformance (28 cases).
- Deleted, and preserved at the tag: map-taskrun, adapters/gateway,
  adapters/x402-adapter, the v1 task msg codecs and state machine,
  pay codec/escrow/credit-ledger/disputes/watermark-invoice verifier, and
  the rfc-title-o-matic / serve-ollama / x402 demos.
- Vectors: 19 marked RETIRED(v1) *in the generator* (`gen_vectors.py`). It
  regenerates the committed file byte-for-byte; the generator needs CPython
  with pynacl/cbor2/base58 (see AGENTS.md).
- Interop matrix: 16/16 core TCP. The libp2p task/pay cells are retired.
  Both implementations stay green; Go keeps its v1 packages until P5.
- Judgment calls:
  1. The v1 `CountersignedCheckpoint`/`checkpoint_ack_signing_input` was
     **not** moved out of dispute.rs. Layer §7 retires that byte format, so
     P3 writes the v2 `CheckpointArtifact` codec fresh.
  2. **R4-05 is moot.** The settled-vs-accepted delta bug lived in
     `verify_watermark`, which was invoice-bound and is deleted.
  3. The x402 adapter is deleted rather than cut down (R4-16 is moot). The
     voucher mapping is written fresh in P4 against x402 V2.
  4. `Amount` still carries an i64 mantissa. M-4 non-negativity lands in P3.
  5. Delegation: resolved in P3a (deleted).

**P3 status (2026-09-25): DONE** (commits 7c73ae6 P3a, 00bac8d P3b, plus the
P3c commit).
- **Delegation deleted** (author ruling), from both map-core and
  map-session. TV-011/012 and NV-007/009 are RETIRED(v1); the matrix is
  now 14 cells.
- **A1 fixed.**
  - `TrustProjection::apply` enforces R-2/R-3/R-4 plus zero-weight/cancelled,
    and returns `Result<(), Skip>`.
  - `IssuerWeights` implements T-3's endpoints (observer = 1, configured,
    else 0). The *derivation* of weights for other issuers is still D5,
    compared in E1.
  - `ProjParams` gains `skew_ms` (120 s) and `issuer_cap` (None; E1 picks
    the default).
  - ReceiptCore keys 12/13 are implemented.
- **A3 fixed:** querier-side shard filter in MeshIndex and in
  `MapNode::find_providers`.
- **A4:** `shard_epoch` exists, unwired and experimental (D-8).
  Disjoint-path lookups are not implemented (D-9 experimental).
- **A8 fixed:**
  - replay cache is amortized O(1);
  - descriptor ingest enforces the 24 h ceiling and 120 s future skew;
  - `serve_one` takes a required per-peer `RateLimiter` (32 burst, 8/s,
    ERROR 1005).
- **M-4 / R4-01 fixed:** `Amount.mantissa` is `u64`, and negative mantissas
  fail to decode.
- **R4-02 fixed:** `map_transcript::checkpoint` holds the v2
  CheckpointArtifact (dual-signed, role-tagged "MAPCheckpoint",
  usage/zone/voucher inside the signed core). It also implements
  `verify_resumption_point`, `check_successor` (C-3/C-4), `verify_prefix`,
  `resume` and `digest` (= resumed-from). `Usage` moved to map-core, shared
  by map-meter.
- **Vectors:**
  - TV-021 (CheckpointArtifact over a 3-chunk prefix), NV-013 (bad
    countersignature), NV-014 (root mismatch on resume).
  - Generated by Python, reproduced byte-exact by Rust conformance (28
    cases), constants extracted by script into `vectors3.rs`.
  - Go does not implement them yet (P5).
- **Formal P2 repaired:**
  - HELLO_ACK carries a fresh responder nonce; Q2 is the injective
    anti-relay correspondence, **true**, with reachability shown; the relay
    control is **false**.
  - `formal/plant-p2.sh` removes the signature check, the CB check and the
    nonce in turn, and Q2 is **false** for each. Run it under WSL2.
  - P1 is still WEAK. P3–P6 models are record-only.
  - `.gitattributes` keeps `*.sh` LF.
- **Still open for later phases:**
  - Go parity for everything above (P5, full second implementation per D4);
  - the unilateral-receipt rule has no cross-implementation vector yet (it
    is a projection rule, so it gets a Go test in P5);
  - the descriptor is still the v1 shape (A2A Agent Card embedding lands in
    P4);
  - D5/D7 are still open (D2/D6 were decided in P4).

**P4 status (2026-09-25): DONE** (commits 6126feb P4a, 6638d4a P4b, plus the
P4c commit). The P4 gate is `pwsh scripts/demo-a2a.ps1`, ALL PASS.
- **`adapters/a2a` (map-a2a):**
  - `jcs`, `b64`;
  - `card`: A2A §8.4 EdDSA JWS, detached payload, default-value removal
    per §8.4.1 including implicit-presence scalars;
  - `descriptor`: DescriptorV2 embeds the signed card; card kid = AgentID
    (D-4);
  - `binding`: `"map/v2"` metadata, error codes, TaskID per D6, and
    Part→leaf mapping by JCS;
  - `flow`: ProviderTask/RequesterTask, checkpoints C-1..C-5, resumption on
    another provider, Z-3;
  - `workload`: the deterministic generator and synthetic usage.
- **Feature `http`** (tiny_http + ureq, no TLS):
  - `server`: A2A JSON-RPC `SendMessage`/`GetTask` plus
    `/.well-known/agent-card.json`. Checkpoints use the A2A-21 fallback:
    pause in INPUT_REQUIRED, and the next `SendMessage` with `taskId`
    carries the countersignature and `voucherDigest`.
  - `client`: discover (verifies the card), `run_task`, resume via
    `referenceTaskIds` + checkpoint + a `mapTranscriptPrefix` DataPart.
  - Bins `map-a2a-provider` (`--fail-after-parts` simulates a crash) and
    `map-a2a-requester` (`--plain` = non-extension client).
- **`map-meter::voucher`:** RAIL-1/2/3/5 digests, exact atomic units, and
  the M-6 ceiling.
- **Demo** (`scripts/demo-a2a.ps1`):
  - A crashes after producing 10 Parts. It was countersigned through 8.
  - The requester resumes on B, which delivers 16.
  - The receipt is dual-signed and carries resumed-from and a voucher digest.
  - Voucher watermarks are A = 8 and B = 16, summing to the 24 Parts
    delivered.
  - A plain client gets COMPLETED with no MAP metadata (A2A-28).
- **Unmodified a2a-sdk 1.0.3** (`interop/a2a-sdk/check_sdk.py`; the demo
  runs it when `MAP_A2A_SDK_PY` is set):
  - it verifies our live card and TV-022 with its own EdDSA verifier;
  - it rejects an altered card (negative control);
  - it completes a task against us.
- **⚠ Interop finding (paper E5 material): the SDK's canonicalization is
  not RFC 8785.** It does `MessageToDict`, then `_clean_empty`, then
  `json.dumps`, which has three consequences:
  - Struct integers are re-emitted as `4.0`;
  - all empties are dropped, even REQUIRED ones;
  - non-ASCII characters are escaped.

  Our first card failed its verifier on `"required": false` (our bug: an
  implicit-presence default, which §8.4.1 omits) and on integers in
  `params` (the SDK's bug). The fix is spec A2A-3a (params carry uints as
  decimal strings) plus A2A-31 (a card profile avoiding every divergent
  construct). TV-022 was regenerated. **Consider reporting upstream to
  a2aproject/a2a-python** (author's call; not filed).
- **Spec additions:**
  - the opening Message MUST carry `agentId` (the requester's AgentID). This
    was a real gap: nothing named the countersigner's key;
  - the prefix DataPart shape `{"data":{"mapTranscriptPrefix":[...]}}`
    (A2A-23).
- **Deviations from the plan:**
  - Ollama is not installed here, so the demo uses the deterministic
    workload rather than ollama-provider. The `Workload` seam takes a real
    model later.
  - The voucher digest is bound on countersign rather than minted on a real
    rail (mock rail = MPP-Tempo fallback digest over the billed cumulative
    amount).
- **Not done yet:**
  - DescriptorV2 is not yet used by mesh/net;
  - no streaming (A2A-20 primary path);
  - the `http` feature has no in-process test; the demo script is the gate.
- **Next: P5** (Go full second implementation, incl. the A2A requester and
  provider) ∥ **P6** (map-sim).

**P5 status (2026-09-26): DONE** (e04d120 P5a, 3ab10b4 P5b, plus the P5c
commit). The gate is `pwsh interop/run-matrix.ps1`: **28/28** with
`MAP_A2A_SDK_PY` set.
- **P5a (Go prune + v2 core):**
  - Deleted from map-go: tasks runtime, pay, delegation, map-requester,
    map-excuse, and the task/pay/delegated interop scenarios.
  - New stdlib packages: `core` (TaskID, Usage), `transcript` checkpoints,
    `receipt` v2 (optional signatures, keys 12/13), `trust` (R-2..R-4
    inside Apply, IssuerWeights = 0 for unknown issuers, cap, rebuild,
    Tolerance, evidence filter), `meter` (Amount, Billed, RAIL-1/5, M-6),
    `discovery` DescriptorV2 and ShardEpoch.
  - Go parity for A3 (dht shard filter) and A8 (O(1) replay expiry).
  - **TV-023** pins the unilateral-receipt rule and rule order in both
    implementations.
- **P5b (Go A2A core):** jcs, card, descriptor, binding, flow, workload.
  - **TV-024** (v2 descriptor envelope) and **TV-025** (TaskID + Part
    leaves over non-canonical Parts).
  - Rust conformance now runs 34 cases; the extractor lives in the repo at
    `map-rs/conformance/map-conformance/extract_vectors3.py`.
- **P5c (Go HTTP):**
  - `a2a/server.go` and `client.go` on net/http, with an injectable
    `Crash` hook for tests.
  - `cmd/map-a2a-{provider,requester}` take the same flags and print the
    same result format as the Rust bins.
  - `interop/run-matrix.ps1` was rewritten: (a) vectors per
    implementation; (b) 8 mixed failover cells (requester × A × B ∈
    {rust, go}), 2 plain-client cells and 2 SDK cells; (c) 14 core cells.
    MATRIX.md re-stamped.
- **⚠ Independence caveat (paper must say this):** the v2 Go packages were
  written WITH the Rust source open. They are a second implementation, not
  an independent one (`map-go/README.md` says so). The cross-checks rest
  on artifacts neither codebase produced:
  - the Python-generated vectors (TV-021..025);
  - a Python cbor2 oracle for the MPP-Tempo digest, pinned in both
    implementations;
  - the unmodified a2a-sdk.
- **Harness validation, done for every new check.** Planted bugs were
  caught:
  - admitting a unilateral positive → TV-023, both implementations;
  - a wrong checkpoint tag → TV-021/NV-014;
  - forgetting to drop Part `metadata` → TV-025;
  - uppercase hex digits in JCS control-character escapes → TV-025, both
    implementations;
  - a Go-only TaskID tag → exactly the 6 mixed A2A cells fail.
- **Lessons:**
  - TV-025 first used U+0001, whose escape has no hex letters, so the
    uppercase plant passed. Choose adversarial inputs that exercise the
    property.
  - Restore planted files with cp or touch, never an mtime-preserving mv:
    cargo reused the stale planted build.
  - PowerShell variables are case-insensitive (`$pA` clobbered `$pa`).
  - The Write tool NFC-normalizes: U+FB33 decomposed. Build such strings
    from code points.
- **Known divergence:** Go rejects unknown envelope protected-header keys;
  Rust ignores them. This is harmless for v2 and documented.
- **Not done:**
  - no Go streaming (A2A-20);
  - Go does not port map-mesh / map-net discovery beyond the existing
    libp2p `net`/`dht` packages;
  - DescriptorV2 is not yet used by mesh/net in either implementation.
- **Next: P6** (map-sim harness), then P7 experiments. D5/D7 are still
  open.

**P6 status (2026-09-26): DONE.** Commits: 2185eac P6a (pre-registration,
tag `prereg-v1`), 65aa368 P6b, b08df48 P6c, and the P6d commit.
- **Pre-registration:** `eval/HYPOTHESES.md` was committed and tagged
  BEFORE any simulator code. It changes only by appending Amendments.
  A1-A3 so far:
  - A1: E1 uses `--fast-crypto`, backed by the realcheck real = fast;
  - A2: every E1 arm gets fallback ≤ 3;
  - A3: B1 keeps decay and the cap.
- **map-sim** (`map-rs/sim`):
  - a deterministic DES; the CSV is byte-identical at any thread count;
  - fast crypto equals real crypto, enforced by tests;
  - scenarios f1, e1, e2, e3a, e3b, e3c, e3d, e4;
  - `eval/run_all.ps1` runs everything, then `REPORT.md` and figs;
  - `eval/analysis` is pinned (numpy 2.5.3, matplotlib 3.11.2).
- **Real-crate changes:**
  - map-trust `admissible()` extracted, and apply calls it;
  - map-transcript `TranscriptBuilder::root()` is now O(log n) via a
    frontier; it was O(n) per call, O(L²/c) per task.
- **F1 = 2603.03753 re-implemented** from their v0.3.10 source (read, not
  run). The published Fig. 5a is reproduced within ±0.05 at every point
  (max |diff| 0.024, 60 seeds).
  - **Finding:** their "trust" is a SELF-REPORTED descriptor hint, and
    Risk-Aware is a memoryless canary. The plan's B2 (receipts + ledger)
    was their prose, not their evaluation, so it is now two arms: B2 and
    B2L.
- **Previews** (NOT results; these are why the rules are pre-registered):
  - F1-b: MAP+RA ≫ RA at 3600 s, no gain at 60 s (as predicted).
  - F1-b: MAP-only collapses under whitewashing. Honest providers failing
    50% of the time score below the unknown prior of 0.5. This is a
    genuine losing regime and argues for §8.5 risk-pricing of unknowns.
  - E1: Ours-L(W0) may FAIL H1 (share 0.074 vs B1 0.088, B2 0.114 under
    farming). Ours-L+C ≈ 0.007. W1/W2 cut the share about 60% vs W0.
- **D5 rule flaw: REPAIRED by Amendment A4 (2026-09-27, author's
  request).**
  - The original tie clause ("<2 pp honest success → simpler") was nearly
    always true, since fallback keeps success ≈ 0.99. It also picked W0
    in one synthetic case where W0's success was 5 pp LOWER.
  - The repaired rule takes the lowest-share arm. A simpler arm replaces
    it only if its share CI overlaps AND its success is ≤ 2 pp below the
    best's.
  - `analyze.py` applies A4 and also prints the original wording's pick.
  - A4 discloses the 2-seed plumbing numbers that were seen before it was
    written (W0 0.081, W1 0.031, W2 0.029). On those numbers the repaired
    rule picks W2, which makes e1_main slow (hours).
  - A synthetic test checked the rule's three branches.
- **A5:** e3b's grind rate is now 60,901/s (bench, one core), plus
  EXPLORATORY `e3b_multicore.toml` at 16×.
- **Benches (this box, 1 core):**
  - keygen+shard 16.5 µs (≈ 60.5k IDs/s; a 16-core attacker ≈ 970k/s);
  - dual receipt 129 µs;
  - checkpoint cycle 126 µs;
  - transcript push+root 1.3 µs.
- **Lessons:**
  - Plants must change an observable. "dual = OR" changed no
    admissibility decision in F1 and passed unnoticed.
  - `/tmp` differs between Windows exes and msys python; use the
    scratchpad.
  - Backslash escapes do not survive the Bash-heredoc → Python-string
    path intact: a doubled backslash-n meant to stay literal is written
    to the target file as a real newline. Use the Edit tool (or
    `chr(92)`) for any text containing backslashes.
- **P7 plan:**
  1. `pwsh eval/run_all.ps1` (full; D5 about 13 min, dominated by W2).
  2. Read the D5 winner (A4 rule) from REPORT.md and record it as an
     Amendment (A6).
  3. Set `e1_main.toml` weights, re-run e1_main (4/8 arms use weights;
     about 45 min if W1, hours if W2).
  4. (done: A5)
  5. Read REPORT.md and report every verdict, including failures.

**P7 status (2026-09-27): DONE.**
- Commits:
  - b80ccf6 P7a: D5 = W2 (A6);
  - 9a15e99: `-text` for eval/results;
  - 5bc7702 P7b: cache-blocked W2, bit-identical, A7;
  - f29a8a2 P7c: results at 5bc7702, plus exploratory A8;
  - a21095d: A9 pre-registered (pushed BEFORE its run).
- **Verdicts** (`eval/results/5bc7702/REPORT.md`). Every CSV except
  e1-main is byte-identical to run 1.

  | Result | Verdict |
  |---|---|
  | F1-a | HOLDS 12/12 |
  | F1-b P1 | PARTIAL 5/6 |
  | F1-b P2, P3 | HOLD |
  | D5 | W2 |
  | H1 | PARTIAL 18/28 |
  | L1 cold start | occurs 35/35 |
  | L3 on-off | occurs 5/5 |
  | L2 | did not occur |
  | L4 | did not occur at α = 0.2 |
  | H2–H5 | HOLD |

  - H1 holds against B2 except at α = 0.3 under camouflage.
  - H1 fails against B1 under whitewash (≈1.04, both arms at the
    ε-exploration floor) and on-off (≈1.4).
  - It also fails under camouflage at α ≥ 0.2.
  - B1 collapses under slander and combined attacks (0.95), while Ours-L
    stays at ≈0.08. Ours-L+C is ≤ 0.15 everywhere.
- **A8 (exploratory):** W2 collapses under camouflage when Sybils
  outnumber honest requesters: 0.551 at α = 0.5, against random 0.500.
- **A9 (author's decision):** a pre-registered minimax re-test over α ×
  attacks gives worst cases W0 0.348, **W1 0.200**, W2 0.550. So the §8.4
  default is **W1**; D5 and H1 stay reported as run (with W2).
- **Lessons:**
  - A background shell dies with the Claude session. For hours-long runs,
    use a detached, hidden pwsh plus `SetThreadExecutionState`, launched
    from a clean worktree.
  - A single-α decision rule can hide a collapse at another α. Check the
    worst case.
  - The power iteration was memory-bound. Cache-blocking across observers
    kept it bit-identical and made it about 5–9× faster.

**P8 status (2026-09-27): DONE.**
- **Spec defaults:**
  - layer C-6: checkpoint interval c = 8;
  - §8.2: parameter table plus `ProjParams::recommended()` (cap 50).
    `default()` stays uncapped because F1 and the golden hashes use it;
  - §8.4: W1 default, with the A9 evidence; W2 is NOT RECOMMENDED where
    Sybils can outnumber honest requesters.
- **Discovery:**
  - §11.5 D-8: NOT RECOMMENDED. A4 is a stated limitation, because
    identities are free and a 1-core grind fills a shard in about 2 s.
  - §11.6 D-9: SHOULD, d = 3 (E3c). It is not in map-net yet.
- **Other:** §12.3/§13 rows; `spec/CHANGELOG.md` (v1 draft → layer map,
  vectors, P8 defaults); binding DHT-12/13; `checkpointIntervalParts` 8;
  provider bins default `--checkpoint-every 8`.
- **No wire change:** gen_vectors regenerates byte-identically.
- **P9 (paper) STARTED 2026-09-27: first full draft in `paper/`.**
  - Format: ACM acmart sigconf `nonacm`, about 12 pages projected.
  - **The PDF has NOT been compiled.** There is no TeX here: the author
    chose "source only", with the build on Overleaf via
    `paper/paper-overleaf.zip`. `paper/lint_tex.py` does static checks
    and was validated with planted errors.
  - **Prior art re-checked 2026-09-27.** Dual-signed receipts alone are
    NOT novel: Signet has BilateralReceipt, agent + MCP server co-signed.
    Adjacent work also includes:
    - Sello (arXiv 2606.04193), receiver-signed;
    - Agent Passport, ACTA (draft-farley-acta-signed-receipts-03);
    - Cairn (2609.19502), community decayed Beta;
    - MG-EigenTrust (2606.17368);
    - the ERC-8004 empirical study (2606.26028: 59–91% Sybil reviewers).

    The paper claims the *combination*. Every bib entry was checked
    against a primary source.
  - **Author TODOs** (listed in `paper/README.md`): the affiliation
    country; repo visibility (jesselarsen/map-rfc is PRIVATE, so the
    paper says the artifact "will be released" at tag `paper-v1`);
    D7 (arXiv category and endorser); compile and proofread.
  - The paper must report H1 as PARTIAL, L1/L3, the W2 camouflage
    collapse, the A9 deviation, and A4 as unmitigated.

## 1. What this project is

A standards-track-style specification family ("MAP") for a decentralized,
permissionless AI-agent network — evolving the Manolito/MP2P capability-
routing idea onto a modern stack — plus a Rust reference-implementation
scaffold. Produced from a critical evaluation of earlier design documents
(HEP v0.1 preprint and three design sketches); every rejected/adopted idea
is traceable in `map-spec/01-design-critique.md` (findings C1–C15 with a
disposition table).

Naming: the protocol is **MAP**; "Hermes" is reserved for an orchestrator
*implementation*. Protocol ≠ product.

## 2. Expected repository layout

```
./memory.md                  this file
./map-spec/                  the specification family (11 markdown docs)
./map-spec/test-vectors/     TV-001/TV-002 + gen_vectors.py (regenerator)
./map-rs/                    Rust workspace (unzip map-rs.zip here)
./map-go/                    Go module — impl #2 (vectors + session layer)
./interop/                   live Rust↔Go interop matrix: driver + MATRIX.md (TASK-020)
```

The two engineering-policy documents from the original project
(`designing-data-intensive-applications.md`,
`implementing-domain-driven-design.md`) are **binding** if present; their
protocol-specific distillation lives in `map-spec/draft-map-ops-01.md` §2
(bounded-context/ownership table + 7 rules). Changes violating that table
are rejected in review — that rule is itself in ops §2.

## 3. Non-negotiable protocol decisions (do not drift)

1. **Signatures cover received bytes** of deterministic CBOR (RFC 8949
   §4.2.1) via COSE_Sign1 (tag 18). Never re-serialize before verifying
   (core invariant I1). Protobuf is at most an optional bulk carrier.
2. **No self-reported trust** anywhere (critique C1). Trust = local,
   rebuildable projection over dual-signed ExecutionReceipts.
3. **Two-plane discovery**: stable signed descriptors in a sharded DHT;
   volatile load/TTFT pulled on demand via STATUS_QUERY. Latency is never
   advertised (C3/C6).
4. **Hard constraints are protocol; ranking is local policy** (C5).
5. **Zones 0–3** gate payload flow, normatively (arch §8.1; tighten-only on
   delegation).
6. **Payment negotiation is protocol; settlement is a method registry**
   (no native token/central credits — C9).
7. Envelope crypto facts: protected header `{1: -8 (EdDSA), 4: kid=AgentID}`;
   `Sig_structure = ["Signature1", protected, external_aad, payload]`;
   `external_aad` = channel binding in-session, empty out-of-session;
   `TH = SHA-256("MAPv1-transcript" || HELLO-env || HELLO_ACK-env)`;
   `CB = HKDF(salt=TH, ikm=transport-exporter, info="map cb", L=32)`.
8. AgentID = `0xED 0x01 || ed25519-pubkey` (34 bytes), multibase `z…`
   base58btc — byte-compatible with did:key and libp2p Ed25519 PeerIDs.
9. Receipt signatures: `Sign(enc(["MAPReceipt", role, core-bytes]))`,
   role 1 = requester, role 2 = provider (matches TV-002).
10. Transcript commitment (tasks §8.1, as patched):
    `leaf = H(0x00 || enc([seq, kind, content]))`,
    `parent = H(0x01 || l || r)`, odd node promoted,
    `empty = H(0x00 || enc([]))`; transcript kinds are 0/1/4.
11. Message-type code points: single source of truth is core §5, mirrored
    in `map-rs/crates/map-core/src/msg_type.rs`. Code points are immutable
    once assigned (ops §3.3); 0x0206 is deliberately unassigned.

## 4. Current state — specification family (**rev-01 cut 2026-07-13**)

All eight drafts are now `draft-map-*-01.md` (renamed from `-00`; every
in-repo reference — specs, map-rs/map-go comments, gen_vectors.py, formal/,
tasks, CLAUDE.md — swept to `-01`). The 2026-07-12 architect rulings are
folded into clean normative text (errata scaffolding removed from the
specs); the classified delta is in `map-spec/CHANGELOG.md` (ops §4 requires a
per-revision changelog: editorial / additive / breaking — no breaking items).
`ERRATA.md` retained as historical rationale. Frozen vector bytes unchanged;
both impls + the 16/16 interop matrix pass at rev-01. **When citing spec
sections use the short form (core §X, tasks §2.1) — version-agnostic; the
`draft-map-*-01` full name only appears in cross-references + frontmatter.**

All eleven documents written, cross-referenced, BCP-14 language:
critique, arch (goals/threats TH1–TH15/zones/scalability), identity,
core, discovery, tasks, trust, pay, ops, README, test vectors.

Patches already applied during implementation feedback (keep this loop —
it found real bugs):
- discovery §6: shard = first byte of the **digest** (the multihash's
  first byte is the constant codec prefix 0x12 — original text collapsed
  all agents into one shard).
- tasks §8.1: Merkle construction fully specified (see decision 10; the
  original `H(seq || kind || content)` concatenation was ambiguous).
- tasks §1 and discovery §5: explicit data-ownership paragraphs (DDIA
  policy).
- pay §3.1 (2026-07-04, from TASK-004 feedback): `PayQuoteRef` /
  `PayAgreeRef` / `PayRef` defined — refs are (quote-id, digest of
  received envelope bytes) pairs per I1; `PayRef` is a pre-quote pricing
  expectation. PayQuote gained optional key 9 (TaskID binding; MUST when
  quoting against an OFFER) closing quote replay across tasks. tasks §2
  comments now cite pay §3.1.
- discovery §7 (2026-07-05, from TASK-006): CAP_QUERY/CAP_RESPONSE content
  maps assigned — `CapQuery {1:CapURI, ?2:shard, ?3:limit}`,
  `CapResponse {1:[* descriptor-envelope bstr], ?2:more}` (envelopes inline,
  querier verifies each; batch ≤ 8). NOTE for the architect: TASK-006 FR-5
  wrote STATUS_QUERY as `{1: CapURI}` (single, required) but §3 already had
  `StatusQuery {?1: [+CapURI], ?2: echo}` (array, optional); map-mesh
  implemented §3's array form (draft-wins) — FR-5 text should be corrected.
- RFC 2119/8174 compliance pass (2026-07-05/06, architect-ruled, editorial-
  with-normative-effect): all seven remaining docs (core, discovery,
  identity, ops, pay, tasks, trust) gained the arch §2 boilerplate verbatim
  + a `## References (selected)` section citing `[RFC2119] [RFC8174]` —
  previously only arch-00 had them, the rest just pointed at it by
  reference. Capitalized every load-bearing lowercase MUST/MUST NOT/MAY
  found in a parallel construction next to an already-capitalized keyword
  or in a formal validation/registry/conformance list: core §9 (retryable
  MUST), discovery §2 (`seq` MUST exceed), identity §4 (validity-window/
  chain-root MUST ×2; REVOKE MAY), ops §2 rules 3+7 (idempotency/
  projections-rebuildable MAY/MUST), ops §3.1 registry table (settlement
  method MUST define...), ops §6.2 (7 instances — the entire "Required
  negative tests" list, incl. the I1 re-serialization test), tasks §2.2/§9
  (bidding-broadcast MAY, zone tighten-only MAY), trust §5/§8 (receipt-
  identity MUST match, holder-answers MAY, rebuild-write-path MAY).
  Prose-only: `gen_vectors.py` regenerated byte-identical (idempotent),
  full workspace suite green — no code/vector drift.
- tasks §2/§11 (2026-07-06, from TASK-007): Accept/Reject content maps
  (`Accept = {}`, `Reject = {1 code}`) and CANCEL field numbers
  (`{1 TaskID, 2 code}`) recorded — the negotiation diagram/§11 had shown
  them only inline (editorial/additive assignment FR-1 makes).
- **`map-spec/ERRATA.md` created (2026-07-05, PENDING-ARCHITECT queue).** Two
  TASK-007 items awaiting a future architect: (a) tasks §2 Quote key 2
  (PayQuoteRef) is draft-REQUIRED but the echo runtime omits it (propose
  `?2`); (b) frozen TV-007 ASSIGN uses `InputSpec {2:[multihash]}` but the
  codec models inline (key 1) only → TV-007 is Rust-consume-only, a
  frozen-vector parity hole for TASK-015. Handoff protocol: wire-affecting
  items go to ERRATA, not unilateral normative edits.
- pay §5.1 (2026-07-06, from TASK-009): `pay:credit-ledger` method defined
  (operator-signed entry format `enc(["MAPLedgerEntry",…])`, proof payload
  `[entry-bytes, sig]`, verify_strict verification, instant/irreversible
  finality). ERRATA gained three TASK-009 items: content-vs-envelope digest
  binding (§3.1 wants envelope bytes; session doesn't expose them → content-
  bind, price-switch defense intact); no distinct 6xxx codes for cap-exceeded/
  digest-mismatch/binding-mismatch/bad-proof (interim 6002); PayInvoice.4 usage
  watermark shape unspecified (omitted, postpaid flat).
- trust §3 (2026-07-06, from TASK-008): countersignature tolerance assigned
  (usage ±5% of the OBSERVED value min ±1, timestamps ±2 s; band anchored to
  the trusted side — farming defense) + the task-terminal→outcome mapping
  table (Verified→0, Failed→1, TTFT-cancel→3). ERRATA gained two TASK-008
  items: (c) no core §9 code for a tolerance-rejected RECEIPT (interim 5000,
  propose a dedicated code); (d) trust §2 diagram shows `RECEIPT_ACK {sig_prov}`
  but the impl returns the full dual receipt (propose diagram fix).

Honest rev-00 gaps (spec side): extensions bid/kvx/git now have code points
(ext keys 1/2/3, TASK-011 — bid is full, kvx/git are reserved stubs w/ container
+ zone rule, full semantics deferred); delegation `crit`/`ext`
processing thinly exampled (delegation CERTS + chain verify now implemented,
TASK-010 — two caveats task-pin/audience, deeper caveats e.g. budget still
future; revocation distribution still absent); reservations (tasks §6) now
normative (CDDL+FSM+MUSTs, codecs only — no runtime yet, TASK-011); formal
analysis (ops §6.3) delivered for P1–P3 (TASK-014); two-independent-
implementations rule (ops §6.1): impl #1 = Rust scaffold, impl #2 = `map-go/`
at frozen-vector parity (TASK-015, 2026-07-11) — the LIVE pairwise interop
matrix is still unmet, so no document leaves draft yet.

## 5. Current state — Rust workspace (`map-rs/`) — **BUILDS + TESTS GREEN**

**TASK-017 DONE 2026-07-12** (implements the architect rulings of §7 step
14): `map-session` `Session::send` now returns the SEALED envelope bytes and
`Incoming` carries `raw: Vec<u8>` (the exact received frame, I1). Payment
digests re-bound to envelope bytes — requester digests `inc.raw`; provider
calls `ProviderPayment::bind_quote_envelope(sealed)` with what `send`
returned (the pump feeds it back; session-less unit tests use the `synth_env`
stand-in in tests.rs). This fixes an interop BUG: the old content-re-encoding
digest never matched frozen TV-013 or map-go. Pay/receipt reject codes split
per core §9: 2006 countersign-refused (`RECEIPT_REJECT_CODE`, was 5000),
6005 cap-exceeded, 6006 agreement-digest-mismatch (price switch), 6007
task-binding-mismatch, 6008 invalid-payment-proof; 6002 stays generic. New
test `payagree_binds_received_envelope_bytes_not_content` (FR-6 b/d) proves
envelope≠content digest. Whole workspace green, zero warnings.

TASK-001 (2026-07-03) took the workspace from never-compiled to
`cargo build --workspace` + `cargo test --workspace` green, zero warnings.
The six pre-existing crates needed **no code changes** — the "small fix loop"
was empty; all their unit tests passed unmodified. Build toolchain note: this
machine has no MSVC C++ build tools installed, so the default
`stable-x86_64-pc-windows-msvc` fails at link (`link.exe` not found). Build/test
with the installed GNU toolchain: `cargo +stable-x86_64-pc-windows-gnu build`.
(Also, run cargo from PowerShell, not the Git-Bash shell — Bash's coreutils
`link` shadows any MSVC linker.)

**CRITICAL toolchain fact (TASK-012, 2026-07-08):** since `map-net` joined the
workspace, building/testing the **whole workspace** requires the mingw-w64
toolchain on PATH — `map-net` pulls libp2p → `windows-sys` (needs `dlltool.exe`
to make Windows import libs on the -gnu target) and `ring` (needs `gcc` for
quic). A complete MSYS2 mingw64 is already installed at **`C:\msys64\mingw64\bin`**
(gcc 16.1, binutils 2.46) but is NOT on PATH by default. Every build/test
command must prepend it, e.g. in PowerShell:
`$env:PATH = "C:\msys64\mingw64\bin;$env:PATH"; cargo +stable-x86_64-pc-windows-gnu test --workspace`.
Without it you get `error calling dlltool 'dlltool.exe': program not found` on
`parking_lot_core`/`windows-sys`. The pure-Rust crates (everything except
map-net) still build without mingw.

| Crate | Status | Contents |
|---|---|---|
| crates/map-identity | written + unit tests | AgentId (validated ctor, multibase, did:key), Keypair, verify (verify_strict) |
| crates/map-core | written + unit tests; +delegation (TASK-010) | cbor.rs (canonical encoder + strict profile decoder), envelope.rs (seal/open, I1, channel-binding via external_aad; TASK-010: `seal_delegated`/`open_delegated` key 33 — `open()` byte-frozen), hello.rs (negotiate, TH, CB), replay.rs (ReplayCache ±120 s / 10 min, SeqTracker), msg_type.rs, errors.rs (codes 1000–6004 + ErrorMsg). `delegation.rs` (TASK-010): DelegateBody codec, issue/validate_cert/validate_chain→EffectiveScope, Caveat task-pin/audience, tighten-only + fail-closed, chain cap 8 |
| crates/map-discovery | written + unit tests | CapURI validation, cap_matches (prefix-segment), multihash_sha256, shard (digest byte), capability keys, minimal Descriptor (fields 1–6 + 12), DescriptorCache (seq-LWW) |
| crates/map-tasks | written + unit tests; +msg codecs (TASK-007); +reservations (TASK-011) | TaskId, ChunkKind, TranscriptBuilder (patched Merkle), FlowAccount (dual-unit credits), requester state machine (ReqState/step). `msg` module: pure to_value/from_value for Offer/Quote/Assign/Accept/Reject/ChunkMsg/Complete/Cancel/FlowCredit + TaskConstraints/InputSpec/FlowInit/Usage. TASK-011: `Reserve`/`Lease`/`LeaseRelease` codecs (tasks §6) + `Lease::granted_window_within`. Complete key 3 = multihash(root); pay refs modeled `Option<Value>`, omitted (ERRATA); InputSpec inline-only |
| crates/map-trust | written + unit tests; +lifecycle (TASK-008) | ReceiptCore/Receipt (TV-002), role signing, idempotent ReceiptStore, TrustProjection (Beta, rebuild==incremental). TASK-008: `Receipt::to_value/from_value` (message content; encode/decode_bytes frozen-wire untouched) + `lifecycle::within_tolerance` (usage ±5% of OBSERVED value, ts ±2 s — anchored to the trusted side, farming defense) |
| crates/map-pay | written + unit tests; +codec/ledger (TASK-009) | Amount (tag-4), Asset, InvoiceId/QuoteId, SettlementLedger. TASK-009: `codec` (PayQuote incl. ?9 task-binding, PayAgree, Pay{Quote,Agree}Ref, PayRef, PayInvoice/Proof/Receipt, MethodOffer, AssetAmount, `pay_ref_digest`) + `credit_ledger` (LedgerOperator: signed entry `enc(["MAPLedgerEntry",…])`, idempotent `pay`, `verify_entry` verify_strict). deps += identity/sha2 |
| crates/map-session | done (TASK-005; HelloAck refactor TASK-006; delegation FR-5 TASK-010) + 21 tests | in-memory session layer: HELLO/HELLO_ACK handshake, TH/CB, core §12 state machine, in-session send/recv with seq+replay+size enforcement, PING/PONG/CLOSE. `Transport`/`Clock`/`MsgIdSource` traits + `InMemoryDuplex` (mpsc). Uses `map_core::HelloAck`. TASK-010: `SessionConfig::new()` + optional `delegation_chain` (handshake carries key 33 via seal_delegated), `accountable_peer()` records the chain ROOT (leaf still = transport-auth peer); bad chain → 2003. Deps: identity/core only. Handshake=empty aad, in-session=CB. Real libp2p exporter still future |
| crates/map-taskrun | done (TASK-007/008/009) + 40 tests | pump-driven runtimes: task (Provider/RequesterTask), receipts.rs + evidence.rs (TASK-008: dual+provider==subject gate), payments.rs (TASK-009: `ProviderPayment`/`RequesterPayment` — PAY_QUOTE(task-bound)→AGREE→INVOICE→pay via LedgerOperator→PROOF→verify+settle_once→RECEIPT; cap/digest/binding/forged-proof defenses; retries move money once). Deps: identity/core/session/tasks/trust/pay (no mesh) |
| crates/map-mesh | done (TASK-006) + 12 tests | discovery flows over sessions: `publish_descriptor`/`ingest_descriptor_envelope` (self-signed, ttl, seq-LWW), `MeshIndex` (in-process DHT stand-in; order-independent ingest ⇒ `rebuild==incremental`), CAP_QUERY/CAP_RESPONSE + STATUS over a live session (`serve_one`/`query_caps`/`query_status`). Index is untrusted transport — client verifies every envelope. Deps: identity/core/session/discovery (no tasks/trust/pay) |
| crates/map-cli | done (TASK-001; +serve-ollama TASK-013) + 6 unit tests | `keygen`, `projection-rebuild-demo`; hand-rolled argv. TASK-013: `serve-ollama --model --prompt [--url --max-tokens]` (feature `real-backend`) — single-process demo: real Ollama call → prints output + transcript root + dual-signed receipt (needs live server). deps: identity/tasks/trust/sha2/hex/getrandom + optional ollama-provider |
| crates/map-net | done (TASK-012, 2026-07-08) + 4 unit tests + 2 gated net tests | **First real network transport.** `framing` (u32-BE length prefix, size-first), `Libp2pTransport` (blocking `map_session::Transport` bridged over tokio mpsc), `MapNode` (blocking API over a background tokio swarm: listen/dial/accept/publish_descriptor/find_providers/fetch_descriptor). Behaviour = libp2p-stream + kad(MemoryStore) + identify. PeerID≡AgentID asserted (FR-3); dial uses DialOpts(peer-id) so libp2p enforces the expected peer. Exporter = init-AID‖resp-AID (FR-4, core §10). Deps: libp2p 0.54.1 (tcp+quic+noise+yamux+kad+identify+dns+ed25519), **libp2p-stream 0.2.0-alpha** (raw substreams; umbrella has no `stream` feature — separate alpha crate), futures 0.3, tokio 1. Net tests behind `net-tests` feature + `#[ignore]` (2-node loopback handshake/echo/kad + dial-wrong-peer, both PASS). NOT-done (deferred, noted in report): map-cli `node` subcommands; anti-relay-across-two-connections + poisoned-kad-record network tests; NAT/relay/eclipse (all future) |
| adapters/ollama-provider | done (TASK-002; real backend TASK-013 2026-07-08) + 13 tests (feature) | `InferenceBackend` trait + deterministic `MockBackend`, `result_to_chunks`/`run_task` glue. TASK-013: `OllamaExecutor<B>` (adapts any backend to `map_taskrun::Executor`; input=prompt UTF-8, errors/invalid→empty output never panic) always-on; **`OllamaBackend`** (real HTTP `/api/generate` stream:false, FR-2 field map, FR-3 errors, no prompt in error strings) behind feature **`real-backend`** (default OFF; deps `ureq` default-features-off + `serde_json`, NOT in default tree). `BackendError::Unavailable` now carries a msg (FR-10). Canned-JSON unit tests; live test `#[ignore]`. `Unavailable`→`Unavailable(String)` |
| adapters/gateway | done (TASK-002; running gateway TASK-016 2026-07-08) + 15 tests | provenance primitives (TASK-002): `GatewayAdvert`/`label_descriptor` (G-1), `effective_zone` (G-3), `valid_namespace` (G-4). TASK-016: **`GatewayProvider<B>`** — a running gateway that IS a `map_taskrun::Executor` (composes `OllamaExecutor`, fronts any `InferenceBackend`) + `admit(cap,zone,now)` gate: FR-5 scope (its own `EffectiveScope::permits` from a delegation chain) → FR-2/G-3 zone (`effective_zone > task_zone` → reject); `GatewayReject{Zone→2004,Scope→2005,BadChain→2003}`. `build_gateway_descriptor` (subject=gateway, G-1/FR-4). FR-6 E2E (task→chunks→COMPLETE→dual-signed receipt naming the gateway) via shuttle. deps: +map-core/tasks/taskrun/ollama-provider (acyclic; InferenceBackend reuse). Discovery §4 patch: gateway/ params key 1=gateway-of (codec doesn't model params yet → GatewayAdvert interim) |
| conformance/map-conformance | done (TASK-003/004; +bound-quote, delegation, spec-round-3 2026-07-08) + 38 cases (+run_all) | reproduces TV-001..015 + NV-001..008 **byte-for-byte** (adds TV-010b bound quote, TV-011 cert, TV-012 delegated OFFER, TV-013 PAY_AGREE, TV-014 RESERVE, TV-015 LEASE; NV-006 task-binding, NV-007 widened scope, NV-008 out-of-window lease); gained a map-pay dep (real PayQuote/PayAgree) + uses map_core delegation + map_tasks Reserve/Lease; ops §6.2 negatives; `run_all() -> Report`. TV-003 upgraded (TASK-006) to a HelloAck PRODUCE-guard (from_value→to_value must reproduce the vector bytes — the tripwire for any HELLO_ACK codec regression), same discipline as the DESCRIPTOR round-trip. Encoder needed NO fix |

**Build-breaking fact (RESOLVED by TASK-001):** the workspace `Cargo.toml`
lists all members (map-session added by TASK-005, map-net by TASK-012); all
exist and build (map-net only with mingw on PATH — see the CRITICAL note above).

Spec-feedback from TASK-005: RESOLVED 2026-07-04. (a) HELLO_ACK — core
§6.1 already had singleton HelloAck CDDL; the gap is code-side (map-core
has no HelloAck type; map-session hand-rolls the codec). A first-class
`HelloAck` in map-core is folded into TASK-006's prep work. **DONE
(TASK-006): `map_core::hello::HelloAck` added (to_value/from_value + `as_hello`
+ `Negotiated::to_ack`); map-session's private codec deleted; TV-003 bytes
unchanged (now a produce-guard).** (b) core §6.1
now mandates the responder MUST echo the initiator's SessionID (Body key
6); mismatch = 1006 + close — matching the implementation. (c) core §6.3
now says receivers SHOULD check the raw frame size before any decode/
signature work; TASK-005 doc carries an erratum fixing its FR-7 ordering
text (implemented order size→open→sender→replay→seq is correct).

Workspace deps pinned: ed25519-dalek 2, sha2 0.10, hkdf 0.12, bs58 0.5,
getrandom 0.2, hex 0.4. Edition 2021. No async/network deps yet by design.

Known code caveats:
- `envelope::open()` verifies only direct kid signatures (byte-frozen); ProtHdr
  key 33 delegation chains are handled by `open_delegated` (TASK-010), which
  layers chain validation on top of `open()`.
- Body keys 8/9 (crit/ext) are not modeled; byte-exact preservation for
  store-and-forward (invariant I3) is achieved by keeping `Opened.payload`
  bytes, not by re-encoding — preserve that pattern.
- Descriptor omits optional keys 7–11/13/14 (models, hardware buckets,
  pricing, security profile, memory topics).
- ReplayCache purges via O(n) retain per call — fine for scaffold.

## 6. Test vectors (ground truth for conformance)

**Counter at NV-009 (TASK-018, 2026-07-12).** NV-009 = a 2-link delegation
chain whose PARENT declares max-depth 0 yet DK delegates further; identical
scopes isolate the identity §4 depth bound → BAD_DELEGATION (2003). Added
additively to gen_vectors.py (TV-001..015 + NV-001..008 frozen bytes
untouched — the script's in-line asserts + a git diff of only-additions are
the guard). **TV-007 is now full produce+consume parity in BOTH impls**: the
Rust map-tasks `InputSpec` codec models keys 1/2/3 (≥1 required), and the
Rust conformance TV-007 case round-trips the frozen ASSIGN byte-exact — this
CLOSES the long-standing ERRATA 2026-07-05 parity hole (Rust was
consume-only).

**Counter at TV-016/NV-010 (TASK-021, 2026-07-14).** TV-016 = a metered
PAY_INVOICE (0x0402, Key B) carrying a UsageWatermark; model-1 rates
3000/5000 µUSD over usage {1523,812,431,42000} bill 8_629_000 µUSD (the
first invoice, so amount==cumulative-billed). NV-010 = the same invoice with
cumulative-billed 8_629_001 (off by one) → INVOICE_WATERMARK_VIOLATION
(6009); differs from TV-016 by only the msg-id, one mantissa nibble, and the
signature (isolating the pay §4.1 arithmetic check, NV-001-style). Both 301
bytes; added additively (frozen TV-001..015 + NV-001..009 untouched). Rust
conformance = **41 cases**; map-go = **27 vectors** + an independent
`billedModel1` arithmetic test (written from pay §4.1, not ported). Next NV
is NV-011; next TV is TV-017.

File: `map-spec/test-vectors/map-test-vectors.md`. Regenerate with
`python3 gen_vectors.py` (run from its directory; needs
`pip install pynacl cbor2 base58`; writes UTF-8 explicitly — don't drop
`encoding="utf-8"` or Windows cp1252 mangles the em-dashes). The script
self-tests (verifies every signature + a TV-001/TV-002 regression assert)
and is idempotent. Facts the conformance crate needs:

TASK-004 status: expanded to TV-003..TV-010 (HELLO_ACK/ERROR/DESCRIPTOR/
OFFER/ASSIGN/CHUNK/COMPLETE/PAY_QUOTE — one signed envelope per family),
NV-001..005 (MUST-reject bytes with core §9 class), and a Derived-constants
section (TH, CB, shard(B)=141, DHT keys, transcript root). The Rust
conformance crate consumes/rejects all of them (`extended.rs`, `vectors2.rs`;
29 cases total, all green). **Field-number gaps: RESOLVED 2026-07-04.**
TASK-004 §2's TV-004/005/006/010 layouts contradicted the drafts; the
vectors followed the drafts, and the task doc now carries errata blocks
correcting each item. The genuine draft gaps are patched: pay §3.1 defines
`PayQuoteRef`/`PayAgreeRef`/`PayRef`, and PayQuote gained optional key 9
(TaskID binding). TV-010 remains a valid *unbound standing quote*; a bound
variant is future vector work. TV-001..TV-010/NV-001..005 are now safe to
treat as the frozen interop contract.

- Seeds: `seed_A = SHA-256("MAP-TEST-VECTOR-KEY-A")`,
  `seed_B = SHA-256("MAP-TEST-VECTOR-KEY-B")` — derive in Rust, then
  assert against the hex in the vectors file.
- TV-001: HELLO envelope, 166 bytes. Body: version 0x0100, msg_type
  0x0001, msg_id `000102030405060708090a0b0c0d0e0f`, timestamp
  1780000000000, content Hello{versions:[0x0100], suites:[1],
  extensions:{}, max_message_size:262144}, external_aad empty.
- TV-002: dual-signed receipt, 327 bytes. Core: task = 32-byte constant
  (BLAKE2b-256 of "example-task" — **hardcode the hex from the vectors
  file; do not add a blake2 dep**), cap
  "map:cap:inference/text-generation", started 1780000000000, ended
  1780000042000, outcome 0, metrics {1523, 812, 431, 42000}, rating 92.
- Python used cbor2 `canonical=True` (RFC 7049 ordering); all keys here
  are small uints where it coincides with RFC 8949 bytewise ordering —
  the Rust encoder is the RFC 8949 one and must reproduce the bytes.

## 7. Immediate next steps (in order)

> 2026-07-03: steps 2–4 are now packaged as implementation specifications
> in `tasks/` (TASK-001..004; see `tasks/README.md`). Split-role model:
> the architect authors packages, coding LLMs execute the §17 prompts,
> reviewers validate per §18. Do not implement ahead of a package.

1. Ensure toolchain: `rustup default stable` (but see §5 — this box builds
   only with the GNU toolchain; no MSVC C++ tools installed).
2. ~~Create the four missing members.~~ DONE (TASK-001): map-cli implemented,
   three stubs created.
3. ~~`cargo build/test --workspace` until green.~~ DONE (TASK-001): green,
   zero warnings, DDIA rebuild test passes. TASK-002 (adapters) and TASK-003
   (conformance) can now run in parallel.
4. ~~Conformance crate: reproduce TV-001/TV-002 byte-for-byte + ops §6.2
   negatives.~~ DONE (TASK-003): 16 tests green; the RFC 8949 encoder matched
   the cbor2-canonical vectors on the first run (no encoder fix needed),
   confirming the R2 ordering note below. ops §6.2 minimum negative suite for
   impl #1 is satisfied.
5. ~~Package/commit; note in ops §8 that the scaffold exists. TASK-004
   (test-vector expansion).~~ DONE (TASK-004, 2026-07-04): TV-003..010 +
   NV-001..005 + derived constants; `gen_vectors.py` extended, conformance
   crate mirrors it in `extended.rs`/`vectors2.rs` (29 cases green). All four
   packages TASK-001..004 committed/pushed to `master`.
6. ~~**Next package (architect-suggested, 2026-07-05) — bound PAY_QUOTE
   coverage.**~~ DONE (2026-07-08): added a *bound* **TV-010b** (PAY_QUOTE with
   key 9 = the TV-006 OFFER's TaskID, pay §3.1 MUST) + **NV-006** (key 9 present
   but mismatched → §3.1 policy REJECT, cf. NV-004 — envelope is valid and OPENS;
   the binding check rejects). Both are additive: frozen TV-001..010 / NV-001..005
   bytes are byte-for-byte undisturbed (markdown diff = 33 insertions, 0 deletions;
   gen_vectors.py re-verified idempotent). Conformance mirrors them via the REAL
   `map_pay::codec::PayQuote` (new dep on map-pay) — `case_tv010b_pay_quote_bound`
   asserts `task_binding == Some(offer_task_id)`, `case_nv006_task_binding_mismatch`
   asserts the §3.1 reject, exactly as `map-taskrun::payments` does. run_all now
   31 cases (was 29); workspace green, zero warnings. COMMITTED + PUSHED to `main`
   as f99f83c (2026-07-08; the commit also folded in the README license-line edit
   MIT OR Apache-2.0 → Apache-2.0). Files:
   `map-spec/test-vectors/gen_vectors.py` + regenerated `map-test-vectors.md`;
   `map-rs/conformance/map-conformance/{Cargo.toml,src/vectors2.rs,src/extended.rs,src/lib.rs}`.

> NOTE (2026-07-08): the repo default branch was renamed `master` → `main`
> (origin/HEAD → origin/main; remote `master` deleted). Any `master` references
> elsewhere in this file are historical.

7. ~~**TASK-010 — delegation certificates + envelope chain verification.**~~ DONE
   (2026-07-08). `map-core::delegation` (NEW module — hosted in map-core, NOT
   map-identity as TASK-010 §4 suggested, because a cert IS an envelope and
   map-identity has no CBOR/envelope dep; hosting it there would invert the dep
   graph, NFR §3. Wire format is spec-exact; only the host crate differs):
   `DelegateBody` codec, `issue`/`validate_cert`, `validate_chain` →
   `EffectiveScope{root,caps,zones,depth_left,task_pin}`, `Caveat`
   (task-pin=1/audience=2), tighten-only attenuation, fail-closed unknown
   caveats, chain cap 8. `envelope::open_delegated` (key 33; verifies LEAF sig,
   returns scope+root; `open()` untouched/byte-frozen) + `seal_delegated`.
   map-session FR-5: `SessionConfig::new()` + `delegation_chain` field +
   `Session.accountable_peer()` (records the root when the peer delegates; leaf
   still satisfies transport-auth). Vectors TV-011 (cert)/TV-012 (delegated
   OFFER, chain in key 33)/**NV-007** (widened scope → 2003); conformance 31→34
   cases; gen_vectors idempotent, TV-001..010b frozen (57 additive md lines).
   Spec patches: identity §4 (caveat registry, fail-closed, cap 8 + editorial
   note) and §11 (caveat table). **ERRATA:** identity-§4 "chain root MUST equal
   kid" clause contradicts the crypto (leaf signs) — logged PENDING-ARCHITECT;
   impl follows kid==leaf-subject, root returned as accountable. NV numbering:
   TASK-010's planned "NV-006" became **NV-007** (NV-006 taken by the bound-quote
   pass); TASK-011 should continue from NV-008. Whole workspace green, zero warns.

8. ~~**TASK-011 — spec extensions + reservations + vector round 3.**~~ DONE
   (2026-07-08). Registry (FINAL): msg-types RESERVE 0x020F / LEASE 0x0210 /
   LEASE_RELEASE 0x0211 (core §5 + msg_type.rs); extension keys 1=ext-bid /
   2=ext-kvx / 3=ext-git (new core §5.1 Extension Keys registry). Spec: tasks §2.2
   ext-bid FULL (BidExt {1 bid-deadline, 2 queue-wait-ms}; needed a new
   `Offer.?6 => ExtMap` — additive), §6 reservations normative (Reserve/Lease/
   LeaseRelease CDDL + FSM Requested→Granted→Active→Released/Expired/Revoked +
   MUSTs: rsv-id idempotency, granted-window⊆requested, missed-lease=outcome-1 +
   squatting/PayRef Security), §8.4 ext-kvx & §9 ext-git STUBS (container
   {1 version,2 bstr} + zone rule + "deferred"). Code: map-tasks `Reserve`/`Lease`/
   `LeaseRelease` codecs (+`granted_window_within`); msg_type consts+name().
   Vectors: TV-013 PAY_AGREE (binds TV-010b envelope digest), TV-014 RESERVE,
   TV-015 LEASE, NV-008 out-of-window lease; conformance 34→38. **Renumbering
   (per check-first rule):** the package's planned TV-013 bound-quote & NV-007
   mismatch ALREADY existed as TV-010b/NV-006 (skipped); its TV-014/15/16→our
   TV-013/14/15, its NV-008 stays NV-008. TV-001..012/NV-001..007 frozen (70
   additive md lines, idempotent). Workspace green, zero warnings. Remaining
   READY packages: TASK-012 (libp2p), TASK-013 (real Ollama); TASK-014/016 need
   their deps; rev-01 doc cut now possible (ops §4, additive changelog).

9. ~~**TASK-012 — libp2p transport `/map/1.0.0`.**~~ DONE (2026-07-08). New crate
   `crates/map-net` (see §5 table). Real two-node loopback proven: TCP handshake +
   in-session echo + Kademlia publish/find/fetch/re-verify, and FR-3 dial-wrong-
   peer refusal — both gated net tests PASS. Core §10 gained the `/map/1.0.0`
   channel-binding ruling (FR-4: exporter = init-AID‖resp-AID; residual noted).
   **Deps decided (FR-1 allowed "latest 0.5x"):** libp2p **0.54.1** + libp2p-stream
   **0.2.0-alpha** (raw substreams — the umbrella crate has NO `stream` feature at
   any 0.5x; libp2p-stream ships only as alpha) + futures 0.3 + tokio 1.
   **Signature deviation:** `MapNode::new(seed, hello)` takes a 32-byte seed, not a
   `Keypair`, because map-identity deliberately doesn't export its secret; both the
   MAP and libp2p keypairs derive from that seed (FR-3). **Build now needs mingw on
   PATH** (see §5 CRITICAL note). NOT-done (deferred, in report): map-cli `node`
   subcommands; anti-relay-across-two-connections + poisoned-kad-record net tests;
   NAT/relay/DCUtR + anti-eclipse (explicitly out of scope, future package).

10. ~~**TASK-013 — real Ollama provider adapter.**~~ DONE (2026-07-08).
    `ollama-provider` gained `OllamaExecutor<B>` (any backend → `map_taskrun::Executor`)
    always-on, and `OllamaBackend` (HTTP `/api/generate` stream:false) behind feature
    `real-backend` (default OFF — `ureq` default-features-off + `serde_json` stay out
    of the default tree, verified `cargo tree`=0). FR-2 field map + FR-3 error classes
    + no-prompt-in-errors (enforced by a test). `map-cli serve-ollama` demo (same
    feature) prints output+root+dual-signed receipt. **Decisions:** enum
    `Unavailable`→`Unavailable(String)` (only def referenced it) to carry FR-10 elapsed;
    used `send_string`+manual serde_json rather than ureq's `json` feature (keeps tree
    minimal, no TLS); Executor is infallible so backend errors/invalid-UTF8 degrade to
    empty output (a fallible Executor to surface COMPLETE outcome=1 is a noted follow-up);
    chose the single-process InMemoryDuplex-free demo (TASK-012 merged, but a networked
    `serve` needs a separate client to show the full loop). Live test `#[ignore]`.
    Default + feature builds both zero-warning; full workspace green.

11. ~~**TASK-016 — federation gateway (running).**~~ DONE (2026-07-08) — **the last
    package; memory.md §8 roadmap is now COMPLETE.** `adapters/gateway` gained
    `GatewayProvider<B>` (a `map_taskrun::Executor` that fronts any `InferenceBackend`,
    composing `OllamaExecutor`) + `admit()` gate enforcing FR-5 scope (own EffectiveScope
    from a delegation chain) then FR-2/G-3 zone (tighten-only). `build_gateway_descriptor`
    keeps the subject = gateway (G-1/FR-4). 15 tests incl. FR-6 E2E (task→receipt naming
    the gateway) + zone matrix + FR-5 scope reject/admit + bad-chain. Discovery §4 spec
    patch: gateway/ params key 1 = gateway-of (FINAL). **Decisions:** gateway deps
    ollama-provider for the `InferenceBackend` trait (acyclic; a shared `map-backend`
    crate is the noted cleanup); reject codes use core §9's precise auth codes
    (2004/2005/2003) not FR-2's imprecise "3xxx"; CapEntry params aren't in the codec so
    provenance rides the GatewayAdvert wrapper (spec assigns the key for later). Whole
    workspace green (30 suites), zero warnings.

## 8. Roadmap after the scaffold is green (ops §8 MVP ordering)

1. **Session layer**: ~~HELLO/HELLO_ACK over an in-memory duplex first~~ DONE
   (TASK-005): `crates/map-session` handshake + state machine + seq/replay/size
   over `InMemoryDuplex`, 21 tests green. ~~REMAINING: libp2p `/map/1.0.0`
   binding.~~ DONE (TASK-012): `crates/map-net` — `Libp2pTransport` behind the
   `Transport` seam, PeerID≡AgentID (FR-3), channel binding = init‖resp AgentIDs
   (FR-4, core §10). Two-node loopback validated.
> 2026-07-04: EVERY remaining roadmap item below now has an authored
> implementation-spec package — TASK-006..TASK-016 in `tasks/` (see
> tasks/README.md for the dependency graph and recommended order:
> 006 → 007 → 008&009 ∥ → 012&013 ∥, 010/011 anytime, then 014/015,
> 016 last). The architect departs 2026-07-06; tasks/README.md
> "Operating without the architect" defines the PENDING-ARCHITECT
> protocol (map-spec/ERRATA.md) for anything the packages don't
> pre-decide. Mapping: step 1-remaining(libp2p)=TASK-012 ·
> step 2=TASK-006 · step 3=TASK-007 · step 4=TASK-008 ·
> step 5=TASK-009 · step 6: delegation=TASK-010, Ollama=TASK-013,
> ext code points+reservations=TASK-011, formal=TASK-014,
> Go=TASK-015, gateway=TASK-016.

2. **Descriptor publish/fetch**: ~~envelope-wrapped descriptors, provider
   records under sharded capability keys, republish ≤6 h.~~ DONE (TASK-006):
   `crates/map-mesh` — publish/ingest, `MeshIndex` (DHT stand-in), CAP_QUERY +
   STATUS over sessions, 12 tests. `map_core::HelloAck` landed (map-session
   refactored off its private codec). ~~REMAINING: real Kademlia replaces
   `MeshIndex`.~~ DONE (TASK-012): `map-net::MapNode` provides kad-backed
   `publish_descriptor`/`find_providers`/`fetch_descriptor` (libp2p-kad,
   MemoryStore). Disjoint-path lookups + anti-eclipse still future (out of
   TASK-012 scope).
3. **Echo task end-to-end**: ~~ASSIGN → CHUNK stream under FlowAccount →
   COMPLETE with transcript root; TTFT ×1.1 cancel rule.~~ DONE (TASK-007):
   `map-tasks::msg` codecs + `crates/map-taskrun` (pump-driven provider/
   requester runtimes), 19 tests incl. live-session E2E, tamper→5000,
   starved-not-overrun. TASK-008 consumes `ReceiptInputs`; TASK-009 wires the
   pay-ref codec fields (already present as `Option`).
4. **Receipts**: ~~dual-sign after verified COMPLETE; EVIDENCE_QUERY/SET.~~
   DONE (TASK-008): receipt lifecycle (requester issue → provider countersign
   → ack) + pull evidence over sessions, in `map-taskrun` (receipts/evidence)
   + `map-trust::lifecycle`. The trust loop is closed: work → dual-signed
   evidence → local score. Two enforcement gates proven (single-signed and
   wrong-provider receipts never reach a projection).
5. **Payments last**: ~~`pay:credit-ledger` as the first settlement method.~~
   DONE (TASK-009): postpaid flat happy path (PAY_QUOTE→AGREE→INVOICE→PROOF→
   RECEIPT) + `LedgerOperator` in `map-pay`, flows in `map-taskrun::payments`,
   11 tests (incl. §11 security: forged entry, proof replay, digest switch,
   cap exceeded, task-binding). Metered terms-3 / escrow / disputes future.
6. Then: ~~DelegationCert implementation + envelope chain verification~~ DONE
   (TASK-010); ~~Ollama adapter for real~~ DONE (TASK-013); ~~extension code
   points (BID/KVX/GIT)~~ DONE (TASK-011); ~~formal models (ProVerif) of HELLO
   + channel binding + delegation attenuation~~ DONE (TASK-014, 2026-07-10 —
   see §7 step 12 below); ~~Go implementation (impl #2 for the two-impl rule,
   TASK-015)~~ DONE (2026-07-11 — see step 13 below); ~~federation gateway~~
   DONE (TASK-016). **The ops §8 roadmap is COMPLETE.**

13. ~~**TASK-015 — Go implementation (impl #2), ops §6.1.**~~ DONE
    (2026-07-11). New top-level `map-go/` Go module (go1.26.5; requires
    ≥1.24 for stdlib `crypto/hkdf` — ZERO external deps, x/crypto not even
    needed). Packages: `cbor` (hand-rolled RFC 8949 §4.2.1 deterministic
    encoder + strict decoder, fuzzed 60s/25M execs clean — the fuzzer DID
    catch and fix one pre-allocation panic on huge length heads), `identity`
    (AgentID + in-package base58 + `VerifyStrict`: canonical A/R/S +
    small-order blacklist, FR-4 tests incl. non-canonical y=p rejection),
    `envelope` (COSE_Sign1 Seal/Open over received bytes, delegation-chain
    validation), `hello` (TH/CB), `receipt`, `discovery`, `conformance`.
    Conformance parses `map-spec/test-vectors/map-test-vectors.md` at test
    time and PRODUCES all 24 frozen vectors (TV-001..015 + TV-010b +
    NV-001..008) byte-exact from seeds, consumes all positives, rejects all
    NVs with required classes (NV-007→2003 via full identity §4 chain
    validation). **Go is produce+consume on TV-007** (InputSpec key 2
    modeled), closing the ERRATA 2026-07-05 parity hole on the Go side —
    Rust remains consume-only there. Independence rule honored: written from
    drafts+vectors only, no map-rs source read (not even the registry
    mirrors). Ambiguity report (5 items, none blocking) in map-go/README.md;
    ops §8 status note updated ("impl #2 at vector parity — live pairwise
    interop matrix remains future; do not overclaim"). Toolchain: prepend
    `C:\Program Files\Go\bin` to PATH in each PowerShell command; no mingw
    needed for Go.

14. **ARCHITECT SESSION 2026-07-12 — PENDING-ARCHITECT queue fully
    resolved.** All 7 queued items + the 5 map-go ambiguities ruled;
    ERRATA.md rewritten as a RESOLVED record (read it for the rulings'
    rationale). Spec patches landed in the same change: tasks §2 Quote.2
    now OPTIONAL; trust §2 RECEIPT_ACK = full dual Receipt + 2006 on
    refusal; core §9 adds 2006 countersign-refused and 6005/6006/6007/6008
    pay codes; identity §3.2 B1/B3 + §4 kid clause corrected (kid = chain
    LEAF subject, root = accountable — wire already frozen by TV-012) and
    max-depth counting pinned (cert admits ≤ d certs leafward of it);
    discovery §6 shard rendered as unpadded decimal; core §6.1
    Hello-vs-HelloAck shape caution. KEY RULING: pay §3.1 envelope-byte
    digest binding STANDS (TV-013 froze it; Rust's content-binding is an
    interop BUG). Two implementation packages authored, NOT yet executed:
    **TASK-017** (P1: map-session raw-bytes seam — `send` returns sealed
    bytes, `Incoming.raw`; re-bind pay digests to envelope bytes; rewire
    RECEIPT_REJECT_CODE→2006 and split PAY_REJECT→6005-6008) and
    **TASK-018** (P2: InputSpec keys 2/3 + TV-007 Rust conformance guard;
    NEW vector NV-009 max-depth-exceeded chain, additive to
    gen_vectors.py, guards in BOTH impls). Still open in ERRATA: PayInvoice
    key 4 watermark CDDL (needs a metered-terms package).
    **TASK-017 + TASK-018 both DONE 2026-07-12 (see §5, §6).** The entire
    PENDING-ARCHITECT queue is now resolved AND implemented; only the
    PayInvoice-watermark item remains (deferred to a metered-terms package).

15. **ARCHITECT SESSION 2026-07-12 (second pass) — interop-matrix packages
    authored.** Line-level extraction of the Rust wire surfaces
    (map-session handshake/recv-order, hello negotiate rules, replay/seq
    constants, framing, exporter construction) distilled INTO the packages
    so the Go implementer never reads Rust (independence rule).
    **TASK-019 DONE 2026-07-13**: `map-go/session` — Hello/HelloAck codecs
    (list-vs-singleton shapes, ACK key 4 = responder's OWN max), Negotiate
    (highest mutual version, first mutual suite in INITIATOR order, ext
    min-version intersection), ReplayCache (±120 s, 10-min window),
    SeqTracker (dense from 0), 4-byte-BE TCP framing with size-first reads,
    exporter = init-AID||resp-AID (68 B), full FR-5 receive order, handshake
    ERROR msg-id 0xEE×16, session-id echo + per-frame 1006 check. envelope
    Body extended with in-session keys 4/5/6/7 + SealBody/SealBodyDelegated
    (buildBody unchanged → frozen 25 vectors still byte-exact). identity
    gained AgentID.Bytes(). 9 tests: TCP + in-mem loopback, PING auto-PONG,
    CLOSE, replay/seq-gap/tamper/wrong-sid/oversize/version-disjoint/
    delegated-handshake (root resolves to RIK). Zero deps, vet+gofmt clean.
    **TASK-020** (BLOCKED on 019, P1):
    the live matrix — 8 scenarios (echo/replay/seq-gap/tamper/cross-session
    relay/version-disjoint/wrong-sid/delegated) × 2 orientations over TCP,
    peer binaries (`map-cli interop`, `map-go/cmd/map-interop`), PowerShell
    driver, `interop/MATRIX.md` as the ops §6.1 record. **FINDING pinned in
    both packages: Rust `Session::recv` never enforces the core §6.1
    per-frame session-id check (1006) — Go implements it in 019 (DONE), Rust
    fixed in 020 FR-2.** **TASK-020 DONE 2026-07-13: the live matrix is
    16/16 PASS (see §5).** on a 16/16 matrix the only remaining spec action
    is the rev-01 editorial cut.

16. **TASK-020 DONE 2026-07-13 — LIVE Rust↔Go interop matrix, 16/16 PASS.**
    The ops §6.1 gate is MET. Added: Rust `TcpTransport` (map-session,
    std::net, 4-byte-BE framing, identity-pair exporter) + `TransportError::
    TooLarge`; the FR-2 **session-id 1006 fix** in `Session::recv` (was
    silently skipped post-handshake — Go had it first) + its unit test;
    `Session::send_raw` (harness hook); `map-cli interop` subcommand
    (crates/map-cli/src/interop.rs; map-cli gained map-core + map-session
    deps) and `map-go/cmd/map-interop` + Go `Session.SendRaw/ChannelBinding/
    SessionID` accessors + `identity.AgentID.Bytes()`. 8 scenarios (echo,
    replay, seqgap, tamper, relay=cross-session anti-relay, versiondisjoint,
    wrongsid, delegated) × 2 orientations, over framed TCP loopback. Driver
    `interop/run-matrix.ps1` (builds both, runs 16 cells, exits nonzero on
    any fail); record `interop/MATRIX.md`; `interop/README.md`. ops §6.1
    status note updated to "live matrix delivered 16/16". Scenario seeds:
    A=01×32 (dialer/root), B=02×32 (listener), DK=03×32 (delegated leaf).
    Whole Rust workspace + map-go green, zero warnings. NEXT: rev-01
    editorial cut (fold ERRATA resolutions into normative text) — the last
    spec action before leaving -00. **rev-01 cut DONE 2026-07-13 (see §4).**

17. **ARCHITECT SESSION 2026-07-14 — UsageWatermark designed; the last
    undefined wire surface is closed.** Normative spec patches landed: pay
    §4/§4.1 pin `PayInvoice` key 4 as `UsageWatermark {1 TaskID, 2
    through-chunk-seq, 3 CUMULATIVE Usage, 4 cumulative-billed}` with exact
    (no-rounding) billing arithmetic and SIX ordered verification checks;
    pay §3 gains the rate-table unit-code registry (0 flat / 1 per-token-in
    / 2 per-token-out / 3 per-MILLISECOND / 4 per-invocation — per-single-
    unit so bills are exact uint×decimal products) + same-exponent/same-
    asset rule; tasks §3 pins kind-3 chunk content = deterministic-CBOR
    cumulative Usage; core §9 adds 6009 invoice-watermark-violation (6007
    wording broadened). 6001's final-unpaid-watermark travels in ERROR
    detail {1 => UsageWatermark}; CANCEL owes to the last countersigned
    checkpoint. Full rationale in ERRATA.md (RESOLVED 2026-07-14);
    CHANGELOG has a post-cut additive addendum. Packages authored, NOT
    executed: **TASK-021** (P2: map-pay codec + `billed`/`verify_watermark`
    exact-arithmetic helpers + code 6009 + vectors TV-016/NV-010 — billing
    constants pre-computed: model-1 rates 3000/5000 µUSD over usage
    {1523,812,431,42000} ⇒ cumulative-billed 8_629_000 — + conformance in
    BOTH impls, Go arithmetic written from spec not ported) and **TASK-022**
    (P3, blocked on 021: terms-3 runtime in map-taskrun — metered provider/
    requester pair, six-check verification incl. meter-consistency vs
    received kind-3 chunks, 6001 pause/top-up, checkpoint-anchored CANCEL;
    R1 flags resume-after-6001 signaling as likely PENDING-ARCHITECT).
    **TASK-021 DONE 2026-07-14** (map-pay `codec::{Usage, UsageWatermark}` +
    `billing::{billed, verify_watermark, AgreedTerms, BillingError}` — exact
    checked arithmetic, no rounding; `PayInvoice.watermark: Option` (postpaid
    bytes byte-stable, regression-tested); `codes::INVOICE_WATERMARK_VIOLATION
    = 6009`; TV-016/NV-010 vectors + conformance in BOTH impls, Go arithmetic
    written from spec. Rust 41 / map-go 27; see §6).

    **TASK-022 DONE 2026-07-14** — `map-taskrun::metered`
    (`Metered{Provider,Requester}Payment`): provider bills kind-3 CHUNK +
    PAY_INVOICE at watermarks; requester runs the six pay §4.1 checks IN
    ORDER (check 4 meter-consistency vs its OWN received kind-3 chunks is the
    new trust boundary — a provider MUST meter before it bills) using
    `map_pay::billed`, pays via the ledger operator idempotently. 6001 pause
    carries `{1 => UsageWatermark}` detail; `retry_pending` resumes after
    top-up; `owed_at_checkpoint(usage)` for CANCEL (checkpoint usage passed
    IN — no circular dep). 12 tests (happy 2-invoice, inflated-usage 6009,
    off-by-one 6009, invoice-before-chunk 6009, over-cap 6005 vs exact,
    task-binding 6007, zero-delta legal, resent idempotent, exhaustion
    6001→topup→resume, owed≤unpaid). **The pay family is now implemented
    end-to-end for terms 0 (postpaid) and 3 (metered).** The
    resume-after-6001 signaling was RESOLVED 2026-07-14 (see §7 step 19 /
    pay §4.2) — the interim is now normative. Future (unpackaged):
    prepaid/escrow reorder (pay §5.2), disputes (pay §6), a Go task runtime +
    a live-interop metered scenario.

19. **ARCHITECT SESSION 2026-07-14 (fourth pass) — metered pause/resume
    signaling RESOLVED (pay §4.2, ERRATA RESOLVED, CHANGELOG).** Chose the
    "NO new message type" option: pause is a PROVIDER-LOCAL decision (not a
    wire handshake), exhaustion = proof absence exactly as postpaid (the
    requester withholds PAY_PROOF; no "insufficient" message); the provider
    signals with the already-spec'd ERROR 6001 + `{1 => UsageWatermark}`
    detail, RECOMMENDED pause policy bounded by the stream's FLOW_CREDIT
    (core §8.1) grant. **RESUME is the covering PAY_PROOF itself** — no
    marker; provider settles + continues from the exact monotonic watermark;
    the next CHUNK is the observable resume. CANCEL owes ≤ the last
    countersigned checkpoint. This makes the TASK-022 interim normative
    verbatim; I only tightened `bill()` to no-op while Paused (pay §4.2 MUST)
    + a test. Whole workspace + map-go green, vectors byte-identical.
    **No open PENDING-ARCHITECT items remain; only TASK-023 (x402) is an
    open implementation package.**

20. **ARCHITECT SESSION 2026-07-16 — pay §5.2 prepaid/escrow DEFINED
    (spec + TASK-024 packaged; implementation OPEN).** The §5.2 stub is
    now four normative subsections under the §4.2 "no new message type"
    doctrine. Terms 1 (prepaid): single pre-delivery invoice == quote
    key 4 total (key 4 now REQUIRED under terms 1/2); billable execution
    gates on *settled*. Terms 2 (escrow): a fourth payment moment
    **held**; escrow = TWO standard invoice→proof→receipt rounds (hold,
    then release), each riding existing §4 idempotency. Additive wire
    fields: `PayInvoice.7 phase` (1 hold / 2 release; present iff terms
    2 — mirror of the watermark presence rule) and `PayReceipt.5 kind`
    (0 settled default-absent / 1 held / 2 released / 3 refunded;
    unknown → reject). Release trigger = COMPLETE + verified
    transcript-root, then a PAYER-signed release-auth (authorization
    symmetry: payee can't self-release, payer can't unwind pre-expiry;
    expiry auto-refunds; CANCEL releases exactly the checkpoint-billed
    amount). Code **6010 ESCROW_VIOLATION** registered (core §9).
    `pay:credit-ledger` escrow extension: `MAPLedgerHold/Release/Refund`
    entries + `MAPLedgerReleaseAuth`/`MAPLedgerRefundAuth` inner sigs
    (§5.2.4). Planned vectors TV-017/TV-018/NV-011 (additive; frozen
    bytes untouched). **TASK-024 is the open implementation package**
    (map-pay codec/ledger/escrow-verify, map-taskrun prepaid+escrow
    pairs, vectors both impls — map-go codec-only). Disputes (pay §6)
    should be designed AFTER 024: its receipts/auths are the evidence
    set.

    **TASK-024 DONE 2026-07-17.** map-pay: `PayInvoice.phase` (key 7) /
    `PayReceipt.kind` (key 5) with FAIL-CLOSED decode ranges + pre-escrow
    byte-regression tests; `escrow::verify_escrow_invoice` (the §5.2.2/
    §5.2.3 checks in order, wire codes out — check 2 task-binding is a
    documented no-op at this layer: terms-1/2 invoices carry no TaskID
    field, binding is at quote time); `AgreedTerms.quoted_total` added;
    credit-ledger escrow: `hold`/`release`/`refund` (idempotent by
    invoice-id, atomic release-with-remainder-refund, expiry rules) +
    `release_auth_bytes`/`refund_auth_bytes` inner-sig domains +
    `verify_hold_entry` (returns expiry) / `verify_release_entry`;
    balance-conservation invariant tested. `codes::ESCROW_VIOLATION =
    6010`. map-taskrun `escrow`: Prepaid pair (`execution_permitted()`
    gates on settled — the TASK-025 hook) + Escrow pair (hold/release
    rounds; requester holds its Keypair and signs release-auths;
    `set_complete_verified`/`set_cancel_checkpoint` feed the ctx;
    `owed_at_checkpoint` for CANCEL; hold expiry travels as CBOR uint in
    the phase-1 method payload). Vectors TV-017 (hold invoice, 231 B) /
    TV-018 (held receipt, 202 B) / NV-011 (kind 9 → reject, 202 B)
    additive — regenerated md diff 56 ins / 0 del, frozen bytes
    untouched. Rust conformance 41→44; map-go 27→30 (+
    `receiptKindRecognized` written from spec, produce parity byte-exact
    incl. TV-016/017/018 consume). Whole workspace + gated x402 build +
    map-go green, zero warnings; gofmt/vet clean. No deviations filed.
    **TASK-025 and TASK-026 are now UNBLOCKED (parallel-safe).**

    **TASK-025 DONE 2026-07-17.** Adapters only, no crates/ changes.
    x402-adapter `http402.rs` (always-on): `HttpUpstream` trait,
    `parse_payment_requirements` (fail-closed brace-matching 402-body
    parser), `SpendPolicy` (first exact-scheme entry on allowed
    network+asset, case-insensitive asset), `upstream_nonce(task,
    attempt)` = SHA-256("MAP-x402-upstream-nonce" || task ||
    be32(attempt)) — domain-separated from `x402_nonce` and tested on
    identical inputs; `pay_402_loop` (G-7 ceiling checked BEFORE
    signing → new `X402Error::PriceCapExceeded`; exactly one payment
    per (task, attempt); every post-payment failure returns
    `LoopFailure{evidence: Some}` — attributable loss); `CannedUpstream`
    (Free / PayThenOk / OverCap / Greedy / FailAfterPay, call+payment
    counters, nonce-single-settle) + gated `RealUpstream` (ureq);
    hand-rolled base64. gateway `paid.rs`: `PaidGatewayProvider<U,S>`
    EMBEDS a `GatewayProvider<MockBackend>` purely for `admit` reuse
    (no logic duplication); `serve` = admit → `set_settled` gate (fed
    by TASK-024 `execution_permitted`) → loop → chunks; G-5: evidence
    only as a kind-4 (ToolIo) chunk, no cross-leg proof API; the
    Executor impl degrades paid-then-failed to the evidence chunk
    ALONE (never empty). 11 gateway paid tests (incl. E2E
    prepaid-MAP-leg gating the rail leg with balance assertions on
    both ledgers; retry-same-attempt settles once / `next_attempt`
    pays anew; no-identifier-leak) + 4 http402 test fns. `map-cli
    x402-gateway-demo` behind `real-x402` (gateway crate is now an
    optional map-cli dep under that feature); gateway README added
    (two-leg model + G-5/6/7 rule mapping). Default `cargo tree`
    unchanged; workspace default + gated builds green, zero warnings.
    Note: TASK-023's gated demo `PayInvoice` literal gained `phase:
    None` (that path wasn't rebuilt during TASK-024 — caught here).
    **Only TASK-026 (disputes) remains open.**

    **TASK-026 DONE 2026-07-17 — the pay family is now implemented
    END-TO-END (quote→agree→invoice→proof→receipt→metering→prepay→
    escrow→disputes).** map-pay `dispute.rs`: §6.1 codecs (PayDispute /
    PayDisputeEvidence / EvidenceItem, fail-closed claim ≤4 + kind ≤9 +
    statement ≤1024 + non-empty items, content-address dedup); §6.2
    `authenticate_item` (envelope kinds open with external_aad = the
    DISCLOSED CB — absent/wrong CB = unavailable evidence; kind 5 =
    countersigned checkpoint with bytes PINNED by inline ruling as
    enc(["MAPCheckpointAck", task, seq, root]) + artifact CBOR
    [checkpoint-map, sig] — ERRATA 2026-07-17; kind 7 via map-trust
    Receipt::verify; foreign signers discarded); §6.3 `verify_dispute`
    PURE three-valued engine reusing billed/verify_watermark/
    verify_escrow_invoice/verify_entry/verify_release_entry/
    TranscriptBuilder — claims 0/2 rebuttal-shaped, claim 1 root
    recompute over kind-9 CHUNK items (**kind 9 ADDED to §6.1** — the
    authored 0–8 registry omitted transcript chunks; second inline
    ruling), claim 3 vs highest countersigned usage, claim 4 re-runs the
    6005/6006/6009/6010 verifiers; §6.4 max_items cap. map-taskrun
    `DisputeState` (open idempotent, respond with chunked `more`
    evidence from stored `Incoming.raw` + CB, accumulate deduped/
    bounded, 6003 refuse_pending, outcome_disputed flag for the receipt
    path). `Session::cb()` getter added (trivial accessor, in-scope).
    Tests: full 15-cell verdict matrix, hostile-bytes fuzz-shaped,
    shuffle/dup determinism, statement-never-evidence, cap-bounded; the
    §6.2 tests use a GENUINE loopback-session CB — incl. P2 session
    separation at the evidence layer (note: fully deterministic harness
    sessions derive IDENTICAL CBs — same seeds/clock/msg-ids → same
    transcript; the test varies the clock, exactly the distinguisher
    core §7.3 defines). Vectors TV-019 (dispute, anchored to TV-013 by
    envelope digest) / TV-020 (evidence carrying the frozen TV-010b
    envelope VERBATIM — I1 exercised by the vector itself) / NV-012
    (claim 9 → reject): md diff 59 ins / 0 del. Rust conformance 44→47;
    map-go 30→33 + `disputeClaimRecognized` written from spec. map-pay
    gains map-tasks + map-trust deps (acyclic, verified). Whole
    workspace + map-go green, zero warnings. **NO open implementation
    packages; NO open PENDING-ARCHITECT items.**

21. **ARCHITECT SESSION 2026-07-16 (second pass) — ops §5.1 paid-API
    gateway (x402 composition) DEFINED; TASK-025 packaged (OPEN, blocked
    by 024).** New ops §5.1: a gateway fronting an external x402-priced
    HTTP service runs TWO independent payment legs (downstream ordinary
    MAP payee, any method/terms; upstream x402 HTTP 402/X-PAYMENT payer
    in its own name). New gateway rules: **G-5** payment legs never
    launder (no cross-leg proof re-presentation; upstream settlement
    evidence enters only as a gateway-attested kind-4 transcript chunk),
    **G-6** spend order (admit → downstream settled — RECOMMENDED terms 1
    prepaid, the TASK-024 `execution_permitted` gate — before ANY
    upstream spend), **G-7** bounded upstream spend (per-task ceiling,
    fail-closed before signing). Upstream retry safety: authorization
    nonce = SHA-256("MAP-x402-upstream-nonce" || TaskID || be32(attempt))
    — domain-separated from the pay §5.1 invoice nonce. No wire changes,
    no vectors, no Go work. TASK-025 targets adapters only
    (x402-adapter `http402.rs` payer loop + gateway `paid.rs`
    `PaidGatewayProvider`; `real-x402` reused; default build zero new
    deps). **Implementation order: 024 then 025.**

22. **ARCHITECT SESSION 2026-07-16 (third pass) — pay §6 disputes
    DEFINED; TASK-026 packaged (OPEN, blocked by 024).** §6 rewritten
    from sketch to normative: §6.1 CDDL for PAY_DISPUTE (dispute-id,
    invoice-id, claim 0..4, PayAgreeRef anchor, optional TaskID +
    free-text statement that is NEVER evidence) and PAY_DISPUTE_EVIDENCE
    (`[+ EvidenceItem]`, FLOW_CREDIT-chunked, dedup by content address);
    EvidenceItem = (kind 0..8 registry, artifact bytes VERBATIM per I1,
    ? CB). **§6.2 CB disclosure — the new mechanism**: in-session
    envelopes sign with external_aad = CB, so third-party verification
    REQUIRES the disclosed CB (safe: HKDF output, not key material;
    bonus: same-CB items prove same-session provenance); CB-less
    in-session items = unavailable evidence. §6.3 mechanical per-claim
    procedures with a THREE-VALUED verdict (supported/rebutted/
    insufficient): claims 0/2 rebuttal-shaped (stand unrebutted against
    the party that must hold the artifacts if honest), claim 1 =
    transcript-root recompute, claim 3 = §4.1 arithmetic vs highest
    COUNTERSIGNED checkpoint usage, claim 4 = re-run existing
    6005/6006/6008/6009/6010 verifiers. §6.4 griefing bounds. No new
    code points (0x0405/0x0406 were pre-assigned); vectors TV-019/020 +
    NV-012 planned additively. TASK-026: map-pay `dispute.rs` (codecs +
    pure verdict engine REUSING existing verifiers — acceptance-grep'd),
    map-taskrun thin hooks (evidence from `Incoming.raw`, session CB
    getter, outcome-4 receipt hook), map-go codec-only. §6.2 tests MUST
    use a real loopback-session CB. **Implementation order: 024 →
    025/026 (025∥026 safe — different files). The pay family design is
    now COMPLETE** (quote→…→disputes); remaining unpackaged: method-
    generic runtimes, Go task runtime + interop scenarios, non-pay
    frontier.

18. **ARCHITECT SESSION 2026-07-14 (third pass) — `pay:x402` REGISTERED
    (pay §5.1 row + full method definition; additive, CHANGELOG updated).**
    x402 (x402.org / Coinbase) enters MAP as a settlement METHOD, not a
    replacement for the pay negotiation layer: MAP adopts x402's signed
    EIP-3009 authorization payload + payee-chosen facilitator verify/settle
    ONLY; x402's HTTP 402/X-PAYMENT loop is NOT used between MAP peers
    (PAY_QUOTE/AGREE carry stronger binding) — it stays appropriate at ops
    §5 gateways fronting external x402-priced services. KEY DESIGN RULINGS:
    (a) invoice-binding nonce MUST = SHA-256("MAP-x402-nonce" || invoice-id)
    — binds the authorization to exactly one invoice; the chain's single-use
    nonce then enforces exactly-once settlement under MAP's invoice-id
    idempotency; (b) /verify alone is NEVER settlement — PAY_RECEIPT only
    after successful settle; (c) Asset = existing `erc20:<chain>:<addr>`
    form, Amount exponent = token decimals negated so mantissas ARE atomic
    units (§3 exact arithmetic unchanged); (d) facilitator is payee-chosen/
    payee-trusted (payer exposure bounded by the exact-amount, time-boxed,
    single-use authorization) — priced in like the credit-ledger operator.
    **TASK-023 authored (READY, P4)**: `adapters/x402-adapter` — always-on
    CBOR codec (X402Params/X402Proof) + nonce fn + payer/payee flows over
    EvmSigner/Facilitator trait seams with canned impls; feature `real-x402`
    HARD-gates ureq + serde_json + k256 (FIRST non-Ed25519 crypto dep in the
    tree — default build must add ZERO deps, verify via cargo tree);
    map-cli `x402-demo` gated; NO vectors, NO Go work (chain-dependent
    proofs, like all rail methods); do NOT make the payment runtimes
    method-generic (altitude: wait for a 3rd method).
    **TASK-023 DONE 2026-07-14** (`adapters/x402-adapter` — new workspace
    member): always-on `codec` (X402Params/X402Proof CBOR), `x402_nonce`
    (SHA-256("MAP-x402-nonce"||invoice-id)), `payload` (hand-rolled x402
    "exact" PaymentPayload JSON — produced+consumed only in-crate, so no
    serde on the always-on path), `flow` (build_proof/verify_and_settle over
    EvmSigner/Facilitator seams + CannedSigner/CannedFacilitator). Payee
    surface returns wire codes 6008 (structural) / 6002 (settle) via
    `wire_code`. 16 always-on tests (nonce both ways, tampered/wrong-nonce/
    value-mismatch/exponent/expired/unknown-version/asset-chain → 6008;
    re-settle single-use nonce → 6002). `real-x402` gates k256+sha3 (EIP-712
    EIP-3009 signing; typehash anchored to published `0x7c7c…2267`) + ureq
    (json feature, TLS kept) + serde_json (facilitator API only); 2 gated
    tests + a #[ignore]d live one; `map-cli x402-demo` behind the gate.
    **Default workspace adds ZERO external deps** (cargo tree verified;
    Cargo.lock records gated deps but they don't compile in default).
    map-go + frozen vectors untouched. **No open implementation packages
    remain; no open PENDING-ARCHITECT items.**

12. ~~**TASK-014 — formal analysis (P1/P2/P3), ops §6.3.**~~ DONE (2026-07-10).
    New top-level `formal/` (spec-adjacent, not Rust): ProVerif 2.05 models.
    `hello-cb.pv` — Q1 (P1 downgrade resistance, core §6.2) **true**; Q2 (P2
    channel-binding session separation, core §7.3) **true**; Q3 reachability
    confirmed. `delegation.pv` — Q4 (P3 tighten-only attenuation, identity §4
    / TASK-010 `validate_chain`, incl. fail-closed caveats) **true**; Q5
    reachability confirmed. **No BLOCKING finding.** Every property has a
    mandatory FR-4 negative control that correctly FAILS with a witnessing
    trace: `hello-cb-control-downgrade.pv` (drops the initiator's
    negotiate-recheck → downgrade to v1 becomes reachable via a sid/msgid-
    reuse attack through the leaked insider agent), `hello-cb-control-relay.pv`
    (drops TH from the CB derivation → CB collides across distinguishable
    sessions), `delegation-control.pv` (drops the tighten-only intersection
    check → an arbitrary/widened scope gets accepted). ops §6.3 updated from
    "plan" to "delivered for P1–P3" with results.
    **Toolchain note (significant time sink, now resolved):** native-Windows
    `opam install proverif` fails (`lablgtk` GTK2 GUI is a HARD dep, doesn't
    compile against the installed mingw gcc); a from-source CLI-only
    (`-nointeract`) build via `ocamlbuild` also fails (native-Windows-opam +
    msys `mkdir` incompatibility, unresolved). **ProVerif works ONLY via
    WSL2** (Ubuntu): `sudo apt-get install -y opam m4 unzip && opam init -y
    --disable-sandboxing && eval $(opam env) && opam install proverif -y`.
    Also hit and fixed: WSL2's network adapter was stuck down post-reboot
    (Cloudflare WARP was blocking WSL2 traffic — disabling WARP + a full
    Windows reboot fixed it, `wsl --shutdown` alone was NOT enough).
    **Non-trivial modeling lessons (see formal/README.md "Modeling notes"
    for the full account — read before extending these models):** (a) `reduc`
    patterns can't match `free` names directly, only constructor structure —
    use nested `if-then-else` as a TERM instead; (b) multi-clause `reduc`
    needs `otherwise`, and the function needs a `fun NAME(...): type` line
    immediately before it with NO trailing period (combined declaration);
    (c) correspondence queries MUST be scoped to the honest principals (top-
    level `[private]` free names referenced directly in the query) — an
    unscoped query lets the Dolev-Yao attacker "attack" by just running a
    fully self-consistent session/chain using its own leaked key, which is
    not a real violation; (d) **the deepest lesson**: an event-correspondence
    over SIGNED-message events is the wrong tool for "does derived value X
    (like CB) separate sessions" — it collapses to plain signature
    unforgeability once the value is embedded in a signed blob, and the
    injective variant gets contaminated by the (correctly out-of-scope)
    msg-id replay-cache gap. A direct `table`-based collision/uniqueness
    check on the derived value itself (`get TBL(...) suchthat ... in ...
    else insert TBL(...)`) is the right tool — this is what Q2 uses.

N. **TASK-020 EXTENSION 2026-07-17 — task protocol over libp2p/QUIC added
   to the live interop matrix in BOTH orientations (Rust↔Go + Go↔Go).** Go
   impl #2 has a complete task runtime (`map-go/tasks`: transcript Merkle root,
   provider/requester FSMs, flow-credit streaming, `RunProvider`/`RunRequester`)
   + a real libp2p/QUIC transport (`map-go/net`: `MapNode`, `/map/1.0.0`
   substream, 4-byte-BE framing, channel-binding exporter = initiator‖responder
   AgentID, Kademlia DHT) + a stdlib-only descriptor codec (`map-go/discovery`)
   + the libp2p Kademlia router split into its OWN package (`map-go/dht`, so
   the codec/conformance surface never links libp2p — see the dep-boundary
   note at the end of this entry).
   The Go demo agent `map-excuse` (`map:cap:demo/excuse`) is a real
   task-answerer over libp2p. Rust impl #1 gained a libp2p task-mode:
   `map-cli` now depends on `map-net` + `map-taskrun`; `interop.rs` has a
   `--transport libp2p` branch with a `Libp2pExecutor` (mirrors the Go
   `scenarioExecutor`) driving `ProviderTask`/`RequesterTask` over
   `map_net::MapNode` (`map-net` gained `MapNode::listen_with_peer` to surface
   the dialable `/p2p/<peerid>` multiaddr). The shared capability is
   `map:cap:demo/excuse`. `interop/run-matrix.ps1` runs the 3 task scenarios
   (`task-stream`, `task-multichunk`, `task-transcript`) in both orientations
   over libp2p/QUIC. Result: **16/16 core (TCP, Rust↔Go) + 6/6 task (libp2p,
   both orientations) PASS**, and the transcript roots printed by each side
   match byte-for-byte (cross-impl parity, TV-008). FINDING/fix: a shutdown
   race (provider closing the session before the requester verified COMPLETE)
   was fixed on both sides by having the provider await the requester's CLOSE
   before teardown (Go `sess.Recv()` drain; Rust best-effort `close` after
   `PeerClosed`). Demo doc: `interop/DEMOS.md`. Phase D also delivers the
   standalone Rust `rfc-title-o-matic` binary (a `[[bin]]` of the `map-cli`
   crate, `src/rfc_title_o_matic.rs`): a real task-answerer for
   `map:cap:demo/rfc-title` (deterministic RFC-title `Executor`), libp2p/QUIC
   accept-loop, best-effort descriptor publish. The capability-agnostic Go
   `map-requester` dials it directly — live verified cross-impl
   (`OK transcript-root: f1752ebae4594c2f0415694143bf881f53230d7669b4ebbb17545ab58218b7d4`).
   Phase D complete.

   **DEPENDENCY BOUNDARY (independence marker preserved).** libp2p/QUIC is a
   core §10 transport *binding*, not part of the frozen wire format, so it is
   confined to the transport/demo layer in BOTH impls: Rust keeps it in the
   `map-net` crate and behind `map-cli`'s `libp2p-interop` feature (default-ON;
   `--no-default-features` gives a libp2p-free map-cli — the `rfc-title-o-matic`
   bin is `required-features`-gated); Go keeps it in `net` + `dht` + `cmd/*`.
   The protocol-core
   Go packages that define wire behavior — `cbor`, `envelope`, `identity`,
   `session`, `hello`, `receipt`, `discovery` (codec), `conformance` — remain
   pure stdlib. The step-13 "ZERO external deps" claim is thus still true FOR
   THE CONFORMANCE SURFACE; the whole-module `go.mod` now lists go-libp2p
   (transport only). Enforce with: `go list -deps ./cbor ./envelope ./identity
   ./session ./hello ./receipt ./discovery ./conformance | grep -E
   'libp2p|multiformats|mr-tron'` MUST be empty (checked 2026-07-17). The DHT
   router lives in `map-go/dht` precisely to keep this true — do NOT merge it
   back into `discovery`. map-go/README.md documents the same boundary.

O. **ARCHITECT SESSION 2026-07-18 — D-milestones PLANNED; TASK-029/030/031
   packaged (all OPEN).** The approved plan is IN-REPO at
   `tasks/PLAN-D-milestones.md` (normative for scope); each package file adds
   the execution contract (FR clarifications, §17 prompt, §18 checklist).
   - **TASK-029** (P2, READY): Go pay runtime — pure-stdlib `map-go/pay`
     (codec byte-arbitrated by the frozen pay TVs; billing six-check parity;
     credit-ledger `MAPLedgerEntry`; raw-aware pump FSMs for terms 0/1/3 —
     the seams already exist: `Incoming.Raw` + `session.Send`'s sealed-bytes
     return) + `pay-postpaid`/`pay-metered` libp2p cells BOTH orientations
     (Rust side composes the existing pairs inside `libp2p_task`), matrix
     22/22 → 26/26, `$SEED_OP = "04"*32`. Escrow/disputes Go port OUT.
   - **TASK-030** (P2, blocked by 029): the headline paid demo —
     `rfc-title-o-matic --price` prepaid gate (REUSES TASK-024
     PrepaidProviderPayment; deferred-ASSIGN preferred; free path
     byte-identical) × Go `map-requester --pay/--budget`; paid/unpaid/
     over-budget transcripts pasted into DEMOS.md from a LIVE run. First
     combined task+pay session loop on the demo path.
   - **TASK-031** (P3, parallel-safe): I-D submission readiness — author
     frontmatter (Jesse Larsen <jesselarsen@gmail.com>; ipr per xml2rfc's
     independent-stream vocabulary, trust200902 fallback), pinned
     `build/requirements.txt`, `build/nits.py` mechanical validation,
     idempotent-render check, `map-spec/SUBMISSION.md` (ISE/RFC 8730 steps +
     cover-note disclosures: CB-exporter stand-in, single author). md2rfc.py
     stays the pipeline of record; NO normative text changes.
   Also: retroactive tasks/README rows added for TASK-027/028 (ProVerif
   P4/P5/P6, DONE in `ac9d4ce` but never registered). **Implementation order:
   029 → 030, with 031 parallel to either.**

P. **TASK-029 DONE 2026-07-18 — pay family interoperates cross-impl LIVE.**
   New pure-stdlib `map-go/pay` package: pay codec (Amount/AssetAmount/
   PayQuote/Agree/Invoice/Proof/Receipt/UsageWatermark, fail-closed phase/kind),
   exact `Billed`/`VerifyWatermark` (six-check, big.Int intermediates),
   `pay:credit-ledger` (`LedgerOperator`/`EntryProof`/`VerifyEntry`, ed25519),
   and raw-aware pump FSMs for terms 0/1/3 (postpaid/prepaid/metered) mirroring
   Rust `payments.rs`/`escrow.rs`(prepaid)/`metered.rs`. Codec byte-verified vs
   frozen TV-010b/013/016/017/018 + NV-010/011; runtime legs mirror the Rust
   test matrices. **Escrow terms-2 + disputes NOT ported (future).** Boundary:
   `map-go/pay` links no libp2p — the `go list -deps` check now includes `./pay`
   (README updated). Live cells: `map-go/cmd/map-interop` + Rust gated
   `libp2p_task` (interop.rs) got `pay-postpaid`/`pay-metered` scenarios
   (payee=listener; payer=dialer hosts the in-process operator via
   `--operator-seed = "04"*32`), run after the task transcript verifies on the
   same session. Rust side REUSES `map-taskrun::{payments,metered}` (first live
   task+pay single-session loop Rust-side); `map-pay` added optional under
   `libp2p-interop`; `operator_seed` Flags field is `allow(dead_code)` in the
   minimal build. **Matrix 26/26** (16 core TCP + 6 task + 4 pay, both
   orientations); MATRIX.md Pay-matrix section added. Rust workspace 33 suites +
   `-p map-cli --no-default-features` zero warnings; map-go green, gofmt/vet
   clean, frozen vectors untouched. Commits: `60828f3` (pay package) + the
   interop-cells commit. **TASK-030 (paid demo) is now unblocked.**

Q. **TASK-030 DONE 2026-07-18 — the paid demo ("no pay, no work") works
   cross-impl LIVE.** `rfc-title-o-matic --price <cents> --operator-seed <hex>`
   runs the terms-1 prepaid gate (REUSES TASK-024 `PrepaidProviderPayment`,
   unchanged) BEFORE any task work — SEQUENTIAL pay-then-task, chosen over the
   deferred-ASSIGN interleave TASK-030 §FR-1 offered (sequential is strictly
   stronger for "no work before pay" — zero task frames before settlement —
   AND composes with the Go requester's task-only `RunRequester`, which can't
   interleave a payment pump). Provider derives its own AgentID from the seed
   and the requester's via `s.accountable_peer()`; prints `SETTLED`/`RECEIPT`.
   Go `map-requester --pay --operator-seed --budget` hosts the in-process
   credit-ledger operator (pre-credited to budget), pays the invoice, then
   runs the task; over-budget → clean refusal BEFORE agreeing (demo-layer
   check on quote total vs budget, no money moved). Shared demo TaskID =
   `[0x01,0…]` (both sides). **Live-verified 4 ways** (real transcripts in
   DEMOS.md): (a) paid → `OK paid: 500 cents` + `OK receipt` + title root
   `7bd6…5137`, provider `SETTLED 500`+`RECEIPT`; (b) over-budget (300<500) →
   `FAIL OVER-BUDGET … no work, no payment`; (c) unpaid (no `--pay`) → provider
   `GATE-HELD (unpaid): first message was 0x0200, not PAY_AGREE`, requester
   `peer_closed unpaid`; (d) free (no `--price`) → byte-identical title, no
   `0x04xx` frame (paid path is purely additive — structural: `if price>0` /
   `if *payFlag`). Stretch pay-prepaid MATRIX cell SKIPPED + noted (demo's
   pay-first flow ≠ the interop cells' task-first flow; not trivial; matrix
   stays 26/26). Harness gotcha found: PowerShell `Start-Process` splits a
   spaced `--input "a b"` arg, silently truncating Go flag parsing — use
   single-word inputs in demo scripts. Rust default+`--no-default-features` +
   map-go all green, zero warnings, gofmt/vet clean.
   **TASK-031 (I-D submission readiness) is the last open D milestone.**

R. **TASK-031 DONE 2026-07-18 — I-D submission readiness (editorial only).**
   All 11 drafts got `author: Jesse Larsen` / `email` / `ipr: trust200902`
   front-matter. **`build/md2rfc.py` hardened** (render-only, no draft
   mutation): UTF-8 read (was cp1252 — mojibaked every §/—/arrow); author from
   front-matter (was hardcoded "M. Architect"); drop the H1 past leading blanks
   AND the hand-written "Status of This Memo" preamble (xml2rfc regenerates the
   correct boilerplate — else the whole doc nested under a spurious section 1);
   route `## Abstract` into RFC front-matter; strip author-written `N.` section
   numbers (was "1.16. 14. Security Considerations"); emit `version="3"`
   (WITHOUT it xml2rfc renders an EMPTY Author's Address — key gotcha); declare
   `submissionType="independent"` only for info/exp/historic categories
   (xml2rfc ERRORS on std+independent — the ISE publishes only info/exp/
   historic per RFC 7841). Normalized Security/IANA heading casing to Title
   Case; added Security+IANA sections to index/intro/wire and IANA to arch/ops
   (concise pointer sections) → `build/nits.py` green (every draft has all
   required sections). New: `build/{nits.py,requirements.txt (xml2rfc==3.34.0),
   README.md}`, `map-spec/SUBMISSION.md` (ISE/Datatracker checklist + cover-
   note disclosures [CB-exporter stand-in; single-author preprint] + 3 tracked
   pre-submission items: (1) the std/bcp-category vs independent-stream
   conflict — AUTHOR DECISION: retarget protocol drafts to IETF stream or
   recategorize Informational for ISE; (2) 192 lines >72 chars — diagram/CDDL/
   wide-table reformatting; (3) References → structured normative/informative
   `<back>` split). Render is idempotent (fixed date). Frozen vectors
   byte-stable, no CDDL touched. **build/ was inadvertently gitignored (blanket
   `build/`) and never tracked — narrowed the ignore to transient artifacts
   only; the render pipeline + 33 rendered `.xml/.txt/.html` are now tracked.**
   **All D milestones (029/030/031) DONE.**

S. **REPO PATH MOVED 2026-07-18: OneDrive relocated the working tree from
   `…/Documents/Code/MP2P RFC` to `…/Documents/Projects/MP2P RFC`** (the old
   `Documents\Code` folder is gone; nothing lost — all work was pushed). Use
   `C:\Users\Master\OneDrive\Documents\Projects\MP2P RFC` from now on. Watch for
   OneDrive dehydration of this path; if Bash reports "No such file or
   directory" for the repo, re-locate it (it may move again).

## 9. Working conventions

- BCP 14 language, unweakened. Registry code points never reused.
- Spec-feedback loop: when implementation exposes ambiguity, patch the
  spec in the same change (precedent: shard fix, Merkle fix) and record
  it in the doc.
- One source of truth per fact; events past-tense, commands imperative;
  idempotency wherever delivery is uncertain; ordering scoped and written
  down; projections rebuildable (full table: ops §2).
- Value types over bare byte strings at API boundaries.
- New wire structures: define CDDL in the spec first, then mirror in code
  with a roundtrip test; anything signed gets a vector.
