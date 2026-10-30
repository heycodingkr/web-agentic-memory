#!/usr/bin/env python3
"""Installed CLI acceptance with loopback synthetic model APIs and one owned MCP.

No real model, account, credential, or browser is used. This verifies the actual
host JSONL schema/process integration, not model quality or host behavior.
"""
import argparse
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import sys
import tempfile
import threading

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from wamtest.bench.host import CliHostAdapter, HostRun
from wamtest.bench.runner import run_plan
from wamtest.bench.schedule import make_plan
from wamtest.evidence import DEFAULT_ROOT, new_run_id, read_jsonl
from wamtest.procs import group_pids, kill_by_arg, pids_with_arg

TEXT = "synthetic host adapter acceptance"


def mcp_child(path):
    """A tiny stdio MCP that can only save the fixed synthetic text."""
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request:
            continue
        method = request.get("method")
        if method == "initialize":
            result = {"protocolVersion": request.get("params", {}).get("protocolVersion", "2025-03-26"),
                      "capabilities": {"tools": {}}, "serverInfo": {"name": "wam-host-fixture", "version": "1"}}
        elif method == "tools/list":
            result = {"tools": [{"name": "save", "description": "Save the synthetic acceptance text.",
                "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}},
                                "required": ["text"], "additionalProperties": False}}]}
        elif method == "tools/call":
            params = request.get("params", {})
            valid = params.get("name") == "save" and params.get("arguments") == {"text": TEXT}
            if valid:
                Path(path).write_text(TEXT, encoding="utf-8")
            result = {"content": [{"type": "text", "text": "saved" if valid else "rejected"}], "isError": not valid}
        else:
            print(json.dumps({"jsonrpc": "2.0", "id": request["id"],
                              "error": {"code": -32601, "message": "unsupported fixture method"}}), flush=True)
            continue
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)
    return 0


def _tool_name(tools, namespace=None):
    for tool in tools:
        if not isinstance(tool, dict):
            continue
        name = tool.get("name", "")
        if name.endswith("fixture__save") or (namespace == "mcp__fixture" and name == "save"):
            return {"name": name, **({"namespace": namespace} if namespace else {})}
        nested = _tool_name(tool.get("tools", []), name if tool.get("type") == "namespace" else namespace)
        if nested:
            return nested
    return None


class ModelFixture:
    def __init__(self, host, scenario="success"):
        self.host = host
        self.requests = 0
        self.errors = []
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def _json(self, value, status=200):
                content = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(content)))
                self.end_headers()
                self.wfile.write(content)

            def do_GET(self):
                self._json({"object": "list", "data": [{"id": "fixture-model", "object": "model", "owned_by": "fixture"}]})

            def do_POST(self):
                size = int(self.headers.get("Content-Length", "0"))
                if not 0 < size <= 4 * 1024 * 1024:
                    return self._json({"error": {"type": "fixture_body_limit"}}, 400)
                body = json.loads(self.rfile.read(size))
                if self.path.endswith("count_tokens"):
                    return self._json({"input_tokens": 1})
                fixture.requests += 1
                if scenario == "auth":
                    return self._json({"type": "error", "error": {"type": "authentication_error",
                        "code": "invalid_api_key", "message": "synthetic authentication required"}}, 401)
                name = _tool_name(body.get("tools", []))
                if fixture.requests > 4 or name is None:
                    fixture.errors.append("request-limit" if fixture.requests > 4 else "fixture-tool-missing")
                    return self._json({"error": {"type": "invalid_request_error", "message": "fixture tool missing or limit reached"}}, 400)
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Cache-Control", "no-cache")
                self.send_header("Connection", "close")
                self.end_headers()
                if fixture.host == "codex":
                    events = fixture.codex_response(body, name)
                else:
                    events = fixture.claude_response(body, name)
                for event in events:
                    self.wfile.write((f"event: {event['type']}\ndata: {json.dumps(event)}\n\n").encode())
                    self.wfile.flush()

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.url = f"http://127.0.0.1:{self.server.server_port}"

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)

    def codex_response(self, body, name):
        has_output = any(item.get("type") == "function_call_output" for item in body.get("input", [])
                         if isinstance(item, dict))
        if has_output:
            item = {"type": "message", "id": "msg_fixture", "role": "assistant", "status": "completed",
                    "content": [{"type": "output_text", "text": "Saved.", "annotations": []}]}
        else:
            item = {"type": "function_call", "id": "fc_fixture", "call_id": "call_fixture", **name,
                    "arguments": json.dumps({"text": TEXT}), "status": "completed"}
        response = {"id": f"resp_fixture_{self.requests}", "object": "response", "created_at": 1,
                    "status": "in_progress", "model": "fixture-model", "output": []}
        events = [{"type": "response.created", "response": dict(response)}]
        events.append({"type": "response.output_item.added", "output_index": 0,
                       "item": {**item, "status": "in_progress", **({"arguments": ""} if not has_output else {})}})
        if not has_output:
            events.append({"type": "response.function_call_arguments.delta", "output_index": 0,
                           "item_id": item["id"], "delta": item["arguments"]})
            events.append({"type": "response.function_call_arguments.done", "output_index": 0,
                           "item_id": item["id"], "arguments": item["arguments"]})
        else:
            events += [
                {"type": "response.content_part.added", "item_id": item["id"], "output_index": 0,
                 "content_index": 0, "part": {"type": "output_text", "text": "", "annotations": []}},
                {"type": "response.output_text.delta", "item_id": item["id"], "output_index": 0,
                 "content_index": 0, "delta": "Saved."},
                {"type": "response.output_text.done", "item_id": item["id"], "output_index": 0,
                 "content_index": 0, "text": "Saved."},
                {"type": "response.content_part.done", "item_id": item["id"], "output_index": 0,
                 "content_index": 0, "part": item["content"][0]},
            ]
        events.append({"type": "response.output_item.done", "output_index": 0, "item": item})
        response.update(status="completed", output=[item], usage={"input_tokens": 1, "output_tokens": 1,
                        "total_tokens": 2, "input_tokens_details": {"cached_tokens": 0}})
        events.append({"type": "response.completed", "response": response})
        for index, event in enumerate(events):
            event["sequence_number"] = index
        return events

    def claude_response(self, body, name):
        has_output = any(block.get("type") == "tool_result" for message in body.get("messages", [])
                         for block in message.get("content", []) if isinstance(block, dict))
        block = {"type": "text", "text": ""} if has_output else {
            "type": "tool_use", "id": "toolu_fixture", "name": name["name"], "input": {},
        }
        delta = {"type": "text_delta", "text": "Saved."} if has_output else {
            "type": "input_json_delta", "partial_json": json.dumps({"text": TEXT}),
        }
        return [
            {"type": "message_start", "message": {"id": f"msg_fixture_{self.requests}", "type": "message",
                "role": "assistant", "content": [], "model": "fixture-model", "stop_reason": None,
                "stop_sequence": None, "usage": {"input_tokens": 1, "output_tokens": 0}}},
            {"type": "content_block_start", "index": 0, "content_block": block},
            {"type": "content_block_delta", "index": 0, "delta": delta},
            {"type": "content_block_stop", "index": 0},
            {"type": "message_delta", "delta": {"stop_reason": "end_turn" if has_output else "tool_use",
                "stop_sequence": None}, "usage": {"output_tokens": 1}},
            {"type": "message_stop"},
        ]


def check(host, output, scenario="success"):
    with tempfile.TemporaryDirectory(prefix="wam-host-oracle-") as temp, ModelFixture(host, scenario) as model:
        saved = Path(temp) / "saved.txt"

        @contextmanager
        def prepare(context, spec):
            yield HostRun(
                f"Use the fixture save MCP tool once with text {TEXT!r}, then finish.",
                lambda: saved.is_file() and saved.read_text() == TEXT,
                {"fixture": {"command": sys.executable, "args": [str(Path(__file__).resolve()), "--mcp-child", str(saved)]}},
                allowed_tools={"fixture": ["save"]},
            )

        adapter = CliHostAdapter(host, "fixture-model", prepare, base_url=model.url + ("/v1" if host == "codex" else ""),
                                 env={"CODEX_API_KEY": "synthetic-no-account", "ANTHROPIC_API_KEY": "synthetic-no-account",
                                      "ENABLE_TOOL_SEARCH": "false"}, timeout=35, retain_raw=True)
        summary = run_plan(make_plan(["native-mcp-save"], ["T"], 1, 1), adapter, output, n_boot=10,
                           metadata={"host": host, "model": "synthetic-loopback", "paid_model_calls": 0})
        row = read_jsonl(output / "results.jsonl")[0]
        meta = json.loads(next((output / "runs").glob("*/meta.json")).read_text())
        clean = not group_pids(meta["native_host"]["pid"]) and not pids_with_arg(str(saved))
        if not clean:
            kill_by_arg(str(saved))
        expected = (summary["successes"] == 1 and row["tool_calls"] == 1 and row["tool_errors"] == 0
                    if scenario == "success" else row["invalid_class"] == "invalid-auth")
        return {"host": host, "scenario": scenario, "passed": expected and clean, "cleanup_passed": clean,
                "model_requests": model.requests,
                "fixture_errors": model.errors, "invalid_class": row["invalid_class"], "evidence": str(output)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=["codex", "claude-code", "all"], default="all")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--scenario", choices=["success", "auth"], default="success")
    parser.add_argument("--mcp-child", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.mcp_child:
        return mcp_child(args.mcp_child)
    root = args.output or DEFAULT_ROOT / "host-native" / new_run_id()
    hosts = ["codex", "claude-code"] if args.host == "all" else [args.host]
    results = [check(host, root / host, args.scenario) for host in hosts]
    print(json.dumps({"paid_model_calls": 0, "checks": results}, indent=2))
    return 0 if all(result["passed"] for result in results) else 1


if __name__ == "__main__":
    sys.exit(main())
