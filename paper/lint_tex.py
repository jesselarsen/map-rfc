"""Static checks for the paper source, for machines without a TeX install.

    python paper/lint_tex.py

Checks: \\ref/\\label pairing, \\cite keys against references.bib, brace and
environment balance per file, figure files present, non-ASCII characters
(pdfLaTeX needs them declared), and unescaped _ & # in running text.
Exit status 1 if anything is found. This is no substitute for compiling.
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def strip_comments(text):
    return re.sub(r"(?<!\\)%.*", "", text)


def main():
    files = [os.path.join(HERE, "main.tex")]
    sec = os.path.join(HERE, "sections")
    files += [os.path.join(sec, f) for f in sorted(os.listdir(sec)) if f.endswith(".tex")]
    problems = []
    labels, refs, cites = set(), [], []
    for path in files:
        name = os.path.relpath(path, HERE)
        raw = open(path, encoding="utf-8").read()
        text = strip_comments(raw)
        for i, line in enumerate(raw.splitlines(), 1):
            bad = [c for c in line if ord(c) > 127]
            if bad and not line.lstrip().startswith("%"):
                problems.append(f"{name}:{i}: non-ASCII {''.join(sorted(set(bad)))!r}")
        depth = 0
        for i, ch in enumerate(text):
            if ch == "{" and (i == 0 or text[i - 1] != "\\"):
                depth += 1
            elif ch == "}" and (i == 0 or text[i - 1] != "\\"):
                depth -= 1
                if depth < 0:
                    problems.append(f"{name}: unbalanced '}}' near offset {i}")
                    depth = 0
        if depth:
            problems.append(f"{name}: {depth} unclosed '{{'")
        stack = []
        for m in re.finditer(r"\\(begin|end)\{([^}]+)\}", text):
            if m.group(1) == "begin":
                stack.append(m.group(2))
            elif not stack or stack.pop() != m.group(2):
                problems.append(f"{name}: \\end{{{m.group(2)}}} does not match")
        if stack:
            problems.append(f"{name}: unclosed environments {stack}")
        labels |= set(re.findall(r"\\label\{([^}]+)\}", text))
        refs += [(name, r) for r in re.findall(r"\\ref\{([^}]+)\}", text)]
        for group in re.findall(r"\\cite\{([^}]+)\}", text):
            cites += [(name, k.strip()) for k in group.split(",")]
        for m in re.finditer(r"\\includegraphics(?:\[[^]]*\])?\{([^}]+)\}", text):
            base = os.path.join(HERE, m.group(1))
            if not any(os.path.exists(base + ext) for ext in ("", ".pdf", ".png")):
                problems.append(f"{name}: missing figure {m.group(1)}")
        # running text: drop math, \texttt{...}, \url{...}, tabular bodies
        body = re.sub(r"\$[^$]*\$|\\\[.*?\\\]", "", text, flags=re.S)
        body = re.sub(r"\\(texttt|url|label|ref|cite|includegraphics|input|bibliography\w*)(\[[^]]*\])?\{[^}]*\}", "", body)
        body = re.sub(r"#\d", "", body)  # macro parameters
        body = re.sub(r"\\begin\{tabular\}.*?\\end\{tabular\}", "", body, flags=re.S)
        for m in re.finditer(r"(?<!\\)[_#]", body):
            line = body.count("\n", 0, m.start()) + 1
            problems.append(f"{name}: unescaped {m.group(0)!r} (approx. line {line})")
        for m in re.finditer(r"(?<!\\)&", body):
            line = body.count("\n", 0, m.start()) + 1
            problems.append(f"{name}: unescaped '&' outside tabular (approx. line {line})")
    bib = open(os.path.join(HERE, "references.bib"), encoding="utf-8").read()
    keys = set(re.findall(r"@\w+\{([^,]+),", bib))
    for name, r in refs:
        if r not in labels:
            problems.append(f"{name}: \\ref{{{r}}} has no \\label")
    for name, k in cites:
        if k not in keys:
            problems.append(f"{name}: \\cite{{{k}}} not in references.bib")
    unused = keys - {k for _, k in cites}
    for p in problems:
        print(p)
    print(f"{len(files)} files, {len(labels)} labels, {len(refs)} refs, "
          f"{len(set(k for _, k in cites))} cited keys, unused bib keys: {sorted(unused) or 'none'}")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
