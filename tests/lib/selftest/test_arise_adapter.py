"""HARN-06 (static part): the isolated Arise launch recipe redirects every data path into the disposable root.

No Arise process is started here. A fake app directory stands in for /Applications/Arise.app.
"""
import os
import plistlib
import shutil
import tempfile
import unittest
from pathlib import Path

from wamtest.arise import LocalArise, arise_version, free_port
from wamtest.procs import Isolation

PATH_VARS = (
    "HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "TMPDIR",
    "AGENT_BROWSER_SESSIONS_DIR", "AGENT_BROWSER_UI_STATE", "AGENT_BROWSER_PROFILES_DIR", "ARRISE_CEF_CACHE",
)
BUNDLE_VARS = ("ARRISE_CEF_FRAMEWORK", "ARRISE_CEF_BUNDLE", "ARRISE_CEF_HELPER")


def fake_app(parent):
    app = Path(parent) / "Arise.app"
    (app / "Contents" / "MacOS").mkdir(parents=True)
    (app / "Contents" / "Frameworks").mkdir()
    (app / "Contents" / "MacOS" / "Arise").write_bytes(b"#!/bin/sh\n")
    (app / "Contents" / "MacOS" / "agent_browser").write_bytes(b"x")
    with open(app / "Contents" / "Info.plist", "wb") as handle:
        plistlib.dump({"CFBundleShortVersionString": "9.9.9"}, handle)
    return app


class AriseAdapterTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.iso = Isolation("unit")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.addCleanup(shutil.rmtree, self.iso.root, True)

    def test_every_data_path_points_into_the_disposable_root(self):
        app = fake_app(self.tmp)
        arise = LocalArise("http://fixture.localhost:1/editor/k", isolation=self.iso, app=app, debug_port=45678)
        argv, env = arise.prepare()
        root = str(self.iso.root)
        for name in PATH_VARS:
            self.assertTrue(env[name].startswith(root), f"{name} escapes the isolation root: {env[name]}")
        self.assertEqual(argv, [str(app / "Contents" / "MacOS" / "Arise")], "the signed installed binary runs in place")
        self.assertFalse((self.iso.root / "Arise.app").exists(), "no copy of the signed binary is made")
        for name in BUNDLE_VARS:
            self.assertNotIn(name, env, "bundle overrides are only for skeleton (unsigned dev) builds")
        self.assertEqual(env["ARRISE_CEF_DEBUG_PORT"], "45678")

    def test_the_users_session_variables_and_keychain_are_handled(self):
        os.environ["ARRISE_BROWSER_SESSION_ID"] = "users-session"
        os.environ["ARRISE_CONTROL_SOCK"] = "/tmp/users.sock"
        try:
            _, env = LocalArise("http://x/editor/k", isolation=self.iso, app=fake_app(self.tmp)).prepare()
        finally:
            del os.environ["ARRISE_BROWSER_SESSION_ID"]
            del os.environ["ARRISE_CONTROL_SOCK"]
        self.assertNotIn("ARRISE_BROWSER_SESSION_ID", env)
        self.assertNotIn("ARRISE_CONTROL_SOCK", env)
        self.assertEqual(env["AGENT_BROWSER_NO_KEYCHAIN"], "1")
        self.assertEqual(env["ARISE_NO_UPDATE_CHECK"], "1")
        self.assertEqual(env["ARRISE_CHAT_AGENT_LOOP"], "0")

    def test_skeleton_mode_for_unsigned_dev_builds_copies_the_binary_and_links_frameworks(self):
        app = fake_app(self.tmp)
        argv, env = LocalArise("http://x/editor/k", isolation=self.iso, app=app).prepare(skeleton=True)
        contents = self.iso.root / "Arise.app" / "Contents"
        self.assertTrue(argv[0].startswith(str(self.iso.root)))
        for name in BUNDLE_VARS:
            self.assertTrue(env[name].startswith(str(self.iso.root)), name)
        self.assertTrue((contents / "MacOS" / "Arise").is_file() and not (contents / "MacOS" / "Arise").is_symlink())
        self.assertTrue((contents / "Frameworks").is_symlink())
        self.assertEqual((contents / "Frameworks").resolve(), (app / "Contents" / "Frameworks").resolve())

    def test_settings_switch_off_network_features(self):
        LocalArise("http://x/editor/k", isolation=self.iso, app=fake_app(self.tmp)).prepare()
        text = (self.iso.root / "xdg" / "config" / "agent-browser" / "settings.toml").read_text()
        for line in ("auto_update_check = false", "telemetry_enabled = false", "web_agentic_memory_enabled = false",
                     "web_workflow_contribution = false", "browser_memory_auto_recall = false"):
            self.assertIn(line, text)

    def test_runtime_config_probe_requires_a_read_of_the_intended_settings_file(self):
        arise = LocalArise("http://x/editor/k", isolation=self.iso, app=fake_app(self.tmp))
        arise.prepare()
        self.assertFalse(arise.runtime_config_path_verified())
        settings = self.iso.root / "xdg" / "config" / "agent-browser" / "settings.toml"
        settings.read_text(encoding="utf-8")
        self.assertTrue(arise.runtime_config_path_verified())

    def test_version_and_free_port(self):
        self.assertEqual(arise_version(fake_app(self.tmp)), "9.9.9")
        self.assertIsNone(arise_version(Path(self.tmp) / "missing.app"))
        self.assertGreater(free_port(), 1023)

    def test_missing_app_is_an_error_not_a_silent_skip(self):
        os.environ["WAM_ARISE_APP"] = str(Path(self.tmp) / "nope.app")
        try:
            with self.assertRaises(FileNotFoundError):
                LocalArise("http://x/editor/k")
        finally:
            del os.environ["WAM_ARISE_APP"]


if __name__ == "__main__":
    unittest.main()
