# MAP contributions filed with INAM

The parts of MAP that INAM (spec v0.44-draft) lacks were filed with
https://github.com/inamprotocol/inam-protocol on 2026-11-13, from the
`jesselarsen` account, after MAP development was suspended. Each one
states that the author isn't implementing it. Links into this repo are
pinned to commit `6cc324e`.

| Draft | Filed as | What it is |
|---|---|---|
| `01-issue50-comment.md` | [comment on #50](https://github.com/inamprotocol/inam-protocol/issues/50#issuecomment-6874208967) | A pointer to MAP's reusable Rust and Go code (did:key, strict Ed25519, JCS) for INAM's requested offline verifier |
| `02-nonperformance-slander.md` | [#174](https://github.com/inamprotocol/inam-protocol/issues/174) | MAP's A12 targeted-slander measurement applied to INAM's stated smearing risk; asks for a test |
| `03-relying-party-appraisal.md` | [#173](https://github.com/inamprotocol/inam-protocol/issues/173) | Proposes `appraiseLocally()` with T-3 and W1 weighting; SDK-only |
| `04-metering-binding.md` | [#175](https://github.com/inamprotocol/inam-protocol/issues/175) | Optional signed `settlement.usage` and `voucherDigest`; a wire change |
| `05-checkpoints-resumption.md` | [#176](https://github.com/inamprotocol/inam-protocol/issues/176) | Design proposal for dual-signed checkpoints and `resumedFrom`; a wire change |

The files hold the text as posted, below each file's first `---` line.
The one exception is the posted #50 comment, which is the whole
remainder of its file. Zones were deliberately not proposed: INAM
defines itself as not doing authorization.

Under INAM's GOVERNANCE.md, changes to wire format or reputation (#174's
cap, #175, #176) need a 7-day comment period and an implementation in
both runtimes and both SDKs, with tests and cross-language vectors.
Nobody has committed to that work.
