import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from unittest.mock import patch
from pathlib import Path

from wamtest.bench import repeatability, trace
from wamtest.evidence import read_jsonl
from wamtest.bench.runner import run_plan
from wamtest.bench.schedule import make_plan


class ClockQualityTest(unittest.TestCase):
    def test_clock_error_uses_wall_timestamps_against_monotonic_elapsed_time(self):
        start = datetime(2026, 10, 28, tzinfo=timezone.utc)
        aligned = [
            {"type": "tool_call", "t": 4.0, "ts": start.isoformat(timespec="milliseconds")},
            {"type": "submit", "t": 5.0, "ts": (start + timedelta(seconds=1, milliseconds=80)).isoformat(timespec="milliseconds")},
        ]
        self.assertAlmostEqual(trace.clock_error_ms(aligned), 80, delta=1)

    def test_clock_error_over_100ms_is_measured_and_missing_or_bad_stamps_are_unavailable(self):
        start = datetime(2026, 10, 28, tzinfo=timezone.utc)
        drifted = [
            {"type": "tool_call", "t": 0.0, "ts": start.isoformat()},
            {"type": "done", "t": 1.0, "ts": (start + timedelta(seconds=1, milliseconds=101)).isoformat()},
        ]
        self.assertGreater(trace.clock_error_ms(drifted), 100)
        self.assertIsNone(trace.clock_error_ms([{"type": "tool_call", "t": 0.0}]))
        self.assertIsNone(trace.clock_error_ms([
            {"type": "tool_call", "t": 0.0, "ts": "not-a-timestamp"},
            {"type": "done", "t": 1.0, "ts": start.isoformat()},
        ]))


class RepeatabilityTest(unittest.TestCase):
    def test_assessment_requires_five_positive_measurements_and_applies_15_percent_limit(self):
        stable = repeatability.assess([1.00, 1.03, 0.98, 1.02, 0.99])
        self.assertEqual(stable["n"], 5)
        self.assertTrue(stable["reproducible"])
        self.assertLessEqual(stable["cv"], 0.15)

        noisy = repeatability.assess([0.4, 0.6, 0.5, 0.8, 0.7])
        self.assertFalse(noisy["reproducible"])
        self.assertGreater(noisy["cv"], 0.15)

        too_few = repeatability.assess([1.0, 1.1])
        self.assertIsNotNone(too_few["cv"])
        self.assertFalse(too_few["reproducible"])
        incomplete = repeatability.assess([1.0, None, 1.1])
        self.assertFalse(incomplete["complete"])
        self.assertFalse(incomplete["reproducible"])
        with self.assertRaises(ValueError):
            repeatability.assess([1.0, -0.1, 1.0, 1.0, 1.0])

    def test_benchmark_summary_reports_tts_repeatability_per_task_and_condition(self):
        plan = make_plan(["fixture"], ["T", "M", "I"], 5, 13)
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "evidence"

            def adapter(context, spec):
                tts = 1.0 + (spec["block"] % 5) * 0.01
                context.event("tool_call", name="save", t=0.0)
                context.event("submit", t=tts)
                context.event("done", t=tts + 0.5)
                return True

            summary = run_plan(plan, adapter, output, n_boot=20)
            results = read_jsonl(output / "results.jsonl")
            for condition in ("T", "M", "I"):
                measured = [row["tts"] for row in results if row["condition"] == condition]
                assessment = summary["repeatability"]["fixture"][condition]
                self.assertEqual(assessment["n"], 5)
                self.assertIsInstance(assessment["cv"], float)
            self.assertEqual(summary["runs"], 15)

    def test_controlled_run_is_invalid_when_clock_drift_exceeds_100ms(self):
        plan = make_plan(["fixture"], ["T"], 1, 13)
        start = datetime(2026, 10, 28, tzinfo=timezone.utc)
        calls = 0

        def clock():
            nonlocal calls
            calls += 1
            if calls == 3:
                return (start + timedelta(milliseconds=250)).isoformat(timespec="milliseconds")
            if calls >= 4:
                return (start + timedelta(milliseconds=300)).isoformat(timespec="milliseconds")
            return start.isoformat(timespec="milliseconds")

        with tempfile.TemporaryDirectory() as tmp, patch("wamtest.evidence.now_iso", side_effect=clock):
            summary = run_plan(
                plan,
                lambda context, _spec: (
                    context.event("tool_call", t=0.0),
                    context.event("submit", t=0.1),
                    context.event("done", t=0.2),
                    True,
                )[-1],
                Path(tmp) / "evidence",
                metadata={"host": "fixture", "model": "fake", "effort": "low",
                          "prompt_hash": "p", "tools_hash": "t"},
                controlled=True,
            )
            row = read_jsonl(Path(tmp) / "evidence" / "results.jsonl")[0]
            self.assertGreater(row["clock_error_ms"], 100)
            self.assertEqual(row["invalid_class"], "invalid-env")
            self.assertFalse(row["success"])
