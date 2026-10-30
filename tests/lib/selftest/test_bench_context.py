import tempfile
import unittest
from pathlib import Path

from wamtest.bench.context import ActionBlocked, RunContext
from wamtest.evidence import RunDir, read_jsonl
from wamtest.procs import Isolation


class RunContextPrivacyTest(unittest.TestCase):
    def test_blocked_action_trace_redacts_url_query_and_fragment(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            isolation_root = root / "isolation"
            isolation_root.mkdir()
            isolation = Isolation("blocked-action-privacy", root=isolation_root)
            evidence = RunDir("blocked-action-privacy", root=root / "evidence")
            context = RunContext(evidence, isolation, allow_hosts=["fixture.localhost"])
            actions = []
            url = "http://fixture.localhost:8080/api/publish?token=sentinel-secret#confirm"
            try:
                with self.assertRaises(ActionBlocked):
                    context.perform("publish", lambda: actions.append(True), url=url)

                self.assertEqual(actions, [])
                events = context.events_snapshot()
                self.assertEqual(events[-1]["type"], "blocked-action")
                self.assertEqual(events[-1]["value"], "http://fixture.localhost")
                saved = read_jsonl(evidence.path / "trace.jsonl")
                self.assertEqual(saved[-1]["value"], "http://fixture.localhost")
                self.assertNotIn("sentinel-secret", str(events))
                self.assertNotIn("confirm", str(events))
                self.assertNotIn(url, str(saved))
            finally:
                evidence.finish()
                isolation.cleanup()


if __name__ == "__main__":
    unittest.main()
