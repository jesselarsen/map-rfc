#!/usr/bin/env python3
"""Minimal Markdown -> RFC 7991 XML converter for the MAP drafts.

The MAP drafts are consistently structured CommonMark with a YAML front-matter
(title/docname/category/submissiontype). This converter emits RFC 7991 XML that
xml2rfc (pip) accepts, so the family can be rendered to I-D text/HTML without
the kramdown-rfc / mmark toolchains (which were unavailable or misbehaving in
this environment). It is intentionally targeted at the constructs the drafts
use: headings, paragraphs, emphasis, inline code, fenced code blocks, pipe
tables, blockquotes, and ordered/unordered lists.

This is a RENDER-ONLY artifact; source .md files are not modified.
"""
import re
import sys
import html
import os

BLOCK_RE = re.compile(r"^```")  # fenced code
HEAD_RE = re.compile(r"^(#{1,6})\s+(.*)$")
TABLE_RE = re.compile(r"^\|.*\|$")
QUOTE_RE = re.compile(r"^>\s?(.*)$")
UL_RE = re.compile(r"^[-*]\s+(.*)$")
OL_RE = re.compile(r"^\d+\.\s+(.*)$")
BLANK_RE = re.compile(r"^\s*$")


def esc(t):
    t = html.escape(t, quote=True)
    # xml2rfc is strict about non-ASCII in element text; emit numeric refs.
    t = "".join(f"&#{ord(c)};" if ord(c) > 127 else c for c in t)
    return t


def inline(t):
    """Escape then apply a few inline transforms: `code`, **bold**, *italic*."""
    t = esc(t)
    t = re.sub(r"`([^`]+)`", lambda m: f"<tt>{m.group(1)}</tt>", t)
    t = re.sub(r"\*\*([^*]+)\*\*", lambda m: f"<strong>{m.group(1)}</strong>", t)
    t = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", lambda m: f"<em>{m.group(1)}</em>", t)
    return t


def parse_front(text):
    m = re.match(r"^---\n(.*?)\n---\n(.*)$", text, re.DOTALL)
    if not m:
        return {}, text
    fm = {}
    for line in m.group(1).splitlines():
        if ":" in line:
            k, v = line.split(":", 1)
            fm[k.strip()] = v.strip().strip('"').strip("'")
    return fm, m.group(2)


def author_xml(fm):
    """Build the <author> element from front-matter `author`/`email`/
    `organization` keys. Falls back to a neutral placeholder if absent."""
    name = fm.get("author", "MAP Specification Architect")
    email = fm.get("email", "")
    org = fm.get("organization", "Independent")
    parts = name.split()
    surname = parts[-1] if parts else name
    initials = "".join(p[0].upper() + "." for p in parts[:-1]) if len(parts) > 1 else ""
    addr = f"<address><email>{esc(email)}</email></address>" if email else ""
    return (
        f'    <author initials="{esc(initials)}" surname="{esc(surname)}" '
        f'fullname="{esc(name)}"><organization>{esc(org)}</organization>{addr}</author>'
    )


def split_rows(line):
    cells = [c.strip() for c in line.strip().strip("|").split("|")]
    return cells


def is_sep_row(line):
    return re.match(r"^\|[\s:|-]+\|$", line.strip()) is not None


def build_section_counter():
    return {}


def process(lines):
    """Return (preamble_flow, sections) where sections is a list of dicts
    {title, level, body:[flow_or_subsection_dict]} and flow are XML strings."""
    sections = []
    stack = [{"level": 0, "body": sections}]  # virtual root at level 0
    current = stack[-1]["body"]
    para_buf = []

    def close_para():
        nonlocal para_buf
        if para_buf:
            para = " ".join(p.strip() for p in para_buf).strip()
            if para:
                current.append(f"<t>{inline(para)}</t>")
            para_buf = []

    i = 0
    n = len(lines)
    while i < n:
        line = lines[i]
        if BLANK_RE.match(line):
            close_para()
            i += 1
            continue
        if BLOCK_RE.match(line):
            close_para()
            lang = line.strip()[3:].strip()
            code = []
            i += 1
            while i < n and not BLOCK_RE.match(lines[i]):
                code.append(lines[i])
                i += 1
            i += 1
            codetext = "\n".join(code)
            attr = f' type="{esc(lang)}"' if lang else ""
            current.append(f"<sourcecode{attr}>{esc(codetext)}</sourcecode>")
            continue
        if TABLE_RE.match(line) and i + 1 < n and is_sep_row(lines[i + 1]):
            close_para()
            header = split_rows(line)
            i += 2
            rows = []
            while i < n and TABLE_RE.match(lines[i]) and not BLANK_RE.match(lines[i]):
                rows.append(split_rows(lines[i]))
                i += 1
            thead = "<tr>" + "".join(f"<th>{inline(c)}</th>" for c in header) + "</tr>"
            trows = "".join(
                "<tr>" + "".join(f"<td>{inline(c)}</td>" for c in r) + "</tr>" for r in rows
            )
            current.append(f"<table><thead>{thead}</thead><tbody>{trows}</tbody></table>")
            continue
        if QUOTE_RE.match(line):
            close_para()
            qbuf = []
            while i < n and QUOTE_RE.match(lines[i]):
                qbuf.append(QUOTE_RE.match(lines[i]).group(1))
                i += 1
            current.append(f"<t>{inline(' '.join(qbuf).strip())}</t>")
            continue
        if UL_RE.match(line) or OL_RE.match(line):
            close_para()
            items = []
            ordered = bool(OL_RE.match(line))
            while i < n and (UL_RE.match(lines[i]) or OL_RE.match(lines[i])):
                mm = UL_RE.match(lines[i]) or OL_RE.match(lines[i])
                items.append(inline(mm.group(1).strip()))
                i += 1
            tag = "ol" if ordered else "ul"
            current.append(f"<{tag}>" + "".join(f"<li>{it}</li>" for it in items) + f"</{tag}>")
            continue
        m = HEAD_RE.match(line)
        if m:
            close_para()
            level = len(m.group(1))
            # Strip any author-written "N. " / "N.M " numeric prefix so
            # xml2rfc's automatic section numbering is the only numbering
            # (avoids "1.16. 14. Security Considerations").
            title = re.sub(r"^\d+(\.\d+)*\.?\s+", "", m.group(2).strip())
            while len(stack) > 1 and stack[-1]["level"] >= level:
                stack.pop()
            sec = {"title": title, "level": level, "body": []}
            stack[-1]["body"].append(sec)
            stack.append({"level": level, "body": sec["body"]})
            current = sec["body"]
            i += 1
            continue
        para_buf.append(line)
        i += 1
    close_para()
    return sections


def render_sections(section_list, prefix=""):
    xml = []
    num = 0
    for s in section_list:
        # Defensive: skip any stray loose flow at section-list top level.
        if not isinstance(s, dict):
            xml.append(s)
            continue
        num += 1
        toc = f"{prefix}{num}"
        safe_title = re.sub(r"[^a-z0-9-]", "", s["title"][:32].lower().replace(" ", "-"))
        xml.append(f'<section anchor="{safe_title}-{num}" numbered="true">')
        xml.append(f'<name>{esc(s["title"])}</name>')
        for el in s["body"]:
            if isinstance(el, dict):
                # recurse; build with this section's numbering prefix
                sub = render_sections([el], prefix + str(num) + ".")
                xml.append(sub)
            else:
                xml.append(el)
        xml.append("</section>")
    return "\n".join(xml)


def main():
    if len(sys.argv) < 2:
        print("usage: md2rfc.py input.md > output.xml", file=sys.stderr)
        sys.exit(1)
    # The drafts are UTF-8 (§, —, ≥, ×, arrows). esc() numeric-encodes every
    # >127 codepoint for xml2rfc, so a correct UTF-8 read yields proper glyphs;
    # the previous cp1252 read mojibaked them (§ -> "Â§").
    with open(sys.argv[1], encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    fm, body = parse_front(text)
    lines = body.splitlines()
    # Drop the document H1 (level-1 title), skipping any leading blank lines
    # first — otherwise a blank first line leaves the H1 in place and nests
    # the whole document under a spurious section 1.
    start = 0
    while start < len(lines) and BLANK_RE.match(lines[start]):
        start += 1
    if start < len(lines):
        hm = HEAD_RE.match(lines[start])
        if hm and len(hm.group(1)) == 1:
            del lines[start]
    # Drop hand-written preamble prose before the first section heading (the
    # "Status of This Memo" / BCP 78-79 boilerplate). xml2rfc regenerates the
    # correct boilerplate for the stream from the ipr / submissionType /
    # category attributes, so keeping the prose in the body would duplicate it.
    # (Render-only: the source .md keeps the prose for GitHub readers.)
    first_head = next((k for k, ln in enumerate(lines) if HEAD_RE.match(ln)), len(lines))
    lines = lines[first_head:]
    sections = process(lines)
    # Route an "Abstract" top-level section into the RFC front matter (an I-D
    # abstract MUST be front matter, not a numbered body section).
    abstract_flow = None
    kept = []
    for s in sections:
        if isinstance(s, dict) and abstract_flow is None and s["title"].strip().lower() == "abstract":
            abstract_flow = [el for el in s["body"] if isinstance(el, str)]
        else:
            kept.append(s)
    sections = kept
    title = fm.get("title", "UNTITLED")
    docname = fm.get("docname", "draft-unknown")
    category = fm.get("category", "info")
    sub = fm.get("submissiontype", "independent")
    # xml2rfc ERRORS if submissionType="independent" is paired with a
    # standards-track category — the Independent Submission stream (ISE)
    # publishes only Informational/Experimental/Historic (RFC 7841). So emit
    # the stream only for those categories; std/bcp drafts render under the
    # xml2rfc default (IETF) with a stream warning. This category/stream
    # conflict is a real pre-submission decision — retarget the protocol
    # drafts to the IETF stream, or recategorize as Informational for the ISE
    # — recorded in map-spec/SUBMISSION.md rather than silently forced.
    subtype_attr = f' submissionType="{esc(sub)}"' if category in ("info", "exp", "historic") else ""
    # ipr is required by xml2rfc; the drafts' BCP 78 note maps to trust200902.
    ipr = fm.get("ipr", "trust200902")
    # consensus applies only to IETF-stream docs; the independent stream omits
    # it.
    consensus_attr = ""
    if abstract_flow:
        abstract_xml = "<abstract>" + "\n".join(abstract_flow) + "</abstract>"
    else:
        abstract_xml = (
            "<abstract><t>This document is part of the MAP (Manolito Agent "
            "Protocol) specification family. See draft-map-index-01 for the family "
            "overview and reading order.</t></abstract>"
        )
    xml = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        f'<rfc docName="{esc(docname)}" category="{esc(category)}" '
        f'ipr="{esc(ipr)}" version="3"{consensus_attr}{subtype_attr}>',
        "  <front>",
        f'    <title abbrev="{esc(docname)}">{esc(title)}</title>',
        author_xml(fm),
        '    <date year="2026" month="July" day="17"/>',
        "    " + abstract_xml,
        "  </front>",
        "  <middle>",
        render_sections(sections),
        "  </middle>",
        "  <back></back>",
        "</rfc>",
    ]
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stdout.write("\n".join(xml) + "\n")


if __name__ == "__main__":
    main()
