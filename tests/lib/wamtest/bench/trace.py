"""Time and call metrics from a normalized trace (docs/test/benchmarks.md section 4).

Events are dicts with "t" (seconds, monotonic) and "type": tool_call, tool_result (is_error), submit,
blocked-action, done, failed. The headline numbers are time to submit and tool calls to submit; the wait after
the submit (server-side generation) is reported separately because memory cannot shorten it. A run without an
explicit done or failed event is not complete: a silent log is not evidence of completion.
"""


from datetime import datetime
import math


def clock_error_ms(events):
    """Return the largest wall-clock vs monotonic elapsed-time divergence.

    Each normalized event must carry monotonic seconds in ``t`` and an ISO-8601
    UTC wall timestamp in ``ts``. Comparing elapsed deltas from the first event
    avoids relying on the clocks' unrelated origins. Fewer than two valid
    stamps means accuracy cannot be established.
    """
    if len(events) < 2:
        return None
    samples = []
    for event in events:
        stamp = event.get("t")
        wall = event.get("ts")
        if (isinstance(stamp, bool) or not isinstance(stamp, (int, float))
                or not math.isfinite(stamp) or stamp < 0 or not isinstance(wall, str)):
            return None
        try:
            parsed = datetime.fromisoformat(wall.replace("Z", "+00:00"))
        except ValueError:
            return None
        if parsed.tzinfo is None:
            return None
        samples.append((float(stamp), parsed.timestamp()))

    base_t, base_wall = samples[0]
    return round(max(abs(((wall - base_wall) - (stamp - base_t)) * 1000)
                     for stamp, wall in samples[1:]), 3)


def analyze(events):
    calls = [e for e in events if e.get("type") == "tool_call"]
    submit = next((e for e in events if e.get("type") == "submit"), None)
    end = next((e for e in reversed(events) if e.get("type") in ("done", "failed")), None)
    first = calls[0]["t"] if calls else None
    out = {
        "completed": end is not None,
        "end": end["type"] if end else None,
        "submitted": submit is not None,
        "tool_calls": len(calls),
        "tool_errors": sum(1 for e in events if e.get("type") == "tool_result" and e.get("is_error")),
        "blocked_actions": sum(1 for e in events if e.get("type") == "blocked-action"),
        "clock_error_ms": clock_error_ms(events),
    }
    if first is not None and submit is not None:
        out["tts"] = round(submit["t"] - first, 3)
        out["calls_to_submit"] = sum(1 for c in calls if c["t"] <= submit["t"])
    if submit is not None and end is not None:
        out["wait"] = round(end["t"] - submit["t"], 3)
    if first is not None and end is not None:
        out["total"] = round(end["t"] - first, 3)
    return out
