# build/ — rendering the MAP Internet-Drafts

This directory renders the `map-spec/draft-map-*-01.md` sources to
Internet-Draft `.xml` (RFC 7991), `.txt`, and `.html`, and checks them for
mechanical submission nits. The source `.md` files are never modified
(render-only).

## Pipeline

```
map-spec/draft-map-*-01.md
        │  md2rfc.py   (Markdown -> RFC 7991 XML; stdlib-only)
        ▼
   build/*.xml
        │  xml2rfc     (pip; XML -> text + html)
        ▼
   build/*.txt, build/*.html
```

`md2rfc.py` is a purpose-built converter (kramdown-rfc2629 and mmark were
unavailable / misbehaving in this environment — see the `*.err` logs). It
reads the YAML front-matter (`title`, `docname`, `category`, `submissiontype`,
`author`, `email`, `ipr`), drops the document H1 and the hand-written
"Status of This Memo" preamble (xml2rfc regenerates the correct boilerplate
from the stream attributes), routes the `## Abstract` section into RFC front
matter, strips author-written `N.` section numbers so xml2rfc's automatic
numbering is authoritative, and emits `version="3"` RFC XML.

## Commands

```powershell
py -3 -m pip install -r build/requirements.txt   # once: xml2rfc==3.34.0
py -3 build/render.py                            # render all 11 drafts
py -3 build/nits.py                              # mechanical pre-submission check
```

`render.py` reads `XML2RFC` from the environment if the `xml2rfc` command is
not on `PATH`.

## Reproducibility

The render is **idempotent**: the `<date>` is fixed (2026-07), so a second
`render.py` run produces byte-identical `.txt`/`.xml`/`.html`. Regenerate and
commit the outputs whenever a draft or the converter changes. (At submission
time, update the date in `md2rfc.py` to the submission date.)

## `nits.py`

A stdlib-only mechanical check over the rendered `.txt`. It verifies each
draft has the required I-D sections (Abstract, Security Considerations, IANA
Considerations, References, Author's Address) — failing the run if any is
missing — and reports two informational counts: lines longer than 72
characters, and lines containing non-ASCII glyphs. It is **not** a substitute
for the real `idnits`; it covers the checks that are mechanical and stable on
Windows.

## Known pre-submission items

Tracked in `map-spec/SUBMISSION.md`:

- **Stream / category conflict.** Seven drafts are `category: std` (one is
  `bcp`); the Independent Submission stream publishes only
  Informational/Experimental/Historic (RFC 7841). Those drafts therefore
  render under the xml2rfc default (IETF) stream with a warning; the `info`
  drafts (index, intro, arch) render as `independent`. Resolving this —
  retarget the protocol drafts to the IETF stream, or recategorize for the
  ISE — is an author decision, not a rendering fix.
- **192 lines > 72 characters** across the family (ASCII-art message-flow
  diagrams, CDDL comments, wide tables, deep TOC entries). idnits flags these;
  they need per-draft reformatting before final submission.
- **References** render as a body section rather than a structured
  `<references>` back-matter split into normative/informative — a
  reference-entry formalization task for a final kramdown-rfc/xml2rfc pass.
