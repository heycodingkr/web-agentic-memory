"""Separate failures (the memory should have prevented them) from invalid runs (not measurable).

docs/test/benchmarks.md section 8. Host adapters and fixtures emit normalized signals; this module maps them
to a class. Invalid runs are excluded from the statistics, reported with their reason, and capped at 20% per suite.
"""

SIGNAL_CLASS = {
    "auth-required": "invalid-auth",
    "captcha": "invalid-auth",
    "two-factor": "invalid-auth",
    "quota-exceeded": "invalid-quota",
    "rate-limited": "invalid-quota",
    "service-5xx": "invalid-service",
    "generation-server-error": "invalid-service",
    "ui-anchor-missing": "invalid-ui-drift",
    "config-dir-mismatch": "invalid-env",
    "clock-skew": "invalid-env",
    "profile-reuse-detected": "invalid-env",
    "harness-error": "invalid-env",
    "no-terminal-event": "invalid-env",
    "feature-unavailable": "unavailable",
}
# A harness fault shadows everything; a feature that does not exist makes the rest moot.
PRECEDENCE = ["invalid-env", "unavailable", "invalid-auth", "invalid-quota", "invalid-service", "invalid-ui-drift"]
INVALID_LIMIT = 0.20


def classify(signals):
    """Class for a set of signals, or None when the run is valid."""
    unknown = [s for s in signals if s not in SIGNAL_CLASS]
    if unknown:
        raise ValueError(f"unknown signals: {unknown}")
    found = {SIGNAL_CLASS[s] for s in signals}
    for cls in PRECEDENCE:
        if cls in found:
            return cls
    return None


def classify_run(run):
    """run: {"signals": [...], "events": [{"type": ...}]}. A run without a done/failed event is invalid-env."""
    signals = list(run.get("signals", []))
    events = run.get("events")
    if events is not None and not any(e.get("type") in ("done", "failed") for e in events):
        signals.append("no-terminal-event")
    return classify(signals)


def signals_from_http(status):
    """Signals implied by a service response status. 403 is ambiguous and yields none."""
    if status == 401:
        return ["auth-required"]
    if status == 429:
        return ["rate-limited"]
    if 500 <= status <= 599:
        return ["service-5xx"]
    return []


def invalid_rate(runs):
    return sum(1 for r in runs if classify_run(r)) / len(runs) if runs else 0.0


def suite_valid(runs, limit=INVALID_LIMIT):
    return invalid_rate(runs) <= limit
