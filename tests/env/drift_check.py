#!/usr/bin/env python3
"""Scripted browser QA of both UI versions; this is not an agent performance result."""
import json
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "lib"))
sys.path.insert(0, str(HERE / "fixture-sites"))

import fixture_server as fx
from selfcheck import typed, wait_attempt
from wamtest.cdp import CDPError
from wamtest.chrome import LocalChrome
from wamtest.evidence import RunDir
from wamtest.procs import is_clean, residue


CASES = (
    ("v1-save", "v1", "current", False, "pass", set()),
    ("v2-current", "v2", "current", False, "pass", set()),
    ("v2-stale-only", "v2", "stale-only", False, "fail", {"not_saved"}),
    ("v2-recover", "v2", "recover", False, "pass", set()),
    ("v2-save-error", "v2", "current", True, "fail", {"save_failed"}),
    ("v2-scripted-input", "v2", "dom", False, "fail", {"native_input_mismatch"}),
)


def check_case(browser, server, evidence, case):
    name, version, behavior, save_error, want, reasons = case
    run = server.new_run("ui-drift", ui_version=version, save_error=save_error)
    page = browser.new_page(run.url)
    try:
        page.wait_for("document.readyState === 'complete' && !!document.getElementById('body')")
        if behavior == "dom":
            page.eval(
                "document.getElementById('title').value = %s; document.getElementById('body').innerHTML = %s"
                % (json.dumps(fx.TITLE), json.dumps("".join(f"<p>{p}</p>" for p in fx.PARAGRAPHS)))
            )
        else:
            typed(page, run, "ui-drift")
        if behavior in ("stale-only", "recover"):
            missing = False
            try:
                page.click("#save-draft")
            except CDPError:
                missing = True
            if not missing:
                raise ValueError("v2 unexpectedly accepted the obsolete Save draft anchor")
        if behavior != "stale-only":
            page.click("#store-draft" if version == "v2" else "#save-draft")
            wait_attempt(run)
        time.sleep(0.2)
        oracle = run.oracle()
        ok = oracle["verdict"] == want and reasons <= set(oracle["reasons"])
        if want == "pass":
            ok = ok and not oracle["reasons"] and oracle["state"]["publish_count"] == 0
        if name in ("v1-save", "v2-recover"):
            evidence.shot(name, page.screenshot())
        evidence.case(
            f"UI-DRIFT/{name}", "pass" if ok else "fail",
            expected={"verdict": want, "reasons": sorted(reasons)},
            actual=oracle, input_summary=f"{version}: {behavior}",
        )
        return ok
    except (CDPError, OSError, ValueError) as error:
        evidence.case(f"UI-DRIFT/{name}", "fail", expected=want,
                      actual={"error": f"{type(error).__name__}: {error}"})
        return False
    finally:
        page.close()


def main():
    evidence = RunDir("ui-drift", host={"adapter": "scripted-headless-chrome"})
    browser = None
    ok = False
    try:
        with fx.FixtureServer() as server:
            browser = LocalChrome()
            with browser:
                evidence.meta["host"]["version"] = browser.version().get("product")
                results = [check_case(browser, server, evidence, case) for case in CASES]
                ok = all(results)
        found = residue(browser.isolation)
        clean = is_clean(found)
        evidence.case("UI-DRIFT/cleanup", "pass" if clean else "fail",
                      expected="no owned process or profile remains", actual=found)
        ok = ok and clean
    except Exception as error:
        evidence.case("UI-DRIFT/harness", "fail", expected="browser checks complete",
                      actual={"error": f"{type(error).__name__}: {error}"})
    finally:
        evidence.finish()
    print(json.dumps({"ok": ok, "evidence": str(evidence.path)}))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
