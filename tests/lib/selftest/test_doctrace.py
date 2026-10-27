"""HARN-11: the specs written for the layered memory, recall, queue, and curator cite only defined test IDs."""
import unittest
from pathlib import Path

from wamtest import doctrace

REPO = Path(__file__).resolve().parents[3]
MINE = ["memory", "recall", "queue", "curator"]


class DocTraceTest(unittest.TestCase):
    def test_range_and_list_expansion(self):
        self.assertEqual(doctrace._ids("MEM", "01", "~03"), ["MEM-01", "MEM-02", "MEM-03"])
        self.assertEqual(doctrace._ids("INST", "43", "/44/46/47"), ["INST-43", "INST-44", "INST-46", "INST-47"])
        self.assertEqual(doctrace._ids("CLI", "25", "~30")[-1], "CLI-30")
        self.assertEqual(doctrace._ids("DB", "10", "~11/13"), ["DB-10", "DB-11", "DB-13"])

    def test_prefixed_acceptance_ids_are_not_misread(self):
        self.assertEqual(sorted(set(doctrace.REF.findall("ACC-MCP-04 and SHA-256, UTF-8")))[0][0], "SHA")
        refs = [m.group(1) + "-" + m.group(2) for m in doctrace.REF.finditer("see ACC-MCP-04")]
        self.assertEqual(refs, [], "MCP-04 inside ACC-MCP-04 must not count as a citation")

    def test_defined_ids_include_rows_and_headings(self):
        defined = doctrace.defined_ids()
        for expected in ("MEM-01", "RCL-16", "QUE-14", "CUR-17", "FLOW-01", "HARN-12"):
            self.assertIn(expected, defined)

    def test_new_specs_cite_only_defined_test_ids(self):
        problems = doctrace.undefined([REPO / "docs" / "spec" / f"{name}.md" for name in MINE])
        self.assertEqual(problems, [])

    def test_a_made_up_citation_is_caught(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            doc = Path(tmp) / "x.md"
            doc.write_text("tests: MEM-01, MEM-99", encoding="utf-8")
            self.assertEqual(doctrace.undefined([doc]), [(str(doc), "MEM-99")])


if __name__ == "__main__":
    unittest.main()
