"""Bounded, isolated Claude/Codex/generic-MCP processes for the benchmark runner.

All model/backend credentials and MCP servers are explicitly supplied. Native
user settings and sessions are never copied. The preparation context manager
owns the browser/network guard and independent oracle for its particular task.
"""
from dataclasses import dataclass, field
import hashlib
import json
import math
import os
from pathlib import Path
import re
import selectors
import shutil
import subprocess
import time

from ..procs import ManagedProcess
from .host_trace import HostTrace, HostTraceError, error_signal

HOSTS = {"codex", "claude-code", "generic-mcp"}
RESERVED_ENV = {"HOME", "TMPDIR", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME",
                "CODEX_HOME", "CLAUDE_CONFIG_DIR", "WAM_HOST_MCP_CONFIG"}


@dataclass
class HostRun:
    prompt: str
    oracle: object
    mcp_servers: dict = field(default_factory=dict)
    observe: object = None
    allowed_tools: dict = field(default_factory=dict)


def _private_stream(path):
    return os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb")


def _private_file(path, content):
    with _private_stream(path) as stream:
        stream.write(content.encode("utf-8") if isinstance(content, str) else content)


class _HostUnavailable(RuntimeError):
    def __init__(self, signal):
        self.signal = signal


def _toml(value):
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if type(value) is bool:
        return "true" if value else "false"
    if isinstance(value, list):
        return "[" + ",".join(_toml(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(f"{_toml(key)}={_toml(item)}" for key, item in value.items()) + "}"
    raise ValueError("unsupported MCP config value")


def _servers(servers):
    if not isinstance(servers, dict):
        raise ValueError("mcp_servers must be an object")
    result = {}
    for name, config in servers.items():
        if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]{0,63}", name):
            raise ValueError("invalid MCP server name")
        if not isinstance(config, dict) or set(config) - {"command", "args", "env", "url", "headers"}:
            raise ValueError("unsupported MCP server fields")
        config = dict(config)
        if "command" in config:
            if set(config) & {"url", "headers"}:
                raise ValueError("MCP server must select one transport")
            command = config["command"]
            if not isinstance(command, str) or not Path(command).is_absolute():
                raise ValueError("MCP commands must be absolute paths")
            args = config.get("args", [])
            env = config.get("env", {})
            if not isinstance(args, list) or any(not isinstance(arg, str) or "\x00" in arg for arg in args):
                raise ValueError("invalid MCP command arguments")
            if not isinstance(env, dict) or any(not isinstance(k, str) or not isinstance(v, str)
                    or k in RESERVED_ENV or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", k) or "\x00" in v
                    for k, v in env.items()):
                raise ValueError("invalid MCP environment")
        elif "url" in config:
            from urllib.parse import urlsplit
            url = urlsplit(config["url"])
            if url.scheme not in {"http", "https"} or not url.hostname or url.username or url.password or url.fragment:
                raise ValueError("invalid MCP endpoint")
            headers = config.get("headers", {})
            if not isinstance(headers, dict) or any(not isinstance(k, str) or not isinstance(v, str)
                                                     for k, v in headers.items()):
                raise ValueError("invalid MCP headers")
        else:
            raise ValueError("MCP server needs a command or URL")
        result[name] = config
    return result


class CliHostAdapter:
    """prepare(context, spec) yields HostRun and cleans up its owned browser.

    Native receive times are measured here. No submit event is invented from a
    model's text or a tool name: a task's independent observer must emit it.
    These adapters are deliberately uncontrolled until live interception and
    model-host acceptance are independently established.
    """
    controlled_ready = False
    replay_only = False

    def __init__(self, host, model, prepare, *, effort=None, command=None, env=None,
                 timeout=180.0, max_output_bytes=8 * 1024 * 1024, retain_raw=False,
                 base_url=None, max_budget_usd=None):
        if host not in HOSTS or not isinstance(model, str) or not model.strip():
            raise ValueError("a supported host and explicit model are required")
        if os.name != "posix":
            raise ValueError("host process-group cleanup currently requires POSIX")
        if not callable(prepare):
            raise ValueError("prepare must be a task context-manager factory")
        if type(timeout) not in (int, float) or not math.isfinite(timeout) or timeout <= 0:
            raise ValueError("timeout must be positive and finite")
        if type(max_output_bytes) is not int or max_output_bytes < 1024:
            raise ValueError("output limit must be at least 1024 bytes")
        if max_budget_usd is not None and (host != "claude-code" or type(max_budget_usd) not in (int, float)
                or not math.isfinite(max_budget_usd) or max_budget_usd <= 0):
            raise ValueError("a positive finite USD budget is supported only by Claude Code")
        if base_url is not None:
            from urllib.parse import urlsplit
            if not isinstance(base_url, str):
                raise ValueError("fixture backend URL must be a string")
            backend = urlsplit(base_url)
            if (backend.scheme not in {"http", "https"} or backend.hostname not in {"127.0.0.1", "localhost", "::1"}
                    or backend.username or backend.password or backend.query or backend.fragment):
                raise ValueError("fixture backend must be a loopback URL without credentials")
        if env is not None and (not isinstance(env, dict) or any(
            not isinstance(k, str) or not isinstance(v, str) or k in RESERVED_ENV
            or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", k) or "\x00" in v
            for k, v in env.items()
        )):
            raise ValueError("invalid or reserved host environment")
        if command is None:
            if host == "generic-mcp":
                raise ValueError("generic MCP requires an explicit command")
            executable = shutil.which("codex" if host == "codex" else "claude")
            if executable is None:
                raise ValueError("host executable is not installed")
            command = [executable]
        if (not isinstance(command, (tuple, list)) or not command
                or any(not isinstance(arg, str) or "\x00" in arg for arg in command)
                or not Path(command[0]).is_absolute()):
            raise ValueError("host command must use an absolute executable")
        self.host, self.model, self.prepare = host, model, prepare
        self.effort, self.command, self.env = effort, list(command), dict(env or {})
        self.timeout, self.max_output_bytes, self.retain_raw = timeout, max_output_bytes, retain_raw
        self.base_url, self.max_budget_usd = base_url, max_budget_usd

    def _configure(self, context, run):
        if (not isinstance(run, HostRun) or not isinstance(run.prompt, str)
                or not run.prompt.strip() or len(run.prompt.encode("utf-8")) > 65536
                or not callable(run.oracle) or (run.observe is not None and not callable(run.observe))):
            raise ValueError("HostRun needs a bounded prompt and an independent oracle")
        servers = _servers(run.mcp_servers)
        if not isinstance(run.allowed_tools, dict) or set(run.allowed_tools) - set(servers):
            raise ValueError("tool grants must name configured MCP servers")
        for names in run.allowed_tools.values():
            if (not isinstance(names, list) or not names
                    or any(not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9_.:/-]{1,128}", name)
                           for name in names) or len(names) != len(set(names))):
                raise ValueError("tool grants must be explicit lists of tool names")
        root = context.isolation.root
        workspace = root / "work"
        workspace.mkdir()
        codex_home, claude_home = root / "codex", root / "claude"
        codex_home.mkdir()
        claude_home.mkdir()
        mcp_path = root / "mcp.json"
        claude_servers = {name: ({"type": "http", **config} if "url" in config else config)
                          for name, config in servers.items()}
        _private_file(mcp_path, json.dumps({"mcpServers": claude_servers}))
        prompt_path = root / "prompt.txt"
        _private_file(prompt_path, run.prompt)
        env = context.isolation.env(self.env)
        env.update(CODEX_HOME=str(codex_home), CLAUDE_CONFIG_DIR=str(claude_home),
                   WAM_HOST_MCP_CONFIG=str(mcp_path))
        args = list(self.command)
        if self.host == "codex":
            config = {
                "model": self.model, "approval_policy": "never", "sandbox_mode": "read-only",
                "web_search": "disabled", "features": {"shell_tool": False, "multi_agent": False},
                "mcp_servers": {},
            }
            if self.effort:
                config["model_reasoning_effort"] = self.effort
            if self.base_url:
                config.update(model_provider="wam-fixture", model_providers={"wam-fixture": {
                    "name": "WAM explicit backend", "base_url": self.base_url,
                    "wire_api": "responses", "requires_openai_auth": False,
                }})
            for name, entry in servers.items():
                config["mcp_servers"][name] = {
                    ("http_headers" if key == "headers" else key): value for key, value in entry.items()
                }
                allowed = run.allowed_tools.get(name, [])
                config["mcp_servers"][name].update(
                    required=True, default_tools_approval_mode="prompt", enabled_tools=allowed,
                    tools={tool: {"approval_mode": "approve"} for tool in allowed},
                )
            _private_file(codex_home / "config.toml", "\n".join(
                f"{_toml(key)} = {_toml(value)}" for key, value in config.items()
            ) + "\n")
            args += ["exec", "--json", "--ephemeral", "--skip-git-repo-check", "--ignore-rules",
                     "--strict-config", "--color", "never", "-"]
        elif self.host == "claude-code":
            args += ["--print", "--output-format", "stream-json", "--verbose", "--bare", "--restricted",
                     "--tools", "", "--no-session-persistence", "--strict-mcp-config",
                     "--mcp-config", str(mcp_path), "--model", self.model]
            allowed = [f"mcp__{server}__{tool}" for server, tools in run.allowed_tools.items() for tool in tools]
            if allowed:
                args += ["--allowedTools", ",".join(allowed)]
            if self.effort:
                args += ["--effort", self.effort]
            if self.max_budget_usd is not None:
                args += ["--max-budget-usd", str(self.max_budget_usd)]
            if self.base_url:
                env["ANTHROPIC_BASE_URL"] = self.base_url
            env.update(CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1", DISABLE_AUTOUPDATER="1")
        context.evidence.meta["native_host"] = {
            "host": self.host, "model": self.model, "effort": self.effort,
            "prompt_sha256": hashlib.sha256(run.prompt.encode()).hexdigest(),
            "mcp_config_sha256": hashlib.sha256(json.dumps(
                {"servers": servers, "allowed_tools": run.allowed_tools}, sort_keys=True,
            ).encode()).hexdigest(),
            "retains_raw": self.retain_raw, "controlled_ready": False,
        }
        return args, env, workspace, prompt_path

    def __call__(self, context, spec):
        with self.prepare(context, spec) as run:
            args, env, workspace, prompt_path = self._configure(context, run)
            allowed = [tool if self.host == "generic-mcp" else f"mcp__{server}__{tool}"
                       for server, tools in run.allowed_tools.items() for tool in tools]
            trace = HostTrace(self.host, context, run.observe, allowed_tools=allowed)
            proc = None
            raw = {}
            digests = {name: hashlib.sha256() for name in ("stdout", "stderr")}
            counts = {"stdout": 0, "stderr": 0}
            stderr_tail = bytearray()
            try:
                if self.retain_raw:
                    for name in digests:
                        path = context.evidence.path / "sinks" / f"host-{name}.raw"
                        raw[name] = _private_stream(path)
                with prompt_path.open("rb") as prompt:
                    proc = ManagedProcess(args, env=env, cwd=workspace, stdin=prompt,
                                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                context.evidence.meta["native_host"]["pid"] = proc.pid
                deadline = time.monotonic() + self.timeout
                buffer = bytearray()
                with selectors.DefaultSelector() as selector:
                    selector.register(proc.popen.stdout, selectors.EVENT_READ, "stdout")
                    selector.register(proc.popen.stderr, selectors.EVENT_READ, "stderr")
                    while selector.get_map():
                        remaining = deadline - time.monotonic()
                        if remaining <= 0:
                            raise HostTraceError("host timeout")
                        for key, _ in selector.select(min(remaining, 0.1)):
                            chunk = os.read(key.fd, 65536)
                            if not chunk:
                                selector.unregister(key.fileobj)
                                continue
                            name = key.data
                            if sum(counts.values()) + len(chunk) > self.max_output_bytes:
                                raise HostTraceError("host output limit exceeded")
                            counts[name] += len(chunk)
                            digests[name].update(chunk)
                            if name in raw:
                                raw[name].write(chunk)
                            if name == "stderr":
                                stderr_tail.extend(chunk)
                                del stderr_tail[:-4096]
                                continue
                            buffer.extend(chunk)
                            while b"\n" in buffer:
                                line, _, rest = buffer.partition(b"\n")
                                buffer = bytearray(rest)
                                if len(line) > 1024 * 1024:
                                    raise HostTraceError("host line limit exceeded")
                                if line.strip():
                                    trace.feed(json.loads(line))
                                    if trace.abort_signal:
                                        raise _HostUnavailable(trace.abort_signal)
                            if len(buffer) > 1024 * 1024:
                                raise HostTraceError("host line limit exceeded")
                    if buffer.strip():
                        trace.feed(json.loads(buffer))
                        if trace.abort_signal:
                            raise _HostUnavailable(trace.abort_signal)
                exit_code = proc.popen.wait(timeout=max(0.01, deadline - time.monotonic()))
                context.evidence.meta["native_host"]["exit_code"] = exit_code
                if trace.terminal is None and exit_code != 0:
                    # Startup failures may occur before a CLI writes any JSONL.
                    text = stderr_tail.decode("utf-8", errors="replace").lower()
                    signal = error_signal({"message": text}, allow_message=True)
                    if signal:
                        context.signal(signal)
                        trace.terminal = False
                host_completed = trace.finish()
                if host_completed and exit_code != 0:
                    context.signal("harness-error")
                completed = host_completed and exit_code == 0
                verdict = run.oracle() if completed else False
                if type(verdict) is not bool:
                    raise HostTraceError("independent oracle must return a bool")
                success = completed and verdict
                context.event("done" if success else "failed", reason="oracle-pass" if success else "host-or-oracle-failed")
                return success
            except _HostUnavailable as error:
                context.event("failed", reason=error.signal)
                return False
            except (OSError, ValueError, subprocess.SubprocessError) as error:
                context.evidence.meta["native_host"]["adapter_error"] = type(error).__name__
                context.signal("harness-error")
                if not context.events or context.events[-1]["type"] not in {"done", "failed"}:
                    context.event("failed", reason="host-adapter-error")
                return False
            finally:
                if proc is not None:
                    proc.stop(term_wait=0.5, kill_wait=0.5)
                    context.evidence.meta["native_host"].setdefault("exit_code", proc.popen.returncode)
                    proc.popen.stdout.close()
                    proc.popen.stderr.close()
                for stream in raw.values():
                    stream.close()
                context.evidence.meta["native_host"]["streams"] = {
                    name: {"bytes": counts[name], "sha256": digests[name].hexdigest()} for name in digests
                }
