import base64
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from urllib.parse import quote
from unittest.mock import patch

from wamtest.sentinels import (
    DEFAULT_MANIFEST,
    build_manifest,
    load_manifest,
    render_manifest,
    scan_paths,
)


class SentinelCorpusTest(unittest.TestCase):
    def test_corpus_is_deterministic_with_500_synthetic_values(self):
        first = build_manifest()
        second = build_manifest()
        self.assertEqual(first, second)
        self.assertTrue(first["synthetic_only"])
        self.assertEqual(first["count"], 500)
        self.assertEqual(len(first["entries"]), 500)
        self.assertEqual(len({item["value"] for item in first["entries"]}), 500)
        self.assertEqual(len({item["id"] for item in first["entries"]}), 500)
        self.assertEqual(
            {family: count for family, count in _counts(first).items()},
            {family: 50 for family in _counts(first)},
        )

    def test_checked_in_manifest_loads_and_is_synthetic(self):
        entries = load_manifest(DEFAULT_MANIFEST)
        self.assertEqual(len(entries), 500)
        self.assertTrue(all(entry.synthetic for entry in entries))
        self.assertEqual(DEFAULT_MANIFEST.read_text(encoding="utf-8"), render_manifest(build_manifest()))

    def test_manifest_rejects_non_synthetic_entries(self):
        manifest = build_manifest()
        manifest["entries"][0]["synthetic"] = False
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "invalid.json"
            path.write_text(render_manifest(manifest), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "synthetic=true"):
                load_manifest(path)

    def test_scanner_finds_raw_and_encoded_sentinels_without_echoing_values(self):
        manifest = build_manifest()
        raw = manifest["entries"][0]["value"]
        encoded = manifest["entries"][1]["value"].encode("utf-8").hex()
        url_encoded = quote(manifest["entries"][400]["value"], safe="")
        base64_encoded = base64.b64encode(manifest["entries"][100]["value"].encode("utf-8"))
        split = manifest["entries"][2]["value"]
        split_at = len(split) // 2
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "nested").mkdir()
            (root / "clean.txt").write_text("ordinary fixture output", encoding="utf-8")
            (root / "nested" / "trace.bin").write_bytes(
                b"prefix:" + raw.encode("utf-8") + b";hex:" + encoded.encode("ascii")
                + b";url:" + url_encoded.encode("ascii") + b";base64:" + base64_encoded
                + b';split:"' + split[:split_at].encode("utf-8")
                + b'","rest":"' + split[split_at:].encode("utf-8") + b'"'
            )

            hits, errors = scan_paths([root], entries=manifest["entries"], manifest_path=None)

        self.assertEqual(errors, [])
        self.assertEqual(
            {hit.sentinel_id for hit in hits},
            {"api_key-000", "api_key-001", "url_query-000", "authorization-000", "api_key-002"},
        )
        serialized_hits = json.dumps([hit.as_dict() for hit in hits])
        self.assertNotIn(raw, serialized_hits)
        self.assertNotIn(manifest["entries"][1]["value"], serialized_hits)
        self.assertEqual({hit.form for hit in hits}, {"literal", "hex", "url", "base64", "split"})

    def test_scanner_detects_unpadded_base64url_when_standard_base64_differs(self):
        sentinel = {
            "id": "unicode-000",
            "family": "test",
            "value": "wam-💥-canaryx",
            "synthetic": True,
        }
        standard = base64.b64encode(sentinel["value"].encode("utf-8"))
        urlsafe = base64.urlsafe_b64encode(sentinel["value"].encode("utf-8")).rstrip(b"=")
        self.assertNotEqual(standard, urlsafe)
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "encoded.txt"
            target.write_bytes(urlsafe)
            hits, errors = scan_paths([target], entries=[sentinel], manifest_path=None)

        self.assertEqual(errors, [])
        self.assertEqual({hit.form for hit in hits}, {"base64url"})

    def test_scanner_detects_a_private_key_in_json_escaped_log_documents(self):
        entries = build_manifest()["entries"]
        sentinel = next(row for row in entries if row["id"] == "private_key-000")
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "diagnostics.jsonl"
            target.write_text(json.dumps({"message": sentinel["value"]}), encoding="utf-8")
            hits, errors = scan_paths([target], entries=entries, manifest_path=None)
        self.assertEqual(errors, [])
        self.assertEqual({hit.sentinel_id for hit in hits}, {sentinel["id"]})
        self.assertNotIn(sentinel["value"], json.dumps([hit.as_dict() for hit in hits]))

    def test_scanner_detects_zero_width_compatibility_and_common_confusable_variants(self):
        sentinel = build_manifest()["entries"][0]["value"]
        variants = (
            sentinel[:5] + "\u200b" + sentinel[5:],
            sentinel.replace("a", "а", 1),  # Cyrillic small a
            "".join(chr(ord(char) + 0xFEE0) if 0x21 <= ord(char) <= 0x7E else char for char in sentinel),
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            paths = []
            for index, variant in enumerate(variants):
                path = root / f"variant-{index}.txt"
                path.write_text(variant, encoding="utf-8")
                paths.append(path)
            hits, errors = scan_paths(paths, entries=build_manifest()["entries"], manifest_path=None)

        self.assertEqual(errors, [])
        self.assertEqual({hit.sentinel_id for hit in hits}, {"api_key-000"})
        self.assertEqual({hit.form for hit in hits}, {"unicode_normalized"})

    def test_unicode_sentinel_in_file_name_is_detected_and_path_metadata_is_redacted(self):
        sentinel = build_manifest()["entries"][0]["value"]
        variant = sentinel.replace("a", "а", 1)
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / variant
            target.touch()
            hits, errors = scan_paths([target], entries=build_manifest()["entries"], manifest_path=None)

        self.assertEqual(errors, [])
        self.assertEqual({hit.sentinel_id for hit in hits}, {"api_key-000"})
        self.assertEqual(hits[0].path, "<redacted-sentinel-metadata>")
        self.assertNotIn(variant, hits[0].path)

    def test_scan_reports_oversized_files_instead_of_silently_skipping(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "large.bin"
            target.write_bytes(b"0123456789")
            hits, errors = scan_paths([target], manifest_path=None, max_file_bytes=4)

        self.assertEqual(hits, [])
        self.assertEqual(len(errors), 1)
        self.assertIn("exceeds scan limit", errors[0])

    def test_scan_reports_directory_walk_errors(self):
        def failed_walk(path, *, followlinks, onerror):
            onerror(PermissionError("synthetic denied"))
            return iter(())

        with tempfile.TemporaryDirectory() as tmp:
            with patch("wamtest.sentinels.os.walk", side_effect=failed_walk):
                hits, errors = scan_paths([tmp])

        self.assertEqual(hits, [])
        self.assertEqual(len(errors), 1)
        self.assertIn("cannot traverse scan directory", errors[0])

    def test_clean_file_has_no_hits_or_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "clean.txt"
            target.write_text("ordinary output", encoding="utf-8")
            hits, errors = scan_paths([target])

        self.assertEqual(hits, [])
        self.assertEqual(errors, [])

    def test_cli_returns_failure_and_safe_metadata_on_a_sentinel_hit(self):
        root = Path(__file__).resolve().parents[3]
        script = root / "tests" / "security" / "scan_sentinels.py"
        sentinel = load_manifest()[0]
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / sentinel.value
            directory.mkdir()
            (directory / "response.json").write_text(sentinel.value, encoding="utf-8")
            evidence = Path(tmp) / "summary.json"
            result = subprocess.run(
                [sys.executable, "-B", str(script), str(directory), "--json-out", str(evidence)],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )
            summary = json.loads(evidence.read_text(encoding="utf-8"))

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertNotIn(sentinel.value, result.stdout + result.stderr)
        self.assertNotIn(sentinel.value, json.dumps(summary))
        self.assertEqual(summary["status"], "leaks_found")
        self.assertEqual(summary["hits"][0]["sentinel_id"], sentinel.id)

    def test_cli_returns_zero_for_clean_artifacts(self):
        root = Path(__file__).resolve().parents[3]
        script = root / "tests" / "security" / "scan_sentinels.py"
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / "clean"
            directory.mkdir()
            (directory / "log.txt").write_text("ordinary test output", encoding="utf-8")
            result = subprocess.run(
                [sys.executable, "-B", str(script), str(directory)],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 0, result.stderr or result.stdout)
        self.assertIn("clean: 0 sentinel leaks", result.stdout)

    def test_cli_returns_scan_error_for_oversized_artifact(self):
        root = Path(__file__).resolve().parents[3]
        script = root / "tests" / "security" / "scan_sentinels.py"
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / "sinks"
            directory.mkdir()
            (directory / "large.bin").write_bytes(b"x" * (1024 * 1024 + 1))
            evidence = Path(tmp) / "summary.json"
            result = subprocess.run(
                [
                    sys.executable,
                    "-B",
                    str(script),
                    str(directory),
                    "--max-file-mib",
                    "1",
                    "--json-out",
                    str(evidence),
                ],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )
            summary = json.loads(evidence.read_text(encoding="utf-8"))

        self.assertEqual(result.returncode, 2, result.stderr or result.stdout)
        self.assertEqual(summary["status"], "error")
        self.assertEqual(summary["hit_count"], 0)

    def test_scanner_checks_file_names_and_redacts_them_from_evidence(self):
        manifest = build_manifest()
        sentinel = manifest["entries"][450]
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / sentinel["value"]
            target.touch()
            hits, errors = scan_paths([target], entries=manifest["entries"], manifest_path=None)

        self.assertEqual(errors, [])
        self.assertIn(sentinel["id"], {hit.sentinel_id for hit in hits})
        self.assertTrue(all(sentinel["value"] not in hit.path for hit in hits))


def _counts(manifest):
    counts = {}
    for item in manifest["entries"]:
        counts[item["family"]] = counts.get(item["family"], 0) + 1
    return counts


if __name__ == "__main__":
    unittest.main()
