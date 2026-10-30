import hashlib
import io
from pathlib import Path
import sys
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))
from release.package import package_artifact, write_checksums
from release.install import host_target, install_release, workflow_identity


class SignedInstallTest(unittest.TestCase):
    def test_linux_arm64_is_a_supported_native_release_target(self):
        with patch('release.install.sys.platform', 'linux'), patch('release.install.platform.machine', return_value='aarch64'):
            self.assertEqual(host_target(), 'aarch64-unknown-linux-gnu')

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="wam-install-test-")
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.dist = self.root / "dist"
        self.dist.mkdir()
        self.target = host_target()
        suffix = ".exe" if "windows" in self.target else ""
        self.names = ["wam" + suffix, "wam-mcp" + suffix]
        for name in self.names: (self.bin / name).write_bytes(name.encode() + b"-fixture-binary")
        self.archive = package_artifact(self.bin, self.dist, "0.1.0", self.target, "a" * 40)
        self.manifest = write_checksums(self.dist)
        for path in (self.archive, self.manifest): Path(str(path) + ".sigstore.json").write_text("{}")
        self.calls = []

    def tearDown(self): self.temp.cleanup()

    def verifier(self, asset, bundle, identity, cosign):
        self.assertNotEqual(asset.parent, self.dist)
        self.assertTrue(bundle.is_file())
        self.assertEqual(identity, "https://github.com/example/wam/.github/workflows/release.yml@refs/heads/main")
        self.calls.append(asset.name)

    def install(self, **kwargs):
        return install_release(self.archive, self.manifest, "0.1.0", self.target, "example/wam", verifier=self.verifier, **kwargs)

    def test_verified_artifacts_are_staged_then_installed_into_a_new_directory(self):
        prefix = self.root / "new-install"
        result = self.install(prefix=prefix)
        self.assertTrue(result["installed"])
        self.assertEqual(self.calls, ["SHA256SUMS", self.archive.name])
        for name in self.names: self.assertEqual((prefix / name).read_bytes(), (self.bin / name).read_bytes())
        self.assertTrue((prefix / ".wam-install.json").is_file())

    def test_verification_failure_never_creates_an_install_directory(self):
        def deny(*_): raise ValueError("invalid signer")
        prefix = self.root / "must-not-exist"
        with self.assertRaisesRegex(ValueError, "invalid signer"):
            install_release(self.archive, self.manifest, "0.1.0", self.target, "example/wam", prefix=prefix, verifier=deny)
        self.assertFalse(prefix.exists())

    def test_tampered_archive_is_rejected_even_when_the_verifier_is_stubbed(self):
        with self.archive.open("ab") as file: file.write(b"tampered")
        prefix = self.root / "must-not-exist"
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.install(prefix=prefix)
        self.assertFalse(prefix.exists())

    def test_existing_installation_is_never_replaced(self):
        prefix = self.root / "existing"
        prefix.mkdir()
        sentinel = prefix / self.names[0]
        sentinel.write_bytes(b"preserve-existing-user-file")
        with self.assertRaises(FileExistsError): self.install(prefix=prefix)
        self.assertEqual(sentinel.read_bytes(), b"preserve-existing-user-file")
        self.assertEqual(self.calls, [])

    def test_unsigned_checksum_duplicate_or_traversal_entries_are_rejected(self):
        for content in ("a" * 64 + "  ../outside\n", ("a" * 64 + "  same.tar.gz\n") * 2):
            self.manifest.write_text(content)
            with self.assertRaisesRegex(ValueError, "invalid or duplicate"):
                self.install()

    @unittest.skipIf(sys.platform == "win32", "tar member test; zip is packaged on Windows")
    def test_signed_archive_with_unexpected_path_or_link_is_rejected(self):
        for kind in ("path", "link"):
            with tarfile.open(self.archive, "w:gz") as archive:
                for name in [*self.names, "BUILD-INFO.txt"]:
                    member = tarfile.TarInfo("../outside" if name == self.names[0] and kind == "path" else name)
                    if name == self.names[0] and kind == "link":
                        member.type = tarfile.SYMTYPE
                        member.linkname = "/outside"
                        archive.addfile(member)
                    else:
                        member.size = 1
                        archive.addfile(member, io.BytesIO(b"x"))
            write_checksums(self.dist)
            with self.assertRaises(ValueError): self.install()

    def test_verification_only_does_not_install_or_execute_binaries(self):
        result = self.install()
        self.assertFalse(result["installed"])
        self.assertEqual(self.calls, ["SHA256SUMS", self.archive.name])

    def test_cli_never_bypasses_real_signature_verification(self):
        prefix = self.root / "not-installed"
        result = subprocess.run([sys.executable, str(ROOT/"tools/release/install.py"), "--archive", str(self.archive), "--checksums", str(self.manifest), "--version", "0.1.0", "--target", self.target, "--repository", "example/wam", "--prefix", str(prefix)], capture_output=True, text=True, timeout=50)
        self.assertEqual(result.returncode, 2)
        self.assertFalse(prefix.exists())
        self.assertIn("signature verification failed", result.stderr)

    def test_expected_signing_identity_is_not_read_from_the_downloaded_bundle(self):
        self.assertEqual(workflow_identity("example/wam", "refs/heads/main"), "https://github.com/example/wam/.github/workflows/release.yml@refs/heads/main")
        for repository, ref in [("../wam", "refs/heads/main"), ("example/wam", ".*"), ("example/wam", "refs/heads/../other")]:
            with self.assertRaises(ValueError): workflow_identity(repository, ref)


if __name__ == "__main__": unittest.main()
