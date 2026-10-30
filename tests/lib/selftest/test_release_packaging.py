import hashlib
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from tarfile import open as open_tar
from zipfile import ZipFile


ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))

from release.package import (  # noqa: E402
    RELEASE_TARGETS,
    package_artifact,
    release_version,
    verify_artifacts,
    version_from_metadata,
    write_checksums,
)

SOURCE_COMMIT = "a" * 40


class ReleasePackagingTest(unittest.TestCase):
    def test_ui_checks_follow_browser_installation_in_ci(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertLess(workflow.index('playwright install --with-deps chrome'), workflow.index('run test:ui'))

    def test_release_and_platform_workflows_cover_all_required_native_targets(self):
        expected = {
            'x86_64-unknown-linux-gnu': 'ubuntu-24.04',
            'aarch64-unknown-linux-gnu': 'ubuntu-24.04-arm',
            'x86_64-apple-darwin': 'macos-15-intel',
            'aarch64-apple-darwin': 'macos-15',
            'x86_64-pc-windows-msvc': 'windows-latest',
        }
        self.assertEqual(set(RELEASE_TARGETS), set(expected))
        workflow = (ROOT / '.github/workflows/release.yml').read_text()
        rows = re.findall(r'- os:\s*(\S+)\s+target:\s*(\S+)', workflow)
        self.assertEqual({target: runner for runner, target in rows}, expected)
        platform_workflow = (ROOT / '.github/workflows/cli-platforms.yml').read_text()
        for runner in expected.values():
            self.assertIn(runner, platform_workflow)

    def setUp(self):
        self.metadata = {
            "packages": [
                {"name": "wam-cli", "version": "0.1.0"},
                {"name": "wam-mcp", "version": "0.1.0"},
            ]
        }

    def test_tag_must_match_both_workspace_binary_versions(self):
        self.assertEqual(version_from_metadata(self.metadata), "0.1.0")
        self.assertEqual(release_version("v0.1.0", self.metadata), "0.1.0")
        with self.assertRaisesRegex(ValueError, "must match"):
            release_version("v0.2.0", self.metadata)

    def test_workspace_binary_versions_must_agree(self):
        metadata = {"packages": [*self.metadata["packages"]]}
        metadata["packages"][1] = {"name": "wam-mcp", "version": "0.2.0"}
        with self.assertRaisesRegex(ValueError, "same version"):
            version_from_metadata(metadata)

    def test_packages_unix_binaries_and_checksum_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary_dir, output_dir = root / "bin", root / "dist"
            binary_dir.mkdir()
            output_dir.mkdir()
            (binary_dir / "wam").write_bytes(b"cli-binary")
            (binary_dir / "wam-mcp").write_bytes(b"mcp-binary")

            archive = package_artifact(
                binary_dir, output_dir, "0.1.0", "x86_64-unknown-linux-gnu", SOURCE_COMMIT
            )
            sums = write_checksums(output_dir)
            expected_hash = hashlib.sha256(archive.read_bytes()).hexdigest()

            with open_tar(archive, "r:gz") as tar:
                self.assertEqual(set(tar.getnames()), {"wam", "wam-mcp", "BUILD-INFO.txt"})
                self.assertEqual(tar.extractfile("wam").read(), b"cli-binary")
                self.assertEqual(
                    tar.extractfile("BUILD-INFO.txt").read().decode("ascii"),
                    f"version=0.1.0\ntarget=x86_64-unknown-linux-gnu\nsource_commit={SOURCE_COMMIT}\n",
                )
            self.assertEqual(sums.read_text(encoding="utf-8"), f"{expected_hash}  {archive.name}\n")

    def test_packages_windows_binaries_as_zip(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary_dir, output_dir = root / "bin", root / "dist"
            binary_dir.mkdir()
            output_dir.mkdir()
            (binary_dir / "wam.exe").write_bytes(b"cli-binary")
            (binary_dir / "wam-mcp.exe").write_bytes(b"mcp-binary")

            archive = package_artifact(
                binary_dir, output_dir, "0.1.0", "x86_64-pc-windows-msvc", SOURCE_COMMIT
            )

            self.assertEqual(archive.suffix, ".zip")
            with ZipFile(archive) as zipped:
                self.assertEqual(set(zipped.namelist()), {"wam.exe", "wam-mcp.exe", "BUILD-INFO.txt"})
                self.assertEqual(zipped.read("wam.exe"), b"cli-binary")

    def test_refuses_missing_binaries_and_unsafe_target_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary_dir, output_dir = root / "bin", root / "dist"
            binary_dir.mkdir()
            output_dir.mkdir()
            (binary_dir / "wam").write_bytes(b"cli-binary")
            with self.assertRaisesRegex(ValueError, "missing release binary"):
                package_artifact(
                    binary_dir, output_dir, "0.1.0", "x86_64-unknown-linux-gnu", SOURCE_COMMIT
                )
            with self.assertRaisesRegex(ValueError, "target triple"):
                package_artifact(binary_dir, output_dir, "0.1.0", "../../outside", SOURCE_COMMIT)
            with self.assertRaisesRegex(ValueError, "full lowercase Git hash"):
                package_artifact(binary_dir, output_dir, "0.1.0", "x86_64-unknown-linux-gnu", "bad")

    def test_release_artifact_verifier_checks_all_targets_and_source_commit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive_dir = root / "dist"
            archive_dir.mkdir()
            targets = (
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                "x86_64-apple-darwin",
                "aarch64-apple-darwin",
                "x86_64-pc-windows-msvc",
            )
            for target in targets:
                binary_dir = root / "targets" / target
                binary_dir.mkdir(parents=True)
                suffix = ".exe" if "windows" in target else ""
                (binary_dir / f"wam{suffix}").write_bytes(b"cli-binary")
                (binary_dir / f"wam-mcp{suffix}").write_bytes(b"mcp-binary")
                package_artifact(binary_dir, archive_dir, "0.1.0", target, SOURCE_COMMIT)

            verify_artifacts(archive_dir, "v0.1.0", SOURCE_COMMIT)
            with self.assertRaisesRegex(ValueError, "metadata does not match"):
                verify_artifacts(archive_dir, "v0.1.0", "b" * 40)
            (archive_dir / "unexpected.txt").write_text("unsigned release payload", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "unexpected file"):
                verify_artifacts(archive_dir, "v0.1.0", SOURCE_COMMIT)

    def test_cli_packages_against_the_real_workspace_metadata(self):
        script = ROOT / "tools" / "release" / "package.py"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary_dir, output_dir = root / "bin", root / "dist"
            binary_dir.mkdir()
            output_dir.mkdir()
            (binary_dir / "wam").write_bytes(b"cli-binary")
            (binary_dir / "wam-mcp").write_bytes(b"mcp-binary")
            result = subprocess.run(
                [
                    sys.executable,
                    "-B",
                    str(script),
                    "build",
                    "--tag",
                    "v0.1.0",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--source-commit",
                    SOURCE_COMMIT,
                    "--binary-dir",
                    str(binary_dir),
                    "--output-dir",
                    str(output_dir),
                    "--workspace",
                    str(ROOT),
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr or result.stdout)
            self.assertTrue((output_dir / "wam-0.1.0-x86_64-unknown-linux-gnu.tar.gz").is_file())


if __name__ == "__main__":
    unittest.main()
