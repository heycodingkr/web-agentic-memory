"""HARN-12: docs/spec/curator/tools.v1.json is well formed and agrees with the specs and the instructions."""
import json
import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
SPEC = REPO / "docs" / "spec"
TOOLS = json.loads((SPEC / "curator" / "tools.v1.json").read_text(encoding="utf-8"))
INSTRUCTIONS = (SPEC / "curator" / "instructions.v1.md").read_text(encoding="utf-8")
READ = {"job_get", "memory_search", "memory_get", "memory_compare"}
DECISION = {"decide_skip", "decide_replace", "decide_create", "decide_retire", "decide_escalate", "decide_reject"}
ENUM = {"duplicate_support", "merge_evidence", "create_new", "supersede", "retire", "needs_human_review", "reject_invalid"}


def walk(node):
    if isinstance(node, dict):
        yield node
        for value in node.values():
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)


class CuratorToolsTest(unittest.TestCase):
    def setUp(self):
        self.by_name = {t["name"]: t for t in TOOLS["tools"]}

    def test_exactly_the_documented_tools(self):
        self.assertEqual(len(self.by_name), len(TOOLS["tools"]), "names are unique")
        self.assertEqual({n for n, t in self.by_name.items() if t["kind"] == "read"}, READ)
        self.assertEqual({n for n, t in self.by_name.items() if t["kind"] == "decision"}, DECISION)

    def test_every_tool_schema_is_closed_and_consistent(self):
        for name, tool in self.by_name.items():
            self.assertTrue(tool["description"], name)
            schema = tool["input_schema"]
            self.assertEqual(schema["type"], "object", name)
            self.assertIs(schema["additionalProperties"], False, name)
            self.assertLessEqual(set(schema.get("required", [])), set(schema["properties"]), name)

    def test_every_ref_resolves_inside_defs(self):
        for node in walk(TOOLS):
            ref = node.get("$ref")
            if ref:
                self.assertTrue(ref.startswith("#/$defs/"), ref)
                self.assertIn(ref.split("/")[-1], TOOLS["$defs"], ref)

    def test_closed_objects_everywhere(self):
        for node in walk(TOOLS["$defs"]):
            if node.get("type") == "object":
                self.assertIs(node.get("additionalProperties"), False)

    def test_the_model_cannot_choose_the_host(self):
        self.assertNotIn("site_host", self.by_name["memory_search"]["input_schema"]["properties"])

    def test_decision_enums_match_the_specs(self):
        decided = {d for t in self.by_name.values() for d in t.get("decision", [])}
        self.assertEqual(decided, ENUM)
        dedup = (SPEC / "dedup.md").read_text(encoding="utf-8")
        model = (SPEC / "model.md").read_text(encoding="utf-8")
        for word in ENUM - {"reject_invalid"}:
            self.assertIn(word, dedup, word)
        for word in ENUM - {"create_new", "reject_invalid"}:
            self.assertIn(word, model, word)

    def test_decision_tools_require_codes_and_replace_requires_the_version(self):
        for name in DECISION:
            self.assertIn("reason_codes", self.by_name[name]["input_schema"]["required"], name)
        for name in ("decide_replace", "decide_retire"):
            self.assertIn("expected_version", self.by_name[name]["input_schema"]["required"], name)
        self.assertIn("record", self.by_name["decide_create"]["input_schema"]["required"])

    def test_limits_agree_with_instructions_and_memory_spec(self):
        defs, limits = TOOLS["$defs"], TOOLS["limits"]
        self.assertEqual(limits["max_read_calls_per_scope_batch"], 8)
        self.assertEqual(limits["decision_calls_per_scope_batch"], 1)
        self.assertEqual(limits["max_candidates_per_batch"], 20)
        self.assertNotIn("max_read_calls_per_job", limits)
        self.assertIn("At most 8 read calls", INSTRUCTIONS)
        self.assertEqual(defs["l0"]["properties"]["keywords"]["maxItems"], 8)
        self.assertEqual(defs["l0"]["properties"]["url_patterns"]["maxItems"], 4)
        self.assertEqual(defs["l1"]["properties"]["summary"]["maxLength"], 400)
        memory = (SPEC / "memory.md").read_text(encoding="utf-8")
        self.assertIn("≤1 KiB", memory)
        self.assertEqual(limits["l0_max_bytes"], 1024)
        self.assertIn("≤8,000", memory)
        self.assertEqual(limits["l2_max_chars"], 8000)

    def test_instructions_name_every_tool_and_error_code(self):
        for name in READ | DECISION:
            self.assertIn(f"`{name}`", INSTRUCTIONS, name)
        curator = (SPEC / "curator.md").read_text(encoding="utf-8")
        for code in TOOLS["error_codes"]:
            self.assertIn(code, INSTRUCTIONS, code)
            self.assertIn(code, curator, code)

    def test_reason_codes_used_in_examples_are_in_the_enum(self):
        allowed = set(TOOLS["$defs"]["reason_codes"]["items"]["enum"])
        for code in re.findall(r'\["([a-z_]+)"(?:, "([a-z_]+)")?\]', INSTRUCTIONS):
            for item in code:
                if item:
                    self.assertIn(item, allowed, item)


if __name__ == "__main__":
    unittest.main()
