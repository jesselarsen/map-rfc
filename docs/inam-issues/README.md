# Drafts: contributing MAP's non-overlapping parts to INAM

These are drafts to paste into GitHub for
https://github.com/inamprotocol/inam-protocol. Nothing has been filed.
They were written on 2026-11-13 against INAM spec v0.44-draft.

| # | File | Where it goes | INAM process cost |
|---|---|---|---|
| 1 | `01-issue50-comment.md` | Comment on issue #50, then a PR | Low: requested by the maintainer, no spec change |
| 2 | `02-nonperformance-slander.md` | New issue | Low (test); medium if a cap follows |
| 3 | `03-relying-party-appraisal.md` | New issue, then a PR | Medium: SDK-only, no wire change |
| 4 | `04-metering-binding.md` | New issue | High: wire change; payments are out of INAM's stated scope |
| 5 | `05-checkpoints-resumption.md` | New issue (design discussion) | High: new resource, wire and signature change |

Zones are deliberately left out: they classify counterparties on the
requester's side, and INAM defines itself as not doing authorization.

## Before filing

- Replace `<MAP_REPO_URL>` and `<PAPER_URL>`. The MAP repo is private, so
  either make it public or tag a release first, or drop the links and
  quote the needed rules inline.
- Re-check INAM's SPEC version and section numbers. The drafts cite
  v0.44-draft, and INAM moves fast.
- File in order, and wait for a reply on #1 before opening 4 and 5. One
  maintainer reviews everything (GOVERNANCE.md).
- Every commit needs DCO sign-off: `git commit -s`.
- Changes to wire format or reputation wait 7 days for comment and need
  both runtimes (Node, Worker), both SDKs (JS, Python), tests, and
  cross-language vectors.
