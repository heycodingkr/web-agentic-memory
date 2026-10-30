#!/usr/bin/env python3
"""Measure repeatability of one identical editor task in disposable Chrome (HARN-05).

This is a deterministic browser fixture check, not a model or production-site
benchmark. It measures from the first guarded browser action to the independent
fixture save oracle. Evidence is written under target/e2e-logs/env-repeatability.

    python3 -B tests/env/repeatability_check.py [--repeats 10]
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
from wamtest.bench.context import RunContext  # noqa: E402
from wamtest.bench.repeatability import assess  # noqa: E402
from wamtest.bench.trace import analyze  # noqa: E402
from wamtest.chrome import LocalChrome, chrome_version, find_chrome  # noqa: E402
from wamtest.evidence import RunDir  # noqa: E402
from wamtest.procs import Isolation, is_clean, residue  # noqa: E402


def oracle_http(server, run):
    url = f"http://127.0.0.1:{server.oracle_port}/oracle/{run.key}"
    with urllib.request.urlopen(url, timeout=5) as response:
        return json.loads(response.read())


def one_repetition(chrome, server, evidence, isolation, index):
    run = server.new_run(style="iframe")
    page = chrome.new_page(run.url)
    context = RunContext(evidence, isolation, ("fixture.localhost", "127.0.0.1", "localhost"))
    try:
        page.wait_for("document.readyState === 'complete' && !!document.getElementById('title')")
        context.perform(
            "edit-title", lambda: (page.click("#title"), page.type_text(fx.TITLE)),
            url=run.url, label="Edit draft title",
        )
        context.perform(
            "edit-body",
            lambda: (
                page.click("#body", "#editor-frame"),
                page.type_text(fx.PARAGRAPHS[0]),
                page.press_enter(),
                page.type_text(fx.PARAGRAPHS[1]),
            ),
            url=run.url, label="Edit draft body",
        )
        context.perform("save-draft", lambda: page.click("#save-draft"), url=run.url, label="Save draft")

        deadline = time.monotonic() + 6.0
        while time.monotonic() < deadline and run.state["saved"] is None and run.state["save_attempts"] == 0:
            time.sleep(0.02)
        local = run.oracle()
        remote = oracle_http(server, run)
        agrees = local["verdict"] == remote["verdict"] and local["reasons"] == remote["reasons"]
        passed = agrees and local["verdict"] == "pass"
        if local["state"]["saved"] is not None:
            context.event("submit", oracle="fixture-save")
        context.event("done" if passed else "failed", reason=None if passed else "oracle-mismatch")
        metrics = analyze(context.events)
        evidence.case(
            f"HARN-05/repetition-{index:02d}",
            "pass" if passed else "fail",
            expected={"verdict": "pass", "same_input": True},
            actual={"verdict": local["verdict"], "oracle_agrees": agrees, "tts": metrics.get("tts")},
            input_summary="same iframe-editor title/body/save sequence",
        )
        clock_ok = metrics["clock_error_ms"] is not None and metrics["clock_error_ms"] <= 100
        evidence.case(
            f"HARN-03/repetition-{index:02d}",
            "pass" if clock_ok else "fail",
            expected={"max_elapsed_clock_divergence_ms": "<=100"},
            actual={"clock_error_ms": metrics["clock_error_ms"]},
        )
        return {"tts": metrics.get("tts") if passed else None, "clock_error_ms": metrics["clock_error_ms"]}
    finally:
        page.close()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repeats", type=int, default=10)
    args = parser.parse_args(argv)
    if args.repeats < 5 or args.repeats > 50:
        parser.error("--repeats must be between 5 and 50")

    binary = find_chrome()
    evidence = RunDir(
        "env-repeatability",
        host={"chrome_binary": binary, "chrome_version": chrome_version(binary) if binary else None},
        extra_meta={"task": "same iframe editor input", "repeats": args.repeats, "cv_limit": 0.15},
    )
    if not binary:
        evidence.case("HARN-05", "skip", reason="Google Chrome not found (set WAM_CHROME_BIN)")
        evidence.finish()
        print(f"SKIP: Chrome not found. Evidence: {evidence.path}")
        return 0

    isolation = Isolation("repeatability")
    server = fx.FixtureServer().start()
    chrome = LocalChrome(isolation)
    samples = []
    try:
        chrome.start()
        evidence.meta["host"]["browser"] = chrome.version().get("product")
        evidence.event("start", agent_port=server.agent_port, oracle_port=server.oracle_port)
        for index in range(1, args.repeats + 1):
            samples.append(one_repetition(chrome, server, evidence, isolation, index))
        tts = [sample["tts"] for sample in samples]
        clock_errors = [sample["clock_error_ms"] for sample in samples]
        result = assess(tts)
        max_clock_error = max((error for error in clock_errors if error is not None), default=None)
        clock_ok = (len(clock_errors) == args.repeats and max_clock_error is not None
                    and all(error is not None and error <= 100 for error in clock_errors))
        evidence.sink("repeatability.json", json.dumps(
            {"repeatability": result, "clock_errors_ms": clock_errors}, allow_nan=False, indent=2,
        ))
        evidence.case(
            "HARN-03/clock", "pass" if clock_ok else "fail",
            expected={"all_runs_max_elapsed_clock_divergence_ms": "<=100"},
            actual={"max_clock_error_ms": max_clock_error,
                    "runs": len(clock_errors)},
        )
        evidence.case(
            "HARN-05/cv",
            "pass" if result["reproducible"] else "fail",
            expected={"n": args.repeats, "cv_at_most": 0.15},
            actual=result,
        )
        evidence.event("done" if result["reproducible"] and clock_ok else "failed",
                       cv=result["cv"], max_clock_error_ms=max_clock_error, n=result["n"])
    except Exception as error:
        evidence.case("HARN-05/run", "fail", expected="all repetitions complete", actual=type(error).__name__)
        evidence.event("failed", error=type(error).__name__)
        raise
    finally:
        chrome.stop()
        server.stop()
        isolation.cleanup()
        left = residue(isolation, [chrome.proc] if chrome.proc else [])
        evidence.case(
            "HARN-04/repeatability-cleanup",
            "pass" if is_clean(left) else "fail",
            expected="no harness-owned process or directory remains",
            actual=left,
        )
        evidence.finish()

    tts = [sample["tts"] for sample in samples]
    result = assess(tts)
    clock_errors = [sample["clock_error_ms"] for sample in samples]
    max_clock_error = max((error for error in clock_errors if error is not None), default=None)
    clock_ok = (len(clock_errors) == args.repeats and max_clock_error is not None
                and all(error is not None and error <= 100 for error in clock_errors))
    print(f"TTS seconds: {tts}; CV: {result['cv']}; max clock error: {max_clock_error} ms; evidence: {evidence.path}")
    return 0 if result["reproducible"] and clock_ok and evidence.counts()["fail"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
