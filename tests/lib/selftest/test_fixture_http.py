"""HARN-01 without a browser: the oracle's verdict logic and the two-port separation, over plain HTTP."""
import json
import sys
import unittest
import urllib.error
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "tests" / "env" / "fixture-sites"))
import fixture_server as fx  # noqa: E402

TITLE, PARAS = fx.TITLE, fx.PARAGRAPHS


def call(url, body=None, method=None):
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, method=method or ("POST" if data is not None else "GET"))
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        try:
            return error.code, error.read()
        finally:
            error.close()


class FixtureHttpTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = fx.FixtureServer().start()
        cls.agent = f"http://127.0.0.1:{cls.server.agent_port}"
        cls.oracle = f"http://127.0.0.1:{cls.server.oracle_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.stop()

    def run_with(self, style="rich", **kw):
        run = self.server.new_run(style, **kw)
        return run, f"{self.agent}/%s/{run.key}"

    def post(self, pattern, kind, body):
        return call(pattern % kind, body)

    def test_pages_are_served_with_the_run_config(self):
        for style in fx.STYLES:
            run, pattern = self.run_with(style)
            status, body = call(pattern % "editor")
            self.assertEqual(status, 200)
            html = body.decode()
            self.assertIn('id="title"', html)
            self.assertIn(run.key, html)
            self.assertNotIn("@@", html, "every template placeholder is filled")
            self.assertEqual('id="editor-frame"' in html, style == "iframe")
        self.assertEqual(call(f"{self.agent}/editor/nope")[0], 404)

    def test_the_oracle_is_not_reachable_through_the_agent_port(self):
        run, _ = self.run_with()
        self.assertEqual(call(f"{self.agent}/oracle/{run.key}")[0], 404)
        status, body = call(f"{self.oracle}/oracle/{run.key}")
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)["verdict"], "fail")

    def test_verdicts_for_made_up_outcomes(self):
        native = {"title": TITLE, "body": PARAS}
        cases = {
            "pass": ({"title": TITLE, "paragraphs": PARAS, "native": native}, 0, set()),
            "title": ({"title": "x", "paragraphs": PARAS, "native": native}, 0, {"title_mismatch"}),
            "blank": ({"title": TITLE, "paragraphs": [PARAS[0], "", PARAS[1]], "native": native}, 0, {"paragraph_mismatch", "empty_paragraph"}),
            "missing": ({"title": TITLE, "paragraphs": PARAS[:1], "native": native}, 0, {"missing_paragraph"}),
            "no-native": ({"title": TITLE, "paragraphs": PARAS, "native": {}}, 0, {"native_input_mismatch"}),
            "published": ({"title": TITLE, "paragraphs": PARAS, "native": native}, 1, {"published"}),
        }
        for name, (payload, publishes, reasons) in cases.items():
            run, pattern = self.run_with()
            self.assertEqual(self.post(pattern, "save", payload)[0], 200)
            for _ in range(publishes):
                self.post(pattern, "publish", {})
            got = run.oracle()
            self.assertEqual(got["verdict"], "pass" if not reasons else "fail", name)
            self.assertTrue(reasons <= set(got["reasons"]), (name, got["reasons"]))
            if not reasons:
                self.assertEqual(got["reasons"], [])

    def test_never_saved_and_save_error(self):
        run, _ = self.run_with()
        self.assertEqual(run.oracle()["reasons"][:1], ["not_saved"])
        run, pattern = self.run_with(save_error=True)
        self.assertEqual(self.post(pattern, "save", {"title": TITLE, "paragraphs": PARAS})[0], 503)
        self.assertIn("save_failed", run.oracle()["reasons"])
        self.assertEqual(run.state["save_attempts"], 1)

    def test_resume_choice_is_recorded(self):
        run, pattern = self.run_with("popup")
        self.post(pattern, "dialog", {"choice": "resume"})
        self.assertIn("resume_chosen", run.oracle()["reasons"])

    def test_bad_requests(self):
        run, pattern = self.run_with()
        self.assertEqual(self.post(pattern, "save", {"x": "y" * 70_000})[0], 400, "oversized body")
        request = urllib.request.Request(pattern % "save", data=b"not json", method="POST")
        with self.assertRaises(urllib.error.HTTPError) as caught:
            urllib.request.urlopen(request, timeout=5)
        self.assertEqual(caught.exception.code, 400)
        caught.exception.close()
        self.assertEqual(call(f"{self.agent}/save/unknown", {})[0], 404)
        self.assertEqual(call(f"{self.agent}/bogus/{run.key}", {})[0], 404)

    def test_runs_are_created_over_the_oracle_port_and_isolated_from_each_other(self):
        status, body = call(f"{self.oracle}/oracle/new", {"style": "iframe", "alias": "naver"})
        self.assertEqual(status, 200)
        created = json.loads(body)
        self.assertIn("naver.localhost", created["url"])
        self.assertEqual(call(f"{self.oracle}/oracle/new", {"style": "bogus"})[0], 400)
        a, pa = self.run_with()
        b, _ = self.run_with()
        self.post(pa, "publish", {})
        self.assertEqual(a.state["publish_count"], 1)
        self.assertEqual(b.state["publish_count"], 0)


if __name__ == "__main__":
    unittest.main()
