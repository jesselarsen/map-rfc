"""Amendment A10 (eval/HYPOTHESES.md), EXPLORATORY: Sybil-count sweep for
review finding R2-04. Reads e1-sybil-sweep.csv; no new simulation.

Usage: python sybil_sweep.py RESULTS_DIR   (writes RESULTS_DIR/SYBIL-SWEEP.md)

Reading rules fixed in A10 before the run, applied to each weighting:
- degrades at s: its CI at s lies entirely above its CI at s = 20;
- loses to self-only at s: its CI lies entirely above W0's CI at s;
- worse than random at s: its CI lies entirely above alpha = 0.2.
"""

import os
import sys

import stats

ALPHA = 0.2
PROVIDERS = 200
HONEST_REQUESTERS = 500
HONEST_TASKS_PER_ROUND = 2 * HONEST_REQUESTERS  # Poisson(2) per requester
CAMOUFLAGE_TASKS = 2  # per Sybil per round (HYPOTHESES.md E1, attack 2)
COUNTS = [5, 20, 80, 320]
WEIGHTS = ["w0", "w1", "w2"]
ATTACKS = ["camouflage", "farming"]


def main(res):
    rows = stats.load(os.path.join(res, "e1-sybil-sweep.csv"))
    n_adv = round(ALPHA * PROVIDERS)
    cell = {}
    for atk in ATTACKS:
        for w in WEIGHTS:
            for s in COUNTS:
                sel = stats.select(rows, attack=atk, weights=w, sybils_per_adv=s)
                if len(sel) != 20:
                    raise SystemExit(f"expected 20 seeds at {atk}/{w}/{s}, got {len(sel)}")
                cell[(atk, w, s)] = (stats.mean_ci([r["attacker_share_ss"] for r in sel]),
                                     stats.mean_ci([r["honest_success_ss"] for r in sel]))

    lines = ["# A10 (EXPLORATORY): Sybil-count sweep, α = 0.2", "",
             "Source: `e1-sybil-sweep.csv` (arm `ours`, 20 seeds). Steady-state attacker share",
             "and honest success, mean [95% CI]. Random selection gives α = 0.2.", ""]
    for atk in ATTACKS:
        lines += [f"## {atk}", ""]
        head = "| Sybils per adv. provider | Sybils : honest requesters |"
        if atk == "camouflage":
            head += " camouflage tasks bought per round (× honest workload) |"
        lines.append(head + " " + " | ".join(f"{w} share" for w in WEIGHTS) + " | " +
                     " | ".join(f"{w} honest success" for w in WEIGHTS) + " |")
        lines.append("|---" * (len(head.split("|")) - 2 + 2 * len(WEIGHTS)) + "|")
        for s in COUNTS:
            sybils = n_adv * s
            row = f"| {s} | {sybils} : {HONEST_REQUESTERS} ({sybils / HONEST_REQUESTERS:.1f}) |"
            if atk == "camouflage":
                bought = sybils * CAMOUFLAGE_TASKS
                row += f" {bought} ({bought / HONEST_TASKS_PER_ROUND:.1f}×) |"
            row += " " + " | ".join(stats.fmt(cell[(atk, w, s)][0]) for w in WEIGHTS)
            row += " | " + " | ".join(stats.fmt(cell[(atk, w, s)][1]) for w in WEIGHTS) + " |"
            lines.append(row)
        lines.append("")

        lines += [f"Readings ({atk}, rules fixed in A10):", ""]
        for w in ("w1", "w2"):
            for s in COUNTS:
                share = cell[(atk, w, s)][0]
                base = cell[(atk, w, 20)][0]
                self_only = cell[(atk, "w0", s)][0]
                tags = []
                if s != 20 and share[1] > base[2]:
                    tags.append("degrades vs s = 20")
                if share[1] > self_only[2]:
                    tags.append("loses to self-only (W0)")
                if share[1] > ALPHA:
                    tags.append("worse than random")
                lines.append(f"- {w}, s = {s}: " + (", ".join(tags) if tags else "none of the three"))
        lines.append("")

    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "SYBIL-SWEEP.md"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
