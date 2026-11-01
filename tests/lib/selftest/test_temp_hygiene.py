import subprocess
import sys
import unittest
from pathlib import Path

CHECKER = Path(__file__).resolve().parents[2] / "env/check_temp_hygiene.py"


class TempHygieneTest(unittest.TestCase):
    def run_command(self, body):
        return subprocess.run(
            [sys.executable, "-B", str(CHECKER), "--", sys.executable, "-B", "-c", body],
            capture_output=True, text=True, check=False,
        )

    def test_command_can_remove_its_own_files(self):
        result = self.run_command(
            "import tempfile; from pathlib import Path; "
            "p=Path(tempfile.gettempdir())/'owned'; p.write_text('fixture'); p.unlink()"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("no leftovers", result.stdout)

    def test_leftover_directory_and_sidecar_fail_and_are_removed(self):
        result = self.run_command(
            "import tempfile; from pathlib import Path; "
            "p=Path(tempfile.gettempdir()); print(p, flush=True); "
            "(p/'repo').mkdir(); (p/'repo'/'application.lock').touch()"
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Temporary-file leak: 1 entries", result.stdout)
        self.assertIn("  repo", result.stdout)
        self.assertFalse(Path(result.stdout.splitlines()[0]).exists())

    def test_child_failure_is_preserved_without_leftovers(self):
        result = self.run_command("raise SystemExit(7)")
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertIn("no leftovers", result.stdout)

    def test_child_failure_with_a_leak_reports_both(self):
        result = self.run_command(
            "import tempfile; from pathlib import Path; "
            "(Path(tempfile.gettempdir())/'application.lock').touch(); raise SystemExit(7)"
        )
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertIn("Temporary-file leak: 1 entries", result.stdout)
