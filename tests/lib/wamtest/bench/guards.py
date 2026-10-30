"""Policy engine for irreversible or costly actions (docs/test/benchmarks.md section 2, rule 4).

Sharing, publishing, paying, and plan changes are forbidden in benchmark runs. Given a URL or a button label this
decides allow or block and records a blocked-action event. It is a pure decision function; wiring it to a browser
(request interception, a filtering proxy, or a host adapter hook) is the host adapter's job.
"""
from urllib.parse import unquote, urlsplit

DENY_LABELS = (
    "publish", "share", "send", "pay", "purchase", "buy", "checkout", "subscribe", "upgrade", "delete",
    "발행", "공유", "결제", "구매", "구독", "업그레이드", "삭제",
)
DENY_PATHS = ("/publish", "/share", "/checkout", "/payment", "/billing", "/upgrade", "/delete")


def redact_url(url):
    """Keep only scheme and host for blocked-URL evidence; drop path, port and credentials."""
    try:
        parts = urlsplit(url)
        host = parts.hostname
    except (TypeError, ValueError):
        return "invalid://invalid"
    scheme = parts.scheme.lower() or "invalid"
    if not host:
        host = "invalid"
    elif ":" in host and not host.startswith("["):
        host = f"[{host}]"
    return f"{scheme}://{host.lower()}"


class ActionGuard:
    def __init__(self, allow_hosts=None, allow_labels=(), deny_labels=DENY_LABELS, deny_paths=DENY_PATHS):
        self.allow_hosts = tuple(h.lower() for h in allow_hosts) if allow_hosts else None
        self.allow_labels = tuple(l.lower() for l in allow_labels)
        self.deny_labels = tuple(l.lower() for l in deny_labels)
        self.deny_paths = tuple(p.lower() for p in deny_paths)
        self.blocked = []

    def _block(self, kind, value, reason):
        if kind == "url" and isinstance(value, str):
            value = redact_url(value)
        self.blocked.append({"type": "blocked-action", "kind": kind, "value": value, "reason": reason})
        return {"allowed": False, "reason": reason}

    def check_url(self, url):
        parts = urlsplit(url)
        if parts.scheme.lower() not in ("http", "https"):
            return self._block("url", url, "only HTTP(S) actions are allowed")
        host = (parts.hostname or "").lower()
        if self.allow_hosts is not None and not any(host == h or host.endswith("." + h) for h in self.allow_hosts):
            return self._block("url", url, f"host {host!r} is not on the allow list")
        path = parts.path
        # Decode a bounded number of times; unresolved encoding is not safe to execute.
        for _ in range(4):
            decoded = unquote(path)
            if decoded == path:
                break
            path = decoded
        if unquote(path) != path:
            return self._block("url", url, "path has excessive nested encoding")
        path = "/" + "/".join(part for part in path.lower().replace("\\", "/").split("/") if part)
        for denied in self.deny_paths:
            route = denied.strip("/")
            if f"/{route}/" in path + "/":
                return self._block("url", url, f"path matches {denied}")
        return {"allowed": True, "reason": "ok"}

    def check_label(self, label):
        text = label.strip().lower()
        if text in self.allow_labels:
            return {"allowed": True, "reason": "allow-listed label"}
        for denied in self.deny_labels:
            if denied in text:
                return self._block("label", label, f"label contains {denied!r}")
        return {"allowed": True, "reason": "ok"}

    def emit(self, evidence):
        """Write every recorded block to a RunDir trace as blocked-action events."""
        for row in self.blocked:
            evidence.event(row["type"], kind=row["kind"], value=row["value"], reason=row["reason"])
