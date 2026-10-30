"""UI drift fixtures keep the same task and independent oracle across layouts."""
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tests/env/fixture-sites"))
import fixture_server as fx

from selftest.test_fixture_http import call


class UiDriftTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = fx.FixtureServer().start()
        cls.agent = f"http://127.0.0.1:{cls.server.agent_port}"
        cls.oracle = f"http://127.0.0.1:{cls.server.oracle_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.stop()

    def test_v2_removes_the_old_anchor_and_moves_the_current_save_action(self):
        for version, current, obsolete in (
            ("v1", "save-draft", "store-draft"), ("v2", "store-draft", "save-draft"),
        ):
            with self.subTest(version=version):
                run = self.server.new_run("ui-drift", ui_version=version)
                status, raw = call(f"{self.agent}/editor/{run.key}")
                html = raw.decode()
                self.assertEqual(status, 200)
                self.assertIn(f'id="{current}"', html)
                self.assertNotIn(f'id="{obsolete}"', html)
                self.assertNotIn("@@", html)
                self.assertNotIn(str(self.server.oracle_port), html)
                before_title = html.index(f'id="{current}"') < html.index('id="title"')
                self.assertEqual(before_title, version == "v2")
                self.assertEqual(run.state["ui_version"], version)
                self.assertFalse(run.state["autosave"])

    def test_both_versions_use_the_same_oracle_and_cannot_pass_without_saving(self):
        for version in ("v1", "v2"):
            with self.subTest(version=version):
                run = self.server.new_run("ui-drift", ui_version=version)
                self.assertIn("not_saved", run.oracle()["reasons"])
                payload = {
                    "title": fx.TITLE, "paragraphs": fx.PARAGRAPHS,
                    "native": {"title": fx.TITLE, "body": fx.PARAGRAPHS},
                    "via": "button",
                }
                self.assertEqual(call(f"{self.agent}/save/{run.key}", payload)[0], 200)
                status, raw = call(f"{self.oracle}/oracle/{run.key}")
                self.assertEqual(status, 200)
                self.assertEqual(json.loads(raw)["verdict"], "pass")

    def test_version_is_explicit_and_bad_versions_are_rejected(self):
        status, raw = call(f"{self.oracle}/oracle/new", {"style": "ui-drift", "ui_version": "v2"})
        self.assertEqual(status, 200)
        self.assertEqual(self.server.runs[json.loads(raw)["key"]]["ui_version"], "v2")
        self.assertEqual(call(f"{self.oracle}/oracle/new", {"style": "ui-drift", "ui_version": "v3"})[0], 400)
        with self.assertRaises(ValueError):
            self.server.new_run("rich", ui_version="v2")


if __name__ == "__main__":
    unittest.main()
