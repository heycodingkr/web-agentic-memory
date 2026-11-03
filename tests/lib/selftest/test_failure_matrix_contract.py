"""Keep failure-matrix cases represented in the test index and release gate."""
import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]


class FailureMatrixContractTest(unittest.TestCase):
    def test_all_matrix_cases_are_in_the_index_and_pass_gate(self):
        matrix = (REPO / "docs/test/failure-matrix.md").read_text(encoding="utf-8")
        index = (REPO / "docs/test/README.md").read_text(encoding="utf-8")
        ids = [int(value) for value in re.findall(r"^\| FAIL-(\d{2}) \|", matrix, re.MULTILINE)]
        self.assertTrue(ids, "the failure matrix must define at least one case")
        self.assertEqual(ids, list(range(1, ids[-1] + 1)), "failure matrix IDs must be contiguous")
        latest_range = f"FAIL-01~{ids[-1]:02d}"

        trace_row = next(line for line in index.splitlines() if line.startswith("| 교차 계층 실패·복구 |"))
        gate_row = next(line for line in index.splitlines() if line.startswith("| `fault-injection` |"))
        pass_gate = next(line for line in matrix.splitlines() if line.startswith("- FAIL-01~"))
        self.assertIn(latest_range, trace_row)
        self.assertIn(latest_range, gate_row)
        self.assertIn(latest_range, pass_gate)

    def test_matrix_status_does_not_call_implemented_cases_pre_implementation(self):
        matrix = (REPO / "docs/test/failure-matrix.md").read_text(encoding="utf-8")
        status = next(line for line in matrix.splitlines() if line.startswith("상태:"))
        self.assertNotIn("개발 전", status)


if __name__ == "__main__":
    unittest.main()
