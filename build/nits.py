#!/usr/bin/env python3
"""Mechanical pre-submission nits check for the rendered MAP Internet-Drafts.

Runs over the xml2rfc-rendered ``build/*.txt`` (run ``build/render.py`` first).
stdlib-only. Reports, per draft:

  * required I-D sections present (Abstract, Security Considerations, IANA
    Considerations, References, Author's Address);
  * count of body lines longer than 72 characters (idnits flags these);
  * a document-specific Abstract — a draft whose source has no ``## Abstract``
    renders md2rfc's generic family placeholder, which satisfies a naive
    "is the word Abstract present" check while leaving the draft without an
    abstract. That is a hard nit here;
  * any non-ASCII byte in the .txt. RFC 7997 permits these in the RFC series,
    so they are reported for information and never fail the run.

Exit status is non-zero if any *required section* is missing (a hard nit);
long-line and width warnings are reported but do not fail the run — they are
tracked as known pre-submission formatting items in ``map-spec/SUBMISSION.md``.
This is not a substitute for the real ``idnits`` tool; it covers the checks
that are mechanical and stable on Windows.
"""
import glob
import io
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUILD = os.path.join(REPO, "build")
MAX_LINE = 72

# md2rfc.py substitutes this sentence when a source draft has no "## Abstract"
# section. It renders under a real "Abstract" heading, so presence-of-heading is
# not enough to tell a written abstract from a missing one. Keep in sync with
# md2rfc.py's fallback text. Matched against whitespace-collapsed text, because
# xml2rfc re-wraps the sentence across lines.
PLACEHOLDER_ABSTRACT = "part of the MAP (Manolito Agent Protocol) specification family"

REQUIRED = [
    ("Abstract", re.compile(r"^Abstract\s*$", re.M | re.I)),
    ("Security Considerations", re.compile(r"Security Considerations", re.M | re.I)),
    ("IANA Considerations", re.compile(r"IANA(?:-Style)? Considerations", re.M | re.I)),
    ("References", re.compile(r"\bReferences\b", re.M | re.I)),
    ("Author's Address", re.compile(r"Author's Address", re.M | re.I)),
]


def check(path):
    name = os.path.basename(path)[:-4]
    with io.open(path, encoding="utf-8") as fh:
        text = fh.read()
    missing = [label for label, rx in REQUIRED if not rx.search(text)]
    if PLACEHOLDER_ABSTRACT in " ".join(text.split()):
        # The heading is there; the abstract is not. Report it as missing so the
        # run fails, rather than passing on the placeholder.
        missing = [m for m in missing if m != "Abstract"]
        missing.append("Abstract (placeholder, not written)")
    long_lines = [
        (i + 1, len(ln)) for i, ln in enumerate(text.splitlines()) if len(ln) > MAX_LINE
    ]
    nonascii = [i + 1 for i, ln in enumerate(text.splitlines()) if any(ord(c) > 127 for c in ln)]
    return name, missing, long_lines, nonascii


def main():
    txts = sorted(glob.glob(os.path.join(BUILD, "draft-map-*-01.txt")))
    if not txts:
        print("no rendered drafts in build/ — run build/render.py first", file=sys.stderr)
        sys.exit(2)
    hard_fail = False
    total_long = 0
    print(f"{'draft':<26} {'missing sections':<28} {'>72ch':>6} {'non-ascii':>9}")
    print("-" * 72)
    for path in txts:
        name, missing, long_lines, nonascii = check(path)
        total_long += len(long_lines)
        if missing:
            hard_fail = True
        miss = ",".join(missing) if missing else "-"
        na = str(len(nonascii)) if nonascii else "-"
        print(f"{name:<26} {miss:<28} {len(long_lines):>6} {na:>9}")
    print("-" * 72)
    print(f"total lines > {MAX_LINE} chars across the family: {total_long}")
    print(
        "\nNOTE: >72-char lines come from wide tables and long CDDL / hex-vector\n"
        "lines that xml2rfc does not wrap; they are tracked as known\n"
        "pre-submission formatting items in map-spec/SUBMISSION.md. The\n"
        "'non-ascii' column counts lines with UTF-8 glyphs (SECTION SIGN, em\n"
        "dash, arrows) in the xml2rfc v3 text output; RFC 7997 permits these, so\n"
        "it is informational, not a defect."
    )
    if hard_fail:
        print("\nFAIL: at least one draft is missing a required section.", file=sys.stderr)
        sys.exit(1)
    print("\nOK: every draft has all required sections.")


if __name__ == "__main__":
    main()
