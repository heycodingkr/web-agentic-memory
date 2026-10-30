#!/usr/bin/env python3
"""Generate, validate, or score the deterministic synthetic recall corpus."""
import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests" / "lib"))

from wamtest.retrieval_corpus import (  # noqa: E402
    DEFAULT_SEED,
    evaluate_results,
    load_corpus,
    read_jsonl,
    validate_corpus,
    write_corpus,
)

DEFAULT_CORPUS = ROOT / "tests" / "fixtures" / "retrieval"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    generate = commands.add_parser("generate", help="write the deterministic corpus files")
    generate.add_argument("--output", type=Path, default=DEFAULT_CORPUS)
    generate.add_argument("--seed", type=int, default=DEFAULT_SEED)

    validate = commands.add_parser("validate", help="verify hashes, references, splits and row counts")
    validate.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)

    score = commands.add_parser("score", help="score normalized recall results against corpus labels")
    score.add_argument("--corpus", type=Path, default=DEFAULT_CORPUS)
    score.add_argument("--results", type=Path, required=True, help="JSONL rows: {query_id, memory_ids}")
    score.add_argument("--output", type=Path, help="optional JSON metrics output")

    args = parser.parse_args(argv)
    try:
        if args.command == "generate":
            manifest = write_corpus(args.output, args.seed)
            result = {
                "output": str(args.output),
                "records": manifest["record_count"],
                "index_rows": manifest["index_row_count"],
                "queries": manifest["query_count"],
                "heldout_queries": manifest["heldout_count"],
                "human_reviewed": manifest["human_reviewed"],
            }
            gate_pass = True
        elif args.command == "validate":
            result = {"corpus": str(args.corpus), **validate_corpus(args.corpus)}
            gate_pass = True
        else:
            corpus = load_corpus(args.corpus)
            result = evaluate_results(corpus, read_jsonl(args.results))
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            gate_pass = result["gate_pass"]
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0 if gate_pass else 1


if __name__ == "__main__":
    sys.exit(main())
