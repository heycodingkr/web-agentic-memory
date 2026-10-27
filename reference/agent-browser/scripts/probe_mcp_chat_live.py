#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""
probe_mcp_chat_live — 채팅 → **실제 CLI 모델** → MCP 브리지 → 앱 허브 → 진짜 MCP 서버, 끝에서 끝까지.

모의가 없다: 실 CLI(사용자 구독) + 실 앱 + 실 stdio 서버(파이썬 장난감 서버). 모델이 서버 도구를 **실제로 불렀는지**는
서버가 돌려주는 값(`mcp.json` 의 env 로만 정해지는 접두사 `[live] `)이 답에 나왔는지로 가린다 — 모델이 혼자 지어낼 수 없는 값이다.

  AGENT_BROWSER_SELFTEST_PROMPT/MODEL 로 앱이 시작하자마자 이 프롬프트를 채팅에 보낸다.
  mcp.json 은 앱이 뜨기 전에 PROBE_CONFIG_SEED 로 심는다(첫 `tools/list` 가 서버를 알아야 한다).

사용:
    SEED=$(mktemp -d) && mkdir -p $SEED/agent-browser && python3 scripts/probe_mcp_chat_live.py --seed $SEED
    PROBE_CONFIG_SEED=$SEED AGENT_BROWSER_SELFTEST_MODEL=claude:haiku \\
    AGENT_BROWSER_SELFTEST_PROMPT="$(python3 scripts/probe_mcp_chat_live.py --prompt)" \\
    PROBE_SCRIPT=scripts/probe_mcp_chat_live.py bash scripts/probe_arise_isolated.sh
"""

import json
import os
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOY = os.path.join(ROOT, "crates/mcp_client/tests/fixtures/toy_mcp_server.py")
TOKEN = "zebra-7731"
PROMPT = (
    "Use the external MCP tools (mcp_tool_search, then mcp_call_tool) to find the tool that echoes text and "
    f"call it with the text {TOKEN}. Reply with exactly the text the tool returned and nothing else."
)


def seed(dir_):
    cfg = os.path.join(dir_, "agent-browser", "mcp.json")
    os.makedirs(os.path.dirname(cfg), exist_ok=True)
    with open(cfg, "w") as f:
        json.dump({"mcpServers": {"toy": {"command": "python3", "args": [TOY], "env": {"TOY_PREFIX": "[live] "}}}}, f)


def transcripts(root):
    out = []
    for dirpath, _, files in os.walk(root):
        for name in files:
            try:
                text = open(os.path.join(dirpath, name), errors="ignore").read()
            except OSError:
                continue
            if PROMPT[:40] in text:
                out.append((name, text))
    return out


def main():
    if "--prompt" in sys.argv:
        print(PROMPT)
        return 0
    if "--seed" in sys.argv:
        seed(sys.argv[sys.argv.index("--seed") + 1])
        return 0
    sessions = os.path.join(os.environ["PROBE_CONFIG_DIR"], "agent-browser", "sessions")
    deadline = time.time() + float(os.environ.get('PROBE_WAIT_SECS', '240'))
    seen = []
    print(f"▶ 채팅 세션 대기: {sessions}")
    while time.time() < deadline:
        seen = transcripts(sessions)
        blob = "\n".join(t for _, t in seen)
        # 턴이 끝나면 assistant 의 최종 답이 세션에 남는다 — 도구 결과(`[live] zebra`)가 답에도 나올 때까지.
        if blob.count(f"[live] {TOKEN}") >= 2:
            break
        time.sleep(3)
    blob = "\n".join(t for _, t in seen)
    keep = "/tmp/probe-mcp-chat-session.json"
    if seen:
        open(keep, "w").write(seen[0][1])
        print(f"(증거 보존: {keep})")
    checks = [
        ("session recorded the prompt", bool(seen)),
        # 프롬프트에도 도구 이름이 들어 있으니 도구 카드 표식("🔧 이름")으로 **실제 호출**만 센다.
        ("model searched the hub (🔧 mcp_tool_search)", "🔧 mcp_tool_search" in blob),
        ("model called the hub tool (🔧 mcp_call_tool)", "🔧 mcp_call_tool" in blob),
        (f"the server's own prefix + token came back (`[live] {TOKEN}`)", f"[live] {TOKEN}" in blob),
        ("…and the model's answer repeats it (appears in tool result AND answer)", blob.count(f"[live] {TOKEN}") >= 2),
    ]
    for name, ok in checks:
        print(("  ✅ " if ok else "  ⛔ ") + name)
    ok = all(c for _, c in checks)
    if not ok:
        # 진단 — 어디까지 갔는지(세션 폴더·앱 로그 꼬리)를 남긴다. 격리 폴더는 곧 지워진다.
        cfg = os.environ["PROBE_CONFIG_DIR"]
        for dirpath, _, files in os.walk(cfg):
            print("  [diag]", dirpath.replace(cfg, "<config>"), files[:8])
        log = os.path.join(os.environ.get("TMPDIR", "/tmp"), "arise-launch.log")
        try:
            tail = open(log, errors="ignore").read()[-1500:]
            print("  [diag] app log tail:\n" + tail)
        except OSError:
            pass
    print(f"\n{'✅ 전부 통과' if ok else '⛔ 실패 있음'} ({sum(1 for _, c in checks if c)}/{len(checks)})")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
