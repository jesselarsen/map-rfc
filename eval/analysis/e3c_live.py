"""Amendment A14: E3c with the corrected colluder model (colluders advertise
only online colluders), next to the original run. Reads e3c.csv and
e3c-live.csv; no new simulation.

Usage: python e3c_live.py RESULTS_DIR   (writes RESULTS_DIR/E3C-LIVE.md)
"""

import os
import sys

import stats

NS = [1000, 10000, 100000]
CHURN = [0.0, 0.1, 0.3]
FS = [0.0, 0.1, 0.2, 0.3]


def main(res):
    orig = stats.load(os.path.join(res, "e3c.csv"))
    live = stats.load(os.path.join(res, "e3c-live.csv"))

    def succ(rows, n, ch, f, p):
        sel = stats.select(rows, n=n, churn=ch, f_malicious=f, paths=p)
        if len(sel) != 20:
            raise SystemExit(f"expected 20 seeds at n={n} churn={ch} f={f} paths={p}, got {len(sel)}")
        return stats.mean_ci([r["success_rate"] for r in sel])

    lines = ["# A14: E3c with colluders advertising only live colluders", "",
             "Lookup success, mean [95% CI], 20 seeds. \"Original\" is `e3c.csv` (colluders also",
             "advertise offline colluders, the R2-12 artefact); \"corrected\" is `e3c-live.csv`.", "",
             "| n | churn | f | original, 1 path | corrected, 1 path | original, 3 paths | corrected, 3 paths |",
             "|---|---|---|---|---|---|---|"]
    for n in NS:
        for ch in CHURN:
            for f in FS:
                lines.append(f"| {n} | {ch:g} | {f:g} | {stats.fmt(succ(orig, n, ch, f, 1))} | "
                             f"{stats.fmt(succ(live, n, ch, f, 1))} | {stats.fmt(succ(orig, n, ch, f, 3))} | "
                             f"{stats.fmt(succ(live, n, ch, f, 3))} |")
    lines += ["", "## Corrected messages per lookup (3 paths / 1 path), n = 10⁵", "",
              "| churn | f | 1 path | 3 paths | ratio |", "|---|---|---|---|---|"]
    for ch in CHURN:
        for f in FS:
            m = []
            for p in (1, 3):
                sel = stats.select(live, n=100000, churn=ch, f_malicious=f, paths=p)
                m.append(sum(r["messages"] for r in sel) / len(sel))
            lines.append(f"| {ch:g} | {f:g} | {m[0]:.1f} | {m[1]:.1f} | {m[1] / m[0]:.2f} |")
    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "E3C-LIVE.md"), "w", encoding="utf-8") as fh:
        fh.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])