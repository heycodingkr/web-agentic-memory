"""Private deployment configuration acceptance tests; no production paths are written."""
import contextlib
import io
import json
import pathlib
import runpy
import tempfile
import unittest
from unittest import mock


SCRIPT = pathlib.Path(__file__).with_name("setup-private-config.py")


class PrivateConfigTests(unittest.TestCase):
    def install(self, environment):
        with tempfile.TemporaryDirectory(prefix="wam-private-config-") as directory:
            root = pathlib.Path(directory)
            (root / "deployment-private.json").write_text(json.dumps({
                "postgres_password": "fixture-postgres-password",
                "redis_password": "fixture-redis-password",
            }))
            (root / "api-env.json").write_text(json.dumps(environment))
            real_path = pathlib.Path

            def isolated_path(value):
                self.assertEqual(value, "/etc/web-agentic-memory")
                return real_path(directory)

            with mock.patch("pathlib.Path", side_effect=isolated_path), mock.patch("os.chown"), contextlib.redirect_stdout(io.StringIO()):
                runpy.run_path(str(SCRIPT), run_name="__main__")
            result = {}
            for line in (root / "api.env").read_text().splitlines():
                key, value = line.split("=", 1)
                result[key] = json.loads(value)
            self.assertEqual((root / "api.env").stat().st_mode & 0o777, 0o600)
            self.assertEqual(root.stat().st_mode & 0o777, 0o700)
            self.assertIn("fixture-postgres-password", (root / "postgres.env").read_text())
            self.assertIn("fixture-redis-password", (root / "redis.conf").read_text())
            return result

    def test_missing_diagnostic_recipient_does_not_invent_a_real_address(self):
        result = self.install({"WAM_EMAIL_IDENTITY_SECRET": "fixture-identity-secret"})
        self.assertFalse("WAM_DIAGNOSTICS_EMAIL_TO" in result, "missing private recipient must not be filled with a real address")
        self.assertEqual(result["WAM_EMAIL_IDENTITY_SECRET"], "fixture-identity-secret")

    def test_explicit_private_recipient_is_preserved(self):
        result = self.install({"WAM_DIAGNOSTICS_EMAIL_TO": "alerts@example.test"})
        self.assertEqual(result["WAM_DIAGNOSTICS_EMAIL_TO"], "alerts@example.test")


if __name__ == "__main__":
    unittest.main()
