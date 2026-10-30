import subprocess
import sys
import unittest
from pathlib import Path


class SecurityCorpusTest(unittest.TestCase):
    def test_checked_in_injection_corpus_matches_deterministic_generator(self):
        root = Path(__file__).resolve().parents[3]
        script = root / "tests" / "security" / "generate_inj_corpus.py"
        result = subprocess.run(
            [sys.executable, "-B", str(script), "--check"],
            cwd=root,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr or result.stdout)
        self.assertIn("160 synthetic cases", result.stdout)


if __name__ == "__main__":
    unittest.main()
