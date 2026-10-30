#!/usr/bin/env python3
"""Verify a downloaded keyless-signed WAM release and install into a new directory."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
from zipfile import BadZipFile, ZipFile

try:
    from .package import RELEASE_TARGETS, SOURCE_COMMIT, VERSION
except ImportError:
    from package import RELEASE_TARGETS, SOURCE_COMMIT, VERSION

MAX_BINARY = 256 * 1024 * 1024


def host_target() -> str:
    machine = platform.machine().lower()
    if sys.platform == "darwin":
        return "aarch64-apple-darwin" if machine in {"arm64", "aarch64"} else "x86_64-apple-darwin"
    if sys.platform == "win32" and machine in {"amd64", "x86_64"}:
        return "x86_64-pc-windows-msvc"
    if sys.platform.startswith("linux") and machine in {"amd64", "x86_64"}:
        return "x86_64-unknown-linux-gnu"
    if sys.platform.startswith("linux") and machine in {"arm64", "aarch64"}:
        return "aarch64-unknown-linux-gnu"
    raise ValueError("no verified release target for this OS and architecture")


def workflow_identity(repository: str, ref: str) -> str:
    if not re.fullmatch(r"[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+", repository) or repository.split("/")[1] in {".", ".."}:
        raise ValueError("repository must be owner/name")
    if not re.fullmatch(r"refs/heads/[A-Za-z0-9_./-]+", ref) or ".." in ref:
        raise ValueError("expected workflow ref must be a branch reference")
    return f"https://github.com/{repository}/.github/workflows/release.yml@{ref}"


def verify_signature(asset: Path, bundle: Path, identity: str, cosign: str) -> None:
    try:
        subprocess.run([cosign, "verify-blob", str(asset), "--bundle", str(bundle),
                        "--certificate-identity", identity,
                        "--certificate-oidc-issuer", "https://token.actions.githubusercontent.com"],
                       stdin=subprocess.DEVNULL, capture_output=True, check=True, timeout=45)
    except (OSError, subprocess.SubprocessError) as error:
        raise ValueError("signature verification failed; install Cosign and check the expected repository/workflow identity") from error


def snapshot(source: Path, root: Path, limit: int) -> Path:
    if source.is_symlink() or not source.is_file() or source.stat().st_size > limit:
        raise ValueError("release input must be a bounded regular file")
    destination = root / source.name
    with source.open("rb") as reader, destination.open("xb") as writer:
        copied = 0
        while chunk := reader.read(64 * 1024):
            copied += len(chunk)
            if copied > limit:
                raise ValueError("release input exceeds its size limit")
            writer.write(chunk)
    return destination


def checksum_for(manifest: Path, name: str) -> str:
    rows = manifest.read_text(encoding="ascii").splitlines()
    if len(rows) > 32:
        raise ValueError("checksum manifest has too many entries")
    entries = {}
    for row in rows:
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_.+-]+)", row)
        if not match or match[2] in entries:
            raise ValueError("checksum manifest has an invalid or duplicate entry")
        entries[match[2]] = match[1]
    if name not in entries:
        raise ValueError("archive is missing from the signed checksum manifest")
    return entries[name]


def unpack_verified(archive: Path, root: Path, version: str, target: str) -> dict:
    suffix = ".exe" if "windows" in target else ""
    names = {"wam" + suffix, "wam-mcp" + suffix, "BUILD-INFO.txt"}

    def copy(name, size, reader):
        maximum = 1024 if name == "BUILD-INFO.txt" else MAX_BINARY
        if size < 1 or size > maximum:
            raise ValueError("release member has invalid size")
        with reader, (root / name).open("xb") as writer:
            remaining = size
            while remaining:
                chunk = reader.read(min(64 * 1024, remaining))
                if not chunk:
                    raise ValueError("release member is truncated")
                writer.write(chunk)
                remaining -= len(chunk)

    if "windows" in target:
        with ZipFile(archive) as zipped:
            members = zipped.infolist()
            if len(members) != 3 or {member.filename for member in members} != names:
                raise ValueError("unexpected or duplicate release member")
            for member in members:
                mode = member.external_attr >> 16
                if member.is_dir() or stat.S_ISLNK(mode) or member.flag_bits & 1:
                    raise ValueError("release member must be a plain file")
                copy(member.filename, member.file_size, zipped.open(member))
    else:
        with tarfile.open(archive, "r|gz") as tar:
            seen = set()
            for member in tar:
                if member.name not in names or member.name in seen:
                    raise ValueError("unexpected or duplicate release member")
                seen.add(member.name)
                if not member.isfile() or member.issym() or member.islnk():
                    raise ValueError("release member must be a plain file")
                reader = tar.extractfile(member)
                if reader is None:
                    raise ValueError("release member cannot be read")
                copy(member.name, member.size, reader)
            if seen != names:
                raise ValueError("missing release member")
    lines = (root / "BUILD-INFO.txt").read_text(encoding="ascii").splitlines()
    metadata = {}
    for line in lines:
        key, separator, value = line.partition("=")
        if not separator or key in metadata:
            raise ValueError("invalid embedded build metadata")
        metadata[key] = value
    if set(metadata) != {"version", "target", "source_commit"} or metadata["version"] != version or metadata["target"] != target or not SOURCE_COMMIT.fullmatch(metadata["source_commit"]):
        raise ValueError("embedded build metadata does not match the requested release")
    return metadata


def install_release(archive: Path, checksums: Path, version: str, target: str, repository: str,
                    ref: str = "refs/heads/main", prefix: Path | None = None, cosign: str = "cosign",
                    *, verifier=verify_signature) -> dict:
    if not VERSION.fullmatch(version) or target not in RELEASE_TARGETS:
        raise ValueError("invalid release version or target")
    identity = workflow_identity(repository, ref)
    extension = ".zip" if "windows" in target else ".tar.gz"
    if archive.name != f"wam-{version}-{target}{extension}" or checksums.name != "SHA256SUMS":
        raise ValueError("archive or checksum filename does not match the requested release")
    if prefix is not None:
        if target != host_target():
            raise ValueError("installation target does not match this host")
        if prefix.exists() or prefix.is_symlink():
            raise FileExistsError("install directory already exists; existing files are never replaced")
    with tempfile.TemporaryDirectory(prefix="wam-release-verify-") as temporary:
        staging = Path(temporary)
        asset = snapshot(archive, staging, MAX_BINARY * 2)
        manifest = snapshot(checksums, staging, 64 * 1024)
        asset_bundle = snapshot(Path(str(archive) + ".sigstore.json"), staging, 4 * 1024 * 1024)
        manifest_bundle = snapshot(Path(str(checksums) + ".sigstore.json"), staging, 4 * 1024 * 1024)
        verifier(manifest, manifest_bundle, identity, cosign)
        verifier(asset, asset_bundle, identity, cosign)
        with asset.open("rb") as reader:
            digest = hashlib.file_digest(reader, "sha256").hexdigest()
        if digest != checksum_for(manifest, asset.name):
            raise ValueError("archive checksum does not match the signed manifest")
        unpacked = staging / "unpacked"
        unpacked.mkdir()
        metadata = unpack_verified(asset, unpacked, version, target)
        report = {**metadata, "signature_identity": identity, "installed": False}
        if prefix is not None:
            prefix.mkdir(parents=True, exist_ok=False)
            created = []
            try:
                for source in unpacked.iterdir():
                    destination = prefix / source.name
                    with source.open("rb") as reader, destination.open("xb") as writer:
                        created.append(destination)
                        shutil.copyfileobj(reader, writer)
                    destination.chmod(0o644 if source.name == "BUILD-INFO.txt" else 0o755)
                report["installed"] = True
                receipt = prefix / ".wam-install.json"
                with receipt.open("x", encoding="utf-8") as writer:
                    created.append(receipt)
                    json.dump(report, writer, indent=2)
                    writer.write("\n")
            except BaseException:
                for path in created:
                    path.unlink(missing_ok=True)
                try: prefix.rmdir()
                except OSError: pass
                raise
        return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", default=None)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--workflow-ref", default="refs/heads/main")
    parser.add_argument("--prefix", type=Path, help="Install into this new directory; omit to verify only")
    args = parser.parse_args()
    try:
        report = install_release(args.archive, args.checksums, args.version, args.target or host_target(), args.repository, args.workflow_ref, args.prefix)
        print(json.dumps(report))
        return 0
    except (OSError, ValueError, tarfile.TarError, BadZipFile, UnicodeError) as error:
        print(f"release verification/installation failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
