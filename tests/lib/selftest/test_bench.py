import tempfile
import unittest

from wamtest.bench import guards, invalid, schedule, stats, trace
from wamtest.evidence import RunDir, read_jsonl


def runs(tts, calls, errors=0, success=True, invalid_=False):
    return [{"tts": t, "calls": c, "errors": errors, "success": success, "invalid": invalid_} for t, c in zip(tts, calls)]


class StatsTest(unittest.TestCase):
    def test_summarize_known_values(self):
        s = stats.summarize([1, 2, 3, 4, 5])
        self.assertEqual((s["n"], s["median"], s["mean"]), (5, 3, 3))
        self.assertAlmostEqual(s["sd"], 1.5811, places=3)
        self.assertIsNone(stats.summarize([7])["sd"])
        self.assertEqual(stats.summarize([]), {"n": 0})

    def test_bootstrap_is_reproducible_per_seed(self):
        a, b = [100, 110, 105, 95, 102], [50, 55, 52, 48, 51]
        self.assertEqual(stats.bootstrap_diff_ci(a, b, seed=7), stats.bootstrap_diff_ci(a, b, seed=7))

    def test_clear_improvement_is_a_claim(self):
        claim = stats.metric_claim([100, 110, 105, 95, 102], [50, 55, 52, 48, 51])
        self.assertTrue(claim["ok"])
        self.assertGreater(claim["reduction"], 0.4)
        self.assertGreater(claim["ci"]["lo"], 0)

    def test_no_difference_is_not_a_claim_and_ci_spans_zero(self):
        same = [100, 110, 105, 95, 102]
        claim = stats.metric_claim(same, same)
        self.assertFalse(claim["ok"])
        self.assertLessEqual(claim["ci"]["lo"], 0)
        self.assertGreaterEqual(claim["ci"]["hi"], 0)

    def test_small_but_certain_gain_below_threshold_is_not_a_claim(self):
        claim = stats.metric_claim([100] * 5, [90] * 5)
        self.assertGreater(claim["ci"]["lo"], 0)
        self.assertFalse(claim["ok"], "10% is below the 20% reduction threshold")

    def test_worse_condition_is_not_a_claim(self):
        self.assertFalse(stats.metric_claim([50, 52, 51, 49, 50], [100, 110, 105, 95, 102])["ok"])

    def test_grade_rules(self):
        g = stats.grade
        self.assertEqual(g(5, True, 0.0), "A")
        self.assertEqual(g(4, True, 0.0), "B")
        self.assertEqual(g(5, False, 0.0), "B")
        self.assertEqual(g(2, True, 0.0), "C")
        self.assertEqual(g(5, True, 0.0, uncontrolled=True), "C")
        self.assertEqual(g(5, True, 0.25), "invalid")
        self.assertEqual(g(5, True, 0.20), "A", "exactly at the limit is still valid")

    def test_compare_condition_grades(self):
        base = runs([100, 110, 105, 95, 102], [10, 11, 10, 12, 10], errors=2)
        better = runs([50, 55, 52, 48, 51], [6, 7, 6, 7, 6], errors=0)
        result = stats.compare_condition(base, better)
        self.assertEqual(result["grade"], "A")
        self.assertTrue(all(result["criteria"].values()))
        worse_success = runs([50, 55, 52, 48, 51], [6, 7, 6, 7, 6], success=False)
        self.assertEqual(stats.compare_condition(base, worse_success)["grade"], "B")
        self.assertFalse(stats.compare_condition(base, worse_success)["criteria"]["success_not_lower"])
        self.assertEqual(stats.compare_condition(base[:3], better[:3])["grade"], "B")

    def test_invalid_runs_are_excluded_but_counted(self):
        base = runs([100, 110, 105, 95, 102], [10] * 5) + runs([1], [1], invalid_=True)
        better = runs([50, 55, 52, 48, 51], [6] * 5)
        result = stats.compare_condition(base, better)
        self.assertEqual(result["n_baseline"], 5)
        self.assertAlmostEqual(result["invalid_rate"], 1 / 11)
        too_many = runs([100, 110], [10, 10]) + runs([1] * 4, [1] * 4, invalid_=True)
        self.assertEqual(stats.compare_condition(too_many, better)["grade"], "invalid")

    def test_success_criterion_uses_rates_when_valid_group_sizes_differ(self):
        base = runs([100] * 5, [10] * 5)
        other = runs([50] * 4, [5] * 4) + runs([1], [1], invalid_=True)
        result = stats.compare_condition(base, other)
        self.assertTrue(result["criteria"]["success_not_lower"])
        # More raw successes must not hide a lower success rate, either.
        base = runs([100] * 3, [10] * 3)
        other = runs([50] * 4, [5] * 4) + runs([50], [5], success=False)
        self.assertFalse(stats.compare_condition(base, other)["criteria"]["success_not_lower"])

    def test_failed_before_submit_stays_in_success_rate_without_invented_times(self):
        base = runs([100] * 5, [10] * 5)
        other = runs([50] * 4, [5] * 4) + [
            {"tts": None, "calls": None, "errors": 1, "success": False, "invalid": False}
        ]
        result = stats.compare_condition(base, other)
        self.assertEqual(result["n_other"], 5)
        self.assertEqual(result["invalid_rate"], 0)
        self.assertFalse(result["criteria"]["success_not_lower"])
        self.assertFalse(result["criteria"]["complete_measurements"])
        self.assertNotEqual(result["grade"], "A")

    def test_zero_baseline_metrics_render_without_a_false_reduction(self):
        result = stats.compare_condition(runs([0] * 5, [0] * 5), runs([0] * 5, [0] * 5))
        self.assertIn("n/a", stats.markdown_row("M", result))
        self.assertFalse(result["criteria"]["tts"])

    def test_markdown_row_mentions_grade(self):
        row = stats.markdown_row("M", stats.compare_condition(runs([100] * 5, [10] * 5), runs([50] * 5, [5] * 5)))
        self.assertIn("| A |", row)
        self.assertIn("-50%", row)


class ScheduleTest(unittest.TestCase):
    def test_every_block_holds_each_condition_once(self):
        for seed in range(50):
            plan = schedule.make_plan(["bm-fx", "medium"], ["T", "M", "I"], 5, seed)
            self.assertEqual(schedule.check_balance(plan), [], seed)
            self.assertEqual(len(plan["runs"]), 2 * 5 * 3)

    def test_same_seed_same_plan_and_other_seed_differs(self):
        a = schedule.make_plan(["x"], ["T", "M", "I"], 5, 1)
        self.assertEqual(a, schedule.make_plan(["x"], ["T", "M", "I"], 5, 1))
        self.assertNotEqual(a["runs"], schedule.make_plan(["x"], ["T", "M", "I"], 5, 2)["runs"])

    def test_fixed_order_is_detected(self):
        plan = schedule.make_plan(["x"], ["T", "M", "I"], 4, 0)
        for row in plan["runs"]:
            row["position"] = ["T", "M", "I"].index(row["condition"]) + 1
        self.assertTrue(schedule.fixed_order(plan))
        self.assertFalse(schedule.fixed_order(schedule.make_plan(["x"], ["T", "M", "I"], 5, 3)))

    def test_extra_runs_outside_declared_tasks_and_blocks_are_rejected(self):
        plan = schedule.make_plan(["x"], ["T", "M", "I"], 2, 0)
        for changes in ({"task": "undeclared"}, {"block": 3}):
            with self.subTest(changes=changes):
                extra = {**plan["runs"][0], **changes, "run_id": "unexpected"}
                self.assertTrue(schedule.check_balance({**plan, "runs": [*plan["runs"], extra]}))

    def test_empty_or_duplicate_design_inputs_are_rejected(self):
        for tasks, conditions, repeats in [
            ([], ["T"], 1), (["x", "x"], ["T"], 1),
            (["x"], [], 1), (["x"], ["T", "T"], 1),
            (["x"], ["T"], 0), (["x"], ["T"], True),
        ]:
            with self.subTest(tasks=tasks, conditions=conditions, repeats=repeats):
                with self.assertRaises(ValueError):
                    schedule.make_plan(tasks, conditions, repeats, 0)

    def test_tampering_is_reported(self):
        plan = schedule.make_plan(["x"], ["T", "M", "I"], 2, 0)
        plan["runs"].pop()
        self.assertTrue(schedule.check_balance(plan))


class InvalidTest(unittest.TestCase):
    def test_signal_classes_and_precedence(self):
        self.assertIsNone(invalid.classify([]))
        self.assertEqual(invalid.classify(["captcha"]), "invalid-auth")
        self.assertEqual(invalid.classify(["rate-limited"]), "invalid-quota")
        self.assertEqual(invalid.classify(["service-5xx"]), "invalid-service")
        self.assertEqual(invalid.classify(["ui-anchor-missing"]), "invalid-ui-drift")
        self.assertEqual(invalid.classify(["feature-unavailable"]), "unavailable")
        self.assertEqual(invalid.classify(["service-5xx", "auth-required"]), "invalid-auth")
        self.assertEqual(invalid.classify(["auth-required", "config-dir-mismatch"]), "invalid-env")

    def test_unknown_signal_is_an_error_not_a_pass(self):
        with self.assertRaises(ValueError):
            invalid.classify(["something-new"])

    def test_silent_run_is_invalid_env(self):
        self.assertEqual(invalid.classify_run({"signals": [], "events": [{"type": "tool_call"}]}), "invalid-env")
        self.assertIsNone(invalid.classify_run({"signals": [], "events": [{"type": "done"}]}))

    def test_http_signals_and_rate(self):
        self.assertEqual(invalid.signals_from_http(401), ["auth-required"])
        self.assertEqual(invalid.signals_from_http(429), ["rate-limited"])
        self.assertEqual(invalid.signals_from_http(503), ["service-5xx"])
        self.assertEqual(invalid.signals_from_http(403), [])
        good, bad = {"signals": []}, {"signals": ["service-5xx"]}
        self.assertAlmostEqual(invalid.invalid_rate([good] * 4 + [bad]), 0.2)
        self.assertTrue(invalid.suite_valid([good] * 4 + [bad]))
        self.assertFalse(invalid.suite_valid([good] * 3 + [bad] * 2))


class TraceTest(unittest.TestCase):
    EVENTS = [
        {"t": 1.0, "type": "tool_call", "name": "navigate"},
        {"t": 2.0, "type": "tool_call", "name": "click"},
        {"t": 2.1, "type": "tool_result", "is_error": True},
        {"t": 3.0, "type": "tool_call", "name": "type"},
        {"t": 4.0, "type": "submit"},
        {"t": 20.0, "type": "done"},
    ]

    def test_submit_split_separates_activity_from_wait(self):
        m = trace.analyze(self.EVENTS)
        self.assertEqual((m["tts"], m["calls_to_submit"], m["wait"], m["total"]), (3.0, 3, 16.0, 19.0))
        self.assertEqual((m["tool_errors"], m["completed"], m["end"]), (1, True, "done"))

    def test_without_explicit_end_the_run_is_not_complete(self):
        m = trace.analyze(self.EVENTS[:-1])
        self.assertFalse(m["completed"])
        self.assertNotIn("wait", m)

    def test_no_submit_means_no_tts(self):
        m = trace.analyze([{"t": 1.0, "type": "tool_call"}, {"t": 5.0, "type": "failed"}])
        self.assertNotIn("tts", m)
        self.assertEqual(m["end"], "failed")


class GuardTest(unittest.TestCase):
    def test_nested_encoded_and_non_http_dangerous_urls_are_blocked(self):
        guard = guards.ActionGuard(allow_hosts=["fixture.localhost"])
        for url in [
            "http://fixture.localhost/api/publish/confirm",
            "http://fixture.localhost/%70ublish",
            "http://fixture.localhost/api%2Fshare%2Fconfirm",
            "http://fixture.localhost/%2570ublish",
            "file://fixture.localhost/etc/passwd",
        ]:
            with self.subTest(url=url):
                self.assertFalse(guard.check_url(url)["allowed"])
        self.assertTrue(guard.check_url("http://fixture.localhost/articles/publisher")["allowed"])

    def test_risky_labels_are_blocked_and_safe_ones_pass(self):
        guard = guards.ActionGuard(allow_labels=["Save draft"])
        self.assertTrue(guard.check_label("Save draft")["allowed"])
        self.assertFalse(guard.check_label("Publish now")["allowed"])
        self.assertFalse(guard.check_label("발행하기")["allowed"])
        self.assertFalse(guard.check_label("Share with anyone")["allowed"])
        self.assertTrue(guard.check_label("Open settings")["allowed"])
        self.assertEqual(len(guard.blocked), 3)

    def test_urls_follow_host_allow_list_and_denied_paths(self):
        guard = guards.ActionGuard(allow_hosts=["docs.google.com"])
        self.assertTrue(guard.check_url("https://docs.google.com/presentation/d/x/edit")["allowed"])
        self.assertFalse(guard.check_url("https://evil.example/edit")["allowed"])
        self.assertFalse(guard.check_url("https://docs.google.com/billing/upgrade")["allowed"])
        self.assertFalse(guard.check_url("https://docs.google.com/x/publish")["allowed"])
        self.assertFalse(guard.check_url("https://notdocs.google.com.evil.example/")["allowed"])

    def test_blocks_are_written_as_events(self):
        guard = guards.ActionGuard()
        guard.check_label("Pay")
        with tempfile.TemporaryDirectory() as tmp:
            run = RunDir("unit", root=tmp)
            guard.emit(run)
            run.finish()
            events = read_jsonl(run.path / "trace.jsonl")
            self.assertEqual(events[0]["type"], "blocked-action")
            self.assertEqual(events[0]["value"], "Pay")

    def test_blocked_url_events_redact_path_query_and_fragment(self):
        guard = guards.ActionGuard(allow_hosts=["fixture.localhost"])
        guard.check_url("https://agent:credential@fixture.localhost:8443/api/publish?token=private#confirm")
        with tempfile.TemporaryDirectory() as tmp:
            run = RunDir("unit", root=tmp)
            guard.emit(run)
            run.finish()
            events = read_jsonl(run.path / "trace.jsonl")
            self.assertEqual(events[0]["value"], "https://fixture.localhost")
            self.assertNotIn("private", str(events))
            self.assertNotIn("confirm", str(events))
            self.assertNotIn("credential", str(events))
            self.assertNotIn("agent", str(events))

    def test_url_redaction_formats_ipv6_origins_without_leaking_port(self):
        self.assertEqual(
            guards.redact_url("http://user:password@[::1]:8080/private?token=x#fragment"),
            "http://[::1]",
        )


if __name__ == "__main__":
    unittest.main()
