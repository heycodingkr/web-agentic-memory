import queue
import tempfile
import unittest
from pathlib import Path

from wamtest.bench.browser_guard import BrowserNetworkGuard
from wamtest.bench.context import RunContext
from wamtest.cdp import CDPError
from wamtest.evidence import RunDir, read_jsonl
from wamtest.procs import Isolation


class FakeCDP:
    def __init__(self, send_error=None):
        self.events = queue.Queue()
        self.commands = []
        self.send_error = send_error

    def wait_event(self, method, session, timeout):
        try:
            return self.events.get(timeout=timeout)
        except queue.Empty as error:
            raise CDPError(f"timeout waiting for event {method}") from error

    def send(self, method, params, session=None, timeout=15.0):
        self.commands.append((method, params, session))
        if method == self.send_error:
            raise CDPError(f"simulated {method} failure")
        return {}


class FakePage:
    session = "page-session"

    def __init__(self, send_error=None, enable_error=False):
        self.cdp = FakeCDP(send_error)
        self.enable_error = enable_error
        self.closed = False
        self.enabled = False

    def call(self, method, _params=None):
        if method == "Fetch.enable":
            if self.enable_error:
                raise CDPError("simulated Fetch.enable failure")
            self.enabled = True

    def close(self):
        self.closed = True


class FakeContext:
    def __init__(self, allowed=True, error=None):
        self.allowed = allowed
        self.error = error
        self.urls = []

    def check_browser_request(self, url):
        self.urls.append(url)
        if self.error:
            raise self.error
        return {"allowed": self.allowed}


class BrowserNetworkGuardTest(unittest.TestCase):
    @staticmethod
    def _request(url="http://127.0.0.1:8000/editor", request_id="request-1"):
        params = {"request": {"url": url}}
        if request_id is not None:
            params["requestId"] = request_id
        return {"method": "Fetch.requestPaused", "params": params}

    def _start_and_emit(self, context, event, page=None):
        page = page or FakePage()
        guard = BrowserNetworkGuard(page, context).start()
        page.cdp.events.put(event)
        return page, guard

    def test_allowed_request_continues_and_preserves_url_for_policy_check(self):
        context = FakeContext(allowed=True)
        page, guard = self._start_and_emit(context, self._request())
        try:
            self.assertTrue(guard.wait_for_count(1))
            self.assertEqual(context.urls, ["http://127.0.0.1:8000/editor"])
            self.assertIn(
                ("Fetch.continueRequest", {"requestId": "request-1"}, page.session),
                page.cdp.commands,
            )
            self.assertFalse(any(command[0] == "Fetch.failRequest" for command in page.cdp.commands))
        finally:
            guard.close()
        self.assertTrue(page.closed)

    def test_denied_request_is_failed_and_never_continued(self):
        context = FakeContext(allowed=False)
        page, guard = self._start_and_emit(context, self._request("https://example.com/publish"))
        try:
            self.assertTrue(guard.wait_for_count(1))
            self.assertIn(
                ("Fetch.failRequest", {"requestId": "request-1", "errorReason": "BlockedByClient"}, page.session),
                page.cdp.commands,
            )
            self.assertFalse(any(command[0] == "Fetch.continueRequest" for command in page.cdp.commands))
        finally:
            guard.close()

    def test_missing_url_is_checked_as_empty_and_denied(self):
        context = FakeContext(allowed=False)
        page, guard = self._start_and_emit(context, {"params": {"requestId": "request-2", "request": {}}})
        try:
            self.assertTrue(guard.wait_for_count(1))
            self.assertEqual(context.urls, [""])
            self.assertTrue(any(command[0] == "Fetch.failRequest" for command in page.cdp.commands))
        finally:
            guard.close()

    def test_missing_request_id_fails_the_run_closed(self):
        page, guard = self._start_and_emit(FakeContext(), self._request(request_id=None))
        try:
            with self.assertRaisesRegex(RuntimeError, "browser request guard failed"):
                guard.wait_for_count(1)
            self.assertEqual(guard.handled_count, 0)
            self.assertFalse(any(command[0] == "Fetch.continueRequest" for command in page.cdp.commands))
        finally:
            guard.close()

    def test_policy_exception_fails_request_and_surfaces_guard_error(self):
        page, guard = self._start_and_emit(
            FakeContext(error=ValueError("policy unavailable")), self._request()
        )
        try:
            with self.assertRaisesRegex(RuntimeError, "browser request guard failed"):
                guard.wait_for_count(1)
            self.assertTrue(any(command[0] == "Fetch.failRequest" for command in page.cdp.commands))
            self.assertFalse(any(command[0] == "Fetch.continueRequest" for command in page.cdp.commands))
        finally:
            guard.close()

    def test_context_logs_only_origin_when_browser_request_is_blocked(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            isolation = Isolation("browser-guard-test", root=root)
            evidence = RunDir("browser-guard-test", root=root / "evidence")
            context = RunContext(evidence, isolation, allow_hosts=["127.0.0.1"])
            try:
                result = context.check_browser_request(
                    "http://127.0.0.1:8000/api/publish?token=private#confirmation"
                )
                self.assertFalse(result["allowed"])
                event = context.events_snapshot()[0]
                self.assertEqual(event["type"], "blocked-action")
                self.assertEqual(event["value"], "http://127.0.0.1")
                self.assertNotIn("private", str(event))
                self.assertNotIn("confirmation", str(event))
                saved = read_jsonl(evidence.path / "trace.jsonl")
                self.assertEqual(saved[0]["value"], "http://127.0.0.1")
            finally:
                evidence.finish()
                isolation.cleanup()

    def test_fetch_enable_failure_stops_listener_thread(self):
        page = FakePage(enable_error=True)
        guard = BrowserNetworkGuard(page, FakeContext())
        with self.assertRaisesRegex(CDPError, "Fetch.enable"):
            guard.start()
        self.assertFalse(guard._thread.is_alive())


if __name__ == "__main__":
    unittest.main()
