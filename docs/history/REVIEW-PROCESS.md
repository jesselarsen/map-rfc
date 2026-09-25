# MAP — how to process external review findings

Companion to `map-spec/REVIEW-PROMPTS.md`. That file gets you findings; this
file is what to do with them without breaking an eleven-document family that
also has two implementations, a frozen vector set, and six formal models
hanging off it.

Repo conventions this process inherits: the conflict rule **drafts > task docs;
test vectors > both** (`tasks/README.md`); the change classification
**editorial / additive / breaking** (`draft-map-ops-01` §4, `CHANGELOG.md`);
the ruling record in `ERRATA.md`; and the standing rule in `AGENTS.md` that any
wire change re-runs the interop matrix and re-stamps `interop/MATRIX.md`.

---

## Rule 0 — the reviewer does not fix its own judgment findings

A reviewer asked to fix has a stake in every finding being real. It will not
come back and say "finding 7 was wrong, identity §4 already covers this" — and
that ruling is the most valuable output of the whole exercise. It is also
anchored: it spent a session building a model of the world in which the defect
exists.

**Exception: self-verifying findings.** A dangling cross-reference, a heading
that should read "IANA Considerations", `draft-map-index-01` §2 saying "10
Internet-Drafts" above a list of 11 — there is no judgment to preserve and the
fix is confirmable by looking. Roles 1 and 6 MAY fix their own mechanical nits
in the same session; re-establishing that context elsewhere costs more than it
saves.

Everything from Roles 2, 3, 4 and 7 goes through the triage gate below before
any fix session opens.

---

## Stage 1 — intake

Collect every finding into one register before ruling on any of them. One row
per finding, with the reviewer's severity kept as *reported* (you will often
disagree, and that disagreement is worth keeping visible):

| ID | Role | Reported severity | Location | One-line claim | Ruling |
|---|---|---|---|---|---|

Assign IDs on intake (`R2-01`, `R4-07`, …) and use them in every commit message
and ERRATA entry afterwards. Findings from different roles will collide — the
CB exporter will be raised by Role 2 as a crypto defect, by Role 1 as a
cover-note disclosure, and by Role 3 as a modeling gap. Merge them into one row
with three sources, or you will fix it three times.

---

## Stage 2 — the triage gate

Every finding gets a ruling **from the author**, before any fix work starts.
Three questions:

1. **Is the defect real in the `-01` text?** Not in the implementation, not in
   the reviewer's reading of it. If the drafts are correct and `map-rs` is
   wrong, this is an implementation bug, not a spec finding — reclassify it.
2. **Does fixing it change the wire?** This sets the blast radius and the
   `CHANGELOG.md` class (editorial / additive / breaking).
3. **Is it in scope for this submission?** A preprint for community review does
   not have to solve everything it names.

Five dispositions:

- **ACCEPT** — fix before submission.
- **ACCEPT-EDITORIAL** — real, but wording only; batch into the Stage 3 step 5
  pass.
- **DOCUMENT** — real and unfixed; goes into Security Considerations or the
  cover note as a stated limitation. Legitimate and often correct for an
  independent-stream preprint. `tasks/README.md` already names NAT/eclipse
  defenses as an open frontier; Role 7 will produce real findings there, and
  "documented limitation" is far cheaper than designing a defense under
  submission pressure.
- **DEFER-02** — real, in scope eventually, not for `-01`.
- **REJECT** — not a defect. **Write the reasoning down anyway.** A rejection
  you do not record is one the next reviewer re-raises, and you re-litigate it
  from scratch.

### Ruling template

Append to `ERRATA.md`, matching its existing format:

```
RULED <date> (review finding <ID>, role <n> — <role name>) — <the finding, in
your own words, one or two sentences> — RULING: <ACCEPT|ACCEPT-EDITORIAL|
DOCUMENT|DEFER-02|REJECT>. <Reasoning. For REJECT, say what the reviewer
missed and where the text already covers it. For DOCUMENT, name the section
that will carry the limitation. For ACCEPT, name the wire class: editorial /
additive / breaking.> — <where implemented, or "no change">.
```

A REJECT entry is worth more than an ACCEPT one: the ACCEPT is evidenced by the
diff, the REJECT exists only here.

---

## Stage 3 — fix order

Work in blast-radius order, largest first. This is **not** the order findings
arrive, and it is not severity order.

| Step | Class | Source roles |
|---|---|---|
| 1 | Normative / wire changes | 2, 4, 7 |
| 2 | Formal-model and test-vector consequences of step 1 | 3, 5 |
| 3 | Implementation changes, both `map-rs` and `map-go` | 2, 4, 5 |
| 4 | Cross-document consistency and registry integrity | 6 |
| 5 | Editorial, boilerplate, references, line width | 1 |

Running Role 1 first is the tempting mistake. The 192 over-72-character lines
are ASCII flow diagrams, CDDL comments and wide tables; reformat them now and a
step-1 wire change discards the work. Editorial is last because its input is
the final text.

Batch by **subsystem**, not by finding. One change to core §6.2 can touch the
draft, the `draft-map-wire-01` registry index, a CDDL block, both
implementations, the vectors, `formal/hello-cb.pv`, `MATRIX.md` and
`CHANGELOG.md`. Forty commits each touching eight files is worse than eight
commits each touching one subsystem.

---

## Stage 4 — blast radius checklist

Run this for every ACCEPTed normative fix. Anything unticked is drift.

- [ ] The draft section itself, and its CDDL block if the fields changed
- [ ] `draft-map-wire-01` §3 — every assigned code point appears exactly once,
      value and owner matching
- [ ] Every cross-reference *into* the changed section from the other ten drafts
- [ ] `map-rs` and `map-go` — both, or the two-implementation bar
      (`draft-map-ops-01` §6.1) is no longer met
- [ ] `map-spec/test-vectors/map-test-vectors.md` if any byte changed
- [ ] The `formal/` model covering the affected property, if the mechanism moved
- [ ] `interop/run-matrix.ps1` re-run and `interop/MATRIX.md` re-stamped
      (`AGENTS.md` standing rule)
- [ ] `CHANGELOG.md` entry, classified editorial / additive / breaking
- [ ] `ERRATA.md` ruling updated with where it landed

### The vector-regeneration trap

If a fix changes bytes you will regenerate the vectors with
`test-vectors/gen_vectors.py` — which is the author's own encoder, and
regenerating re-anchors exactly the self-referential problem Role 5 was hired
to find. Whatever independent cross-checks Role 5 established for the affected
vectors (published RFC test vectors, a third-party library, a hand
computation) MUST be re-run against the regenerated values. A diff of
old-bytes-vs-new-bytes proves only that the generator changed.

---

## Stage 5 — running the fix sessions

- **Fresh session per batch.** Do not continue the review session.
- **Input is the finding plus your ruling**, not the reviewer's transcript. The
  transcript carries the reviewer's certainty; you want the fixer to re-derive
  the problem and push back if it disagrees.
- **Give the fixer the conflict rule**: drafts > task docs; vectors > both.
- **Model tiering**: the stronger model for normative and protocol fixes
  (steps 1–3), a cheaper one for the mechanical batches (steps 4–5).
- **One ruling, one commit message reference.** Commit messages cite the
  finding ID so `git log` and `ERRATA.md` line up later.

---

## Stage 6 — verification gates

Per fix batch, before it is considered done:

- steps 1–2: `cargo +stable-x86_64-pc-windows-gnu test --workspace`,
  `go test ./...`, `gofmt -l`, `go vet`, and the affected ProVerif models
  re-run under WSL2 — **including their negative controls**, which must still
  fail
- step 3: the full interop matrix, both orientations, `MATRIX.md` re-stamped
- steps 4–5: `python build/render.py` then `python build/nits.py`, clean

A green suite after a spec change means less than it looks like if the suite
was never shown to go red. If Role 5 found planted bugs the suite failed to
catch, those coverage holes are themselves ACCEPT findings — fix the harness
before trusting it to verify the rest of the campaign.

---

## Stage 7 — close the loop

1. Re-run **Role 6 only**, scoped to the changed material. Consistency across
   eleven interlocking documents is precisely what a fix campaign breaks, and
   it is the cheapest role to re-run.
2. Re-run **Role 1** once, at the very end, against frozen text.
3. Re-read the `ERRATA.md` rulings as a set. If the DOCUMENT and DEFER-02
   entries together describe a protocol with a different maturity than the
   drafts claim, the fix is in the Status-of-This-Memo and cover-note text, not
   in the mechanisms.

Do not re-run Roles 2, 3, 4 or 7 against the same drafts. They have seen this
text; a second pass from the same role returns diminishing and increasingly
speculative findings. If a normative area changed enough to need fresh eyes,
give it to a new session with no knowledge of the first review.
