#!/usr/bin/env python3
"""Generate or verify the synthetic 500-value sentinel inventory."""

import argparse
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "lib"))

from wamtest.sentinels import DEFAULT_MANIFEST, build_manifest, render_manifest  # noqa: E402


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--check", action="store_true", help="fail if the checked-in inventory differs")
    args = parser.parse_args(argv)
    expected = render_manifest(build_manifest())
    if args.check:
        if not args.output.is_file() or args.output.read_text(encoding="utf-8") != expected:
            parser.error(f"generated sentinel inventory differs: {args.output}")
        print("verified 500 synthetic sentinel values")
        return 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(expected, encoding="utf-8")
    print(f"wrote 500 synthetic sentinel values to {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
