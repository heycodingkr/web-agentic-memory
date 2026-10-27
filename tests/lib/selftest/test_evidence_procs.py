import os
import secrets
import shutil
import tempfile
import time
import unittest
from pathlib import Path

from wamtest.evidence import RunDir, read_jsonl
from wamtest.procs import MARKER, Isolation, ManagedProcess, group_pids, is_clean, kill_by_arg, pids_with_arg, residue


class EvidenceTest(unittest.TestCase):
    def test_layout_summary_and_explicit_end(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = RunDir("unit", root=tmp)
            run.event("start")
            run.case("A-01", "pass", expected=1, actual=1)
            run.case("A-02", "fail", expected=1, actual=2)
            run.case("A-03", "skip", reason="not applicable here")
            self.assertEqual(run.finish(), "failed")
            for name in ("meta.json", "cases.jsonl", "trace.jsonl", "summary.md"):
                self.assertTrue((run.path / name).is_file(), name)
            self.assertTrue((run.path / "sinks").is_dir() and (run.path / "shots").is_dir())
            trace = read_jsonl(run.path / "trace.jsonl")
            self.assertEqual(trace[-1]["type"], "failed")
            self.assertEqual([c["verdict"] for c in read_jsonl(run.path / "cases.jsonl")], ["pass", "fail", "skip"])
            summary = (run.path / "summary.md").read_text()
            self.assertIn("A-02", summary)
            self.assertIn("not applicable here", summary)

    def test_clean_run_ends_with_done(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = RunDir("unit", root=tmp)
            run.case("A-01", "pass")
            self.assertEqual(run.finish(), "done")
            self.assertEqual(read_jsonl(run.path / "trace.jsonl")[-1]["type"], "done")

    def test_skip_needs_reason_and_verdicts_are_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = RunDir("unit", root=tmp)
            with self.assertRaises(ValueError):
                run.case("A-01", "skip")
            with self.assertRaises(ValueError):
                run.case("A-01", "maybe")
            run.finish()

    def test_exception_in_context_records_failed(self):
        with tempfile.TemporaryDirectory() as tmp:
            holder = {}
            with self.assertRaises(RuntimeError):
                with RunDir("unit", root=tmp) as run:
                    holder["run"] = run
                    raise RuntimeError("boom")
            trace = read_jsonl(holder["run"].path / "trace.jsonl")
            self.assertEqual(trace[-1]["type"], "failed")
            self.assertIn("boom", trace[-1]["error"])

    def test_run_ids_do_not_collide(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = RunDir("unit", root=tmp), RunDir("unit", root=tmp)
            self.assertNotEqual(a.run_id, b.run_id)
            a.finish()
            b.finish()


class ProcsTest(unittest.TestCase):
    def test_isolation_env_and_cleanup(self):
        iso = Isolation("unit")
        try:
            env = iso.env()
            self.assertTrue(env["HOME"].startswith(str(iso.root)))
            self.assertTrue(env["TMPDIR"].endswith("/"))
            self.assertLess(len(str(iso.root)), 24, "short paths keep unix sockets under the macOS limit")
            self.assertTrue(iso.cleanup())
            self.assertFalse(iso.root.exists())
        finally:
            shutil.rmtree(iso.root, ignore_errors=True)

    def test_cleanup_refuses_a_directory_it_does_not_own(self):
        iso = Isolation("unit")
        try:
            (iso.root / MARKER).unlink()
            self.assertFalse(iso.cleanup())
            self.assertTrue(iso.root.exists())
        finally:
            shutil.rmtree(iso.root, ignore_errors=True)

    def test_stop_kills_the_whole_group_and_nothing_else(self):
        bystander = ManagedProcess(["sleep", "30"])
        proc = ManagedProcess(["sh", "-c", "sleep 60 & sleep 60 & wait"])
        try:
            deadline = time.monotonic() + 5
            while len(group_pids(proc.pgid)) < 3 and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertGreaterEqual(len(group_pids(proc.pgid)), 3)
            proc.stop()
            self.assertEqual(group_pids(proc.pgid), [])
            self.assertTrue(bystander.alive(), "a process outside the group must be untouched")
        finally:
            bystander.stop()
            proc.stop()

    def test_arg_fragment_finds_and_kills_only_marked_processes(self):
        marker = f"wam-unit-{secrets.token_hex(4)}"
        # A compound command keeps the shell (and its $0 marker) alive; a lone command would be exec'd away.
        marked = ManagedProcess(["sh", "-c", "sleep 30; true", marker])
        other = ManagedProcess(["sleep", "30"])
        try:
            time.sleep(0.2)
            self.assertIn(marked.pid, pids_with_arg(marker))
            self.assertNotIn(other.pid, pids_with_arg(marker))
            kill_by_arg(marker)
            self.assertEqual(pids_with_arg(marker), [])
            self.assertTrue(other.alive())
        finally:
            marked.stop()
            other.stop()

    def test_residue_report(self):
        iso = Isolation("unit")
        proc = ManagedProcess(["sleep", "30"])
        try:
            self.assertFalse(is_clean(residue(iso, [proc])))
            proc.stop()
            iso.cleanup()
            self.assertTrue(is_clean(residue(iso, [proc])))
        finally:
            proc.stop()
            shutil.rmtree(iso.root, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
