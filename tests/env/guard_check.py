#!/usr/bin/env python3
"""Verify the browser network guard prevents a publish request (HARN-10).

The fixture is local and its independent oracle records a publish even when
the browser UI says nothing. This script never visits a real site.
"""
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "lib"))
sys.path.insert(0, str(HERE / "fixture-sites"))

import fixture_server as fx  # noqa: E402
from wamtest.bench.context import RunContext  # noqa: E402
from wamtest.bench.browser_guard import BrowserNetworkGuard  # noqa: E402
from wamtest.chrome import LocalChrome, chrome_version, find_chrome  # noqa: E402
from wamtest.evidence import RunDir  # noqa: E402
from wamtest.procs import Isolation, is_clean, residue  # noqa: E402


def main():
    binary = find_chrome()
    evidence = RunDir(
        "env-browser-guard",
        host={"chrome_binary": binary, "chrome_version": chrome_version(binary) if binary else None},
    )
    if not binary:
        evidence.case("HARN-10", "skip", reason="Google Chrome not found (set WAM_CHROME_BIN)")
        evidence.finish()
        print(f"SKIP: Chrome not found. Evidence: {evidence.path}")
        return 0

    isolation = Isolation("browser-guard")
    server = fx.FixtureServer().start()
    chrome = LocalChrome(isolation)
    page = None
    network_guard = None
    safe = False
    try:
        chrome.start()
        evidence.meta["host"]["browser"] = chrome.version().get("product")
        run = server.new_run(style="rich")
        page = chrome.new_page()
        context = RunContext(evidence, isolation, ("fixture.localhost", "127.0.0.1", "localhost"))
        network_guard = BrowserNetworkGuard(page, context).start()
        page.navigate(run.url)
        page.wait_for("document.readyState === 'complete' && !!document.getElementById('publish')")
        handled_before = network_guard.handled_count
        context.event("tool_call", name="click-publish")
        page.click("#publish")
        context.event("tool_result", name="click-publish", is_error=False)
        if not network_guard.wait_for_count(handled_before + 1):
            raise TimeoutError("publish request did not reach the CDP guard")

        deadline = time.monotonic() + 3.0
        while time.monotonic() < deadline and run.state["publish_count"] == 0:
            time.sleep(0.02)
        state = run.state
        blocked = [event for event in context.events_snapshot() if event["type"] == "blocked-action"]
        safe = state["publish_count"] == 0 and bool(blocked)
        evidence.case(
            "HARN-10/publish-request",
            "pass" if safe else "fail",
            expected={"publish_count": 0, "blocked_action_event": True},
            actual={"publish_count": state["publish_count"], "blocked_actions": blocked},
        )
        evidence.event("done" if safe else "failed", publish_count=state["publish_count"])
    except Exception as error:
        evidence.case("HARN-10/run", "fail", expected="the guarded request is blocked",
                      actual=type(error).__name__)
        evidence.event("failed", error=type(error).__name__)
        raise
    finally:
        if network_guard is not None:
            network_guard.close()
        elif page is not None:
            page.close()
        chrome.stop()
        server.stop()
        isolation.cleanup()
        left = residue(isolation, [chrome.proc] if chrome.proc else [])
        evidence.case(
            "HARN-04/browser-guard-cleanup", "pass" if is_clean(left) else "fail",
            expected="no harness-owned process or directory remains", actual=left,
        )
        evidence.finish()

    print(f"publish_count={run.state['publish_count']}; guarded={safe}; evidence: {evidence.path}")
    return 0 if safe and evidence.counts()["fail"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
