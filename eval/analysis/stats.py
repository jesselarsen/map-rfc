"""Bootstrap statistics exactly as pre-registered (eval/HYPOTHESES.md §0).

- Point estimates are means of per-seed values.
- Confidence intervals are 95% percentile bootstrap with 10,000 resamples
  over seeds, from a fixed generator seed so reports are reproducible.
- Comparisons resample seed indices jointly: the same resampled index set
  is applied to both arms.
"""

import csv
import math

import numpy as np

RESAMPLES = 10_000
LEVEL = 0.95
_SEED = 20260926


def load(path):
    """Rows of a map-sim CSV, numeric fields converted to float."""
    rows = []
    with open(path, newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            out = {}
            for k, v in r.items():
                try:
                    out[k] = float(v)
                except ValueError:
                    out[k] = v
            rows.append(out)
    return rows


def select(rows, **match):
    """Rows whose fields equal `match` (numbers compared as floats)."""
    def eq(a, b):
        if isinstance(a, float) or isinstance(b, (int, float)):
            try:
                return float(a) == float(b)
            except (TypeError, ValueError):
                return False
        return a == b
    return [r for r in rows if all(eq(r.get(k), v) for k, v in match.items())]


def by_seed(rows, metric):
    """{seed: value} for one sweep point (NaN values dropped)."""
    out = {}
    for r in rows:
        v = r[metric]
        if isinstance(v, float) and math.isnan(v):
            continue
        out[int(r["seed"])] = v
    return out


def _ci(samples):
    lo, hi = np.percentile(samples, [100 * (1 - LEVEL) / 2, 100 * (1 + LEVEL) / 2])
    return float(lo), float(hi)


def mean_ci(values):
    """(mean, lo, hi) of a list of per-seed values."""
    v = np.asarray(list(values), dtype=float)
    if len(v) == 0:
        return (math.nan, math.nan, math.nan)
    rng = np.random.default_rng(_SEED)
    idx = rng.integers(0, len(v), size=(RESAMPLES, len(v)))
    boots = v[idx].mean(axis=1)
    lo, hi = _ci(boots)
    return float(v.mean()), lo, hi


def _paired(a, b):
    seeds = sorted(set(a) & set(b))
    if not seeds:
        raise ValueError("no common seeds")
    return np.array([a[s] for s in seeds], float), np.array([b[s] for s in seeds], float)


def diff_ci(a, b):
    """mean(a) - mean(b) with seeds resampled jointly. a, b: {seed: value}."""
    x, y = _paired(a, b)
    rng = np.random.default_rng(_SEED)
    idx = rng.integers(0, len(x), size=(RESAMPLES, len(x)))
    boots = x[idx].mean(axis=1) - y[idx].mean(axis=1)
    lo, hi = _ci(boots)
    return float(x.mean() - y.mean()), lo, hi


def ratio_ci(a, b):
    """mean(a) / mean(b) with seeds resampled jointly (inf if mean(b) = 0)."""
    x, y = _paired(a, b)
    rng = np.random.default_rng(_SEED)
    idx = rng.integers(0, len(x), size=(RESAMPLES, len(x)))
    num, den = x[idx].mean(axis=1), y[idx].mean(axis=1)
    with np.errstate(divide="ignore", invalid="ignore"):
        boots = np.where(den > 0, num / den, np.inf)
    lo, hi = _ci(boots)
    point = x.mean() / y.mean() if y.mean() > 0 else math.inf
    return float(point), lo, hi


def fmt(t, digits=3):
    """'m [lo, hi]' for a (mean, lo, hi) triple."""
    m, lo, hi = t
    return f"{m:.{digits}f} [{lo:.{digits}f}, {hi:.{digits}f}]"
