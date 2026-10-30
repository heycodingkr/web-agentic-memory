"""Fail-closed inventory checks for an explicitly collected local privacy sweep.

Only fixture artifacts are accepted here. This is not a staging/release attestation.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

from .sentinels import load_manifest, manifest_sha256, scan_paths

SINKS = frozenset({"client", "web", "api", "database", "diagnostics", "cache", "process", "gatekeeper"})
MAX_BYTES = 64 * 1024 * 1024


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate inventory key")
        result[key] = value
    return result


def inspect_inventory(path: Path) -> dict:
    """Check coverage, artifacts and leaks. Never include captured payloads in the result."""
    entries = load_manifest()
    result = {"schema_version": 1, "fixture": True, "sentinel_count": len(entries),
              "manifest_sha256": manifest_sha256(), "passed": False, "errors": [], "sinks": []}
    try:
        data = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=_unique_object)
        if (not isinstance(data, dict) or set(data) != {"schema_version", "fixture", "sentinel_count", "manifest_sha256", "sinks"}
                or type(data["schema_version"]) is not int or data["schema_version"] != 1
                or data["fixture"] is not True or data["sentinel_count"] != len(entries)
                or data["manifest_sha256"] != manifest_sha256() or not isinstance(data["sinks"], list)):
            raise ValueError()
    except (OSError, ValueError, UnicodeError):
        result["errors"].append("invalid-inventory")
        return result
    found, artifacts = set(), set()
    for row in data["sinks"]:
        if (not isinstance(row, dict) or set(row) != {"name", "status", "artifacts"}
                or not isinstance(row["name"], str) or row["name"] not in SINKS or row["name"] in found):
            result["errors"].append("invalid-or-duplicate-sink")
            continue
        name = row["name"]
        found.add(name)
        outcome = {"name": name, "passed": False, "files": 0, "hits": [], "scan_error_count": 0}
        result["sinks"].append(outcome)
        if row["status"] != "collected" or not isinstance(row["artifacts"], list) or not row["artifacts"]:
            result["errors"].append(f"{name}:not-collected")
            continue
        paths, invalid = [], False
        for artifact in row["artifacts"]:
            try:
                if not isinstance(artifact, dict) or set(artifact) != {"path", "sha256"}:
                    raise ValueError()
                relative = artifact["path"]
                if (not isinstance(relative, str) or not relative or "\\" in relative
                        or Path(relative).is_absolute() or any(p in {".", ".."} for p in relative.split("/"))):
                    raise ValueError()
                base = path.parent.resolve(strict=True)
                parts = Path(relative).parts
                target = base / relative
                if any(base.joinpath(*parts[:index]).is_symlink() for index in range(1, len(parts) + 1)):
                    raise ValueError()
                if not target.is_file() or not 0 < target.stat().st_size <= MAX_BYTES:
                    raise ValueError()
                resolved = target.resolve(strict=True)
                if not resolved.is_relative_to(path.parent.resolve()) or resolved in artifacts:
                    raise ValueError()
                if hashlib.sha256(target.read_bytes()).hexdigest() != artifact["sha256"]:
                    raise ValueError()
                artifacts.add(resolved)
                paths.append(target)
            except (OSError, ValueError, TypeError):
                invalid = True
                result["errors"].append(f"{name}:invalid-artifact")
        if invalid:
            continue
        hits, errors = scan_paths(paths, entries=entries)
        outcome.update(files=len(paths), hits=[hit.as_dict() for hit in hits], scan_error_count=len(errors))
        outcome["passed"] = not hits and not errors
    result["errors"].extend(f"{name}:missing-sink" for name in sorted(SINKS - found))
    result["passed"] = not result["errors"] and len(found) == len(SINKS) and all(row["passed"] for row in result["sinks"])
    return result
