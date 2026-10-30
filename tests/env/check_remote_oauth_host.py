#!/usr/bin/env python3
"""Exercise installed Codex OAuth against the loopback smoke fixture, without model calls."""
import argparse
import http.cookiejar
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

class LocalRedirects(urllib.request.HTTPRedirectHandler):
    def __init__(self, origin):
        self.origin = origin
    def redirect_request(self, req, fp, code, msg, headers, url):
        parsed = urllib.parse.urlsplit(url)
        if f"{parsed.scheme}://{parsed.netloc}" != self.origin:
            raise RuntimeError("fixture redirect left the configured loopback origin")
        return super().redirect_request(req, fp, code, msg, headers, url)

def find_access(value):
    if isinstance(value, dict):
        if isinstance(value.get("access_token"), str):
            return value["access_token"]
        for child in value.values():
            token = find_access(child)
            if token:
                return token
    if isinstance(value, list):
        for child in value:
            token = find_access(child)
            if token:
                return token
    return None

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--origin", default="http://127.0.0.1:32055")
    parser.add_argument("--explicit-resource", action="store_true")
    args = parser.parse_args()
    parsed = urllib.parse.urlsplit(args.origin)
    if parsed.scheme != "http" or parsed.hostname not in {"localhost", "127.0.0.1", "::1"} or parsed.path:
        raise RuntimeError("only an explicit loopback fixture origin is allowed")
    origin = args.origin.rstrip("/")
    executable = shutil.which("codex")
    if not executable:
        raise RuntimeError("Codex CLI is not installed")
    cookies = http.cookiejar.CookieJar()
    opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cookies), LocalRedirects(origin))
    host_opener = urllib.request.build_opener(LocalRedirects(origin))
    def request(path, payload=None, headers=None, *, host=False):
        target = path if path.startswith(origin + "/") else origin + path
        if not target.startswith(origin + "/"):
            raise RuntimeError("request left fixture origin")
        data = json.dumps(payload).encode() if payload is not None else None
        req = urllib.request.Request(target, data=data, headers=headers or {})
        if data is not None:
            req.add_header("content-type", "application/json")
        try:
            with (host_opener if host else opener).open(req, timeout=10) as response:
                body = response.read()
                return response.url, json.loads(body) if response.headers.get_content_type() == "application/json" else None
        except urllib.error.HTTPError as error:
            try:
                error_code = json.loads(error.read()).get("error", "unknown")
            except (ValueError, AttributeError):
                error_code = "unknown"
            if error_code not in {"invalid_request", "invalid_client", "invalid_client_metadata", "invalid_scope",
                                  "invalid_redirect_uri", "invalid_grant", "rate_limited"}:
                error_code = "unknown"
            values = urllib.parse.parse_qs(urllib.parse.urlsplit(target).query)
            diagnostic = {"status": error.code, "error": error_code,
                "present": [key for key in ("client_id", "redirect_uri", "resource", "scope", "state",
                    "code_challenge", "code_challenge_method", "response_type") if key in values],
                "resource_matches": values.get("resource") == [origin + "/mcp"],
                "resource_count": len(values.get("resource", [])),
                "resource_values_match": [value == origin + "/mcp" for value in values.get("resource", [])],
                "s256": values.get("code_challenge_method") == ["S256"],
                "challenge_length": len(values.get("code_challenge", [""])[0])}
            raise RuntimeError(json.dumps(diagnostic)) from None

    # The fixture provider redirects directly back to this same loopback origin.
    request("/v1/auth/github/start")
    _, session = request("/v1/auth/session")
    resource = origin + "/mcp"
    with tempfile.TemporaryDirectory(prefix="wam-codex-oauth-") as root:
        root = Path(root)
        codex_home = root / "codex"
        codex_home.mkdir()
        (codex_home / "config.toml").write_text(
            'mcp_oauth_credentials_store = "file"\n'
            '[mcp_servers.wam-oauth-fixture]\n'
            f'url = "{resource}"\n'
            + (f'oauth_resource = "{resource}"\n' if args.explicit_resource else '')
        )
        env = dict(os.environ, HOME=str(root), CODEX_HOME=str(codex_home),
                   XDG_CONFIG_HOME=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"))
        env["NO_COLOR"] = "1"
        for key in ("OPENAI_API_KEY", "CODEX_API_KEY", "ANTHROPIC_API_KEY", "WAM_TOKEN", "RUST_LOG", "FORCE_COLOR", "CLICOLOR_FORCE"):
            env.pop(key, None)
        process = subprocess.Popen(
            [executable, "mcp", "login", "wam-oauth-fixture", "--no-browser",
             "--oauth-client-registration", "dcr", "--scopes", "read:memories,read:dashboard"],
            cwd=root, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, text=True, bufsize=1)
        try:
            selector = selectors.DefaultSelector()
            selector.register(process.stdout, selectors.EVENT_READ)
            deadline = time.monotonic() + 60
            authorization_url = None
            captured = ''
            os.set_blocking(process.stdout.fileno(), False)
            while time.monotonic() < deadline and process.poll() is None:
                if not selector.select(timeout=1):
                    continue
                chunk = os.read(process.stdout.fileno(), 4096)
                if not chunk:
                    break
                captured = (captured + chunk.decode('utf-8', errors='replace'))[-32768:]
                plain = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", captured)
                match = re.search(re.escape(origin) + r"/oauth/authorize\?[^\s]+(?=\s)", plain)
                if match:
                    authorization_url = match[0].rstrip("'\"")
                    break
            selector.close()
            if not authorization_url:
                raise RuntimeError('Codex did not emit an authorization URL (raw output suppressed)')
            os.set_blocking(process.stdout.fileno(), True)
            location, _ = request(authorization_url)
            request_id = urllib.parse.parse_qs(urllib.parse.urlsplit(location).query)["request"][0]
            request("/v1/oauth/requests/" + request_id)
            _, decision = request("/v1/oauth/requests/" + request_id + "/decision",
                                  {"approve": True}, {"x-csrf-token": session["csrf_token"]})
            callback = decision["redirect_uri"]
            process.stdin.write(callback + "\n")
            process.stdin.flush()
            _, _ = process.communicate(timeout=30)
            if process.returncode:
                raise RuntimeError("Codex rejected the OAuth callback (raw output suppressed)")
            access = None
            for path in codex_home.glob("*.json"):
                try:
                    access = find_access(json.loads(path.read_text()))
                except (ValueError, OSError):
                    continue
                if access:
                    if os.name != "nt" and path.stat().st_mode & 0o077:
                        raise RuntimeError("host credential file permissions are too broad")
                    break
            if not access:
                raise RuntimeError("Codex did not persist an OAuth access token in the isolated file store")
            _, result = request("/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": "web_memory_my_rank", "arguments": {}}},
                {"authorization": "Bearer " + access, "accept": "application/json, text/event-stream",
                 "mcp-protocol-version": "2025-11-25"}, host=True)
            if result["result"]["isError"]:
                raise RuntimeError("server did not accept the host-issued credential")
            _, usage = request("/mcp", {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": "web_memory_my_usage", "arguments": {}}},
                {"authorization": "Bearer " + access, "accept": "application/json, text/event-stream",
                 "mcp-protocol-version": "2025-11-25"}, host=True)
            if usage["result"]["isError"]:
                raise RuntimeError("owner usage failed with the isolated host credential")
            logout = subprocess.run([executable, "mcp", "logout", "wam-oauth-fixture"],
                                    cwd=root, env=env, capture_output=True, timeout=15)
            if logout.returncode:
                raise RuntimeError("isolated Codex logout failed")
            print(json.dumps({"host": "codex", "oauth_login": "passed",
                "private_read_with_host_credential": "passed", "owner_usage": "passed", "logout": "passed",
                "model_calls": 0, "real_accounts": 0}))
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)

if __name__ == "__main__":
    main()
