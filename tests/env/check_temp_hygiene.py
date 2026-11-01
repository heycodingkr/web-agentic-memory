#!/usr/bin/env python3
"""Run a command in an owned temp folder and fail if it leaves anything behind.

    python3 tests/env/check_temp_hygiene.py -- node --test web/tests/landing.test.mjs
    python3 tests/env/check_temp_hygiene.py -- cargo test -p wam-server --test admin_auth --test email_auth
"""
import argparse
import os
import subprocess
import tempfile
from pathlib import Path


def check(command):
    with tempfile.TemporaryDirectory(prefix="wam-temp-check-") as root:
        temporary = Path(root) / "tmp"
        temporary.mkdir()
        env = {**os.environ, **{key: str(temporary) for key in ("TMPDIR", "TMP", "TEMP")}}
        result = subprocess.run(command, env=env, check=False)
        leftovers = sorted(path.name for path in temporary.iterdir())
        if leftovers:
            print(f"Temporary-file leak: {len(leftovers)} entries", flush=True)
            for name in leftovers[:20]:
                print(f"  {name}", flush=True)
        else:
            print("Temporary-file check: no leftovers", flush=True)
        return result.returncode or (1 if leftovers else 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("provide a command after --")
    return check(command)


if __name__ == "__main__":
    raise SystemExit(main())
