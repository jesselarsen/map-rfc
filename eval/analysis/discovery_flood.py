"""Amendment A11 (eval/HYPOTHESES.md), EXPLORATORY: discovery flooding
composed with trust, for review finding R2-03. Reads e1-discovery-flood.csv;
no new simulation.

Usage: python discovery_flood.py RESULTS_DIR
       (writes RESULTS_DIR/DISCOVERY-FLOOD.md)

Reading rules fixed in A11 before the run, for Ours-L and Ours-L+C:
- capture defeats it: steady-state share CI entirely above 0.5;
- it still beats random: its CI entirely below B0's at the same flood level;
- memory helps: the k = 3 CI entirely below the k = 0 CI.
"""

import os
import sys

import stats

PROVIDERS = 200
N_ADV = 40  # alpha = 0.2
FLOODS = [0.0, 1.0, 10.0, 100.0]
MEMORY = [0, 3]
ARMS = [("b0", "B0 random"), ("b2", "B2 canary"), ("ours", "Ours-L (W1)"), ("ours_c", "Ours-L+C (W1)")]
METRICS = [("attacker_share_ss", "steady-state attacker share (rounds 31–60)"),
           ("attacker_share_r1_5", "rounds 1–5 attacker share (new requesters)"),
           ("honest_success_ss", "steady-state honest success")]


def main(res):
    rows = stats.load(os.path.join(res, "e1-discovery-flood.csv"))
    cell = {}
    for fl in FLOODS:
        for k in MEMORY:
            for arm, _ in ARMS:
                sel = stats.select(rows, flood_per_honest=fl, shortlist_memory=k, arm=arm)
                if len(sel) != 20:
                    raise SystemExit(f"expected 20 seeds at flood={fl} k={k} arm={arm}, got {len(sel)}")
                cell[(fl, k, arm)] = {m: stats.mean_ci([r[m] for r in sel]) for m, _ in METRICS}

    lines = ["# A11 (EXPLORATORY): discovery flooding composed with trust", "",
             "Source: `e1-discovery-flood.csv`: α = 0.2, farming, W1, 20 seeds. Mean [95% CI].",
             "k is `shortlist_memory`. B0 and B2 hold no evidence, so their k = 3 rows are replicates.", ""]
    for m, title in METRICS:
        lines += [f"## {title}", ""]
        lines.append("| flood per honest | adversarial share of discovery | k | " + " | ".join(n for _, n in ARMS) + " |")
        lines.append("|---|---|---|" + "---|" * len(ARMS))
        for fl in FLOODS:
            f = round(fl * (PROVIDERS - N_ADV))
            disc = (N_ADV + f) / (PROVIDERS + f)
            for k in MEMORY:
                lines.append(f"| {fl:g} | {disc:.3f} | {k} | "
                             + " | ".join(stats.fmt(cell[(fl, k, a)][m]) for a, _ in ARMS) + " |")
        lines.append("")

    lines += ["## Readings (rules fixed in A11; steady-state attacker share)", ""]
    for arm, name in ARMS[2:]:
        for fl in FLOODS:
            for k in MEMORY:
                s = cell[(fl, k, arm)]["attacker_share_ss"]
                b0 = cell[(fl, 0, "b0")]["attacker_share_ss"]
                tags = []
                if s[1] > 0.5:
                    tags.append("capture defeats it")
                if s[2] < b0[1]:
                    tags.append("beats random")
                if k == 3 and s[2] < cell[(fl, 0, arm)]["attacker_share_ss"][1]:
                    tags.append("memory helps")
                lines.append(f"- {name}, flood {fl:g}, k = {k}: " + (", ".join(tags) if tags else "none"))
    lines.append("")

    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "DISCOVERY-FLOOD.md"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
