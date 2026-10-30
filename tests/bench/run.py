#!/usr/bin/env python3
"""Reprocess normalized benchmark traces; replay never earns a controlled performance claim."""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from wamtest.bench.replay import ReplayAdapter
from wamtest.bench.runner import run_plan


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--traces", type=Path, required=True, help="JSON object keyed by run_id")
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory")
    args = parser.parse_args(argv)
    try:
        plan = json.loads(args.plan.read_text(encoding="utf-8"))
        records = json.loads(args.traces.read_text(encoding="utf-8"))
        expected = {row["run_id"] for row in plan["runs"]}
        if set(records) != expected:
            raise ValueError("trace IDs must exactly match the plan")
        summary = run_plan(plan, ReplayAdapter(records), args.output,
                           metadata={"source": "normalized-trace-replay"}, controlled=False)
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    print(json.dumps({"evidence": str(args.output), **summary}, ensure_ascii=False))
    return 0 if summary["suite_valid"] else 1


if __name__ == "__main__":
    sys.exit(main())
