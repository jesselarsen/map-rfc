"""Figure 1 (paper/figs/flow.pdf): a MAP task over A2A with a resumption.

    python paper/figs/make_flow.py

A hand-laid sequence diagram, drawn with matplotlib so the paper builds
without TikZ. Message names follow spec/binding-a2a.md.
"""

import os

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

LANES = {"A": 0.0, "R": 1.0, "B": 2.0}
TITLES = {"A": "Provider A", "R": "Requester", "B": "Provider B"}

# (from, to, y, label); y grows downward
MSGS = [
    ("R", "A", 1.0, "message/send (agentId, nonce, zone)"),
    ("A", "R", 2.0, "Parts 0..7"),
    ("A", "R", 2.8, "checkpoint k + sig_prov"),
    ("R", "A", 3.8, "sig_req"),
    ("A", "R", 4.6, "Parts 8..10"),
    ("R", "B", 6.2, "message/send (ckpt k,\nprefix 0..7)"),
    ("B", "R", 7.6, "Parts 8..L"),
    ("B", "R", 8.4, "receipt + sig_prov\n(resumed-from = H(ckpt k))"),
    ("R", "B", 9.2, "sig_req"),
]
NOTES = [
    ("R", 3.3, "verify root(0..7)"),
    ("A", 5.2, "fails"),
    ("B", 6.95, "verify root(0..7) = ckpt k"),
    ("R", 10.0, "A is owed usage(ckpt k) only (C-5)"),
]


def main():
    fig, ax = plt.subplots(figsize=(3.4, 3.9))
    y_end = 10.4
    for k, x in LANES.items():
        top = 0.35
        bottom = 5.2 if k == "A" else y_end
        ax.plot([x, x], [top, bottom], color="0.55", lw=0.8, zorder=1)
        ax.text(x, 0.1, TITLES[k], ha="center", va="bottom", fontsize=7, weight="bold")
    ax.plot([LANES["A"]], [5.2], marker="x", color="C3", ms=8, mew=2, zorder=3)
    for src, dst, y, label in MSGS:
        x0, x1 = LANES[src], LANES[dst]
        ax.annotate("", xy=(x1, y), xytext=(x0, y),
                    arrowprops=dict(arrowstyle="-|>", lw=0.8, color="C0", shrinkA=0, shrinkB=0))
        xm = (x0 + x1) / 2
        ax.text(xm, y - 0.12, label, ha="center", va="bottom", fontsize=5.2, linespacing=1.0)
    for lane, y, text in NOTES:
        x = LANES[lane]
        ha = "left" if lane != "B" else "right"
        dx = 0.05 if ha == "left" else -0.05
        color = "C3" if "fails" in text or "owed" in text else "C2"
        ax.text(x + dx, y, text, ha=ha, va="center", fontsize=5.2, color=color, style="italic")
    ax.set_xlim(-0.35, 2.35)
    ax.set_ylim(y_end + 0.2, -0.1)
    ax.axis("off")
    out = os.path.dirname(os.path.abspath(__file__))
    for ext in ("pdf", "png"):
        fig.savefig(os.path.join(out, f"flow.{ext}"), bbox_inches="tight", dpi=200)
    print("wrote flow")


if __name__ == "__main__":
    main()
