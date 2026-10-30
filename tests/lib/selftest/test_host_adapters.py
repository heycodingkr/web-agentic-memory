"""Process/trace boundary tests use disposable fake executables, never models."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

from wamtest.bench.context import RunContext
from wamtest.bench.host import CliHostAdapter, HostRun
from wamtest.bench.host_trace import HostTrace, HostTraceError
from wamtest.bench.runner import run_plan
from wamtest.bench.schedule import make_plan
from wamtest.evidence import RunDir, read_jsonl
from wamtest.procs import Isolation, group_pids

REPO = Path(__file__).resolve().parents[3]


def codex_events():
    return [
        {"type": "thread.started", "thread_id": "private-thread"},
        {"type": "turn.started"},
        {"type": "item.started", "item": {"id": "private-id", "type": "mcp_tool_call",
            "server": "fixture", "tool": "save", "arguments": {"text": "PRIVATE-SENTINEL"}}},
        {"type": "item.updated", "item": {"id": "private-id", "type": "mcp_tool_call",
            "server": "fixture", "tool": "save"}},
        {"type": "item.completed", "item": {"id": "private-id", "type": "mcp_tool_call",
            "server": "fixture", "tool": "save", "status": "completed",
            "result": {"content": [{"type": "text", "text": "PRIVATE-SENTINEL"}]}}},
        {"type": "item.completed", "item": {"id": "answer", "type": "agent_message",
            "text": "PRIVATE-SENTINEL"}},
        {"type": "turn.completed", "usage": {"input_tokens": 10, "output_tokens": 3}},
    ]


def claude_events():
    return [
        {"type": "system", "subtype": "init", "mcp_servers": [{"name": "fixture", "status": "connected"}]},
        {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "private-id",
            "name": "mcp__fixture__save", "input": {"text": "PRIVATE-SENTINEL"}}]}},
        {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "private-id",
            "content": "PRIVATE-SENTINEL", "is_error": False}]}},
        {"type": "assistant", "message": {"content": [{"type": "text", "text": "PRIVATE-SENTINEL"}]}},
        {"type": "result", "subtype": "success", "is_error": False, "result": "PRIVATE-SENTINEL"},
    ]


def mcp_events():
    return [
        {"type": "mcp", "schema_version": 1, "direction": "client", "message": {"jsonrpc": "2.0", "id": "private-id",
            "method": "tools/call", "params": {"name": "save", "arguments": {"text": "PRIVATE-SENTINEL"}}}},
        {"type": "mcp", "schema_version": 1, "direction": "server", "message": {"jsonrpc": "2.0", "id": "private-id",
            "result": {"content": [{"type": "text", "text": "PRIVATE-SENTINEL"}]}}},
        {"type": "host.completed", "schema_version": 1, "success": True},
    ]


class HostTraceTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.evidence = RunDir("trace", root=self.temp.name)
        self.isolation = Isolation("host-trace-test")
        self.context = RunContext(self.evidence, self.isolation, ["localhost"])

    def tearDown(self):
        self.evidence.finish()
        self.isolation.cleanup()
        self.temp.cleanup()

    def test_supported_native_streams_count_once_and_drop_private_payloads(self):
        for host, events in [("codex", codex_events()), ("claude-code", claude_events()),
                             ("generic-mcp", mcp_events())]:
            context = RunContext(self.evidence, self.isolation, ["localhost"])
            native = HostTrace(host, context)
            for event in events:
                native.feed(event)
            self.assertTrue(native.finish())
            self.assertEqual([event["type"] for event in context.events], ["tool_call", "tool_result"])
            self.assertEqual(context.events[0]["call_id"], "call-1")
            serialized = json.dumps(context.events)
            self.assertNotIn("PRIVATE-SENTINEL", serialized)
            self.assertNotIn("private-id", serialized)
            self.assertFalse(any(event["type"] == "submit" for event in context.events))

    def test_error_flags_and_structured_quota_are_preserved(self):
        events = claude_events()
        events[2]["message"]["content"][0]["is_error"] = True
        events[-1] = {"type": "result", "subtype": "error_during_execution", "is_error": True,
                      "error": {"type": "rate_limit_error"}}
        native = HostTrace("claude-code", self.context)
        for event in events:
            native.feed(event)
        self.assertFalse(native.finish())
        self.assertTrue(self.context.events[-1]["is_error"])
        self.assertIn("rate-limited", self.context.signals)

    def test_no_terminal_pending_calls_unknown_tools_and_subagents_are_rejected(self):
        cases = [
            ("codex", codex_events()[:-1]),
            ("codex", codex_events()[:3] + [codex_events()[-1]]),
            ("codex", [{"type": "item.started", "item": {"type": "command_execution", "id": "x"}}]),
            ("claude-code", [{"type": "assistant", "parent_tool_use_id": "agent",
                              "message": {"content": []}}]),
            ("claude-code", [{"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": "x", "name": "Bash"}]}}]),
            ("codex", [{"type": "unknown"}]),
            ("claude-code", [{"type": "result", "subtype": "success"}]),
        ]
        for host, events in cases:
            with self.subTest(host=host, events=events):
                native = HostTrace(host, self.context)
                with self.assertRaises(HostTraceError):
                    for event in events:
                        native.feed(event)
                    native.finish()

    def test_results_must_match_open_calls_and_boolean_flags(self):
        for host, row in [
            ("codex", codex_events()[4]),
            ("claude-code", claude_events()[2]),
            ("generic-mcp", {"type": "mcp", "direction": "client", "message": {
                "jsonrpc": "2.0", "id": True, "method": "tools/call", "params": {"name": "save"}}}),
        ]:
            with self.subTest(host=host):
                with self.assertRaises(HostTraceError):
                    HostTrace(host, self.context).feed(row)
        native = HostTrace("claude-code", self.context)
        native.feed(claude_events()[1])
        bad = claude_events()[2]
        bad["message"]["content"][0]["is_error"] = "false"
        with self.assertRaises(HostTraceError):
            native.feed(bad)

    def test_failed_mcp_initialization_is_unavailable_even_when_model_finishes(self):
        native = HostTrace("claude-code", self.context)
        native.feed({"type": "system", "subtype": "init", "mcp_servers": [{"name": "x", "status": "failed"}]})
        native.feed(claude_events()[-1])
        self.assertTrue(native.finish())
        self.assertIn("feature-unavailable", self.context.signals)

    def test_codex_metadata_warning_does_not_abort_an_otherwise_complete_native_trace(self):
        native = HostTrace("codex", self.context)
        native.feed({"type": "item.completed", "item": {
            "id": "warning", "type": "error", "message": "Model metadata not found; using fallback."}})
        for event in codex_events():
            native.feed(event)
        self.assertTrue(native.finish())
        self.assertEqual(len(self.context.events), 2)

    def test_malformed_host_shapes_raise_a_sanitized_protocol_error(self):
        rows = [
            ("claude-code", {"type": "system", "subtype": "init", "mcp_servers": None}),
            ("claude-code", {"type": "rate_limit_event", "rate_limit_info": None}),
            ("claude-code", {"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": [], "is_error": False}]}}),
            ("generic-mcp", {"type": "mcp", "direction": "server", "message": {"jsonrpc": "2.0", "id": []}}),
        ]
        for host, row in rows:
            with self.subTest(host=host, row=row), self.assertRaises(HostTraceError):
                HostTrace(host, self.context).feed(row)

    def test_ungranted_tool_names_are_rejected_before_they_can_enter_evidence(self):
        native = HostTrace("codex", self.context, allowed_tools={"mcp__fixture__other"})
        with self.assertRaises(HostTraceError):
            native.feed(codex_events()[2])
        self.assertEqual(self.context.events, [])

    def test_codex_terminal_cli_errors_are_classified_but_tool_text_is_not(self):
        native = HostTrace("codex", self.context)
        native.feed({"type": "turn.failed", "error": {"message": "unexpected status 401 Unauthorized"}})
        self.assertFalse(native.finish())
        self.assertIn("auth-required", self.context.signals)
        other = RunContext(self.evidence, self.isolation, ["localhost"])
        native = HostTrace("codex", other)
        events = codex_events()
        events[4]["item"]["result"]["content"][0]["text"] = "HTTP 401 rate_limit_error"
        for event in events:
            native.feed(event)
        self.assertTrue(native.finish())
        self.assertEqual(other.signals, [])

    def test_claude_auth_retry_requests_an_early_stop_without_becoming_a_timeout(self):
        native = HostTrace("claude-code", self.context)
        native.feed({"type": "system", "subtype": "api_retry", "attempt": 1,
                     "error_status": 401, "error": "authentication_failed"})
        self.assertIn("auth-required", self.context.signals)
        self.assertEqual(native.abort_signal, "auth-required")


FAKE = '''import json, os, pathlib, subprocess, sys, time
probe = pathlib.Path(os.environ["HOST_PROBE"])
data = {"argv": sys.argv[1:], "prompt": sys.stdin.read(), "env": {
    key: os.environ.get(key) for key in ["HOME", "TMPDIR", "CODEX_HOME", "CLAUDE_CONFIG_DIR", "HOST_SECRET"]},
    "cwd": os.getcwd(), "config": pathlib.Path(os.environ["WAM_HOST_MCP_CONFIG"]).read_text()}
probe.write_text(json.dumps(data))
mode = os.environ.get("HOST_MODE", "ok")
if mode == "timeout":
    child = subprocess.Popen([sys.executable, "-c", "import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); time.sleep(120)"])
    data["child_pid"] = child.pid
    probe.write_text(json.dumps(data))
    time.sleep(120)
if mode == "overflow":
    print("x" * 10000, flush=True)
    time.sleep(120)
if mode == "longline":
    print(json.dumps({"type":"item.completed","item":{"type":"agent_message","text":"x" * 1100000}}), flush=True)
    time.sleep(120)
if mode == "bad-json":
    print("not json", flush=True)
    sys.exit()
if mode == "auth":
    print("Error: 401 PRIVATE-SENTINEL", file=sys.stderr, flush=True)
    sys.exit(1)
if mode == "retryauth":
    print(json.dumps({"type":"system","subtype":"api_retry","error_status":401,"error":"authentication_failed"}), flush=True)
    time.sleep(120)
events = json.loads(os.environ["HOST_EVENTS"])
if mode == "incomplete": events.pop()
if mode == "nonzero": code = 2
else: code = 0
for event in events:
    print(json.dumps(event), flush=True)
print("PRIVATE-SENTINEL", file=sys.stderr)
if mode == "ok": pathlib.Path(os.environ["HOST_ORACLE"]).write_text("saved")
sys.exit(code)
'''


class HostProcessTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.fake = self.root / "fake.py"
        self.fake.write_text(FAKE)
        self.probe = self.root / "probe.json"
        self.oracle = self.root / "oracle.txt"
        self.plan = make_plan(["fixture"], ["T"], 1, 3)

    def tearDown(self):
        self.temp.cleanup()

    def adapter(self, host="codex", mode="ok", verdict=None, **kwargs):
        @contextmanager
        def prepare(context, spec):
            yield HostRun("PRIVATE-SENTINEL prompt", lambda: self.oracle.exists() if verdict is None else verdict,
                          {"fixture": {"command": sys.executable, "args": [str(self.fake)]}},
                          allowed_tools={"fixture": ["save"]})

        events = {"codex": codex_events, "claude-code": claude_events, "generic-mcp": mcp_events}[host]()
        return CliHostAdapter(host, "fixture-model", prepare, command=[sys.executable, str(self.fake)],
                              env={"HOST_MODE": mode, "HOST_PROBE": str(self.probe),
                                   "HOST_ORACLE": str(self.oracle), "HOST_SECRET": "PRIVATE-SENTINEL",
                                   "HOST_EVENTS": json.dumps(events)}, **kwargs)

    def run_host(self, adapter):
        output = self.root / f"evidence-{len(list(self.root.glob('evidence-*')))}"
        summary = run_plan(self.plan, adapter, output, n_boot=10)
        return output, summary, read_jsonl(output / "results.jsonl")[0]

    def test_all_hosts_launch_in_fresh_profiles_keep_prompt_off_argv_and_drop_raw(self):
        for host in ("codex", "claude-code", "generic-mcp"):
            output, summary, row = self.run_host(self.adapter(host))
            self.assertEqual(summary["successes"], 1)
            self.assertFalse(row["invalid"])
            probe = json.loads(self.probe.read_text())
            self.assertNotIn("PRIVATE-SENTINEL", json.dumps(probe["argv"]))
            self.assertEqual(probe["prompt"], "PRIVATE-SENTINEL prompt")
            self.assertNotEqual(probe["env"]["HOME"], os.environ.get("HOME"))
            self.assertFalse(Path(probe["env"]["HOME"]).exists())
            meta = json.loads(next((output / "runs").glob("*/meta.json")).read_text())
            self.assertEqual(meta["native_host"]["exit_code"], 0)
            self.assertEqual(group_pids(meta["native_host"]["pid"]), [])
            for path in output.rglob("*"):
                if path.is_file():
                    self.assertNotIn(b"PRIVATE-SENTINEL", path.read_bytes(), str(path))

    def test_model_completion_never_overrides_the_independent_oracle(self):
        _, summary, row = self.run_host(self.adapter(verdict=False))
        self.assertEqual(summary["successes"], 0)
        self.assertFalse(row["invalid"])
        self.assertEqual(row["end"], "failed")

    def test_bad_or_incomplete_output_and_nonzero_exit_do_not_pass(self):
        for mode in ("bad-json", "incomplete", "nonzero", "overflow"):
            _, summary, row = self.run_host(self.adapter(mode=mode, max_output_bytes=2048, timeout=2))
            self.assertEqual(summary["successes"], 0)
            self.assertEqual(row["end"], "failed")
            if mode != "nonzero":
                self.assertEqual(row["invalid_class"], "invalid-env")

    def test_startup_auth_error_has_its_own_class_and_no_raw_message(self):
        _, _, row = self.run_host(self.adapter(mode="auth"))
        self.assertEqual(row["invalid_class"], "invalid-auth")

    def test_timeout_stops_parent_and_child_processes(self):
        started = time.monotonic()
        output, _, row = self.run_host(self.adapter(mode="timeout", timeout=0.4))
        self.assertLess(time.monotonic() - started, 5)
        self.assertEqual(row["invalid_class"], "invalid-env")
        meta = json.loads(next((output / "runs").glob("*/meta.json")).read_text())
        self.assertEqual(group_pids(meta["native_host"]["pid"]), [])
        self.assertIn("child_pid", json.loads(self.probe.read_text()))

    def test_structured_auth_retry_stops_the_cli_before_its_long_retry_sleep(self):
        started = time.monotonic()
        _, _, row = self.run_host(self.adapter("claude-code", mode="retryauth", timeout=10))
        self.assertLess(time.monotonic() - started, 3)
        self.assertEqual(row["invalid_class"], "invalid-auth")

    def test_raw_capture_is_explicit_and_private(self):
        output, _, _ = self.run_host(self.adapter(retain_raw=True))
        streams = list(output.glob("runs/*/sinks/host-*.raw"))
        self.assertEqual(len(streams), 2)
        for stream in streams:
            self.assertEqual(stream.stat().st_mode & 0o777, 0o600)
        self.assertIn(b"PRIVATE-SENTINEL", streams[0].read_bytes() + streams[1].read_bytes())

    def test_single_line_limit_applies_even_when_the_newline_arrives_in_the_same_chunk(self):
        _, summary, row = self.run_host(self.adapter(mode="longline", max_output_bytes=2 * 1024 * 1024))
        self.assertEqual(summary["successes"], 0)
        self.assertEqual(row["invalid_class"], "invalid-env")

    def test_native_tool_grants_are_explicit_in_config_and_cli(self):
        import tomllib
        for host in ("codex", "claude-code"):
            adapter = self.adapter(host)
            isolation = Isolation("host-config")
            try:
                with adapter.prepare(None, {}) as run:
                    class Context:
                        pass
                    context = Context()
                    context.isolation = isolation
                    context.evidence = Context()
                    context.evidence.meta = {}
                    argv, _, _, _ = adapter._configure(context, run)
                    if host == "codex":
                        config = tomllib.loads((isolation.root / "codex/config.toml").read_text())
                        self.assertFalse(config["features"]["shell_tool"])
                        self.assertFalse(config["features"]["multi_agent"])
                        self.assertEqual(config["mcp_servers"]["fixture"]["enabled_tools"], ["save"])
                        self.assertEqual(config["mcp_servers"]["fixture"]["tools"]["save"]["approval_mode"], "approve")
                    else:
                        self.assertIn("--restricted", argv)
                        self.assertEqual(argv[argv.index("--tools") + 1], "")
                        self.assertEqual(argv[argv.index("--allowedTools") + 1], "mcp__fixture__save")
                    for file in [isolation.root / "mcp.json", isolation.root / "prompt.txt"]:
                        self.assertEqual(file.stat().st_mode & 0o777, 0o600)
            finally:
                isolation.cleanup()

    def test_cli_runs_a_generic_driver_and_a_task_factory(self):
        factory = self.root / "task_factory.py"
        factory.write_text('''from contextlib import contextmanager
from pathlib import Path
import os
from wamtest.bench.host import HostRun
@contextmanager
def prepare(context, spec):
    yield HostRun("PRIVATE-SENTINEL", lambda: Path(os.environ["HOST_ORACLE"]).exists(),
        {"fixture":{"command":os.environ["HOST_PYTHON"]}}, allowed_tools={"fixture":["save"]})
''')
        plan = self.root / "plan.json"
        plan.write_text(json.dumps(self.plan))
        output = self.root / "cli-output"
        variables = {"HOST_PROBE": str(self.probe), "HOST_ORACLE": str(self.oracle),
                     "HOST_EVENTS": json.dumps(mcp_events()), "HOST_PYTHON": sys.executable}
        argv = [sys.executable, "-B", str(REPO / "tests/bench/run_host.py"), "--host", "generic-mcp",
                "--model", "fixture", "--plan", str(plan), "--output", str(output),
                "--factory", "task_factory:prepare", "--executable", sys.executable,
                "--host-arg", str(self.fake)]
        for name in variables:
            argv += ["--pass-env", name]
        env = {**os.environ, **variables, "PYTHONPATH": str(self.root)}
        done = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=10)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(json.loads(done.stdout)["successes"], 1)
        self.assertNotIn("PRIVATE-SENTINEL", done.stdout + done.stderr)

    def test_live_adapter_cannot_make_a_controlled_claim(self):
        metadata = {key: "fixture" for key in ("host", "model", "effort", "prompt_hash", "tools_hash")}
        with self.assertRaisesRegex(ValueError, "controlled-comparison"):
            run_plan(self.plan, self.adapter(), self.root / "controlled", controlled=True, metadata=metadata)
        self.assertFalse((self.root / "controlled").exists())

    def test_profile_environment_and_invalid_limits_are_rejected_before_launch(self):
        for options in ({"timeout": float("nan")}, {"timeout": 0}, {"max_output_bytes": 3},
                        {"env": {"HOME": "elsewhere"}}, {"base_url": "https://outside.example"},
                        {"base_url": "http://user:secret@localhost"}, {"max_budget_usd": float("nan")}):
            with self.subTest(options=options), self.assertRaises(ValueError):
                CliHostAdapter("codex", "fixture", lambda *_: None,
                               command=[sys.executable, str(self.fake)], **options)


if __name__ == "__main__":
    unittest.main()
