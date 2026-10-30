"""Repeatability of time-to-submit measurements (HARN-05)."""
import math
import statistics

DEFAULT_CV_LIMIT = 0.15
DEFAULT_MIN_SAMPLES = 5


def assess(values, *, limit=DEFAULT_CV_LIMIT, min_samples=DEFAULT_MIN_SAMPLES):
    """Assess TTS measurements without hiding missing or invalid observations.

    ``None`` represents a planned run that did not reach submit. It remains in
    the planned count and prevents a reproducible verdict. CV uses sample
    standard deviation divided by the mean.
    """
    if isinstance(limit, bool) or not isinstance(limit, (int, float)) or not math.isfinite(limit) or limit < 0:
        raise ValueError("limit must be a finite nonnegative number")
    if isinstance(min_samples, bool) or not isinstance(min_samples, int) or min_samples < 2:
        raise ValueError("min_samples must be an integer of at least 2")

    planned_values = list(values)
    measured = []
    for value in planned_values:
        if value is None:
            continue
        if (isinstance(value, bool) or not isinstance(value, (int, float))
                or not math.isfinite(value) or value < 0):
            raise ValueError("measurements must be finite nonnegative numbers or None")
        measured.append(float(value))

    n = len(measured)
    mean = statistics.fmean(measured) if measured else None
    sd = statistics.stdev(measured) if n >= 2 else None
    cv = sd / mean if sd is not None and mean is not None and mean > 0 else None
    complete = n == len(planned_values)
    reproducible = bool(complete and n >= min_samples and cv is not None and cv <= limit)
    return {
        "n": n,
        "planned": len(planned_values),
        "complete": complete,
        "mean": round(mean, 6) if mean is not None else None,
        "sd": round(sd, 6) if sd is not None else None,
        "cv": round(cv, 6) if cv is not None else None,
        "limit": limit,
        "min_samples": min_samples,
        "reproducible": reproducible,
    }
