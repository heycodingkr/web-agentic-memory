"""One isolated benchmark run and the guarded boundary used by live adapters."""
import math
import time
from threading import RLock

from .guards import ActionGuard, redact_url

EVENT_TYPES = {"tool_call", "tool_result", "submit", "blocked-action", "done", "failed"}


class ActionBlocked(RuntimeError):
    pass


class RunContext:
    def __init__(self, evidence, isolation, allow_hosts):
        self.evidence = evidence
        self.isolation = isolation
        self.guard = ActionGuard(allow_hosts=allow_hosts)
        self._started = time.monotonic()
        self.events = []
        self.signals = []
        self._lock = RLock()

    def event(self, type_, **fields):
        with self._lock:
            if type_ not in EVENT_TYPES:
                raise ValueError(f"unknown event type: {type_}")
            if self.events and self.events[-1]["type"] in ("done", "failed"):
                raise ValueError("an event followed a terminal event")
            if "type" in fields or "ts" in fields:
                raise ValueError("event type and wall timestamp are reserved")
            stamp = fields.setdefault("t", round(time.monotonic() - self._started, 3))
            if type(stamp) not in (int, float) or not math.isfinite(stamp) or stamp < 0:
                raise ValueError("event time must be a finite nonnegative number")
            if self.events and stamp < self.events[-1]["t"]:
                raise ValueError("event times must be monotonic")
            if type_ == "tool_result" and type(fields.get("is_error", False)) is not bool:
                raise ValueError("is_error must be a bool")
            row = self.evidence.event(type_, **fields)
            self.events.append(row)
            return row

    def events_snapshot(self):
        with self._lock:
            return [dict(event) for event in self.events]

    def signal(self, name):
        from .invalid import classify
        classify([name])  # Unknown signals cannot silently turn into a valid run.
        with self._lock:
            self.signals.append(name)

    def check_browser_request(self, url):
        """Apply URL policy and log a redacted blocked event for the CDP network guard."""
        with self._lock:
            try:
                result = self.guard.check_url(url)
            except ValueError:
                result = {"allowed": False, "reason": "malformed URL"}
            if not result["allowed"]:
                value = redact_url(url)
                self.event("blocked-action", kind="url", value=value, reason=result["reason"])
            return result

    def perform(self, name, callback, *, url=None, label=None):
        """Check before an adapter executes an action. This is an adapter hook, not a process sandbox."""
        if url is None and label is None:
            raise ValueError("an action needs a URL or label for the guard")
        with self._lock:
            if self.events and self.events[-1]["type"] in ("done", "failed"):
                raise ValueError("an action followed a terminal event")
            for check, value in ((self.guard.check_url, url), (self.guard.check_label, label)):
                if value is None:
                    continue
                result = check(value)
                if not result["allowed"]:
                    blocked = self.guard.blocked[-1]
                    self.event("blocked-action", **{k: v for k, v in blocked.items() if k != "type"})
                    raise ActionBlocked(result["reason"])
            self.event("tool_call", name=name)
        try:
            result = callback()
        except Exception:
            self.event("tool_result", name=name, is_error=True)
            raise
        self.event("tool_result", name=name, is_error=False)
        return result
