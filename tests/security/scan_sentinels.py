#!/usr/bin/env python3
"""Search local test artifacts for synthetic sentinel leaks, without echoing values."""

import argparse
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "lib"))

from wamtest.sentinels import DEFAULT_MANIFEST, load_manifest, manifest_sha256, safe_metadata, scan_paths  # noqa: E402


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", type=Path, help="local files or directories to scan")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--exclude", action="append", default=[], type=Path, help="path to omit (repeatable)")
    parser.add_argument("--max-file-mib", type=int, default=64)
    parser.add_argument("--json-out", type=Path, help="write safe metadata only; never writes sentinel text")
    args = parser.parse_args(argv)
    if args.max_file_mib <= 0:
        parser.error("--max-file-mib must be positive")

    try:
        sentinels = load_manifest(args.manifest)
        manifest_hash = manifest_sha256(args.manifest)
        hits, errors = scan_paths(
            args.paths,
            entries=sentinels,
            manifest_path=args.manifest,
            exclude_paths=args.exclude,
            max_file_bytes=args.max_file_mib * 1024 * 1024,
        )
    except ValueError as exc:
        parser.error(str(exc))

    result = {
        "manifest_sha256": manifest_hash,
        "sentinel_count": len(sentinels),
        "scan_roots": [safe_metadata(str(path), sentinels) for path in args.paths],
        "status": "error" if errors else ("leaks_found" if hits else "clean"),
        "hit_count": len({(hit.path, hit.sentinel_id) for hit in hits}),
        "match_form_count": len(hits),
        "hits": [hit.as_dict() for hit in hits],
        "errors": errors,
    }
    rendered = json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.json_out:
        try:
            args.json_out.parent.mkdir(parents=True, exist_ok=True)
            args.json_out.write_text(rendered, encoding="utf-8")
        except OSError:
            print("scanner error: could not write JSON evidence", file=sys.stderr)
            return 2
    print(f"{result['status']}: {result['hit_count']} sentinel leaks across {len(args.paths)} scan roots")
    if errors:
        print(f"scanner errors: {len(errors)} (details in JSON output when requested)")
    if hits:
        print("hit metadata:")
        for hit in hits:
            print(f"  {hit.path}: {hit.sentinel_id} ({hit.form})")
    return 2 if errors else (1 if hits else 0)


if __name__ == "__main__":
    raise SystemExit(main())
