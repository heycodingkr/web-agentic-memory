"""Fail-closed HTTP request interception for live CDP benchmark pages (HARN-10)."""
import threading
import time

from ..cdp import CDPError


class BrowserNetworkGuard:
    """Intercept every page request and continue only URLs accepted by RunContext.

    The guard owns the page lifecycle while active. Close the guard to close its
    page and stop interception; leaving Fetch enabled without its listener would
    strand requests, so it is intentionally not detached from a live page.
    """

    def __init__(self, page, context):
        self.page = page
        self.context = context
        self._stop = threading.Event()
        self._condition = threading.Condition()
        self._handled = 0
        self._error = None
        self._thread = None

    @property
    def handled_count(self):
        with self._condition:
            return self._handled

    def start(self):
        if self._thread is not None:
            raise RuntimeError("browser network guard already started")
        self._thread = threading.Thread(target=self._serve, name="wam-cdp-request-guard", daemon=True)
        self._thread.start()
        try:
            self.page.call("Fetch.enable", {"patterns": [{"urlPattern": "*"}]})
        except Exception:
            self._stop.set()
            self._thread.join(timeout=1)
            raise
        return self

    def wait_for_count(self, count, timeout=3.0):
        deadline = time.monotonic() + timeout
        with self._condition:
            while self._handled < count and self._error is None:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return False
                self._condition.wait(remaining)
            if self._error is not None:
                raise RuntimeError(f"browser request guard failed ({self._error})")
            return self._handled >= count

    def _record_error(self, error):
        with self._condition:
            self._error = type(error).__name__
            self._condition.notify_all()

    def _send(self, method, params):
        return self.page.cdp.send(method, params, session=self.page.session, timeout=5.0)

    def _handle(self, event):
        params = event.get("params") or {}
        request_id = params.get("requestId")
        request = params.get("request") or {}
        url = request.get("url")
        decision = self.context.check_browser_request(url if isinstance(url, str) else "")
        if not request_id:
            raise ValueError("Fetch.requestPaused omitted requestId")
        if decision["allowed"]:
            self._send("Fetch.continueRequest", {"requestId": request_id})
        else:
            self._send("Fetch.failRequest", {"requestId": request_id, "errorReason": "BlockedByClient"})
        with self._condition:
            self._handled += 1
            self._condition.notify_all()

    def _serve(self):
        while not self._stop.is_set():
            try:
                event = self.page.cdp.wait_event("Fetch.requestPaused", self.page.session, timeout=0.1)
            except CDPError as error:
                if "timeout waiting for event" in str(error):
                    continue
                if not self._stop.is_set():
                    self._record_error(error)
                return
            try:
                self._handle(event)
            except Exception as error:
                params = event.get("params") or {}
                request_id = params.get("requestId")
                if request_id:
                    try:
                        self._send("Fetch.failRequest", {"requestId": request_id, "errorReason": "BlockedByClient"})
                    except Exception:
                        pass
                self._record_error(error)
                return

    def close(self):
        if self._thread is None:
            return
        try:
            self.page.close()
        finally:
            self._stop.set()
            self._thread.join(timeout=2.0)
        if self._thread.is_alive():
            raise RuntimeError("browser request guard did not stop")

    def __enter__(self):
        return self.start()

    def __exit__(self, *_exc):
        self.close()
        return False
