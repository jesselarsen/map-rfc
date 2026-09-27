"""Amendment A13 (eval/HYPOTHESES.md), EXPLORATORY: partial receipt
visibility, for review finding RT-03. Reads e1-visibility.csv; no new
simulation.

Usage: python visibility.py RESULTS_DIR   (writes RESULTS_DIR/VISIBILITY.md)

Reading rules fixed in A13 before the run, for Ours-L with W1 per attack
and visibility v:
- keeps its advantage: its CI lies entirely below W0's;
- loses to self-only: its CI lies entirely above W0's;
- otherwise indistinguishable.
W0 ignores v; its row at each v is the control. B1 ignores weights; its W0
rows stand for it.
"""

import os
import sys

import stats

ATTACKS = ["farming", "camouflage", "slander"]
VIS = [1.0, 0.5, 0.2, 0.05]
ARMS = [("B1", "b1", "w0"), ("Ours-L W0", "ours", "w0"), ("Ours-L W1", "ours", "w1"),
        ("Ours-L+C W1", "ours_c", "w1")]


def main(res):
    rows = stats.load(os.path.join(res, "e1-visibility.csv"))

    def cell(arm, w, attack, v, metric):
        sel = stats.select(rows, arm=arm, weights=w, attack=attack, visibility=v)
        if len(sel) != 20:
            raise SystemExit(f"expected 20 seeds at {arm}/{w}/{attack}/{v}, got {len(sel)}")
        return stats.mean_ci([r[metric] for r in sel])

    lines = ["# A13 (EXPLORATORY): partial receipt visibility, α = 0.2", "",
             "Source: `e1-visibility.csv` (20 seeds). Steady-state attacker share, mean [95% CI].",
             "v is the probability that an observer sees a given other honest requester's receipts;",
             "Sybil receipts stay fully visible. Random selection gives 0.2.", ""]
    readings = []
    for atk in ATTACKS:
        lines += [f"## {atk}", "", "| v | " + " | ".join(n for n, _, _ in ARMS) + " | W1 reading |",
                  "|---|" + "---|" * len(ARMS) + "---|"]
        for v in VIS:
            c = {n: cell(a, w, atk, v, "attacker_share_ss") for n, a, w in ARMS}
            w1, w0 = c["Ours-L W1"], c["Ours-L W0"]
            if w1[2] < w0[1]:
                rd = "keeps its advantage"
            elif w1[1] > w0[2]:
                rd = "loses to self-only"
            else:
                rd = "indistinguishable"
            readings.append(f"- {atk}, v = {v:g}: {rd}")
            lines.append(f"| {v:g} | " + " | ".join(stats.fmt(c[n]) for n, _, _ in ARMS) + f" | {rd} |")
        lines.append("")
    lines += ["## Honest success (steady state), Ours-L W1", "", "| attack | " + " | ".join(f"v = {v:g}" for v in VIS) + " |",
              "|---|" + "---|" * len(VIS)]
    for atk in ATTACKS:
        lines.append(f"| {atk} | " + " | ".join(stats.fmt(cell("ours", "w1", atk, v, "honest_success_ss")) for v in VIS) + " |")
    lines += ["", "## Readings (rules fixed in A13)", ""] + readings + [""]

    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "VISIBILITY.md"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
