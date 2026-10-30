#!/usr/bin/env python3
"""Check all eight collected synthetic privacy sinks, without echoing payloads."""
import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from wamtest.sink_inventory import inspect_inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inventory", type=Path)
    parser.add_argument("--json-out", type=Path, required=True)
    args = parser.parse_args()
    report = inspect_inventory(args.inventory)
    # The caller uses an owned evidence directory. Do not replace existing reports.
    try:
        with os.fdopen(os.open(args.json_out, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w", encoding="utf-8") as output:
            json.dump(report, output, indent=2)
            output.write("\n")
    except OSError:
        print("cannot create new private sink report", file=sys.stderr)
        return 2
    print(json.dumps({"passed": report["passed"], "sink_count": len(report["sinks"]),
                      "error_count": len(report["errors"]),
                      "hit_count": sum(len(sink["hits"]) for sink in report["sinks"])}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
