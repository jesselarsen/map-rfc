"""Amendment A9 (eval/HYPOTHESES.md): minimax choice of the layer §8.4
default issuer weighting over the whole E1 grid.

Usage: python minimax.py RESULTS_DIR

W2's cells are e1-main.csv rows with arm = ours (weights w2, Amendment A6).
W0's and W1's are e1-minimax.csv. Cells: alpha in {0.05, 0.1, 0.2, 0.3, 0.5}
x the seven attacks. Winner: lowest worst-cell mean steady-state attacker
share. If the winner's CI at its worst cell overlaps another arm's CI at that
arm's worst cell, the simplest of the tied arms wins (w0 < w1 < w2).
"""

import os
import sys

import stats

ALPHAS = [0.05, 0.1, 0.2, 0.3, 0.5]
ATTACKS = ["farming", "camouflage", "collusion", "whitewash", "onoff", "slander", "combined"]
ARMS = ["w0", "w1", "w2"]  # simplest first


def cells(rows, **fixed):
    out = {}
    for a in ALPHAS:
        for atk in ATTACKS:
            sel = stats.select(rows, alpha=a, attack=atk, **fixed)
            if len(sel) != 20:
                raise SystemExit(f"expected 20 seeds at alpha={a} attack={atk} {fixed}, got {len(sel)}")
            out[(a, atk)] = (
                stats.mean_ci([r["attacker_share_ss"] for r in sel]),
                stats.mean_ci([r["honest_success_ss"] for r in sel]),
            )
    return out


def overlap(x, y):
    return x[1] <= y[2] and y[1] <= x[2]


def main(res):
    main_rows = stats.load(os.path.join(res, "e1-main.csv"))
    mm_rows = stats.load(os.path.join(res, "e1-minimax.csv"))
    table = {
        "w0": cells(mm_rows, weights="w0"),
        "w1": cells(mm_rows, weights="w1"),
        "w2": cells(main_rows, arm="ours"),
    }
    worst = {}
    for arm in ARMS:
        cell = max(table[arm], key=lambda c: table[arm][c][0][0])
        worst[arm] = (cell, table[arm][cell][0], table[arm][cell][1])

    lines = ["# A9: minimax choice of the layer §8.4 default weighting", ""]
    lines.append("| arm | worst cell (α, attack) | attacker share [CI] | honest success [CI] |")
    lines.append("|---|---|---|---|")
    for arm in ARMS:
        (a, atk), share, hon = worst[arm]
        lines.append(f"| {arm} | {a}, {atk} | {stats.fmt(share)} | {stats.fmt(hon)} |")

    best = min(ARMS, key=lambda arm: worst[arm][1][0])
    tied = [arm for arm in ARMS if arm == best or overlap(worst[arm][1], worst[best][1])]
    winner = tied[0]  # ARMS is ordered simplest first
    lines += ["", f"Lowest worst case: **{best}**. Tied (overlapping CIs): {', '.join(tied)}.",
              f"**A9 winner: {winner}**", ""]

    lines.append("## All cells: steady-state attacker share (mean [95% CI])")
    lines.append("")
    lines.append("| α | attack | " + " | ".join(ARMS) + " |")
    lines.append("|---|---|" + "---|" * len(ARMS))
    for a in ALPHAS:
        for atk in ATTACKS:
            lines.append(f"| {a} | {atk} | " + " | ".join(stats.fmt(table[arm][(a, atk)][0]) for arm in ARMS) + " |")
    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "MINIMAX.md"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
