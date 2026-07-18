# TASK-031 — I-D submission readiness (hardening the existing render pipeline)

Status: READY · Priority: P3 · Target: `map-spec/draft-map-*-01.md`
(frontmatter + editorial nits only), `build/{render.py,md2rfc.py}`,
`build/requirements.txt` + `build/README.md` (new), `map-spec/SUBMISSION.md`
(new)
Depends on: nothing (independent of TASK-029/030 — parallel-safe; touches
disjoint files) · Blocks: the actual datatracker upload (user action).

Authored by the 2026-07-18 architect session, from the approved D-milestones
plan (`tasks/PLAN-D-milestones.md` §TASK-031 — normative for scope; this file
adds the execution contract).

---

## 1. Problem Statement

The render pipeline (`build/render.py` + `build/md2rfc.py` → xml2rfc
`.txt`/`.html`) works but is not submittable: the 11 drafts lack the
author/ipr metadata a datatracker submission requires, the toolchain is
unpinned, no validation pass has been recorded, and no submission checklist
exists. Close that gap without touching any normative content.

## 2. Functional Requirements

FR-1..FR-5 are the five numbered items of the plan §TASK-031, verbatim.
Binding clarifications:

- **FR-1 (metadata)**: author = `Jesse Larsen <jesselarsen@gmail.com>` in all
  11 drafts' frontmatter. For `ipr`: verify what the xml2rfc v3 vocabulary
  actually accepts for an independent-stream draft (`ipr="none"` is only
  valid for certain streams; if strict mode rejects it, use `trust200902`
  and note why in build/README.md). `date` auto-derived at render time is
  fine; the IDEMPOTENCE check therefore compares outputs modulo the date
  line. md2rfc.py reads the new keys; render.py's `DRAFTS` tuples remain as
  fallback only. Frontmatter edits are EDITORIAL (CHANGELOG classifies them
  so); zero changes to body normative text except what FR-3 finds.
- **FR-3 (validation)**: the mechanical-nits script lives at
  `build/nits.py` (stdlib-only Python): per rendered `.txt` check — required
  sections present (Abstract, Security Considerations, IANA[-Style]
  Considerations, References), references split normative/informative (add
  the split to drafts that lack it — editorial), ≤72-char lines in the .txt,
  no stray non-ASCII in the .txt output, boilerplate paragraph present.
  Every fix it forces is editorial-only; frozen vectors and CDDL blocks
  untouched byte-for-byte (diff-check the fenced blocks).
- **FR-4 (SUBMISSION.md)**: independent-stream/ISE process per RFC 8730 —
  steps, the 11 rendered filenames in submission order (index first), and
  the cover-note disclosures: the CB-exporter stand-in (core §14) and
  single-author status. Non-goal reminder: no actual upload.

## 3. Non-Functional / House Rules

Render-only pipeline (never mutates source .md — preserve this property).
`py -3` toolchain; pin xml2rfc in `build/requirements.txt`. No wire or
normative changes; CHANGELOG entry is Editorial. Frozen vectors untouched.

## 15. Acceptance Criteria

The plan §TASK-031 Acceptance block, verbatim, plus: `build/nits.py` exists,
runs clean (or each remaining nit is listed + justified in SUBMISSION.md),
and the CDDL-fence byte-diff check is part of the report.

## 17. Coding Task Prompt (hand to the coding LLM verbatim)

> Implement TASK-031 per `tasks/PLAN-D-milestones.md` §TASK-031 plus this
> file's clarifications. Read FIRST: `build/render.py` + `build/md2rfc.py`
> (the pipeline of record — do NOT migrate to kramdown-rfc/mmark; their
> failure logs are in build/), one rendered output, and ops §4 (changelog
> discipline). Toolchain: `py -3`; xml2rfc from pip (pin the working version
> in build/requirements.txt). Sequence: metadata → renderer support →
> regenerate all 11 → nits.py → fix editorial nits → regenerate → idempotence
> check (two renders byte-stable modulo date) → SUBMISSION.md. Verify the
> CDDL fences and all normative text are byte-identical before/after your
> draft edits except the deliberate editorial fixes, and list every editorial
> change in the CHANGELOG entry. If xml2rfc strict mode and the independent-
> stream ipr question can't be reconciled, STOP and file PENDING-ARCHITECT in
> map-spec/ERRATA.md with the exact xml2rfc error. Update memory.md +
> CHANGELOG (Editorial) + tasks/README.md row 031; commit + push (include the
> regenerated build/ outputs).

## 18. Validation Checklist (reviewer)

```
py -3 build/render.py && py -3 build/render.py        # twice; diff outputs modulo date line
py -3 build/nits.py                                    # clean or justified in SUBMISSION.md
git diff --word-diff map-spec/draft-map-*.md           # frontmatter + listed editorial only
py -3 map-spec/test-vectors/gen_vectors.py             # byte-stable (untouched)
test -f build/requirements.txt && test -f map-spec/SUBMISSION.md
```

## 19. Known Risks

R1: xml2rfc v3 vocabulary strictness around `ipr`/`submissionType` for the
independent stream — the STOP rule + trust200902 fallback contain it.
R2: md2rfc.py is custom — metadata parsing changes can silently alter
rendered headers; the idempotence + before/after diff checks catch it.
R3: editorial fixes drifting into normative text — the CDDL byte-diff check
and the CHANGELOG enumeration are the guard.

## 20. Handoff Notes

After this the family renders reproducibly with complete metadata and a
recorded validation pass; the remaining step to "presented to the IETF" is
the human one: datatracker account + upload per SUBMISSION.md, and the ISE
conversation. Draft -02 content work (if review feedback arrives) is a
future architect pass.
