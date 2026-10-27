"""Spec to test traceability: every test ID that a spec cites must be defined in a docs/test document.

Doc-first rule (docs/test/README.md principle 1): IDs are agreed in the docs before code uses them. IDs are
defined by table rows (| MEM-01 |) or headings (### FLOW-01). Citations may be ranges (MEM-01~02) or lists
(INST-43/44). Only prefixes that some test document defines are checked.

    python3 -B -m wamtest.doctrace docs/spec/memory.md docs/spec/recall.md   (PYTHONPATH=tests/lib)
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
DEF_ROW = re.compile(r"^\|\s*([A-Z]{2,6}-\d{2,3})\s*\|", re.M)
DEF_HEAD = re.compile(r"^#{2,4}\s+([A-Z]{2,6}-\d{2,3})\b", re.M)
REF = re.compile(r"(?<![A-Za-z-])([A-Z]{2,6})-(\d{2,3})((?:[~/]\d{2,3})*)")


def _ids(prefix, first, tail):
    nums = [int(first)]
    width = len(first)
    previous, op = int(first), None
    for token in re.findall(r"[~/]|\d+", tail):
        if token in "~/":
            op = token
            continue
        value = int(token)
        if op == "~":
            nums.extend(range(previous + 1, value + 1))
        else:
            nums.append(value)
        previous = value
    return [f"{prefix}-{n:0{width}d}" for n in nums]


def defined_ids(test_root=None):
    root = Path(test_root) if test_root else REPO / "docs" / "test"
    found = set()
    for path in root.rglob("*.md"):
        text = path.read_text(encoding="utf-8")
        found.update(DEF_ROW.findall(text))
        found.update(DEF_HEAD.findall(text))
    return found


def references(path):
    text = Path(path).read_text(encoding="utf-8")
    refs = []
    for match in REF.finditer(text):
        refs.extend(_ids(match.group(1), match.group(2), match.group(3)))
    return refs


def undefined(paths, test_root=None):
    """(file, id) pairs cited by the given docs whose prefix is a defined test prefix but whose id is not defined."""
    defined = defined_ids(test_root)
    prefixes = {i.rsplit("-", 1)[0] for i in defined}
    problems = []
    for path in paths:
        for ref in sorted(set(references(path))):
            if ref.rsplit("-", 1)[0] in prefixes and ref not in defined:
                problems.append((str(path), ref))
    return problems


def main(argv=None):
    paths = [Path(p) for p in (argv if argv is not None else sys.argv[1:])]
    problems = undefined(paths)
    for path, ref in problems:
        print(f"UNDEFINED {ref}  cited in {path}")
    print("OK: every cited test ID is defined" if not problems else f"{len(problems)} undefined citation(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
