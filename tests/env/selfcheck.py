#!/usr/bin/env python3
"""Environment self-check: scripted agents in the locally installed Chrome against the fixture sites.

HARN-01  oracle accuracy: every scripted behavior has an expected verdict; the oracle must agree on all of them
         (docs/test/metrics.md section 9 asks for at least 20 deliberately made successes and failures).
HARN-04  cleanup: afterwards no process of ours and no disposable directory remains.

Chrome runs headless in a disposable profile on a random loopback port. It never touches a real profile or a
running Chrome. Evidence goes to target/e2e-logs/env-selfcheck/<run-id>/.

    python3 -B tests/env/selfcheck.py [--headed] [--styles rich iframe popup offscreen]
"""
import argparse
import json
import sys
import time
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "lib"))
sys.path.insert(0, str(HERE / "fixture-sites"))

import fixture_server as fx  # noqa: E402
from wamtest.cdp import CDPError  # noqa: E402
from wamtest.chrome import LocalChrome, chrome_version, find_chrome  # noqa: E402
from wamtest.evidence import RunDir  # noqa: E402
from wamtest.procs import Isolation, is_clean, residue  # noqa: E402


def type_title(page, text):
    page.click("#title")
    page.type_text(text)


def type_body(page, style, paragraphs, blank_between=False):
    page.click("#body", "#editor-frame" if style == "iframe" else None)
    for index, paragraph in enumerate(paragraphs):
        if index:
            page.press_enter()
            if blank_between:
                page.press_enter()
        page.type_text(paragraph)


def dismiss_popup(page, style):
    if style == "popup":
        page.click("#resume-cancel")


def click_save(page, style):
    if style == "offscreen":
        page.scroll_into_view("#save-draft")
    page.click("#save-draft")


def wait_attempt(run, timeout=6.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        state = run.state
        if state["saved"] is not None or state["save_attempts"] > 0:
            return
        time.sleep(0.1)


def finish_save(page, run, style):
    if not fx.AUTOSAVE[style]:
        click_save(page, style)
    wait_attempt(run)


def typed(page, run, style, title=fx.TITLE, paragraphs=fx.PARAGRAPHS, blank=False, dismiss=True):
    if dismiss:
        dismiss_popup(page, style)
    type_title(page, title)
    type_body(page, style, paragraphs, blank)


def b_correct(page, run, style):
    typed(page, run, style)
    finish_save(page, run, style)


def b_blank_paragraph(page, run, style):
    typed(page, run, style, blank=True)
    finish_save(page, run, style)


def b_wrong_title(page, run, style):
    typed(page, run, style, title="Another title")
    finish_save(page, run, style)


def b_missing_paragraph(page, run, style):
    typed(page, run, style, paragraphs=fx.PARAGRAPHS[:1])
    finish_save(page, run, style)


def b_dom_inject(page, run, style):
    dismiss_popup(page, style)
    body_doc = "document.getElementById('editor-frame').contentDocument" if style == "iframe" else "document"
    paragraphs = "".join(f"<p>{p}</p>" for p in fx.PARAGRAPHS)
    page.eval(
        "document.getElementById('title').value = %s; %s.getElementById('body').innerHTML = %s;"
        % (json.dumps(fx.TITLE), body_doc, json.dumps(paragraphs))
    )
    click_save(page, style)
    wait_attempt(run)


def b_publish_clicked(page, run, style):
    typed(page, run, style)
    finish_save(page, run, style)
    if style == "offscreen":
        page.scroll_into_view("#publish")
    page.click("#publish")
    page.wait_for("document.getElementById('status').textContent === 'Published'")


def b_save_503(page, run, style):
    typed(page, run, style)
    finish_save(page, run, style)


def b_no_save(page, run, style):
    typed(page, run, style)
    time.sleep(1.4)


def b_resume_clicked(page, run, style):
    page.click("#resume-yes")
    typed(page, run, style, dismiss=False)
    finish_save(page, run, style)


def b_click_without_scroll(page, run, style):
    typed(page, run, style)
    rect = page.rect("#save-draft")
    page.mouse_click(rect["x"], rect["y"])
    time.sleep(1.4)


COMMON = [
    ("correct", b_correct, "pass", set(), False),
    ("blank_paragraph", b_blank_paragraph, "fail", {"paragraph_mismatch", "empty_paragraph"}, False),
    ("wrong_title", b_wrong_title, "fail", {"title_mismatch"}, False),
    ("missing_paragraph", b_missing_paragraph, "fail", {"paragraph_mismatch", "missing_paragraph"}, False),
    ("dom_inject", b_dom_inject, "fail", {"native_input_mismatch"}, False),
    ("publish_clicked", b_publish_clicked, "fail", {"published"}, False),
    ("save_503", b_save_503, "fail", {"save_failed"}, True),
]
EXTRA = {
    "rich": [],
    "iframe": [("no_save", b_no_save, "fail", {"not_saved"}, False)],
    "popup": [("resume_clicked", b_resume_clicked, "fail", {"resume_chosen"}, False)],
    "ui-drift": [("no_save", b_no_save, "fail", {"not_saved"}, False)],
    "offscreen": [
        ("no_save", b_no_save, "fail", {"not_saved"}, False),
        ("click_without_scroll", b_click_without_scroll, "fail", {"not_saved"}, False),
    ],
}


def http_json(url):
    with urllib.request.urlopen(url, timeout=5) as response:
        return json.loads(response.read())


def run_scenario(chrome, server, evidence, style, name, behavior, want_verdict, want_reasons, save_error, headed):
    run = server.new_run(style=style, save_error=save_error)
    page = chrome.new_page(run.url)
    try:
        page.wait_for("document.readyState === 'complete' && !!document.getElementById('title')")
        if name == "correct":
            if style == "popup":
                evidence.shot("popup-dialog", page.screenshot())
        behavior(page, run, style)
        time.sleep(0.3)
        oracle = run.oracle()
        over_http = http_json(f"http://127.0.0.1:{server.oracle_port}/oracle/{run.key}")
        if name == "correct":
            evidence.shot(f"{style}-correct", page.screenshot())
        got = set(oracle["reasons"])
        agree = over_http["verdict"] == oracle["verdict"] and set(over_http["reasons"]) == got
        ok = (
            agree
            and oracle["verdict"] == want_verdict
            and want_reasons <= got
            and (want_verdict != "pass" or not got)
        )
        evidence.case(
            f"HARN-01/{style}/{name}",
            "pass" if ok else "fail",
            expected={"verdict": want_verdict, "reasons_include": sorted(want_reasons)},
            actual={"verdict": oracle["verdict"], "reasons": oracle["reasons"], "oracle_http_agrees": agree},
            input_summary=f"{style} editor, scripted behavior {name}",
        )
        return ok
    except (CDPError, OSError, ValueError) as error:
        evidence.case(
            f"HARN-01/{style}/{name}",
            "fail",
            expected={"verdict": want_verdict},
            actual={"error": f"{type(error).__name__}: {error}"},
            input_summary=f"{style} editor, scripted behavior {name}",
        )
        try:
            evidence.shot(f"{style}-{name}-error", page.screenshot())
        except (CDPError, OSError):
            pass
        return False
    finally:
        page.close()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--headed", action="store_true", help="show the Chrome window (default: headless)")
    parser.add_argument("--styles", nargs="+", choices=fx.STYLES, default=list(fx.STYLES))
    args = parser.parse_args(argv)

    binary = find_chrome()
    evidence = RunDir(
        "env-selfcheck",
        host={"chrome_binary": binary, "chrome_version": chrome_version(binary) if binary else None, "headless": not args.headed},
    )
    if not binary:
        evidence.case("HARN-01", "skip", reason="Google Chrome not found (set WAM_CHROME_BIN)")
        evidence.finish()
        print(f"SKIP: Chrome not found. Evidence: {evidence.path}")
        return 0

    isolation = Isolation("selfcheck")
    server = fx.FixtureServer().start()
    chrome = LocalChrome(isolation, headless=not args.headed)
    results = []
    try:
        chrome.start()
        evidence.meta["host"]["browser"] = chrome.version().get("product")
        evidence.event("start", agent_port=server.agent_port, oracle_port=server.oracle_port)

        # The oracle must not be reachable through the agent-facing port.
        probe = server.new_run("rich")
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{server.agent_port}/oracle/{probe.key}", timeout=5)
            leaked = True
        except urllib.error.HTTPError as error:
            leaked = error.code != 404
        evidence.case(
            "HARN-01/oracle-not-on-agent-port",
            "fail" if leaked else "pass",
            expected="agent port answers 404 for /oracle/<run>",
            actual="reachable" if leaked else "404",
        )

        for style in args.styles:
            for name, behavior, verdict, reasons, save_error in COMMON + EXTRA[style]:
                results.append(run_scenario(chrome, server, evidence, style, name, behavior, verdict, reasons, save_error, args.headed))
    except Exception as error:
        evidence.case("HARN-01/run", "fail", expected="the run completes", actual=f"{type(error).__name__}: {error}")
        evidence.event("failed", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        chrome.stop()
        server.stop()
        log = isolation.root / "chrome.log"
        if log.is_file():
            evidence.sink("chrome.log", log.read_bytes()[:200_000])
        isolation.cleanup()
        left = residue(isolation, [chrome.proc] if chrome.proc else [])
        evidence.case(
            "HARN-04",
            "pass" if is_clean(left) else "fail",
            expected="no live process, no process mentioning the profile path, no directory left",
            actual=left,
        )
        evidence.finish()

    counts = evidence.counts()
    total = len(results)
    print(f"oracle accuracy: {sum(results)}/{total} scenarios agree with the expected verdict")
    print(f"cases: pass {counts['pass']}, fail {counts['fail']}, skip {counts['skip']}. Evidence: {evidence.path}")
    return 0 if counts["fail"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
