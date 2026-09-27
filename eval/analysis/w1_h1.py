"""EXPLORATORY (review finding RT-02): the H1 criterion re-evaluated for W1,
the layer §8.4 default, next to W2, the pre-registered D5 winner. Reads the
committed CSVs only; no new simulation.

Usage: python w1_h1.py RESULTS_DIR   (writes RESULTS_DIR/W1-H1.md)

W1's cells are e1-minimax.csv (weights = w1); W2, B1 and B2 are e1-main.csv.
They come from separate runs with independent random streams, so every
ratio CI resamples each arm independently. W2 is recomputed the same way,
so the two weightings are compared by one method. The pre-registered H1
verdict (REPORT.md) used seed-paired ratios and stays the verdict of record.

H1 holds at a point if the upper CI bound of Ours/B1 and of Ours/B2 are both
<= 0.5 (HYPOTHESES.md E1). The pre-registered grid is alpha <= 0.3; alpha =
0.5 is reported separately.
"""

import os
import sys

import numpy as np

import stats

H1_ALPHAS = [0.05, 0.1, 0.2, 0.3]
ALL_ALPHAS = H1_ALPHAS + [0.5]
ATTACKS = ["farming", "camouflage", "collusion", "whitewash", "onoff", "slander", "combined"]


def shares(rows, **fixed):
    sel = stats.select(rows, **fixed)
    if len(sel) != 20:
        raise SystemExit(f"expected 20 seeds for {fixed}, got {len(sel)}")
    return np.array([r["attacker_share_ss"] for r in sel], dtype=float)


def unpaired_ratio_ci(x, y):
    """mean(x) / mean(y), each arm resampled independently."""
    rng = np.random.default_rng(stats._SEED)
    bx = x[rng.integers(0, len(x), size=(stats.RESAMPLES, len(x)))].mean(axis=1)
    by = y[rng.integers(0, len(y), size=(stats.RESAMPLES, len(y)))].mean(axis=1)
    with np.errstate(divide="ignore", invalid="ignore"):
        boots = np.where(by > 0, bx / by, np.inf)
    lo, hi = stats._ci(boots)
    return float(x.mean() / y.mean()) if y.mean() > 0 else float("inf"), lo, hi


def main(res):
    main_rows = stats.load(os.path.join(res, "e1-main.csv"))
    mm_rows = stats.load(os.path.join(res, "e1-minimax.csv"))
    arms = {"W1": lambda a, t: shares(mm_rows, weights="w1", alpha=a, attack=t),
            "W2": lambda a, t: shares(main_rows, arm="ours", alpha=a, attack=t)}
    b1 = {(a, t): shares(main_rows, arm="b1", alpha=a, attack=t) for a in ALL_ALPHAS for t in ATTACKS}
    b2 = {(a, t): shares(main_rows, arm="b2", alpha=a, attack=t) for a in ALL_ALPHAS for t in ATTACKS}

    res_cells = {}
    for w, f in arms.items():
        for a in ALL_ALPHAS:
            for t in ATTACKS:
                x = f(a, t)
                r1 = unpaired_ratio_ci(x, b1[(a, t)])
                r2 = unpaired_ratio_ci(x, b2[(a, t)])
                res_cells[(w, a, t)] = (stats.mean_ci(x), r1, r2, r1[2] <= 0.5 and r2[2] <= 0.5)

    lines = ["# EXPLORATORY (RT-02): the H1 criterion for W1 and W2, unpaired", "",
             "Ours-L steady-state attacker share and its ratio to B1 and to B2, mean [95% CI].",
             "Each arm is resampled independently. H1 holds when both upper bounds are ≤ 0.5.", ""]
    lines += ["## Summary: points where H1 holds (α ≤ 0.3, the pre-registered grid)", "",
              "| attack | W2 holds | W1 holds | W1 ratio to B1 (range over α) | W1 ratio to B2 (range over α) |",
              "|---|---|---|---|---|"]
    tot = {"W1": [0, 0, 0], "W2": [0, 0, 0]}  # [both, vs B2, points]
    for t in ATTACKS:
        cnt = {}
        for w in ("W1", "W2"):
            c = [res_cells[(w, a, t)] for a in H1_ALPHAS]
            cnt[w] = sum(1 for x in c if x[3])
            tot[w][0] += cnt[w]
            tot[w][1] += sum(1 for x in c if x[2][2] <= 0.5)
            tot[w][2] += len(c)
        r1 = [res_cells[("W1", a, t)][1][0] for a in H1_ALPHAS]
        r2 = [res_cells[("W1", a, t)][2][0] for a in H1_ALPHAS]
        lines.append(f"| {t} | {cnt['W2']}/4 | {cnt['W1']}/4 | {min(r1):.2f}–{max(r1):.2f} | {min(r2):.2f}–{max(r2):.2f} |")
    lines += ["",
              f"- W2: H1 holds at {tot['W2'][0]}/{tot['W2'][2]}; against B2 alone at {tot['W2'][1]}/{tot['W2'][2]}.",
              f"- W1: H1 holds at {tot['W1'][0]}/{tot['W1'][2]}; against B2 alone at {tot['W1'][1]}/{tot['W1'][2]}.", ""]

    for w in ("W1", "W2"):
        lines += [f"## {w}, all cells", "",
                  "| α | attack | share | ratio to B1 | ratio to B2 | H1 |", "|---|---|---|---|---|---|"]
        for a in ALL_ALPHAS:
            for t in ATTACKS:
                s, r1, r2, ok = res_cells[(w, a, t)]
                note = "" if a in H1_ALPHAS else " (outside H1 grid)"
                lines.append(f"| {a} | {t} | {stats.fmt(s)} | {stats.fmt(r1, 2)} | {stats.fmt(r2, 2)} | "
                             f"{'holds' if ok else 'fails'}{note} |")
        lines.append("")

    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "W1-H1.md"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
