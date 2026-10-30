"""Statistics for the benchmark claims in docs/test/metrics.md section 1.

Median and mean/sd, a percentile bootstrap CI for the difference of medians (10,000 resamples by default,
seeded so results are reproducible), and the A/B/C claim grade. Outliers are never dropped.
"""
import random
import statistics

MIN_REDUCTION = 0.20
INVALID_LIMIT = 0.20


def summarize(values):
    vals = [float(v) for v in values]
    if not vals:
        return {"n": 0}
    return {
        "n": len(vals),
        "median": statistics.median(vals),
        "mean": statistics.fmean(vals),
        "sd": statistics.stdev(vals) if len(vals) > 1 else None,
        "min": min(vals),
        "max": max(vals),
    }


def bootstrap_diff_ci(baseline, other, n_boot=10000, alpha=0.05, seed=0):
    """Percentile bootstrap CI of median(baseline) - median(other). Groups are resampled independently.

    A positive difference means the other condition is lower (better for time and call counts).
    """
    b, o = [float(v) for v in baseline], [float(v) for v in other]
    if not b or not o:
        raise ValueError("both groups need at least one value")
    rng = random.Random(seed)
    diffs = []
    for _ in range(n_boot):
        rb = [b[rng.randrange(len(b))] for _ in b]
        ro = [o[rng.randrange(len(o))] for _ in o]
        diffs.append(statistics.median(rb) - statistics.median(ro))
    diffs.sort()
    lo = diffs[int((alpha / 2) * n_boot)]
    hi = diffs[min(n_boot - 1, int((1 - alpha / 2) * n_boot))]
    return {"point": statistics.median(b) - statistics.median(o), "lo": lo, "hi": hi, "n_boot": n_boot, "alpha": alpha, "seed": seed}


def metric_claim(baseline, other, min_reduction=MIN_REDUCTION, n_boot=10000, alpha=0.05, seed=0):
    """True claim only if the median drops by at least min_reduction AND the whole CI is above zero."""
    ci = bootstrap_diff_ci(baseline, other, n_boot, alpha, seed)
    med_b, med_o = statistics.median(baseline), statistics.median(other)
    reduction = (med_b - med_o) / med_b if med_b else None
    ok = reduction is not None and reduction >= min_reduction and ci["lo"] > 0
    return {"median_baseline": med_b, "median_other": med_o, "reduction": reduction, "ci": ci, "ok": ok}


def grade(n, criteria_met, invalid_rate, uncontrolled=False, invalid_limit=INVALID_LIMIT):
    """A: n>=5, every criterion met, invalid <= limit. B: n 3-4 or a criterion missed. C: n<3 or uncontrolled.

    Above the invalid limit the suite is reported as invalid, whatever the numbers say.
    """
    if invalid_rate > invalid_limit:
        return "invalid"
    if uncontrolled or n < 3:
        return "C"
    if n >= 5 and criteria_met:
        return "A"
    return "B"


def _valid(runs):
    return [r for r in runs if not r.get("invalid")]


def compare_condition(baseline_runs, other_runs, uncontrolled=False, seed=0, n_boot=10000, min_reduction=MIN_REDUCTION):
    """Compare one condition (M or I) with the baseline (T).

    A run is a dict: tts, calls, errors, success (bool), invalid (bool). Invalid runs are excluded from the
    statistics but counted in the invalid rate. Pre-registered primary metrics: time to submit and tool calls.
    """
    all_runs = list(baseline_runs) + list(other_runs)
    b, o = _valid(baseline_runs), _valid(other_runs)
    invalid_rate = (len(all_runs) - len(b) - len(o)) / len(all_runs) if all_runs else 0.0
    out = {"n_baseline": len(b), "n_other": len(o), "invalid_rate": invalid_rate}
    n = min(len(b), len(o))
    if n == 0:
        out.update(criteria={}, grade="invalid" if invalid_rate > INVALID_LIMIT else "C")
        return out
    def measured_claim(key):
        left = [r[key] for r in b if r.get(key) is not None]
        right = [r[key] for r in o if r.get(key) is not None]
        if not left or not right:
            return None
        return metric_claim(left, right, min_reduction, n_boot, seed=seed)

    tts, calls = measured_claim("tts"), measured_claim("calls")
    complete = all(r.get("tts") is not None and r.get("calls") is not None for r in b + o)
    succ_b, succ_o = sum(bool(r["success"]) for r in b), sum(bool(r["success"]) for r in o)
    err_b = statistics.fmean(r["errors"] for r in b)
    err_o = statistics.fmean(r["errors"] for r in o)
    criteria = {
        "tts": tts is not None and tts["ok"],
        "calls": calls is not None and calls["ok"],
        "complete_measurements": complete,
        "success_not_lower": succ_o * len(b) >= succ_b * len(o),
        "errors_not_higher": err_o <= err_b,
    }
    out.update(
        tts=tts,
        calls=calls,
        success={"baseline": succ_b, "other": succ_o, "baseline_rate": succ_b / len(b), "other_rate": succ_o / len(o)},
        errors={"baseline": err_b, "other": err_o},
        criteria=criteria,
        grade=grade(n, all(criteria.values()), invalid_rate, uncontrolled),
    )
    return out


def markdown_row(label, result):
    """One README table row: label, n, median time and calls with change, success, errors, invalid rate, grade."""
    if not result.get("tts") or not result.get("calls"):
        return f"| {label} | {result['n_other']} | n/a | n/a | | | {result['invalid_rate']:.0%} | {result['grade']} |"
    t, c = result["tts"], result["calls"]

    def change(metric):
        reduction = metric["reduction"]
        return "n/a" if reduction is None else f"{-reduction:+.0%}"
    return (
        f"| {label} | {result['n_other']} "
        f"| {t['median_baseline']:.1f}s → {t['median_other']:.1f}s ({change(t)}) "
        f"| {c['median_baseline']:.0f} → {c['median_other']:.0f} ({change(c)}) "
        f"| {result['success']['baseline']}/{result['n_baseline']} → {result['success']['other']}/{result['n_other']} "
        f"| {result['errors']['baseline']:.1f} → {result['errors']['other']:.1f} "
        f"| {result['invalid_rate']:.0%} | {result['grade']} |"
    )
