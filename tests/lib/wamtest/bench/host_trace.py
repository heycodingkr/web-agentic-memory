"""Normalize host JSONL without retaining prompts, arguments, results, or credentials.

Native streams describe host completion, not task success. The caller must use an
independent oracle and finish the RunContext after checking the child exit code.
"""
import re


class HostTraceError(ValueError):
    """A malformed/incomplete host stream; messages contain no host payloads."""


def error_signal(error, allow_message=False):
    """Classify structured API/host errors, never ordinary model/tool text."""
    if not isinstance(error, dict):
        return None
    status = error.get("status_code", error.get("status", error.get("http_status")))
    code = error.get("code", error.get("type"))
    if not isinstance(code, str):
        code = None
    if status == 401 or code in {"authentication_error", "authentication_failed", "unauthorized", "invalid_api_key"}:
        return "auth-required"
    if code in {"insufficient_quota", "quota_exceeded", "billing_error"}:
        return "quota-exceeded"
    if status == 429 or code in {"rate_limit_error", "rate_limit_exceeded"}:
        return "rate-limited"
    if (type(status) is int and 500 <= status <= 599) or code in {
        "api_error", "server_error", "overloaded_error", "internal_server_error",
    }:
        return "service-5xx"
    # Codex's terminal ThreadError has only a CLI-generated message. This branch
    # is never used for model text, tool output, or informational warning items.
    message = error.get("message")
    if allow_message and isinstance(message, str):
        text = message.lower()
        for pattern, signal in (
            (r"\b401\b|invalid.api.key|not logged in|authentication required", "auth-required"),
            (r"insufficient.quota|quota.exceeded|billing.hard.limit", "quota-exceeded"),
            (r"\b429\b|rate.limit", "rate-limited"),
            (r"\b(?:500|502|503|504)\b", "service-5xx"),
        ):
            if re.search(pattern, text):
                return signal
    return None


class HostTrace:
    def __init__(self, host, context, observe=None, allowed_tools=None):
        if host not in {"codex", "claude-code", "generic-mcp"}:
            raise ValueError("unsupported host")
        self.host = host
        self.context = context
        self.observe = observe
        self.allowed_tools = None if allowed_tools is None else set(allowed_tools)
        self.terminal = None
        self.calls = {}
        self.completed = set()
        self.event_count = 0
        self.abort_signal = None

    def _unavailable(self, signal):
        self.context.signal(signal)
        self.abort_signal = signal

    def _emit(self, type_, **fields):
        row = self.context.event(type_, **fields)
        if self.observe is not None:
            self.observe(dict(row))

    def _start(self, id_, name):
        if self.terminal is not None:
            raise HostTraceError("tool call after host completion")
        self._id(id_)
        if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9_.:/-]{1,256}", name):
            raise HostTraceError("invalid tool name")
        if self.allowed_tools is not None and name not in self.allowed_tools:
            raise HostTraceError("ungranted tool in host trace")
        if id_ in self.calls:
            if self.calls[id_][0] != name or id_ in self.completed:
                raise HostTraceError("reused native tool ID")
            return  # Claude may repeat an in-progress assistant content block.
        local_id = f"call-{len(self.calls) + 1}"
        self.calls[id_] = (name, local_id)
        self._emit("tool_call", name=name, call_id=local_id)

    def _result(self, id_, is_error=False):
        self._id(id_)
        if self.terminal is not None or id_ not in self.calls or id_ in self.completed:
            raise HostTraceError("unmatched native tool result")
        if type(is_error) is not bool:
            raise HostTraceError("invalid tool error flag")
        name, local_id = self.calls[id_]
        self.completed.add(id_)
        self._emit("tool_result", name=name, call_id=local_id, is_error=is_error)

    @staticmethod
    def _id(value):
        if (not isinstance(value, (str, int)) or isinstance(value, bool)
                or not str(value) or len(str(value)) > 256):
            raise HostTraceError("invalid native tool ID")

    def _end(self, success, error=None):
        if self.terminal is not None or type(success) is not bool:
            raise HostTraceError("invalid or repeated host completion")
        self.terminal = success
        signal = error_signal(error, allow_message=True)
        if signal:
            self._unavailable(signal)

    def feed(self, row):
        if not isinstance(row, dict) or not isinstance(row.get("type"), str):
            raise HostTraceError("host event must be a typed object")
        self.event_count += 1
        if self.host == "codex":
            self._codex(row)
        elif self.host == "claude-code":
            self._claude(row)
        else:
            self._mcp(row)

    def finish(self):
        if self.terminal is None:
            raise HostTraceError("host stream has no terminal event")
        if self.terminal and len(self.calls) != len(self.completed):
            raise HostTraceError("host completed with pending tools")
        return self.terminal

    def _codex(self, row):
        kind = row["type"]
        if kind in {"thread.started", "turn.started"}:
            if self.terminal is not None:
                raise HostTraceError("turn started after host completion")
            return
        if kind in {"turn.completed", "turn.failed"}:
            self._end(kind == "turn.completed", row.get("error"))
            return
        if kind == "error":
            signal = error_signal(row.get("error", row), allow_message=True)
            if signal:
                self._unavailable(signal)
            return
        if kind not in {"item.started", "item.updated", "item.completed"}:
            raise HostTraceError("unsupported Codex event type")
        item = row.get("item")
        if not isinstance(item, dict):
            raise HostTraceError("invalid Codex item")
        type_ = item.get("type")
        if not isinstance(type_, str):
            raise HostTraceError("invalid Codex item type")
        if type_ == "error":
            signal = error_signal(item)
            if signal:
                self.context.signal(signal)
            return  # The CLI also emits nonfatal metadata warnings as error items.
        if type_ in {"agent_message", "reasoning", "todo_list", "plan_update"}:
            return
        if type_ != "mcp_tool_call":
            # Native command/file/web/delegation tools are disabled by the launcher.
            raise HostTraceError("unexpected built-in Codex tool")
        name = f"mcp__{item.get('server', '')}__{item.get('tool', '')}"
        id_ = item.get("id")
        self._id(id_)
        if kind == "item.started":
            self._start(id_, name)
        elif kind == "item.completed":
            result = item.get("result") or {}
            if not isinstance(result, dict):
                raise HostTraceError("invalid Codex tool result")
            is_error = result.get("isError", False)
            if type(is_error) is not bool:
                raise HostTraceError("invalid Codex tool error flag")
            self._result(id_, bool(item.get("error")) or item.get("status") == "failed" or is_error)
        elif id_ not in self.calls:
            raise HostTraceError("tool update without a start")

    def _claude(self, row):
        kind = row["type"]
        if row.get("parent_tool_use_id") is not None:
            raise HostTraceError("unexpected subagent trace")
        if kind == "system":
            if row.get("subtype") == "api_retry":
                signal = error_signal({"status": row.get("error_status"), "code": row.get("error")})
                if signal:
                    self._unavailable(signal)
            if row.get("subtype") == "init":
                if row.get("mcp_server_errors"):
                    self.context.signal("feature-unavailable")
                servers = row.get("mcp_servers", [])
                if not isinstance(servers, list):
                    raise HostTraceError("invalid MCP initialization metadata")
                for server in servers:
                    if not isinstance(server, dict):
                        raise HostTraceError("invalid MCP initialization metadata")
                    if server.get("status") != "connected":
                        self.context.signal("feature-unavailable")
            return
        if kind in {"assistant", "user"}:
            signal = error_signal({"code": row.get("error")})
            if signal:
                self._unavailable(signal)
            message = row.get("message")
            if not isinstance(message, dict) or not isinstance(message.get("content"), list):
                raise HostTraceError("invalid Claude message")
            for block in message["content"]:
                if not isinstance(block, dict) or not isinstance(block.get("type"), str):
                    raise HostTraceError("invalid Claude content block")
                if block.get("type") == "tool_use":
                    name = block.get("name", "")
                    if not isinstance(name, str) or not name.startswith("mcp__"):
                        raise HostTraceError("unexpected built-in Claude tool")
                    self._start(block.get("id"), name)
                elif block.get("type") == "tool_result":
                    self._result(block.get("tool_use_id"), block.get("is_error", False))
                elif block.get("type") not in {"text", "thinking", "redacted_thinking", "image"}:
                    raise HostTraceError("unsupported Claude content block")
            return
        if kind == "result":
            is_error = row.get("is_error")
            if type(is_error) is not bool:
                raise HostTraceError("Claude result omitted its error flag")
            error = row.get("error")
            if is_error and error is None and isinstance(row.get("errors"), list):
                error = {"message": "\n".join(value for value in row["errors"] if isinstance(value, str))}
            self._end(not is_error and row.get("subtype") == "success", error)
            return
        if kind in {"rate_limit_event", "tool_progress", "tool_use_summary"}:
            if kind == "rate_limit_event":
                info = row.get("rate_limit_info")
                if not isinstance(info, dict):
                    raise HostTraceError("invalid rate limit metadata")
                if info.get("status") == "rejected":
                    self._unavailable("rate-limited")
            return
        raise HostTraceError("unsupported Claude event type")

    def _mcp(self, row):
        if row.get("schema_version") != 1:
            raise HostTraceError("unsupported generic MCP trace version")
        if row["type"] == "host.completed":
            self._end(row.get("success"), row.get("error"))
            return
        if row["type"] != "mcp" or row.get("direction") not in ("client", "server"):
            raise HostTraceError("invalid generic MCP envelope")
        message = row.get("message")
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            raise HostTraceError("invalid MCP message")
        if "id" in message:
            self._id(message["id"])
        if message.get("method") == "tools/call" and row["direction"] == "client":
            params = message.get("params") or {}
            if not isinstance(params, dict):
                raise HostTraceError("invalid MCP call params")
            self._start(message.get("id"), params.get("name"))
        elif row["direction"] == "server" and message.get("id") in self.calls:
            result = message.get("result") or {}
            if not isinstance(result, dict):
                raise HostTraceError("invalid MCP result")
            flag = result.get("isError", False)
            if type(flag) is not bool:
                raise HostTraceError("invalid MCP error flag")
            self._result(message["id"], "error" in message or flag)
