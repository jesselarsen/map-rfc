# MAP — Internet-Draft submission checklist

Pre-submission guide for the MAP specification family. This is a checklist and
a disclosure record, not a normative document. The rendering pipeline is
`build/` (`build/README.md`); this file is what a human needs before uploading
to the IETF Datatracker.

## What gets submitted

**Two** Internet-Drafts, not eleven — see "Stream" below for why. Both
Informational and registry-free, extracted from `pay-01` and `trust-01` and
rendered through `build/`:

- **exact incremental metering** — the agreed/submitted/settled discipline,
  `UsageWatermark`, exact decimal arithmetic with no rounding rule,
  metered pause/resume (from `pay-01` §1, §3, §4.1, §4.2)
- **the countersignature tolerance band** — fields 1–4 exact, usage within ±5%
  anchored to the *observed* value, timestamps within ±2000 ms (from
  `trust-01` §3)

The eleven documents below remain the specification family, published and
versioned outside the RFC series. They render through the same pipeline and the
mechanical checklist still applies to them, but they are not submitted.

1. `draft-map-index-01` — family index and submission overview (info)
2. `draft-map-intro-01` — motivation and design rationale (info)
3. `draft-map-arch-01` — architecture, threat model, zones (info)
4. `draft-map-wire-01` — PDU layering + consolidated registry index (info)
5. `draft-map-core-01` — envelope, sessions, negotiation, errors (std)
6. `draft-map-identity-01` — agent identity, delegation (std)
7. `draft-map-discovery-01` — two-plane discovery / DHT (std)
8. `draft-map-tasks-01` — task lifecycle, transcripts, checkpoints (std)
9. `draft-map-trust-01` — receipt-based trust projection (std)
10. `draft-map-pay-01` — payment negotiation + settlement + disputes (std)
11. `draft-map-ops-01` — operations, registries, conformance (bcp)

Author metadata is now in every draft's front-matter (`author: Jesse Larsen`,
`email: jesselarsen@gmail.com`, `ipr: trust200902`) and renders into the
front page and Authors' Addresses.

## Stream: the ISE path is closed for the family as constituted

The family front-matter marks `submissiontype: independent`. **An external
review established that the Independent Submission stream cannot publish this
family**, for a reason relabeling does not fix.

RFC 8726 §4: Independent Stream documents "in general … cannot request the
creation of a new IANA registry." §5: "The ISE will not appoint a DE. That
means … no new subregistry can be created that uses the Specification Required
or Expert Review policies." §2: a registry whose policy is IETF Review or
Standards Action "is not available to Independent Stream documents." The ISE's
own criteria page adds that independent submissions are not "foundational
formats upon which standards are expected to be built."

`draft-map-wire-01` §4 requests ~20 new registries, and every one of them uses
Standards Action, Specification Required, or Expert Review. `ops` §3.5's
assertion that "the designated experts become IANA-appointed Expert Reviewers"
will not happen on this stream. (§3.5 also says the IESG provides oversight of
the Independent Stream; it does not — the IESG performs RFC 5742 conflict
review only, and oversight is the ISE's.)

RFC 8726 §4 names the way out: "In some cases, this may mean that registries
are hosted by organizations other than IANA."

### The path now

1. **The specification family's home is a versioned, self-hosted spec** with
   registries under the steward process `ops` §3.5 already describes — as the
   permanent arrangement, not a pre-publication one. This is where all eleven
   documents, both implementations, the vectors and the interop matrix live.
2. **Two narrow Informational, registry-free Internet-Drafts** carry the two
   contributions a prior-art survey found genuinely novel: exact incremental
   metering (the agreed/submitted/settled discipline, `UsageWatermark`, exact
   decimal arithmetic, metered pause/resume) and the dual-signature
   countersignature tolerance band (trust §3). Neither is submitted until the
   remediation below is complete.

## MUST resolve before either draft is submitted

1. **The demonstrated defects.** An external review executed working exploits
   against the unmodified reference crates: a negative `Amount` moves funds from
   payee to payer (no draft requires non-negativity), dispute claim 0 is rebutted
   by one self-signed COMPLETE, dispute claim 1 is unilaterally forgeable, and
   receipt farming reaches score 0.998 / confidence 501 in 154 ms. Nothing ships
   while these stand. Several are resolved by removing the affected material.
2. **Provenance.** `arch` §1 and `ops` §7.2 refer to "the attached design work" /
   "the attached preprint" — unpublished material, dangling from a standalone
   document. `01-design-critique.md`'s C1–C15 findings, cited ~25–30 times across
   nine drafts, critique private design transcripts under `input-designs/`.
   Neither can appear in a submitted document.
3. **Related work.** No draft cites any actual prior art — not A2A, MCP, AP2,
   DIDComm, AGNTCY/ADS, SCITT, WIMSE, GNAP, ERC-8004, FIPA or KQML. "Does this
   duplicate existing work?" is the first question asked, and the documents
   currently contain no answer.
4. **References.** Six of eleven drafts contain exactly `[RFC2119] [RFC8174]`
   and nothing else, while normatively depending on did:key, multicodec,
   multibase, libp2p, Kademlia, x402 and EIP-3009. This is authoring, not a
   formatting pass. `build/md2rfc.py` hardcodes an empty `<back>` and must emit
   structured Normative/Informative references.
5. **Abstracts.** `draft-map-wire-01` and `draft-map-ops-01` have no `## Abstract`
   section; both render the generic family placeholder. `build/nits.py` reports
   them green because it greps for the word "Abstract" anywhere in the output.
6. **Category.** Whatever ships must be Informational (or Experimental).
   `build/md2rfc.py:252` additionally drops `submissionType` for any category
   other than info/exp/historic, so the seven std/bcp drafts render with no
   stream attribute at all.
7. **Line width.** 192 lines across the family exceed 72 characters — ASCII
   flows, CDDL comments, wide tables, and deep TOC entries with em-dashes, which
   need shorter titles rather than rewrapping.
8. **URI schemes.** `map:cap:` and `pay:x402` are unregistered; BCP 35 /
   RFC 7595 applies.

## Cover-note disclosures (state these to the ISE up front)

- **Channel-binding exporter is a stand-in on the reference libp2p binding.**
  The `/map/1.0.0` sessions use `initiator-AgentID ‖ responder-AgentID` as the
  CB exporter because rust-libp2p exposes no portable Noise handshake-hash /
  TLS-exporter on its stream API. This binds the identity pair, not the
  specific connection instance — weaker than a true key-schedule exporter.
  It is analyzed honestly in core §7.3 / §10 / §14; a deployment needing
  instance-level binding runs MAP over an exporter-bearing transport
  (TLS/QUIC), with no wire change. This is an implementation-maturity
  limitation, not a protocol-design one.

- **Single author, community-review preprint.** The drafts are marked as
  preprints for open-source community review. Two session-layer properties
  (negotiation recheck, transcript-dependence of the channel binding) and one
  delegation property (scope attenuation over a fixed lattice) have ProVerif
  models with negative controls (ops §6.3); **the payment-layer models are not
  sound and no payment property is claimed as machine-checked.** Two
  implementations interoperate live across 26 cells, but both are the work of
  one author under a source-isolation rule — the ops §6.1 independence
  criterion is met mechanically, not substantively. The documents should be
  cited only as "work in progress".

## Non-goals of this checklist

Actual Datatracker upload (a human action requiring an account); draft `-02`
content changes arising from review; the remediation itself (tracked as
findings with rulings in `ERRATA.md`, process in `REVIEW-PROCESS.md`); the
standing-up of the self-hosted registry, which needs its own document; and
migration off `md2rfc.py` to kramdown-rfc (deferred — that toolchain was
unavailable in this environment; `md2rfc.py` is the pipeline of record).
