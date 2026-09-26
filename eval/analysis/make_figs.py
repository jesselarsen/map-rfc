"""Figures for the paper, from the CSVs only.

    python eval/analysis/make_figs.py eval/results/<sha>

Writes <dir>/figs/*.pdf and *.png. Every point is a mean over seeds, with
95% bootstrap CIs as error bars (stats.py). Figures whose CSV is absent are
skipped with a message.
"""

import os
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

from analyze import ATTACKS, F1_PUBLISHED  # noqa: E402
from stats import by_seed, load, mean_ci, select  # noqa: E402


def errbar(ax, xs, rows_for, metric, label, **kw):
    ms, lo, hi = [], [], []
    for x in xs:
        m, l, h = mean_ci(by_seed(rows_for(x), metric).values())
        ms.append(m)
        lo.append(m - l)
        hi.append(h - m)
    ax.errorbar(xs, ms, yerr=[lo, hi], label=label, capsize=2, marker="o", ms=3, **kw)


def save(fig, d, name):
    os.makedirs(os.path.join(d, "figs"), exist_ok=True)
    for ext in ("pdf", "png"):
        fig.savefig(os.path.join(d, "figs", f"{name}.{ext}"), bbox_inches="tight", dpi=150)
    plt.close(fig)
    print("wrote", name)


def need(d, stem):
    p = os.path.join(d, stem + ".csv")
    if not os.path.exists(p):
        print(f"skip: {stem}.csv missing")
        return None
    return load(p)


def fig_f1(d):
    a, b = need(d, "f1a"), need(d, "f1b")
    if a is None:
        return
    alphas = sorted(F1_PUBLISHED)
    fig, ax = plt.subplots(figsize=(4.2, 3))
    ax.plot(alphas, [F1_PUBLISHED[x][0] for x in alphas], "k--", label="No Verify (published)")
    ax.plot(alphas, [F1_PUBLISHED[x][1] for x in alphas], "k:", label="Risk-Aware (published)")
    errbar(ax, alphas, lambda x: select(a, alpha=x, policy="noverify"), "workflow_success", "No Verify (ours)")
    errbar(ax, alphas, lambda x: select(a, alpha=x, policy="riskaware"), "workflow_success", "Risk-Aware (ours)")
    if b is not None:
        errbar(ax, alphas, lambda x: select(b, alpha=x, policy="map_ra", horizon_s=3600.0, whitewash_s=0.0),
               "workflow_success", "MAP-L+RA, 3600 s")
    ax.set_xlabel("Sybil fraction α")
    ax.set_ylabel("workflow success")
    ax.legend(fontsize=6)
    save(fig, d, "f1_replication")


def fig_e1(d):
    m = need(d, "e1-main")
    if m is None:
        return
    alphas = [0.0, 0.05, 0.1, 0.2, 0.3, 0.5]
    arms = [("b1", "B1 global"), ("b2", "B2 canary"), ("b2l", "B2L"), ("ours", "Ours-L"), ("ours_c", "Ours-L+C")]
    fig, axes = plt.subplots(1, len(ATTACKS), figsize=(2.2 * len(ATTACKS), 2.4), sharey=True)
    for ax, att in zip(axes, ATTACKS):
        for arm, label in arms:
            errbar(ax, alphas, lambda x: select(m, alpha=x, attack=att, arm=arm), "attacker_share_ss", label)
        ax.plot(alphas, alphas, color="grey", lw=0.5)  # B0 = random = α
        ax.set_title(att, fontsize=8)
        ax.set_xlabel("α")
    axes[0].set_ylabel("attacker share (rounds 31–60)")
    axes[-1].legend(fontsize=6)
    save(fig, d, "e1_attacker_share")


def fig_e2(d):
    x = need(d, "e2")
    if x is None:
        return
    cs = [1, 8, 32, 128]
    fig, ax = plt.subplots(figsize=(4, 3))
    for lam in [0.1, 1.0, 3.0]:
        errbar(ax, cs, lambda c: select(x, L=1000, lambda_L=lam, c=c, f_dishonest=0.0, arm="countersigned"),
               "waste_parts", f"countersigned, λL={lam}")
        m, _, _ = mean_ci(by_seed(select(x, L=1000, lambda_L=lam, c=8, f_dishonest=0.0, arm="restart"), "waste_parts").values())
        ax.axhline(m, ls=":", lw=0.8, label=f"restart, λL={lam}")
    ax.set_xscale("log", base=2)
    ax.set_yscale("symlog")
    ax.set_xlabel("checkpoint interval c (Parts)")
    ax.set_ylabel("wasted Parts per task (L = 1000)")
    ax.legend(fontsize=6)
    save(fig, d, "e2_waste")


def fig_e3(d):
    c = need(d, "e3c")
    if c is not None:
        fs = [0.0, 0.1, 0.2, 0.3]
        fig, ax = plt.subplots(figsize=(4, 3))
        for n in [1000, 100000]:
            for p in [1, 3]:
                errbar(ax, fs, lambda f: select(c, n=n, churn=0.1, f_malicious=f, paths=p), "success_rate", f"N={n}, {p} path(s)")
        ax.set_xlabel("malicious router fraction f (churn 10%)")
        ax.set_ylabel("lookup success")
        ax.legend(fontsize=6)
        save(fig, d, "e3c_lookup")
    dd = need(d, "e3d")
    if dd is not None:
        ttls = [10.0, 60.0, 600.0]
        fig, ax = plt.subplots(figsize=(4, 3))
        for rho in [0.5, 0.8]:
            errbar(ax, ttls, lambda t: select(dd, rho=rho, ttl_s=t), "desc_regret", f"descriptor, ρ={rho}")
            errbar(ax, ttls, lambda t: select(dd, rho=rho, ttl_s=t), "pull_regret", f"pull, ρ={rho}")
        ax.set_xscale("log")
        ax.set_xlabel("descriptor TTL (s)")
        ax.set_ylabel("selection regret (queue length)")
        ax.legend(fontsize=6)
        save(fig, d, "e3d_staleness")


if __name__ == "__main__":
    d = sys.argv[1]
    fig_f1(d)
    fig_e1(d)
    fig_e2(d)
    fig_e3(d)
