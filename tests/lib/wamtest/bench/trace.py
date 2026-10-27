"""Time and call metrics from a normalized trace (docs/test/benchmarks.md section 4).

Events are dicts with "t" (seconds, monotonic) and "type": tool_call, tool_result (is_error), submit,
blocked-action, done, failed. The headline numbers are time to submit and tool calls to submit; the wait after
the submit (server-side generation) is reported separately because memory cannot shorten it. A run without an
explicit done or failed event is not complete: a silent log is not evidence of completion.
"""


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
    }
    if first is not None and submit is not None:
        out["tts"] = round(submit["t"] - first, 3)
        out["calls_to_submit"] = sum(1 for c in calls if c["t"] <= submit["t"])
    if submit is not None and end is not None:
        out["wait"] = round(end["t"] - submit["t"], 3)
    if first is not None and end is not None:
        out["total"] = round(end["t"] - first, 3)
    return out
