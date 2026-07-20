# MAP — Internet-Draft submission checklist

Pre-submission guide for the MAP specification family. This is a checklist and
a disclosure record, not a normative document. The rendering pipeline is
`build/` (`build/README.md`); this file is what a human needs before uploading
to the IETF Datatracker.

## What gets submitted

Eleven Internet-Drafts, rendered from `map-spec/draft-map-*-01.md` to
`build/draft-map-*-01.xml` (RFC 7991 v3). Suggested reading / submission order
(index first, then the family it indexes):

1. `draft-map-index-01` — family index and submission overview (info)
2. `draft-map-intro-01` — motivation and design rationale (info)
3. `draft-map-arch-01` — architecture, threat model, zones (info)
4. `draft-map-wire-01` — PDU layering + consolidated registry index (std)
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

## Stream: the Independent Submission stream (ISE)

The family front-matter marks `submissiontype: independent`. The Independent
Submission Editor (ISE) process is RFC 4846 / RFC 8730. Two-implementation
interop is met (`interop/MATRIX.md`, ops §6.1: 16 core + 6 task + 4 pay cells,
both orientations), which is the family's own advancement bar.

Submission steps (Datatracker, https://datatracker.ietf.org/):

1. Create or sign in to a Datatracker account.
2. For each draft, upload the rendered `build/draft-map-*-01.xml` via
   "Submit a draft" (the submission tool re-renders text/html from the XML).
3. Submit the index first so the others can reference it.
4. For the ISE path, follow up per RFC 8730 (the ISE handles independent-stream
   review and RFC-Editor coordination).

## MUST resolve before formal submission

These are real, tracked pre-submission items (mechanically surfaced by
`build/nits.py` and xml2rfc warnings). None is a wire-format or normative
protocol issue.

1. **Stream / category conflict (author decision).** The Independent
   Submission stream publishes only Informational / Experimental / Historic
   (RFC 7841). Eight drafts are `category: std` or `bcp`, which the ISE cannot
   publish as-is; xml2rfc renders them under the default IETF stream with a
   warning. Choose one:
   - retarget the standards-track protocol drafts to the **IETF stream**
     (requires WG adoption or AD sponsorship), keeping `std`/`bcp`; or
   - recategorize them as **Informational** for the ISE (the mechanisms are
     specified either way; only the maturity label changes).
   The `info` drafts (index, intro, arch) already render as `independent`.

2. **Line width (idnits).** 192 lines across the family exceed 72 characters —
   ASCII-art message-flow diagrams, CDDL comments, wide tables (e.g. the
   crypto-suite table in core §2), and deep TOC entries. These need per-draft
   reformatting (narrower diagrams, wrapped CDDL comments, restructured tables)
   before a clean idnits run.

3. **References formalization.** References currently render as a body section
   ("References (selected)") rather than a structured `<references>` back
   split into Normative and Informative with per-entry `<reference>` elements.
   A final kramdown-rfc / xml2rfc pass should formalize these (the citation
   tags — [RFC2119], [RFC8949], etc. — are already present in the text).

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
  preprints for open-source community review; they have not yet had IETF or
  ISE expert review. Formal properties P1–P6 are machine-checked (ProVerif,
  ops §6.3), and two independent implementations interoperate live, but the
  documents should be cited only as "work in progress" pending that review.

## Non-goals of this checklist

Actual Datatracker upload (a human action requiring an account and the
stream decision above); draft `-02` content changes arising from review;
and migration off `md2rfc.py` to kramdown-rfc (deferred — that toolchain was
unavailable in this environment; `md2rfc.py` is the pipeline of record).
