#!/usr/bin/env python3
"""Same environment self-check as selfcheck.py, against an isolated instance of the locally installed Arise.

The user's running Arise is never touched: this script records the pids of existing Arise processes before the
launch and requires them to be alive and unchanged afterwards (HARN-04 includes 'nothing else was affected').
A window of the isolated instance appears briefly on the desktop.

    python3 -B tests/env/arise_check.py [--styles rich iframe popup offscreen]
"""
import argparse
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "lib"))
sys.path.insert(0, str(HERE / "fixture-sites"))
sys.path.insert(0, str(HERE))

import fixture_server as fx  # noqa: E402
import selfcheck as sc  # noqa: E402
from wamtest.arise import LocalArise, arise_version, find_arise  # noqa: E402
from wamtest.evidence import RunDir  # noqa: E402
from wamtest.procs import Isolation, is_clean, residue  # noqa: E402


def users_arise_pids():
    out = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, text=True, check=True).stdout.splitlines()
    return sorted(int(l.split()[0]) for l in out if "/Applications/Arise.app/" in l)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--styles", nargs="+", choices=fx.STYLES, default=list(fx.STYLES))
    args = parser.parse_args(argv)

    app = find_arise()
    evidence = RunDir("env-selfcheck-arise", host={"app": str(app) if app else None, "version": arise_version(app) if app else None})
    if not app:
        evidence.case("HARN-01", "skip", reason="Arise.app not found (set WAM_ARISE_APP)")
        evidence.finish()
        print(f"SKIP: Arise not found. Evidence: {evidence.path}")
        return 0

    before = users_arise_pids()
    isolation = Isolation("arise-check")
    server = fx.FixtureServer().start()
    probe = server.new_run("rich")
    arise = LocalArise(probe.url, isolation=isolation)
    results = []
    try:
        arise.start()
        evidence.meta["host"]["browser"] = arise.version().get("product")
        evidence.event("start", agent_port=server.agent_port, oracle_port=server.oracle_port, users_arise_pids=before)

        try:
            urllib.request.urlopen(f"http://127.0.0.1:{server.agent_port}/oracle/{probe.key}", timeout=5)
            leaked = True
        except urllib.error.HTTPError as error:
            leaked = error.code != 404
        evidence.case("HARN-01/oracle-not-on-agent-port", "fail" if leaked else "pass",
                      expected="agent port answers 404 for /oracle/<run>", actual="reachable" if leaked else "404")

        for style in args.styles:
            for name, behavior, verdict, reasons, save_error in sc.COMMON + sc.EXTRA[style]:
                results.append(sc.run_scenario(arise, server, evidence, style, name, behavior, verdict, reasons, save_error, False))
    except Exception as error:
        evidence.case("HARN-01/run", "fail", expected="the run completes", actual=f"{type(error).__name__}: {error}")
        evidence.event("failed", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        arise.stop()
        server.stop()
        log = isolation.root / "arise.log"
        if log.is_file():
            evidence.sink("arise.log", log.read_bytes()[:200_000])
        isolation.cleanup()
        left = residue(isolation, [arise.proc] if arise.proc else [])
        after = users_arise_pids()
        evidence.case("HARN-04", "pass" if is_clean(left) else "fail",
                      expected="no live process, none mentioning the profile path, no directory left", actual=left)
        evidence.case("HARN-04/users-arise-untouched", "pass" if after == before else "fail",
                      expected=f"user's Arise pids unchanged: {before}", actual=after)
        evidence.finish()

    counts = evidence.counts()
    print(f"oracle accuracy: {sum(results)}/{len(results)} scenarios agree with the expected verdict")
    print(f"cases: pass {counts['pass']}, fail {counts['fail']}, skip {counts['skip']}. Evidence: {evidence.path}")
    return 0 if counts["fail"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
