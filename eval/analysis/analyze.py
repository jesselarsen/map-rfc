"""Evaluate every pre-registered hypothesis (eval/HYPOTHESES.md) from the
CSVs in one results directory. Writes <dir>/REPORT.md.

    python eval/analysis/analyze.py eval/results/<sha>

Each check applies the decision rule exactly as registered and lists every
tested point. A missing CSV is reported as NOT RUN, never skipped silently.
"""

import json
import math
import os
import sys

from stats import diff_ci, fmt, load, mean_ci, ratio_ci, select, by_seed

# 2603.03753 results/fig5_a_robustness.csv (read 2026-09-26).
F1_PUBLISHED = {
    0.0: (0.8779916029436636, 0.8707133763280874),
    0.1: (0.03514119424271028, 0.771652851890001),
    0.2: (0.017250565692647428, 0.6379670920276166),
    0.3: (0.012908946489051701, 0.510972366946453),
    0.4: (0.011953693333595295, 0.44002290860163396),
    0.5: (0.015590049233735559, 0.40727048410327843),
}
ATTACKS = ["farming", "camouflage", "collusion", "whitewash", "onoff", "slander", "combined"]


class Report:
    def __init__(self):
        self.summary = []  # (id, claim, verdict)
        self.sections = []

    def add(self, hid, claim, verdict, body):
        self.summary.append((hid, claim, verdict))
        self.sections.append(f"## {hid}: {claim}\n\n**Verdict: {verdict}**\n\n{body}\n")


def csv_or_none(d, stem):
    p = os.path.join(d, stem + ".csv")
    return load(p) if os.path.exists(p) else None


def count_verdict(held, total):
    if total == 0:
        return "NOT RUN"
    if held == total:
        return f"HOLDS ({held}/{total} points)"
    if held == 0:
        return f"FAILS (0/{total} points)"
    return f"PARTIAL ({held}/{total} points)"


def f1(d, rep):
    a = csv_or_none(d, "f1a") or csv_or_none(d, "f1")
    if a is None:
        rep.add("F1-a", "replication of 2603.03753 Fig. 5(a) within ±0.05", "NOT RUN", "")
        return
    lines = ["| α | policy | ours [95% CI] | published | diff |", "|---|---|---|---|---|"]
    ok = 0
    for alpha, pub in F1_PUBLISHED.items():
        for i, pol in enumerate(["noverify", "riskaware"]):
            t = mean_ci(by_seed(select(a, alpha=alpha, policy=pol), "workflow_success").values())
            diff = t[0] - pub[i]
            ok += abs(diff) <= 0.05
            lines.append(f"| {alpha} | {pol} | {fmt(t)} | {pub[i]:.3f} | {diff:+.3f} |")
    rep.add("F1-a", "replication of 2603.03753 Fig. 5(a) within ±0.05", count_verdict(ok, 12), "\n".join(lines))

    b = csv_or_none(d, "f1b")
    if b is None:
        for h in ("F1-b P1", "F1-b P2", "F1-b P3"):
            rep.add(h, "", "NOT RUN", "")
        return

    def gain(alpha, horizon, ww):
        ra = by_seed(select(b, alpha=alpha, horizon_s=horizon, policy="riskaware", whitewash_s=ww), "workflow_success")
        mr = by_seed(select(b, alpha=alpha, horizon_s=horizon, policy="map_ra", whitewash_s=ww), "workflow_success")
        return diff_ci(mr, ra)

    rows, held = ["| α | MAP-L+RA − RA at 60 s [CI] | CI contains 0 |", "|---|---|---|"], 0
    for alpha in F1_PUBLISHED:
        t = gain(alpha, 60.0, 0.0)
        c = t[1] <= 0 <= t[2]
        held += c
        rows.append(f"| {alpha} | {fmt(t)} | {c} |")
    rep.add("F1-b P1", "at 60 s MAP-L+RA does not beat Risk-Aware (CI contains 0 at every α)", count_verdict(held, 6), "\n".join(rows))

    for hid, claim, ww, alphas, thr in [
        ("F1-b P2", "at 3600 s MAP-L+RA − RA ≥ 0.10 at α ∈ {0.2..0.5}", 0.0, [0.2, 0.3, 0.4, 0.5], 0.10),
        ("F1-b P3", "with 60 s whitewashing, MAP-L+RA − RA ≥ 0.05 at α ∈ {0.2, 0.3}", 60.0, [0.2, 0.3], 0.05),
    ]:
        rows, held = [f"| α | gain [CI] | lower bound ≥ {thr} |", "|---|---|---|"], 0
        for alpha in alphas:
            t = gain(alpha, 3600.0, ww)
            c = t[1] >= thr
            held += c
            rows.append(f"| {alpha} | {fmt(t)} | {c} |")
        rep.add(hid, claim, count_verdict(held, len(alphas)), "\n".join(rows))


def d5(d, rep):
    x = csv_or_none(d, "e1-d5")
    if x is None:
        rep.add("D5", "issuer-weighting decision", "NOT RUN", "")
        return None
    arms = ["w0", "w1", "w2"]

    def avg(w, metric):
        # Per seed: the mean over attacks; then bootstrap over seeds.
        per = {}
        for att in ATTACKS:
            for s, v in by_seed(select(x, weights=w, attack=att), metric).items():
                per.setdefault(s, []).append(v)
        return {s: sum(v) / len(v) for s, v in per.items() if len(v) == len(ATTACKS)}

    share = {w: avg(w, "attacker_share_ss") for w in arms}
    succ = {w: avg(w, "honest_success_ss") for w in arms}
    cis = {w: mean_ci(share[w].values()) for w in arms}
    best = min(arms, key=lambda w: cis[w][0])
    succ_mean = {w: mean_ci(succ[w].values())[0] for w in arms}

    def overlaps(w):
        return not (cis[w][1] > cis[best][2] or cis[best][1] > cis[w][2])

    # Repaired rule (Amendment A4): a simpler arm (W0 < W1 < W2) replaces the
    # lowest-share arm only if its share CI overlaps the best's AND its
    # honest success is not more than 2 points below the best's.
    winner = best
    for w in arms:
        if w == best:
            break
        if overlaps(w) and succ_mean[w] >= succ_mean[best] - 0.02:
            winner = w
            break
    # Original wording (prereg-v1), kept on record: overlap OR success within
    # 2 points.
    original = best
    for w in arms:
        if w == best:
            break
        if overlaps(w) or abs(succ_mean[w] - succ_mean[best]) < 0.02:
            original = w
            break
    rows = ["| weighting | steady attacker share, mean over attacks [CI] | honest success [CI] |", "|---|---|---|"]
    for w in arms:
        rows.append(f"| {w} | {fmt(cis[w], 4)} | {fmt(mean_ci(succ[w].values()), 4)} |")
    rows.append(f"\nLowest share: {best}. Repaired rule (Amendment A4) selects: **{winner}**.")
    rows.append(
        f"\nFor the record, the rule as worded in `prereg-v1` would select: **{original}**. "
        "Its tie clause (\"or differ by < 2 points of honest success\") is satisfied almost always in E1, "
        "because the shared fallback keeps honest success near 0.99 in every arm (Amendment A4)."
    )
    rep.add("D5", "issuer-weighting decision (α = 0.2, all attacks; rule per Amendment A4)", f"WINNER = {winner}", "\n".join(rows))
    return winner


def e1(d, rep, winner):
    m = csv_or_none(d, "e1-main")
    if m is None:
        rep.add("H1", "Ours-L share ≤ 0.5 × B1 and ≤ 0.5 × B2", "NOT RUN", "")
    else:
        rows = ["| α | attack | Ours/B1 [CI] | Ours/B2 [CI] | holds |", "|---|---|---|---|---|"]
        held = total = 0
        for alpha in [0.05, 0.1, 0.2, 0.3]:
            for att in ATTACKS:
                o = by_seed(select(m, alpha=alpha, attack=att, arm="ours"), "attacker_share_ss")
                r1 = ratio_ci(o, by_seed(select(m, alpha=alpha, attack=att, arm="b1"), "attacker_share_ss"))
                r2 = ratio_ci(o, by_seed(select(m, alpha=alpha, attack=att, arm="b2"), "attacker_share_ss"))
                ok = r1[2] <= 0.5 and r2[2] <= 0.5
                held += ok
                total += 1
                rows.append(f"| {alpha} | {att} | {fmt(r1)} | {fmt(r2)} | {ok} |")
        rep.add("H1", f"Ours-L ({winner}) steady-state attacker share ≤ 0.5 × B1 and ≤ 0.5 × B2", count_verdict(held, total), "\n".join(rows))

        # Descriptive: every arm, steady-state attacker share.
        arms = ["b0", "b1", "b2", "b2l", "ours", "ours_c", "a_prior", "a_unilateral"]
        rows = ["| α | attack | " + " | ".join(arms) + " |", "|---|---|" + "---|" * len(arms)]
        for alpha in [0.0, 0.05, 0.1, 0.2, 0.3, 0.5]:
            for att in ATTACKS:
                cells = [f"{mean_ci(by_seed(select(m, alpha=alpha, attack=att, arm=a), 'attacker_share_ss').values())[0]:.3f}" for a in arms]
                rows.append(f"| {alpha} | {att} | " + " | ".join(cells) + " |")
        rep.sections.append("### E1: steady-state attacker share, all arms (means)\n\n" + "\n".join(rows) + "\n")

        # L1 cold start: Ours-L rounds 1-5 share > B2's.
        rows, held, tot = ["| α | attack | Ours − B2, rounds 1–5 [CI] | occurs |", "|---|---|---|---|"], 0, 0
        for alpha in [0.05, 0.1, 0.2, 0.3, 0.5]:
            for att in ATTACKS:
                t = diff_ci(by_seed(select(m, alpha=alpha, attack=att, arm="ours"), "attacker_share_r1_5"),
                            by_seed(select(m, alpha=alpha, attack=att, arm="b2"), "attacker_share_r1_5"))
                occ = t[1] > 0
                held += occ
                tot += 1
                rows.append(f"| {alpha} | {att} | {fmt(t)} | {occ} |")
        rep.add("L1", "cold start: Ours-L (no canary) worse than B2 in rounds 1–5", count_verdict(held, tot), "\n".join(rows))

        # L3 on-off: share at the start of an off phase > own steady state.
        rows, held, tot = ["| α | off-start − steady [CI] | occurs |", "|---|---|---|"], 0, 0
        for alpha in [0.05, 0.1, 0.2, 0.3, 0.5]:
            sel = select(m, alpha=alpha, attack="onoff", arm="ours")
            t = diff_ci(by_seed(sel, "attacker_share_off_start"), by_seed(sel, "attacker_share_ss"))
            occ = t[1] > 0
            held += occ
            tot += 1
            rows.append(f"| {alpha} | {fmt(t)} | {occ} |")
        rep.add("L3", "on-off: decay lags behavior at the start of each off phase", count_verdict(held, tot), "\n".join(rows))

    s = csv_or_none(d, "e1-sparse")
    if s is None:
        rep.add("L2", "sparse evidence: W0 not significantly below B0", "NOT RUN", "")
    else:
        rows, held = ["| attack | Ours(W0) − B0 [CI] | not significantly below |", "|---|---|---|"], 0
        for att in ATTACKS:
            t = diff_ci(by_seed(select(s, attack=att, arm="ours"), "attacker_share_ss"),
                        by_seed(select(s, attack=att, arm="b0"), "attacker_share_ss"))
            occ = t[2] >= 0
            held += occ
            rows.append(f"| {att} | {fmt(t)} | {occ} |")
        rep.add("L2", "sparse evidence (0.1 tasks/round): W0 not significantly below B0", count_verdict(held, len(ATTACKS)), "\n".join(rows))

    x = csv_or_none(d, "e1-d5")
    if x is not None:
        rows, held = ["| weighting | − W0 under camouflage [CI] | worse |", "|---|---|---|"], 0
        w0 = by_seed(select(x, weights="w0", attack="camouflage"), "attacker_share_ss")
        for w in ["w1", "w2"]:
            t = diff_ci(by_seed(select(x, weights=w, attack="camouflage"), "attacker_share_ss"), w0)
            occ = t[1] > 0
            held += occ
            rows.append(f"| {w} | {fmt(t)} | {occ} |")
        rep.add("L4", "camouflage: W1 and W2 worse than W0", count_verdict(held, 2), "\n".join(rows))


def e2(d, rep):
    x = csv_or_none(d, "e2")
    if x is None:
        rep.add("H2", "countersigned resume wastes ≥ 60% less than restart", "NOT RUN", "")
        return
    cs = by_seed(select(x, L=1000, lambda_L=1.0, c=8, f_dishonest=0.0, arm="countersigned"), "waste_parts")
    rs = by_seed(select(x, L=1000, lambda_L=1.0, c=8, f_dishonest=0.0, arm="restart"), "waste_parts")
    t = ratio_ci(cs, rs)
    rep.add("H2", "at λL = 1, L = 10³, c = 8: waste(countersigned)/waste(restart) upper CI ≤ 0.4",
            "HOLDS" if t[2] <= 0.4 else "FAILS",
            f"ratio {fmt(t, 4)}\n\nH2-bound (C-5) is asserted inside every countersigned run; a violation aborts the run, so a completed CSV means 0 violations.")
    rows = ["| L | λL | c | waste | signatures | checkpoint bytes | argmin c for κ=0.01/0.1/1 |", "|---|---|---|---|---|---|---|"]
    for L in [100, 1000, 10000]:
        for lam in [0.1, 1.0, 3.0]:
            best = {}
            for kappa in [0.01, 0.1, 1.0]:
                cost = {}
                for c in [1, 8, 32, 128, 0]:
                    sel = select(x, L=L, lambda_L=lam, c=c, f_dishonest=0.0, arm="countersigned")
                    w = mean_ci(by_seed(sel, "waste_parts").values())[0]
                    sg = mean_ci(by_seed(sel, "signatures").values())[0]
                    cost[c] = w + kappa * sg
                best[kappa] = min(cost, key=cost.get)
            for c in [1, 8, 32, 128, 0]:
                sel = select(x, L=L, lambda_L=lam, c=c, f_dishonest=0.0, arm="countersigned")
                w = mean_ci(by_seed(sel, "waste_parts").values())
                sg = mean_ci(by_seed(sel, "signatures").values())[0]
                by = mean_ci(by_seed(sel, "checkpoint_bytes").values())[0]
                tag = "/".join("∞" if best[k] == 0 else str(best[k]) for k in [0.01, 0.1, 1.0]) if c == 1 else ""
                rows.append(f"| {L} | {lam} | {'∞' if c == 0 else c} | {fmt(w, 1)} | {sg:.0f} | {by:.0f} | {tag} |")
    po = select(x, L=1000, lambda_L=3.0, c=8, f_dishonest=0.2, arm="provider_only")
    rows.append(f"\nProvider-only resume at f = 0.2, L = 10³, λL = 3: overbilled Parts {fmt(mean_ci(by_seed(po, 'overbilled_parts').values()), 1)}, "
                f"integrity failures per task {fmt(mean_ci(by_seed(po, 'integrity_failures').values()))}.")
    rep.sections.append("### E2: waste/signature trade-off (countersigned, f = 0)\n\n" + "\n".join(rows) + "\n")


def e3(d, rep):
    a = csv_or_none(d, "e3a")
    if a is None:
        rep.add("H3", "with D-6, attacker share ≤ fair share (+0.01)", "NOT RUN", "")
    else:
        rows, held, tot = ["| n | a | share without D-6 | share with D-6 [CI] | (share − a) upper ≤ 0.01 |", "|---|---|---|---|---|"], 0, 0
        for n in [1000, 10000]:
            for frac in [0.01, 0.05, 0.1]:
                off = mean_ci(by_seed(select(a, n=n, a=frac, filter="off"), "attacker_share").values())
                on = by_seed(select(a, n=n, a=frac, filter="on"), "attacker_share")
                t = mean_ci([v - frac for v in on.values()])
                ok = t[2] <= 0.01
                held += ok
                tot += 1
                rows.append(f"| {n} | {frac} | {off[0]:.3f} | {fmt(mean_ci(on.values()))} | {ok} |")
        rep.add("H3", "with the D-6 filter the attacker's candidate share ≤ its fair share a (upper CI of share − a ≤ 0.01)", count_verdict(held, tot), "\n".join(rows))

    for stem, title in [
        ("e3b", "E3b: grinding into a target shard, one core (descriptive)"),
        ("e3b-multicore", "E3b, EXPLORATORY (Amendment A5): grinding at 16 cores"),
    ]:
        b = csv_or_none(d, stem)
        if b is None:
            continue
        rows = ["| n | window | epoch | attacker share of target shard [CI] |", "|---|---|---|---|"]
        for n in [1000.0, 10000.0, 100000.0]:
            for w in [3600.0, 86400.0, 604800.0]:
                for e in [0.0, 3600.0, 86400.0]:
                    t = mean_ci(by_seed(select(b, n=n, window_s=w, epoch_s=e), "attacker_shard_share").values())
                    rows.append(f"| {int(n)} | {int(w)} s | {'fixed' if e == 0 else str(int(e)) + ' s'} | {fmt(t)} |")
        rep.sections.append(f"### {title}\n\n" + "\n".join(rows) + "\n")

    c = csv_or_none(d, "e3c")
    if c is not None:
        rows = ["| n | churn | f | success 1 path [CI] | success 3 paths [CI] | hops 1/3 | messages 1/3 |", "|---|---|---|---|---|---|---|"]
        for n in [1000, 10000, 100000]:
            for ch in [0.0, 0.1, 0.3]:
                for f in [0.0, 0.1, 0.2, 0.3]:
                    p1 = select(c, n=n, churn=ch, f_malicious=f, paths=1)
                    p3 = select(c, n=n, churn=ch, f_malicious=f, paths=3)
                    g = lambda s, k: mean_ci(by_seed(s, k).values())
                    rows.append(f"| {n} | {ch} | {f} | {fmt(g(p1, 'success_rate'))} | {fmt(g(p3, 'success_rate'))} | "
                                f"{g(p1, 'hops')[0]:.1f}/{g(p3, 'hops')[0]:.1f} | {g(p1, 'messages')[0]:.1f}/{g(p3, 'messages')[0]:.1f} |")
        rep.sections.append("### E3c: lookups under malicious routers (descriptive)\n\n" + "\n".join(rows) + "\n")

    dd = csv_or_none(d, "e3d")
    if dd is None:
        rep.add("H4", "pull has lower regret than descriptor-carried at TTL ≥ 60 s", "NOT RUN", "")
    else:
        rows, held, tot = ["| ρ | TTL | pull − descriptor regret [CI] | descriptor msgs/query | holds |", "|---|---|---|---|---|"], 0, 0
        for rho in [0.5, 0.8]:
            for ttl in [10.0, 60.0, 600.0]:
                sel = select(dd, rho=rho, ttl_s=ttl)
                t = diff_ci(by_seed(sel, "pull_regret"), by_seed(sel, "desc_regret"))
                msgs = mean_ci(by_seed(sel, "desc_msgs_per_query").values())[0]
                ok = t[2] < 0
                if ttl >= 60:
                    held += ok
                    tot += 1
                rows.append(f"| {rho} | {int(ttl)} s | {fmt(t)} | {msgs:.2f} (pull: 20) | {ok if ttl >= 60 else '(no hypothesis)'} |")
        rep.add("H4", "pull-on-demand regret < descriptor-carried regret at TTL ≥ 60 s", count_verdict(held, tot), "\n".join(rows))


def bench_ns(d, name):
    p = os.path.join(d, "bench", name, "estimates.json")
    if not os.path.exists(p):
        return None
    with open(p, encoding="utf-8") as f:
        return json.load(f)["mean"]["point_estimate"]


def e4(d, rep):
    x = csv_or_none(d, "e4")
    if x is None:
        rep.add("H5", "metering properties and < 1% time overhead at c ≥ 8", "NOT RUN", "")
        return
    ck = bench_ns(d, "checkpoint_cycle")
    rc = bench_ns(d, "receipt_dual")
    rows = ["| c | λL | max exposure / bound (atomic) | checkpoints | time overhead vs 20 ms/token |", "|---|---|---|---|---|"]
    held = tot = 0
    for c in [1, 8, 32, 128]:
        for lam in [0.0, 1.0]:
            sel = select(x, c=c, lambda_L=lam)
            ex = mean_ci(by_seed(sel, "max_exposure_atomic").values())[0]
            bd = mean_ci(by_seed(sel, "exposure_bound_atomic").values())[0]
            n_ck = mean_ci(by_seed(sel, "checkpoints").values())[0]
            parts = mean_ci(by_seed(sel, "parts").values())[0]
            if ck is None or rc is None:
                ov = "bench missing"
            else:
                frac = (n_ck * ck + rc) / (parts * 20e6)
                ov = f"{100 * frac:.4f}%"
                if c >= 8:
                    held += frac < 0.01
                    tot += 1
            rows.append(f"| {c} | {lam} | {ex:.0f} / {bd:.0f} | {n_ck:.1f} | {ov} |")
    rows.append("\nH5-a (monotone, idempotent, non-negative, exposure ≤ price(c)) is asserted inside every run; a completed CSV means no violation.")
    rep.add("H5", "H5-a asserted; H5-b time overhead < 1% at c ≥ 8", count_verdict(held, tot) if tot else "NOT RUN (bench)", "\n".join(rows))


def main():
    d = sys.argv[1]
    rep = Report()
    f1(d, rep)
    winner = d5(d, rep)
    e1(d, rep, winner or "?")
    e2(d, rep)
    e3(d, rep)
    e4(d, rep)
    out = ["# Evaluation report", "", f"Results: `{d}`. Generated by `eval/analysis/analyze.py` from the CSVs only; "
           "rules as pre-registered in `eval/HYPOTHESES.md` (tag `prereg-v1`) plus its Amendments.", "",
           "| id | claim | verdict |", "|---|---|---|"]
    out += [f"| {h} | {c} | {v} |" for h, c, v in rep.summary]
    out += [""] + rep.sections
    with open(os.path.join(d, "REPORT.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(out))
    for h, c, v in rep.summary:
        print(f"{h:10} {v}")


if __name__ == "__main__":
    main()
