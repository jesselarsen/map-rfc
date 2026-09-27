"""Pre-registered items that REPORT.md does not cover (review finding R2-06).
Reads only the committed CSVs; no new simulation.

Usage: python prereg_extra.py RESULTS_DIR   (writes RESULTS_DIR/PREREG-EXTRA.md)

1. E1 "time to detect" and "attacker share over all rounds" (HYPOTHESES.md
   E1 Metrics). Time to detect is the first round r at which the attacker
   share averaged over rounds r..r+4 is <= alpha/4, right-censored at 60.
2. Checkpoint byte overhead relative to Part payload (E4, "reported without
   hypothesis"). E2 Parts carry an 8-byte payload (map-sim e2.rs).
3. E3b at 16 cores (Amendment A5: "the paper reports both").
4. E3d message-rate crossover (E3d: "2 K query-rate equals N / (0.8 TTL)").
"""

import os
import sys

import numpy as np

import stats

ALPHAS = [0.05, 0.1, 0.2, 0.3, 0.5]
ATTACKS = ["farming", "camouflage", "collusion", "whitewash", "onoff", "slander", "combined"]
CENSOR = 60.0
PART_PAYLOAD_BYTES = 8


def e1(res, lines):
    main = stats.load(os.path.join(res, "e1-main.csv"))
    mm = stats.load(os.path.join(res, "e1-minimax.csv"))
    arms = [("B1", main, {"arm": "b1"}), ("B2", main, {"arm": "b2"}), ("B2L", main, {"arm": "b2l"}),
            ("Ours-L W2", main, {"arm": "ours"}), ("Ours-L W1", mm, {"weights": "w1"}),
            ("Ours-L+C W2", main, {"arm": "ours_c"})]

    def sel(rows, fixed, a, atk):
        s = stats.select(rows, alpha=a, attack=atk, **fixed)
        if len(s) != 20:
            raise SystemExit(f"expected 20 seeds for {fixed} alpha={a} attack={atk}, got {len(s)}")
        return s

    lines += ["## 1. E1: time to detect and all-rounds attacker share", "",
              "Time to detect, mean round over 20 seeds (number of seeds censored at 60 in",
              "brackets). A seed is censored when the 5-round average never reaches α/4.", ""]
    lines.append("| α | attack | " + " | ".join(n for n, _, _ in arms) + " |")
    lines.append("|---|---|" + "---|" * len(arms))
    never = {n: 0 for n, _, _ in arms}
    for a in ALPHAS:
        for atk in ATTACKS:
            cells = []
            for name, rows, fixed in arms:
                t = np.array([r["time_to_detect"] for r in sel(rows, fixed, a, atk)])
                c = int((t >= CENSOR).sum())
                if c == 20:
                    never[name] += 1
                cells.append(f"{t.mean():.1f} ({c})")
            lines.append(f"| {a} | {atk} | " + " | ".join(cells) + " |")
    lines += ["", "Cells (of 35) in which no seed ever detects: "
              + ", ".join(f"{n} {k}" for n, k in never.items()) + ".", ""]

    lines += ["All-rounds attacker share (rounds 1–60), mean [95% CI]:", ""]
    lines.append("| α | attack | " + " | ".join(n for n, _, _ in arms) + " |")
    lines.append("|---|---|" + "---|" * len(arms))
    for a in ALPHAS:
        for atk in ATTACKS:
            cells = [stats.fmt(stats.mean_ci([r["attacker_share_all"] for r in sel(rows, fixed, a, atk)]))
                     for _, rows, fixed in arms]
            lines.append(f"| {a} | {atk} | " + " | ".join(cells) + " |")
    lines.append("")


def bytes_overhead(res, lines):
    e2 = stats.load(os.path.join(res, "e2.csv"))
    lines += ["## 2. Checkpoint bytes per Part (E2 countersigned, f = 0)", "",
              f"E2 Parts carry {PART_PAYLOAD_BYTES} payload bytes. Checkpoint bytes are the real",
              "encodings of every checkpoint in the task (signatures included), per Part of L.", ""]
    lines.append("| L | λL | c | checkpoint bytes per Part [CI] | ratio to 8-byte payload |")
    lines.append("|---|---|---|---|---|")
    for L in (100, 1000, 10000):
        for lam in (0.1, 1.0, 3.0):
            for c in (1, 8, 32, 128):
                s = stats.select(e2, arm="countersigned", L=L, lambda_L=lam, c=c, f_dishonest=0.0)
                if len(s) != 20:
                    raise SystemExit(f"expected 20 seeds for E2 L={L} lambdaL={lam} c={c}, got {len(s)}")
                per = stats.mean_ci([r["checkpoint_bytes"] / L for r in s])
                lines.append(f"| {L} | {lam} | {c} | {stats.fmt(per, 1)} | {per[0] / PART_PAYLOAD_BYTES:.1f}× |")
    e4 = stats.load(os.path.join(res, "e4.csv"))
    lines += ["", "E4 voucher digests, bytes per Part (λL = 0):", ""]
    for c in (1, 8, 32, 128):
        s = stats.select(e4, c=c, lambda_L=0.0)
        v = np.mean([r["voucher_digest_bytes"] / r["parts"] for r in s])
        lines.append(f"- c = {c}: {v:.2f}")
    lines.append("")


def e3b_multicore(res, lines):
    lines += ["## 3. E3b grinding at 1 core and 16 cores (Amendment A5)", ""]
    for name in ("e3b", "e3b-multicore"):
        rows = stats.load(os.path.join(res, f"{name}.csv"))
        col = next(k for k in rows[0] if "share" in k)
        vals = [r[col] for r in rows]
        lines.append(f"- `{name}` ({col}): min {min(vals):.4f}, max {max(vals):.4f} over "
                     f"{len(rows)} seed-configurations")
    lines.append("")


def e3d_crossover(res, lines):
    rows = stats.load(os.path.join(res, "e3d.csv"))
    pull = float(np.mean([r["pull_msgs_per_query"] for r in rows]))
    # Descriptor messages per query scale as 1/TTL: N / (0.8 TTL q). k = N / (0.8 q).
    k = float(np.mean([r["desc_msgs_per_query"] * r["ttl_s"] for r in rows]))
    lines += ["## 4. E3d message-rate crossover", "",
              f"- Pull: {pull:.1f} messages per query (2·K).",
              f"- Descriptor refresh: {k:.2f}/TTL messages per query, i.e. N / (0.8·q) with N = {0.8 * k:.1f} providers refreshing and q = 1 query/s.",
              f"- Equal message cost at TTL = {k / pull:.2f} s. Above that TTL, pull sends more "
              "messages per query than descriptor refresh at the simulated query rate; in general "
              "pull is the cheaper of the two only while query rate q < N / (1.6·K·TTL).", ""]


def main(res):
    lines = ["# Pre-registered items not in REPORT.md (review finding R2-06)", "",
             "Generated by `analysis/prereg_extra.py` from the committed CSVs.", ""]
    e1(res, lines)
    bytes_overhead(res, lines)
    e3b_multicore(res, lines)
    e3d_crossover(res, lines)
    text = "\n".join(lines) + "\n"
    with open(os.path.join(res, "PREREG-EXTRA.md"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text)


if __name__ == "__main__":
    main(sys.argv[1])
