"""EXPLORATORY (not pre-registered): two E1 comparisons the paper reports
alongside H1, read from the existing e1-main.csv. No new simulation.

Usage: python b2l.py RESULTS_DIR   (writes RESULTS_DIR/B2L.md)

1. B2L (2603.03753's prose design: canary + provider-signed ledger) against
   Ours-L and Ours-L+C, over alpha in {0.05, 0.1, 0.2, 0.3, 0.5} x the seven
   attacks (review finding R2-02).
2. The A-unilateral ablation against Ours-L over the same cells (R2-01).

The arms of e1-main are separate sweep points with independent RNG streams,
so seed indices carry no pairing (review finding RT-12). Differences are
therefore bootstrapped with each arm resampled independently. All Ours arms
use the D5 winner, W2 (Amendment A6).
"""

import os
import sys

import numpy as np

import stats

ALPHAS = [0.05, 0.1, 0.2, 0.3, 0.5]
ATTACKS = ["farming", "camouflage", "collusion", "whitewash", "onoff", "slander", "combined"]
ARMS = ["b2l", "ours", "ours_c", "a_unilateral"]


def seeds(rows, arm, alpha, attack):
    sel = stats.select(rows, arm=arm, alpha=alpha, attack=attack)
    if len(sel) != 20:
        raise SystemExit(f"expected 20 seeds for {arm} alpha={alpha} attack={attack}, got {len(sel)}")
    return np.array([r["attacker_share_ss"] for r in sel], dtype=float)


def unpaired_diff_ci(x, y):
    """mean(x) - mean(y), each arm resampled independently."""
    rng = np.random.default_rng(stats._SEED)
    bx = x[rng.integers(0, len(x), size=(stats.RESAMPLES, len(x)))].mean(axis=1)
    by = y[rng.integers(0, len(y), size=(stats.RESAMPLES, len(y)))].mean(axis=1)
    lo, hi = stats._ci(bx - by)
    return float(x.mean() - y.mean()), lo, hi


def side(d):
    """'lower' / 'higher' if the CI of the difference excludes 0, else 'overlap'."""
    if d[2] < 0:
        return "lower"
    if d[1] > 0:
        return "higher"
    return "overlap"


def main(res):
    rows = stats.load(os.path.join(res, "e1-main.csv"))
    data = {(arm, a, atk): seeds(rows, arm, a, atk) for arm in ARMS for a in ALPHAS for atk in ATTACKS}

    lines = ["# EXPLORATORY: B2L and the A-unilateral ablation (E1, not pre-registered)", "",
             "Source: `e1-main.csv` (arms b2l, ours, ours_c, a_unilateral; Ours arms use W2).",
             "Steady-state attacker share, mean [95% CI]. Differences resample each arm",
             "independently (the arms do not share random streams).", ""]

    lines += ["## 1. B2L against Ours-L and Ours-L+C", ""]
    lines.append("| α | attack | B2L | Ours-L | Ours-L+C | Ours-L − B2L | Ours-L+C − B2L |")
    lines.append("|---|---|---|---|---|---|---|")
    count = {"ours": {}, "ours_c": {}}
    for a in ALPHAS:
        for atk in ATTACKS:
            b = data[("b2l", a, atk)]
            d_o = unpaired_diff_ci(data[("ours", a, atk)], b)
            d_c = unpaired_diff_ci(data[("ours_c", a, atk)], b)
            for arm, d in (("ours", d_o), ("ours_c", d_c)):
                count[arm].setdefault(atk, []).append(side(d))
            cols = [stats.fmt(stats.mean_ci(data[(arm, a, atk)])) for arm in ("b2l", "ours", "ours_c")]
            lines.append(f"| {a} | {atk} | " + " | ".join(cols)
                         + f" | {stats.fmt(d_o)} ({side(d_o)}) | {stats.fmt(d_c)} ({side(d_c)}) |")

    lines += ["", "Cells (of 5 α values) where the arm is lower / overlapping / higher than B2L:", "",
              "| attack | Ours-L | Ours-L+C |", "|---|---|---|"]
    for atk in ATTACKS:
        def tally(arm):
            s = count[arm][atk]
            return f"{s.count('lower')} / {s.count('overlap')} / {s.count('higher')}"
        lines.append(f"| {atk} | {tally('ours')} | {tally('ours_c')} |")

    lines += ["", "## 2. A-unilateral ablation against Ours-L", ""]
    lines.append("| α | attack | Ours-L | A-unilateral | A-unilateral − Ours-L |")
    lines.append("|---|---|---|---|---|")
    diffs = []
    for a in ALPHAS:
        for atk in ATTACKS:
            o, u = data[("ours", a, atk)], data[("a_unilateral", a, atk)]
            d = unpaired_diff_ci(u, o)
            diffs.append(d)
            lines.append(f"| {a} | {atk} | {stats.fmt(stats.mean_ci(o))} | {stats.fmt(stats.mean_ci(u))} | {stats.fmt(d)} |")
    excl = sum(1 for d in diffs if d[1] > 0 or d[2] < 0)
    lines += ["", f"Largest |A-unilateral − Ours-L|: {max(abs(d[0]) for d in diffs):.4f}. "
              f"Cells whose difference CI excludes 0: {excl} of {len(diffs)}."]

    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "B2L.md"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
