import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from wamtest.sentinels import load_manifest, manifest_sha256
from wamtest.sink_inventory import SINKS, inspect_inventory


class SinkInventoryTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.path = self.root / "inventory.json"
        self.inventory = {"schema_version": 1, "fixture": True, "sentinel_count": 500,
                          "manifest_sha256": manifest_sha256(), "sinks": []}
        for name in sorted(SINKS):
            target = self.root / f"{name}.txt"
            target.write_text("owned capture; one real fixture record\n")
            self.inventory["sinks"].append({"name": name, "status": "collected", "artifacts": [
                {"path": target.name, "sha256": hashlib.sha256(target.read_bytes()).hexdigest()}]})

    def check(self):
        self.path.write_text(json.dumps(self.inventory))
        return inspect_inventory(self.path)

    def test_requires_every_collected_nonempty_sink(self):
        self.assertTrue(self.check()["passed"])
        self.inventory["sinks"].pop()
        self.assertFalse(self.check()["passed"])

    def test_skipped_or_empty_collection_cannot_pass(self):
        self.inventory["sinks"][0]["status"] = "skipped"
        self.assertFalse(self.check()["passed"])
        self.inventory["sinks"][0]["status"] = "collected"
        self.inventory["sinks"][0]["artifacts"] = []
        self.assertFalse(self.check()["passed"])

    def test_duplicate_or_unknown_sink_cannot_substitute_for_coverage(self):
        for name in (self.inventory["sinks"][1]["name"], "unknown"):
            self.inventory["sinks"][0]["name"] = name
            self.assertFalse(self.check()["passed"])

    def test_mutated_missing_empty_and_escaping_artifacts_are_rejected(self):
        row = self.inventory["sinks"][0]["artifacts"][0]
        for name in ("../outside", "/etc/passwd", "missing.txt", "..\\outside"):
            row["path"] = name
            self.assertFalse(self.check()["passed"])
        row["path"] = "api.txt"
        (self.root / "api.txt").write_text("")
        self.assertFalse(self.check()["passed"])
        (self.root / "api.txt").write_text("mutated capture")
        self.assertFalse(self.check()["passed"])

    def test_symlink_and_shared_artifact_do_not_count_as_independent_sinks(self):
        row = self.inventory["sinks"][0]["artifacts"][0]
        (self.root / "alias.txt").symlink_to(self.root / row["path"])
        row["path"] = "alias.txt"
        self.assertFalse(self.check()["passed"])
        row.update(self.inventory["sinks"][1]["artifacts"][0])
        self.assertFalse(self.check()["passed"])

    def test_actual_leak_positive_control_fails_without_echoing_its_value(self):
        entry = load_manifest()[150]
        target = self.root / "api.txt"
        target.write_text(json.dumps({"error": entry.value}))
        row = next(r for r in self.inventory["sinks"] if r["name"] == "api")["artifacts"][0]
        row["sha256"] = hashlib.sha256(target.read_bytes()).hexdigest()
        report = self.check()
        self.assertFalse(report["passed"])
        self.assertEqual({h["sentinel_id"] for s in report["sinks"] for h in s["hits"]}, {entry.id})
        self.assertNotIn(entry.value, json.dumps(report))

    def test_scanner_failure_and_wrong_manifest_fail_closed(self):
        with patch("wamtest.sink_inventory.scan_paths", return_value=([], ["unreadable"])):
            self.assertFalse(self.check()["passed"])
        self.inventory["manifest_sha256"] = "0" * 64
        self.assertFalse(self.check()["passed"])

    def test_duplicate_json_keys_and_boolean_schema_cannot_be_accepted(self):
        self.check()
        source = self.path.read_text()
        self.path.write_text(source.replace('"fixture": true', '"fixture": false, "fixture": true'))
        self.assertFalse(inspect_inventory(self.path)["passed"])
        self.inventory["schema_version"] = True
        self.assertFalse(self.check()["passed"])

    def test_cli_creates_a_private_report_and_refuses_to_replace_it(self):
        self.check()
        report = self.root / 'report.json'
        command = [sys.executable, '-B', str(Path(__file__).resolve().parents[2] / 'security/check_sink_inventory.py'),
                   str(self.path), '--json-out', str(report)]
        first = subprocess.run(command, capture_output=True, text=True, timeout=30)
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertTrue(json.loads(report.read_text())['passed'])
        if os.name == 'posix':
            self.assertEqual(report.stat().st_mode & 0o777, 0o600)
        original = report.read_bytes()
        second = subprocess.run(command, capture_output=True, text=True, timeout=30)
        self.assertEqual(second.returncode, 2)
        self.assertEqual(report.read_bytes(), original)
