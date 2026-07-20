#!/usr/bin/env python3
"""Render MAP Internet-Drafts to xml/txt/html via mmark + xml2rfc.

The source drafts use kramdown-rfc-style YAML front-matter. This script runs
build/md2rfc.py (a small Markdown -> RFC 7991 XML converter) to emit XML, then
xml2rfc (pip) to emit text + HTML. The source .md files are NOT modified
(render-only). Rationale: kramdown-rfc2629 and mmark were unavailable or
misbehaved in this environment; md2rfc.py produces xml2rfc-valid XML.

Usage:
    python3 build/render.py            # renders all 10 drafts into build/
Requires: xml2rfc on PATH (pip install xml2rfc).
"""
import os
import re
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUILD = os.path.join(REPO, "build")
SPECDIR = os.path.join(REPO, "map-spec")

# (file, abbreviation shown in running headers, ipr value)
DRAFTS = [
    ("draft-map-index-01", "MAP-Index", "none"),
    ("draft-map-intro-01", "MAP-Intro", "none"),
    ("draft-map-arch-01", "MAP-Arch", "none"),
    ("draft-map-wire-01", "MAP-Wire", "none"),
    ("draft-map-core-01", "MAP-Core", "none"),
    ("draft-map-identity-01", "MAP-Identity", "none"),
    ("draft-map-discovery-01", "MAP-Discovery", "none"),
    ("draft-map-tasks-01", "MAP-Tasks", "none"),
    ("draft-map-trust-01", "MAP-Trust", "none"),
    ("draft-map-pay-01", "MAP-Pay", "none"),
    ("draft-map-ops-01", "MAP-Ops", "none"),
]

CATEGORY_MAP = {"std": "std", "info": "info", "bcp": "bcp", "exp": "exp"}


def main():
    os.makedirs(BUILD, exist_ok=True)
    script_dir = os.path.dirname(os.path.abspath(__file__))
    md2rfc = os.path.join(script_dir, "md2rfc.py")
    xml2rfc = os.environ.get("XML2RFC", "xml2rfc")
    failures = []
    for name, abbrev, ipr in DRAFTS:
        src = os.path.join(SPECDIR, name + ".md")
        if not os.path.exists(src):
            print(f"[skip] {name}: not found")
            continue
        xml_path = os.path.join(BUILD, name + ".xml")
        r = subprocess.run([sys.executable, md2rfc, src],
                           stdout=open(xml_path, "w", encoding="utf-8"),
                           stderr=subprocess.PIPE, text=True)
        if r.returncode != 0:
            failures.append(f"{name}: md2rfc failed: {r.stderr}")
            continue
        # xml2rfc -> txt and html
        txt_path = os.path.join(BUILD, name + ".txt")
        html_path = os.path.join(BUILD, name + ".html")
        r2 = subprocess.run([xml2rfc, xml_path, "--text", "-o", txt_path],
                            stderr=subprocess.PIPE, text=True)
        if r2.returncode != 0:
            failures.append(f"{name}: xml2rfc txt failed: {r2.stderr}")
        r3 = subprocess.run([xml2rfc, xml_path, "--html", "-o", html_path],
                            stderr=subprocess.PIPE, text=True)
        if r3.returncode != 0:
            failures.append(f"{name}: xml2rfc html failed: {r3.stderr}")
        print(f"[ok] {name}: xml/txt/html written")
    if failures:
        print("\nFAILURES:")
        for f in failures:
            print("  " + f)
        sys.exit(1)
    print("\nAll drafts rendered.")


if __name__ == "__main__":
    main()
