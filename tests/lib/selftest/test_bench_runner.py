import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from wamtest.bench import schedule
from wamtest.bench.replay import ReplayAdapter
from wamtest.bench.runner import run_plan
from wamtest.evidence import read_jsonl

REPO = Path(__file__).resolve().parents[3]


def good_events(tts=1.0):
    return [
        {"type": "tool_call", "name": "save", "t": 0},
        {"type": "submit", "t": tts},
        {"type": "done", "t": tts + 1},
    ]


class BenchRunnerTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.output = self.root / "evidence"
        self.plan = schedule.make_plan(["fixture"], ["T", "M", "I"], 1, 3)

    def tearDown(self):
        self.temp.cleanup()

    def records(self):
        return {row["run_id"]: {"events": good_events(), "oracle_pass": True}
                for row in self.plan["runs"]}

    def run_records(self, records=None, **kw):
        return run_plan(self.plan, ReplayAdapter(records or self.records()), self.output, n_boot=100, **kw)

    def test_plan_is_frozen_before_execution_and_every_run_gets_a_fresh_cleaned_profile(self):
        original = copy.deepcopy(self.plan)
        roots = []

        def adapter(context, spec):
            frozen = json.loads((self.output / "plan.json").read_text())
            self.assertEqual(frozen, original)
            roots.append(context.isolation.root)
            self.assertTrue((context.isolation.root / "home").is_dir())
            spec["condition"] = "tampered"
            self.plan["runs"][0]["condition"] = "also-tampered"
            for event in good_events():
                event = dict(event)
                context.event(event.pop("type"), **event)
            return True

        summary = run_plan(self.plan, adapter, self.output, n_boot=100)
        self.assertEqual(summary["runs"], 3)
        self.assertEqual(summary["successes"], 3)
        self.assertEqual(len(set(roots)), 3)
        self.assertTrue(all(not root.exists() for root in roots))
        rows = read_jsonl(self.output / "results.jsonl")
        self.assertEqual([row["condition"] for row in rows],
                         [row["condition"] for row in original["runs"]])
        self.assertEqual(len(list((self.output / "runs").iterdir())), 3)

    def test_missing_terminal_event_is_invalid_even_when_evidence_bookkeeping_finishes(self):
        records = self.records()
        for record in records.values():
            record["events"] = good_events()[:-1]
        summary = self.run_records(records)
        self.assertFalse(summary["suite_valid"])
        for row in read_jsonl(self.output / "results.jsonl"):
            self.assertEqual(row["invalid_class"], "invalid-env")
            self.assertFalse(row["completed"])
            self.assertFalse(row["success"])

    def test_valid_failure_without_submit_is_retained_without_inventing_measurements(self):
        records = self.records()
        records[self.plan["runs"][0]["run_id"]] = {
            "events": [{"type": "tool_call", "t": 0}, {"type": "failed", "t": 2}],
            "oracle_pass": False,
        }
        summary = self.run_records(records)
        rows = read_jsonl(self.output / "results.jsonl")
        self.assertTrue(summary["suite_valid"])
        self.assertFalse(rows[0]["invalid"])
        self.assertFalse(rows[0]["success"])
        self.assertIsNone(rows[0]["tts"])
        self.assertIsNone(rows[0]["calls"])

    def test_guard_blocks_before_the_action_and_the_attempt_is_a_valid_failure(self):
        actions = []

        def adapter(context, spec):
            context.perform("publish", lambda: actions.append(spec), url="http://fixture.localhost/publish")
            return True

        summary = run_plan(self.plan, adapter, self.output, n_boot=100)
        self.assertEqual(actions, [])
        self.assertEqual(summary["invalid_rate"], 0)
        self.assertEqual(summary["successes"], 0)
        rows = read_jsonl(self.output / "results.jsonl")
        self.assertTrue(all(row["blocked_actions"] == 1 for row in rows))
        self.assertTrue(all(row["end"] == "failed" for row in rows))

    def test_allowed_guarded_action_can_submit_and_use_an_independent_verdict(self):
        actions = []

        def adapter(context, spec):
            context.perform("save", lambda: actions.append(spec["run_id"]),
                            url="http://fixture.localhost/save", label="Save draft")
            context.event("submit")
            context.event("done")
            return len(actions) > 0

        summary = run_plan(self.plan, adapter, self.output, n_boot=100)
        self.assertEqual(summary["successes"], 3)
        rows = read_jsonl(self.output / "results.jsonl")
        self.assertTrue(all(row["calls"] == 1 and row["tool_errors"] == 0 for row in rows))

    def test_adapter_exceptions_and_unknown_signals_are_recorded_and_later_runs_continue(self):
        def adapter(context, spec):
            if spec["condition"] == "T":
                raise RuntimeError("private details should not be in the error summary")
            if spec["condition"] == "M":
                context.signal("unrecognized")
            for event in good_events():
                event = dict(event)
                context.event(event.pop("type"), **event)
            return True

        summary = run_plan(self.plan, adapter, self.output, n_boot=100)
        self.assertEqual(summary["runs"], 3)
        self.assertEqual(summary["successes"], 1)
        self.assertFalse(summary["suite_valid"])
        self.assertTrue(all(result["grade"] == "invalid"
                            for result in summary["comparisons"]["fixture"].values()))

    def test_invalid_trace_times_or_events_after_end_are_environment_failures(self):
        for events in (
            [{"type": "tool_call", "t": float("nan")}],
            [{"type": "tool_call", "t": 3}, {"type": "done", "t": 2}],
            [{"type": "done", "t": 0}, {"type": "tool_call", "t": 1}],
            [{"type": "tool_result", "t": 0, "is_error": "false"}],
        ):
            with self.subTest(events=events):
                output = self.root / f"invalid-{len(list(self.root.iterdir()))}"
                records = {row["run_id"]: {"events": events, "oracle_pass": True}
                           for row in self.plan["runs"]}
                result = run_plan(self.plan, ReplayAdapter(records), output, n_boot=10)
                self.assertEqual(result["invalid_rate"], 1)

    def test_replay_is_uncontrolled_and_cannot_earn_an_A_grade(self):
        self.plan = schedule.make_plan(["fixture"], ["T", "M", "I"], 5, 3)
        records = self.records()
        for spec in self.plan["runs"]:
            records[spec["run_id"]]["events"] = good_events(10 if spec["condition"] == "T" else 1)
        summary = self.run_records(records)
        self.assertFalse(summary["controlled"])
        self.assertEqual(summary["comparisons"]["fixture"]["M"]["grade"], "C")
        self.assertTrue((self.output / "summary.md").exists())

    def test_invalid_design_and_missing_control_metadata_fail_before_execution(self):
        bad = copy.deepcopy(self.plan)
        bad["runs"].append({**bad["runs"][0], "run_id": "extra", "task": "unregistered"})
        with self.assertRaises(ValueError):
            run_plan(bad, lambda *_: True, self.output)
        self.assertFalse(self.output.exists())
        with self.assertRaises(ValueError):
            self.run_records(controlled=True)
        self.assertFalse(self.output.exists())

    def test_replay_rejects_controlled_mode_even_with_complete_metadata(self):
        metadata = {key: "synthetic-test" for key in ("host", "model", "effort", "prompt_hash", "tools_hash")}
        with self.assertRaisesRegex(ValueError, "replay cannot"):
            self.run_records(controlled=True, metadata=metadata)
        self.assertFalse(self.output.exists())

    def test_cli_replay_checks_exact_run_ids_and_writes_artifacts(self):
        plan = self.root / "plan.json"
        traces = self.root / "traces.json"
        plan.write_text(json.dumps(self.plan))
        records = self.records()
        traces.write_text(json.dumps(records))
        command = [sys.executable, "-B", str(REPO / "tests/bench/run.py"),
                   "--plan", str(plan), "--traces", str(traces), "--output", str(self.output)]
        done = subprocess.run(command, capture_output=True, text=True, timeout=15)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(json.loads(done.stdout)["successes"], 3)
        records["extra"] = {"events": good_events(), "oracle_pass": True}
        traces.write_text(json.dumps(records))
        rejected = subprocess.run(command, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("exactly match", rejected.stderr)


if __name__ == "__main__":
    unittest.main()
