"""Evidence directories under target/e2e-logs/<suite>/<run-id>/ (docs/test/README.md section 5).

Files: meta.json, cases.jsonl, trace.jsonl, sinks/, shots/, summary.md.
A run ends with an explicit done or failed event; a silent log is never evidence of completion.
A skipped case needs a reason (no silent skipping).
"""
import json
import platform
import secrets
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_ROOT = REPO_ROOT / "target" / "e2e-logs"
VERDICTS = ("pass", "fail", "skip")


def _git(*args):
    try:
        done = subprocess.run(["git", *args], cwd=REPO_ROOT, capture_output=True, text=True, timeout=10)
    except (OSError, subprocess.SubprocessError):
        return None
    return done.stdout.strip() if done.returncode == 0 else None


def now_iso():
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def new_run_id():
    return datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S") + "-" + secrets.token_hex(2)


def read_jsonl(path):
    rows = []
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


class RunDir:
    def __init__(self, suite, run_id=None, root=None, host=None, extra_meta=None):
        self.suite = suite
        self.run_id = run_id or new_run_id()
        self.path = Path(root or DEFAULT_ROOT) / suite / self.run_id
        self.path.mkdir(parents=True, exist_ok=False)
        for sub in ("sinks", "shots"):
            (self.path / sub).mkdir()
        self._t0 = time.monotonic()
        self._cases = []
        self._ended = None
        self._trace = (self.path / "trace.jsonl").open("a", encoding="utf-8")
        self._cases_file = (self.path / "cases.jsonl").open("a", encoding="utf-8")
        dirty = _git("status", "--porcelain")
        self.meta = {
            "suite": suite,
            "run_id": self.run_id,
            "commit": _git("rev-parse", "HEAD"),
            "dirty": None if dirty is None else bool(dirty),
            "os": platform.platform(),
            "python": sys.version.split()[0],
            "host": host or {},
            "started_at": now_iso(),
            "argv": sys.argv,
        }
        if extra_meta:
            self.meta.update(extra_meta)
        self._write_meta()

    def _write_meta(self):
        (self.path / "meta.json").write_text(json.dumps(self.meta, ensure_ascii=False, indent=2), encoding="utf-8")

    def event(self, type_, **fields):
        """Append a structured trace event. Types 'done' and 'failed' end the run."""
        row = {"t": round(time.monotonic() - self._t0, 3), "ts": now_iso(), "type": type_, **fields}
        self._trace.write(json.dumps(row, ensure_ascii=False) + "\n")
        self._trace.flush()
        if type_ in ("done", "failed"):
            self._ended = type_
        return row

    def case(self, case_id, verdict, expected=None, actual=None, input_summary=None, evidence=None, reason=None):
        if verdict not in VERDICTS:
            raise ValueError(f"verdict must be one of {VERDICTS}, got {verdict!r}")
        if verdict == "skip" and not reason:
            raise ValueError("a skipped case needs a reason")
        row = {
            "case": case_id,
            "verdict": verdict,
            "input": input_summary,
            "expected": expected,
            "actual": actual,
            "evidence": evidence,
            "reason": reason,
        }
        self._cases.append(row)
        self._cases_file.write(json.dumps(row, ensure_ascii=False) + "\n")
        self._cases_file.flush()
        return row

    def shot(self, name, png_bytes):
        path = self.path / "shots" / f"{name}.png"
        path.write_bytes(png_bytes)
        return str(path.relative_to(self.path))

    def sink(self, name, data):
        path = self.path / "sinks" / name
        if isinstance(data, str):
            data = data.encode("utf-8")
        path.write_bytes(data)
        return str(path.relative_to(self.path))

    def counts(self):
        out = {v: 0 for v in VERDICTS}
        for row in self._cases:
            out[row["verdict"]] += 1
        return out

    def finish(self):
        counts = self.counts()
        if self._ended is None:
            self.event("failed" if counts["fail"] else "done", **counts)
        self.meta["ended_at"] = now_iso()
        self.meta["result"] = self._ended
        self.meta["counts"] = counts
        self._write_meta()
        self._write_summary(counts)
        self._trace.close()
        self._cases_file.close()
        return self._ended

    def _write_summary(self, counts):
        lines = [
            f"# {self.suite} / {self.run_id}",
            "",
            f"Result: **{self._ended}**. pass {counts['pass']}, fail {counts['fail']}, skip {counts['skip']}.",
            f"Commit: {self.meta.get('commit')} (dirty: {self.meta.get('dirty')}). OS: {self.meta['os']}.",
            "",
        ]
        for title, verdict in (("Failures", "fail"), ("Skipped", "skip")):
            rows = [r for r in self._cases if r["verdict"] == verdict]
            if rows:
                lines.append(f"## {title}")
                for r in rows:
                    detail = r["reason"] if verdict == "skip" else f"expected {r['expected']}, actual {r['actual']}"
                    lines.append(f"- `{r['case']}`: {detail}")
                lines.append("")
        (self.path / "summary.md").write_text("\n".join(lines), encoding="utf-8")

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        if exc is not None and self._ended is None:
            self.event("failed", error=f"{exc_type.__name__}: {exc}")
        self.finish()
        return False
