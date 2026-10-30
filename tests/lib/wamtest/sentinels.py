"""Synthetic secret sentinels and a local-only artifact leak scanner."""

from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import unicodedata
import urllib.parse
from collections import deque
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_MANIFEST = ROOT / "tests" / "sentinels.json"
FAMILY_COUNT = 50
FAMILIES = (
    "api_key",
    "jwt",
    "authorization",
    "private_key",
    "email",
    "phone",
    "local_path",
    "session_cookie",
    "url_query",
    "opaque",
)
_COMMON_CONFUSABLES = str.maketrans({
    "а": "a", "А": "A", "в": "b", "В": "B", "с": "c", "С": "C",
    "е": "e", "Е": "E", "ғ": "f", "Ғ": "F", "һ": "h", "Һ": "H",
    "і": "i", "І": "I", "ј": "j", "Ј": "J", "к": "k", "К": "K",
    "м": "m", "М": "M", "о": "o", "О": "O", "р": "p", "Р": "P",
    "ѕ": "s", "Ѕ": "S", "т": "t", "Т": "T", "у": "y", "У": "Y",
    "х": "x", "Х": "X", "ӏ": "l", "Ӏ": "I",
    "α": "a", "Α": "A", "ε": "e", "Ε": "E", "ι": "i", "Ι": "I",
    "κ": "k", "Κ": "K", "ο": "o", "Ο": "O", "ρ": "p", "Ρ": "P",
    "τ": "t", "Τ": "T", "υ": "u", "Υ": "Y", "χ": "x", "Χ": "X",
    "ϲ": "c", "Ϲ": "C", "ϳ": "j", "ϴ": "O",
})


@dataclass(frozen=True)
class Sentinel:
    id: str
    family: str
    value: str
    synthetic: bool = True


@dataclass(frozen=True)
class ScanHit:
    path: str
    sentinel_id: str
    form: str

    def as_dict(self) -> dict[str, str]:
        """Safe evidence metadata; deliberately excludes the matched value."""
        return {"path": self.path, "sentinel_id": self.sentinel_id, "form": self.form}


@dataclass
class _Node:
    edges: dict[int, int]
    failure: int
    outputs: list[int]


def _digest(family: str, index: int) -> str:
    return hashlib.sha256(f"wam-synthetic-sentinel-v1:{family}:{index:03d}".encode()).hexdigest()[:32]


def _sentinel_value(family: str, index: int) -> str:
    token = _digest(family, index)
    short = token[:16]
    if family == "api_key":
        return f"wam_test_sk_{token}"
    if family == "jwt":
        header = base64.urlsafe_b64encode(b'{"alg":"HS256","typ":"JWT"}').rstrip(b"=").decode("ascii")
        payload_json = json.dumps({"sub": "synthetic", "canary": token}, separators=(",", ":"))
        payload = base64.urlsafe_b64encode(payload_json.encode("utf-8")).rstrip(b"=").decode("ascii")
        return f"{header}.{payload}.{token[:24]}"
    if family == "authorization":
        return f"Bearer wam_test_{token}"
    if family == "private_key":
        return f"-----BEGIN TEST PRIVATE KEY-----\n{token}\n-----END TEST PRIVATE KEY-----"
    if family == "email":
        return f"wam-{short}@example.invalid"
    if family == "phone":
        return f"+000-000-000-{index:04d}"
    if family == "local_path":
        return f"/Users/wam-synthetic-{short}/Documents/fixture.txt"
    if family == "session_cookie":
        return f"wam_session_test={token}"
    if family == "url_query":
        return f"https://fixture.invalid/path?access_token={token}"
    if family == "opaque":
        return f"WAM_SYNTHETIC_ONLY_{token}"
    raise ValueError(f"unsupported sentinel family: {family}")


def build_manifest() -> dict:
    entries = []
    for family in FAMILIES:
        for index in range(FAMILY_COUNT):
            entries.append({
                "id": f"{family}-{index:03d}",
                "family": family,
                "value": _sentinel_value(family, index),
                "synthetic": True,
            })
    return {
        "schema_version": 1,
        "synthetic_only": True,
        "count": len(entries),
        "generator": "tests/security/generate_sentinels.py",
        "entries": entries,
    }


def render_manifest(manifest: dict | None = None) -> str:
    return json.dumps(manifest or build_manifest(), ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def load_manifest(path: str | Path = DEFAULT_MANIFEST) -> list[Sentinel]:
    manifest_path = Path(path)
    try:
        data = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read sentinel manifest {manifest_path}: {exc}") from exc
    if not isinstance(data, dict) or data.get("schema_version") != 1 or data.get("synthetic_only") is not True:
        raise ValueError("sentinel manifest must be schema version 1 and synthetic-only")
    rows = data.get("entries")
    if not isinstance(rows, list) or data.get("count") != len(rows) or len(rows) < 500:
        raise ValueError("sentinel manifest must declare at least 500 entries with a matching count")
    sentinels = []
    ids, values = set(), set()
    families = {}
    for row in rows:
        if not isinstance(row, dict):
            raise ValueError("sentinel manifest entry must be an object")
        if not all(isinstance(row.get(key), str) for key in ("id", "family", "value")):
            raise ValueError("sentinel id, family, and value must be strings")
        sentinel = Sentinel(
            id=row.get("id", ""),
            family=row.get("family", ""),
            value=row.get("value", ""),
            synthetic=row.get("synthetic") is True,
        )
        if not sentinel.synthetic or not _SAFE_ID.fullmatch(sentinel.id) or not sentinel.family or not sentinel.value:
            raise ValueError("every sentinel must have an id, family, value, and synthetic=true")
        if sentinel.id in ids or sentinel.value in values:
            raise ValueError("sentinel IDs and values must be unique")
        if len(sentinel.value.encode("utf-8")) > 1024:
            raise ValueError("sentinel values must be at most 1024 bytes")
        ids.add(sentinel.id)
        values.add(sentinel.value)
        families[sentinel.family] = families.get(sentinel.family, 0) + 1
        sentinels.append(sentinel)
    if set(families) != set(FAMILIES) or any(count != FAMILY_COUNT for count in families.values()):
        raise ValueError("sentinel manifest must contain 50 entries for each of the ten expected families")
    return sentinels


def _entries(values: Iterable[Sentinel | dict]) -> list[Sentinel]:
    result = []
    for item in values:
        if isinstance(item, Sentinel):
            result.append(item)
        else:
            result.append(Sentinel(
                id=str(item.get("id", "")),
                family=str(item.get("family", "")),
                value=str(item.get("value", "")),
                synthetic=item.get("synthetic") is True,
            ))
    if not result or any(not row.synthetic or not row.id or not row.value for row in result):
        raise ValueError("scanner requires non-empty synthetic sentinel entries")
    return result


def _patterns(sentinels: list[Sentinel]) -> list[tuple[bytes, str, str]]:
    patterns = {}
    split_patterns = {}
    for sentinel in sentinels:
        raw = sentinel.value.encode("utf-8")
        candidates = (
            ("literal", raw),
            ("json", json.dumps(sentinel.value, ensure_ascii=True)[1:-1].encode("ascii")),
            ("nested_json", json.dumps(json.dumps(sentinel.value)[1:-1])[1:-1].encode("ascii")),
            ("url", urllib.parse.quote(sentinel.value, safe="").encode("ascii")),
            ("base64", base64.b64encode(raw)),
            ("base64url", base64.urlsafe_b64encode(raw).rstrip(b"=")),
            ("hex", raw.hex().encode("ascii")),
        )
        for form, candidate in candidates:
            if candidate:
                patterns.setdefault((candidate, sentinel.id), form)
        # Detect a synthetic value split into multiple fields in one artifact.
        for split_index, offset in enumerate((len(raw) // 2, len(raw) * 2 // 3), start=1):
            if offset >= 12 and len(raw) - offset >= 12:
                split_patterns[(raw[:offset], sentinel.id, f"split_{split_index}_left")] = None
                split_patterns[(raw[offset:], sentinel.id, f"split_{split_index}_right")] = None
    regular = [(pattern, sentinel_id, form) for (pattern, sentinel_id), form in patterns.items()]
    split = [(pattern, sentinel_id, form) for pattern, sentinel_id, form in split_patterns]
    return regular + split


def _normalize_unicode_text(text: str) -> str:
    """Normalize compatibility forms, invisible format/combining marks and common homoglyphs."""
    decomposed = unicodedata.normalize("NFKD", text)
    visible = "".join(
        char for char in decomposed
        if unicodedata.category(char) not in {"Cf", "Mn", "Me"}
    )
    return visible.translate(_COMMON_CONFUSABLES)


def _unicode_patterns(sentinels: list[Sentinel]) -> list[tuple[bytes, str, str]]:
    patterns = []
    for sentinel in sentinels:
        normalized = _normalize_unicode_text(sentinel.value)
        if not normalized:
            continue
        patterns.append((normalized.encode("utf-8"), sentinel.id, "unicode_normalized"))
        for split_index, offset in enumerate((len(normalized) // 2, len(normalized) * 2 // 3), start=1):
            left, right = normalized[:offset], normalized[offset:]
            if len(left.encode("utf-8")) >= 12 and len(right.encode("utf-8")) >= 12:
                patterns.append((left.encode("utf-8"), sentinel.id, f"split_unicode_{split_index}_left"))
                patterns.append((right.encode("utf-8"), sentinel.id, f"split_unicode_{split_index}_right"))
    return patterns


def _automaton(patterns: list[tuple[bytes, str, str]]) -> list[_Node]:
    nodes = [_Node({}, 0, [])]
    for pattern_index, (pattern, _, _) in enumerate(patterns):
        state = 0
        for byte in pattern:
            next_state = nodes[state].edges.get(byte)
            if next_state is None:
                next_state = len(nodes)
                nodes[state].edges[byte] = next_state
                nodes.append(_Node({}, 0, []))
            state = next_state
        nodes[state].outputs.append(pattern_index)

    queue = deque(nodes[0].edges.values())
    while queue:
        state = queue.popleft()
        for byte, child in nodes[state].edges.items():
            queue.append(child)
            failure = nodes[state].failure
            while failure and byte not in nodes[failure].edges:
                failure = nodes[failure].failure
            nodes[child].failure = nodes[failure].edges.get(byte, 0)
            nodes[child].outputs.extend(nodes[nodes[child].failure].outputs)
    return nodes


def _find_patterns(data: bytes, nodes: list[_Node], patterns: list[tuple[bytes, str, str]]) -> dict[str, set[str]]:
    state = 0
    matched: dict[str, set[str]] = {}
    for byte in data:
        while state and byte not in nodes[state].edges:
            state = nodes[state].failure
        state = nodes[state].edges.get(byte, 0)
        for pattern_index in nodes[state].outputs:
            _, sentinel_id, form = patterns[pattern_index]
            matched.setdefault(sentinel_id, set()).add(form)
    return matched


def _files(paths: Iterable[str | Path], excluded: set[Path]) -> tuple[list[Path], list[str]]:
    files, errors = [], []
    seen = set()
    for raw_path in paths:
        path = Path(raw_path)
        if path.is_symlink():
            errors.append(f"refusing symlink scan path: {path}")
            continue
        if not path.exists():
            errors.append(f"scan path does not exist: {path}")
            continue
        if path.is_file():
            candidates = [path]
        elif path.is_dir():
            candidates = []
            def on_walk_error(error: OSError) -> None:
                failed_path = getattr(error, "filename", None) or path
                errors.append(f"cannot traverse scan directory {failed_path}: {error}")

            for root, dirs, names in os.walk(path, followlinks=False, onerror=on_walk_error):
                root_path = Path(root)
                dirs[:] = sorted(name for name in dirs if not (root_path / name).is_symlink())
                candidates.extend(root_path / name for name in sorted(names))
        else:
            errors.append(f"scan path is not a regular file or directory: {path}")
            continue
        for candidate in candidates:
            if candidate.is_symlink():
                continue
            try:
                resolved = candidate.resolve(strict=True)
            except OSError as exc:
                errors.append(f"cannot resolve scan file {candidate}: {exc}")
                continue
            if resolved in excluded or resolved in seen:
                continue
            seen.add(resolved)
            files.append(candidate)
    return files, errors


def scan_paths(
    paths: Iterable[str | Path],
    *,
    entries: Iterable[Sentinel | dict] | None = None,
    manifest_path: str | Path | None = DEFAULT_MANIFEST,
    exclude_paths: Iterable[str | Path] = (),
    max_file_bytes: int = 64 * 1024 * 1024,
) -> tuple[list[ScanHit], list[str]]:
    """Search files/directories recursively; report metadata, never matched text."""
    if max_file_bytes <= 0:
        raise ValueError("max_file_bytes must be positive")
    if entries is None:
        entries = load_manifest(manifest_path or DEFAULT_MANIFEST)
    sentinels = _entries(entries)
    patterns = _patterns(sentinels)
    nodes = _automaton(patterns)
    unicode_patterns = _unicode_patterns(sentinels)
    unicode_nodes = _automaton(unicode_patterns)
    excluded = set()
    for raw_path in (*([manifest_path] if manifest_path is not None else []), *exclude_paths):
        try:
            excluded.add(Path(raw_path).resolve(strict=False))
        except OSError:
            continue
    files, errors = _files(paths, excluded)
    hits = set()
    for path in files:
        try:
            size = path.stat().st_size
            if size > max_file_bytes:
                errors.append(f"file exceeds scan limit ({size} > {max_file_bytes} bytes): {path}")
                continue
            with path.open("rb") as stream:
                data = stream.read(max_file_bytes + 1)
            if len(data) > max_file_bytes:
                errors.append(f"file exceeds scan limit (> {max_file_bytes} bytes): {path}")
                continue
        except OSError as exc:
            errors.append(f"cannot read scan file {path}: {exc}")
            continue
        data = str(path).encode("utf-8", "surrogatepass") + b"\0" + data
        matched = _find_patterns(data, nodes, patterns)
        if not data.isascii():
            normalized_data = _normalize_unicode_text(data.decode("utf-8", "ignore")).encode("utf-8")
            normalized_matches = _find_patterns(normalized_data, unicode_nodes, unicode_patterns)
            for sentinel_id, forms in normalized_matches.items():
                full_match = "unicode_normalized" in forms or any(
                    {f"split_unicode_{split_index}_left", f"split_unicode_{split_index}_right"} <= forms
                    for split_index in (1, 2)
                )
                raw_full_match = any(
                    not form.startswith("split_") for form in matched.get(sentinel_id, set())
                )
                if full_match and not raw_full_match:
                    matched.setdefault(sentinel_id, set()).add("unicode_normalized")
        for sentinel_id, forms in matched.items():
            regular_forms = {form for form in forms if not form.startswith("split_")}
            for form in regular_forms:
                hits.add(ScanHit(_safe_metadata(str(path), sentinels), sentinel_id, form))
            for split_index in (1, 2):
                if "literal" not in forms and {f"split_{split_index}_left", f"split_{split_index}_right"} <= forms:
                    hits.add(ScanHit(_safe_metadata(str(path), sentinels), sentinel_id, "split"))
    safe_errors = sorted(_safe_metadata(error, sentinels) for error in errors)
    return sorted(hits, key=lambda hit: (hit.path, hit.sentinel_id, hit.form)), safe_errors


def manifest_sha256(path: str | Path = DEFAULT_MANIFEST) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def _safe_metadata(text: str, sentinels: list[Sentinel]) -> str:
    if not text.isascii():
        normalized = _normalize_unicode_text(text)
        if any(_normalize_unicode_text(sentinel.value) in normalized for sentinel in sentinels):
            return "<redacted-sentinel-metadata>"
    replacements = []
    for sentinel in sentinels:
        raw = sentinel.value.encode("utf-8")
        forms = (
            sentinel.value,
            json.dumps(sentinel.value, ensure_ascii=True)[1:-1],
            json.dumps(json.dumps(sentinel.value)[1:-1])[1:-1],
            urllib.parse.quote(sentinel.value, safe=""),
            base64.b64encode(raw).decode("ascii"),
            base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii"),
            raw.hex(),
        )
        replacements.extend((value, f"<sentinel:{sentinel.id}>") for value in forms if value)
        for offset in (len(raw) // 2, len(raw) * 2 // 3):
            if offset >= 12 and len(raw) - offset >= 12:
                for part in (raw[:offset], raw[offset:]):
                    replacements.append((part.decode("utf-8", "ignore"), f"<sentinel:{sentinel.id}:part>"))
    for value, replacement in sorted(replacements, key=lambda item: len(item[0]), reverse=True):
        text = text.replace(value, replacement)
    return text


def safe_metadata(text: str, sentinels: list[Sentinel]) -> str:
    """Redact sentinel text from paths or other evidence labels."""
    return _safe_metadata(text, sentinels)


_SAFE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$")
