#!/usr/bin/env python3
"""Check candidate-bound release evidence; never publish or fabricate test results."""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
POLICY = ROOT / "docs/test/release-gates.json"
COMMIT = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
STATUSES = {"pass", "fail", "pending", "blocked", "skipped"}
KINDS = {"local", "ci", "native", "live", "integration", "staging", "manual", "benchmark"}
MAX_ARTIFACT = 64 * 1024 * 1024


def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON field")
        result[key] = value
    return result


def _constant(_):
    raise ValueError("nonfinite JSON value")


def _load(path):
    path = Path(path)
    if path.is_symlink() or path.stat().st_size > 1024 * 1024:
        raise ValueError("invalid evidence document")
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=_object, parse_constant=_constant)


def _fields(value, required, optional=()):
    if not isinstance(value, dict) or not set(required) <= set(value) or set(value) - set(required) - set(optional):
        raise ValueError("invalid evidence fields")


def load_policy(path=POLICY):
    policy = _load(path)
    _fields(policy, {"schema_version", "max_age_days", "gates"})
    if (type(policy["schema_version"]) is not int or policy["schema_version"] != 1 or type(policy["max_age_days"]) is not int
            or not 0 < policy["max_age_days"] <= 90 or not isinstance(policy["gates"], list)):
        raise ValueError("invalid release policy")
    seen = set()
    for gate in policy["gates"]:
        _fields(gate, {"id", "profiles", "cases", "kinds", "allow_fixture"}, {"optional", "files"})
        id_ = gate["id"]
        if not isinstance(id_, str) or not re.fullmatch(r"REL-\d{2}", id_) or id_ in seen:
            raise ValueError("invalid or duplicate gate")
        seen.add(id_)
        for name in ("profiles", "cases", "kinds"):
            values = gate[name]
            if (not isinstance(values, list) or not values
                    or any(not isinstance(value, str) or not re.fullmatch(r"[a-z0-9_-]+", value) for value in values)
                    or len(values) != len(set(values))):
                raise ValueError("invalid gate values")
        if (set(gate["profiles"]) - {"client", "service"} or set(gate["kinds"]) - KINDS
                or type(gate["allow_fixture"]) is not bool or gate.get("optional", "claims") != "claims"):
            raise ValueError("invalid gate contract")
        files = gate.get("files", {})
        if not isinstance(files, dict) or set(files) - set(gate["cases"]):
            raise ValueError("invalid required files")
        for paths in files.values():
            if not isinstance(paths, list) or not paths or any(_path_parts(path) is None for path in paths):
                raise ValueError("invalid required file path")
    if not seen:
        raise ValueError("empty release policy")
    return policy


def _path_parts(value):
    if not isinstance(value, str) or not value or "\\" in value or ":" in value or "\x00" in value:
        return None
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in value.split("/")):
        return None
    return path.parts


def _artifact(root, value):
    parts = _path_parts(value)
    if parts is None:
        return None, "artifact-path"
    root = Path(root).resolve()
    path = root
    for part in parts:
        path = path / part
        if path.is_symlink():
            return None, "artifact-symlink"
    if not path.is_file():
        return None, "artifact-missing"
    if not path.resolve().is_relative_to(root):
        return None, "artifact-path"
    if not 0 < path.stat().st_size <= MAX_ARTIFACT:
        return None, "artifact-size"
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            size += len(block)
            if size > MAX_ARTIFACT:
                return None, "artifact-size"
            digest.update(block)
    return digest.hexdigest(), None


def _required(policy, profile, claims):
    return [gate for gate in policy["gates"] if profile in gate["profiles"]
            and (not gate.get("optional") or claims)]


def template(candidate, profile, claims=False, policy_path=POLICY):
    if not isinstance(candidate, str) or not COMMIT.fullmatch(candidate) or profile not in {"client", "service"}:
        raise ValueError("invalid candidate or profile")
    policy = load_policy(policy_path)
    return {
        "schema_version": 1, "candidate": candidate,
        "policy_sha256": hashlib.sha256(Path(policy_path).read_bytes()).hexdigest(),
        "records": [{"gate": gate["id"], "case": case, "status": "pending", "source_commit": candidate,
                     "dirty": True, "kind": gate["kinds"][0], "fixture_only": False,
                     "observed_at": None, "artifact": None, "sha256": None}
                    for gate in _required(policy, profile, claims) for case in gate["cases"]],
    }


def check(evidence, candidate, profile, *, claims=False, repo=ROOT, policy_path=POLICY,
          now=None, verify_checkout=True):
    """Check evidence metadata/integrity, not the truth of a declared test result."""
    policy = load_policy(policy_path)
    if not isinstance(candidate, str) or not COMMIT.fullmatch(candidate) or profile not in {"client", "service"}:
        raise ValueError("invalid candidate or profile")
    path = Path(evidence)
    manifest = _load(path)
    _fields(manifest, {"schema_version", "candidate", "policy_sha256", "records"})
    if type(manifest["schema_version"]) is not int or manifest["schema_version"] != 1 or not isinstance(manifest["records"], list):
        raise ValueError("invalid evidence schema")
    report = {"schema_version": 1, "candidate": candidate, "profile": profile,
              "claims_checked": claims, "ready": False, "passed": [], "blockers": []}

    def block(gate, case, reason):
        report["blockers"].append({"gate": gate, "case": case, "reason": reason})

    if manifest["candidate"] != candidate:
        block("checkout", "candidate", "candidate-mismatch")
    if manifest["policy_sha256"] != hashlib.sha256(Path(policy_path).read_bytes()).hexdigest():
        block("checkout", "policy", "policy-mismatch")
    if verify_checkout:
        try:
            head = subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"], check=True,
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=10).stdout.strip()
            dirty = subprocess.run(["git", "-C", str(repo), "status", "--porcelain"], check=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, timeout=10).stdout
            if head != candidate:
                block("checkout", "head", "checkout-mismatch")
            if dirty:
                block("checkout", "tree", "dirty-checkout")
        except (OSError, subprocess.SubprocessError):
            block("checkout", "head", "checkout-unavailable")
    definitions = {gate["id"]: gate for gate in policy["gates"]}
    records = {}
    fields = {"gate", "case", "status", "source_commit", "dirty", "kind", "fixture_only", "observed_at", "artifact", "sha256"}
    for record in manifest["records"]:
        _fields(record, fields, {"reviewer"})
        gate_id, case = record["gate"], record["case"]
        if not isinstance(gate_id, str) or gate_id not in definitions or case not in definitions[gate_id]["cases"]:
            raise ValueError("unknown release gate/case")
        key = (gate_id, case)
        if key in records or not isinstance(record["status"], str) or record["status"] not in STATUSES:
            raise ValueError("duplicate or invalid release record")
        records[key] = record
    now = now or datetime.now(timezone.utc)
    if now.tzinfo is None:
        raise ValueError("now must be timezone-aware")
    for gate in _required(policy, profile, claims):
        for case in gate["cases"]:
            key = (gate["id"], case)
            record = records.get(key)
            reason = None
            for required_file in gate.get("files", {}).get(case, []):
                _, issue = _artifact(repo, required_file)
                if issue:
                    reason = "required-file-missing"
            if reason is None and record is None:
                reason = "missing-evidence"
            if reason is None and record["status"] != "pass":
                reason = "not-pass"
            if reason is None and record["source_commit"] != candidate:
                reason = "source-mismatch"
            if reason is None and record["dirty"] is not False:
                reason = "dirty-result"
            if reason is None and record["kind"] not in gate["kinds"]:
                reason = "wrong-evidence-kind"
            if reason is None and (type(record["fixture_only"]) is not bool
                                   or (record["fixture_only"] and not gate["allow_fixture"])):
                reason = "fixture-not-allowed"
            if reason is None and record["kind"] == "manual" and (
                not isinstance(record.get("reviewer"), str) or not record["reviewer"].strip()
            ):
                reason = "missing-reviewer"
            if reason is None:
                stamp = record["observed_at"]
                try:
                    observed = datetime.fromisoformat(stamp.replace("Z", "+00:00"))
                    if observed.utcoffset() != timedelta(0):
                        raise ValueError("non-UTC evidence")
                    age = now - observed
                    if age < -timedelta(minutes=5):
                        reason = "future-evidence"
                    elif age > timedelta(days=policy["max_age_days"]):
                        reason = "stale-evidence"
                except (ValueError, TypeError, AttributeError):
                    reason = "invalid-timestamp"
            if reason is None:
                digest, reason = _artifact(path.parent, record["artifact"])
                if reason is None and (not isinstance(record["sha256"], str)
                                       or not SHA256.fullmatch(record["sha256"]) or digest != record["sha256"]):
                    reason = "artifact-hash"
            if reason:
                block(*key, reason)
            else:
                report["passed"].append({"gate": key[0], "case": key[1]})
    report["ready"] = not report["blockers"]
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--init", type=Path, dest="initialize")
    mode.add_argument("--evidence", type=Path)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--profile", choices=["client", "service"], required=True)
    parser.add_argument("--claims", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.initialize:
            value = template(args.candidate, args.profile, args.claims)
            args.initialize.parent.mkdir(parents=True, exist_ok=True)
            fd = os.open(args.initialize, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "w", encoding="utf-8") as stream:
                json.dump(value, stream, ensure_ascii=False, indent=2, allow_nan=False)
                stream.write("\n")
            print(json.dumps({"ready": False, "template_created": True, "records": len(value["records"])}))
            return 0
        report = check(args.evidence, args.candidate, args.profile, claims=args.claims)
    except (OSError, ValueError, TypeError):
        print(json.dumps({"ready": False, "error": "invalid-evidence"}))
        return 2
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        print("Evidence checks passed" if report["ready"] else "Release blocked")
        for row in report["blockers"]:
            print(f"{row['gate']}/{row['case']}: {row['reason']}")
    return 0 if report["ready"] else 1


if __name__ == "__main__":
    sys.exit(main())
