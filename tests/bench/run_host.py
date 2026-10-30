#!/usr/bin/env python3
"""Run an explicit native host against task factories and independent oracles."""
import argparse
import importlib
import json
import os
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from wamtest.bench.host import CliHostAdapter
from wamtest.bench.runner import run_plan


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--host", choices=["codex", "claude-code", "generic-mcp"], required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--effort")
    parser.add_argument("--factory", required=True, help="Python module:function yielding HostRun(context, spec)")
    parser.add_argument("--pass-env", action="append", default=[], metavar="NAME",
                        help="explicitly forward this environment variable; never pass its value on the command line")
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--host-arg", action="append", default=[])
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--max-output-bytes", type=int, default=8 * 1024 * 1024)
    parser.add_argument("--max-budget-usd", type=float)
    parser.add_argument("--retain-raw", action="store_true", help="retain private bounded native streams (0600)")
    args = parser.parse_args(argv)
    try:
        module, separator, name = args.factory.partition(":")
        if not separator or not module or not name or "." in name:
            raise ValueError("factory must have the form module:function")
        prepare = getattr(importlib.import_module(module), name)
        if not callable(prepare):
            raise ValueError("factory is not callable")
        if args.host_arg and args.executable is None:
            raise ValueError("host arguments require an explicit executable")
        command = [str(args.executable), *args.host_arg] if args.executable else None
        env = {}
        for key in args.pass_env:
            if key not in os.environ:
                raise ValueError("an explicitly requested environment variable is not set")
            env[key] = os.environ[key]
        adapter = CliHostAdapter(
            args.host, args.model, prepare, effort=args.effort, command=command, env=env,
            timeout=args.timeout, max_output_bytes=args.max_output_bytes,
            retain_raw=args.retain_raw, max_budget_usd=args.max_budget_usd,
        )
        plan = json.loads(args.plan.read_text(encoding="utf-8"))
        summary = run_plan(plan, adapter, args.output, controlled=False, metadata={
            "source": "native-host-jsonl", "host": args.host, "model": args.model, "effort": args.effort,
        })
    except (OSError, ValueError, KeyError, TypeError, AttributeError, ImportError) as error:
        # Import/JSON exceptions may contain user payloads. Report the class only.
        parser.error(f"host run configuration failed ({type(error).__name__})")
    print(json.dumps({"evidence": str(args.output), **summary}, ensure_ascii=False))
    return 0 if summary["suite_valid"] else 1


if __name__ == "__main__":
    sys.exit(main())
