"""Amendment A12 (eval/HYPOTHESES.md), EXPLORATORY: camouflaged targeted
slander, for review finding RT-04. Reads e1-targeted-slander.csv; no new
simulation.

Usage: python targeted_slander.py RESULTS_DIR
       (writes RESULTS_DIR/TARGETED-SLANDER.md)

Reading rules fixed in A12 before the run:
- the attack harms the targets in a cell if its steady-state target-share
  CI lies entirely below the same arm/weights under attack = none;
- camouflage worsens it if, under the attack, the camouflage_tasks = 2 CI
  lies entirely below the camouflage_tasks = 0 CI.
B1 ignores `weights`; its W0 rows stand for it (the others are replicates).
"""

import os
import sys

import stats

ARMS = [("B1 global", "b1", "w0"), ("Ours-L W0", "ours", "w0"), ("Ours-L W1", "ours", "w1"), ("Ours-L W2", "ours", "w2")]
CAMO = [0, 2]


def main(res):
    rows = stats.load(os.path.join(res, "e1-targeted-slander.csv"))

    def cell(arm, w, attack, camo, metric):
        sel = stats.select(rows, arm=arm, weights=w, attack=attack, camouflage_tasks=camo)
        if len(sel) != 20:
            raise SystemExit(f"expected 20 seeds at {arm}/{w}/{attack}/{camo}, got {len(sel)}")
        return stats.mean_ci([r[metric] for r in sel])

    lines = ["# A12 (EXPLORATORY): camouflaged targeted slander, α = 0.2, 5 targets", "",
             "Source: `e1-targeted-slander.csv` (scenario `e1t`, 20 seeds). Mean [95% CI].",
             "Target share is the share of honest requesters' executions that go to the 5 targets",
             "(rounds 31–60). Relative harm = 1 − share(attack) / share(none).", ""]
    lines.append("| arm | camouflage tasks | target share, none | target share, attack | relative harm | attacker share, attack | reading |")
    lines.append("|---|---|---|---|---|---|---|")
    under = {}
    for name, arm, w in ARMS:
        for camo in CAMO:
            base = cell(arm, w, "none", camo, "target_share_ss")
            att = cell(arm, w, "targeted_slander", camo, "target_share_ss")
            adv = cell(arm, w, "targeted_slander", camo, "attacker_share_ss")
            under[(name, camo)] = att
            harm = 1 - att[0] / base[0] if base[0] > 0 else float("nan")
            tags = []
            if att[2] < base[1]:
                tags.append("harms targets")
            lines.append(f"| {name} | {camo} | {stats.fmt(base)} | {stats.fmt(att)} | {harm:.2f} | "
                         f"{stats.fmt(adv)} | {', '.join(tags) if tags else 'no harm'} |")
    lines += ["", "Camouflage worsens the attack (camouflage 2 CI entirely below camouflage 0 CI):", ""]
    for name, _, _ in ARMS:
        a0, a2 = under[(name, 0)], under[(name, 2)]
        lines.append(f"- {name}: {'yes' if a2[2] < a0[1] else 'no'}")
    lines.append("")

    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "TARGETED-SLANDER.md"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
