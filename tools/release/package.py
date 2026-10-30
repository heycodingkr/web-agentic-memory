#!/usr/bin/env python3
"""Package the WAM CLI/MCP binaries for a tagged, review-only release draft."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tarfile
from io import BytesIO
from pathlib import Path
from zipfile import BadZipFile, ZIP_DEFLATED, ZipFile


VERSION = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
TAG = re.compile(r"^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
TARGET = re.compile(r"^[a-z0-9_]+(?:-[a-z0-9_]+)+$")
SOURCE_COMMIT = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
RELEASE_PACKAGES = {"wam-cli", "wam-mcp"}
RELEASE_TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
)


def version_from_metadata(metadata: dict) -> str:
    packages = [item for item in metadata.get("packages", []) if item.get("name") in RELEASE_PACKAGES]
    versions: dict[str, set[str]] = {name: set() for name in RELEASE_PACKAGES}
    for package in packages:
        versions[package["name"]].add(package.get("version", ""))
    if any(len(found) != 1 for found in versions.values()):
        raise ValueError("wam-cli and wam-mcp must each have one workspace version")
    cli_version = next(iter(versions["wam-cli"]))
    mcp_version = next(iter(versions["wam-mcp"]))
    if cli_version != mcp_version:
        raise ValueError("wam-cli and wam-mcp must use the same version")
    if not VERSION.fullmatch(cli_version):
        raise ValueError("workspace release version is not valid SemVer")
    return cli_version


def release_version(tag: str, metadata: dict) -> str:
    if not TAG.fullmatch(tag):
        raise ValueError("release tag must use vMAJOR.MINOR.PATCH SemVer format")
    version = version_from_metadata(metadata)
    if tag != f"v{version}":
        raise ValueError(f"tag {tag} must match both binary package versions (v{version})")
    return version


def _binary_names(target: str) -> tuple[str, str]:
    if not TARGET.fullmatch(target):
        raise ValueError("target triple contains unsafe characters")
    extension = ".exe" if "windows" in target else ""
    return f"wam{extension}", f"wam-mcp{extension}"


def package_artifact(
    binary_dir: str | Path,
    output_dir: str | Path,
    version: str,
    target: str,
    source_commit: str,
) -> Path:
    if not VERSION.fullmatch(version):
        raise ValueError("version must be valid SemVer")
    if not SOURCE_COMMIT.fullmatch(source_commit):
        raise ValueError("source commit must be a full lowercase Git hash")
    cli_name, mcp_name = _binary_names(target)
    binary_root = Path(binary_dir)
    binaries = [binary_root / cli_name, binary_root / mcp_name]
    for binary in binaries:
        if binary.is_symlink() or not binary.is_file():
            raise ValueError(f"missing release binary: {binary.name}")

    output_root = Path(output_dir)
    output_root.mkdir(parents=True, exist_ok=True)
    extension = ".zip" if "windows" in target else ".tar.gz"
    archive = output_root / f"wam-{version}-{target}{extension}"
    if archive.exists():
        raise FileExistsError(f"release archive already exists: {archive.name}")
    build_info = f"version={version}\ntarget={target}\nsource_commit={source_commit}\n"
    temporary = output_root / f".{archive.name}.tmp"
    if temporary.exists() or temporary.is_symlink():
        raise FileExistsError(f"temporary release archive already exists: {temporary.name}")
    try:
        if extension == ".zip":
            with ZipFile(temporary, "w", compression=ZIP_DEFLATED) as zipped:
                for binary in binaries:
                    zipped.write(binary, arcname=binary.name)
                zipped.writestr("BUILD-INFO.txt", build_info)
        else:
            with tarfile.open(temporary, "w:gz", format=tarfile.PAX_FORMAT) as tar:
                for binary in binaries:
                    tar.add(binary, arcname=binary.name)
                info = tarfile.TarInfo("BUILD-INFO.txt")
                encoded = build_info.encode("utf-8")
                info.size = len(encoded)
                info.mode = 0o644
                tar.addfile(info, fileobj=BytesIO(encoded))
        os.replace(temporary, archive)
    except BaseException:
        temporary.unlink(missing_ok=True)
        raise
    return archive


def write_checksums(directory: str | Path) -> Path:
    root = Path(directory)
    archives = sorted(
        path for path in root.iterdir()
        if path.is_file() and not path.is_symlink() and (path.name.endswith(".tar.gz") or path.suffix == ".zip")
    )
    if not archives:
        raise ValueError("no release archives found for checksum manifest")
    lines = [f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}" for path in archives]
    target = root / "SHA256SUMS"
    temporary = root / ".SHA256SUMS.tmp"
    if temporary.exists() or temporary.is_symlink():
        raise FileExistsError(f"temporary checksum file already exists: {temporary.name}")
    temporary.write_text("\n".join(lines) + "\n", encoding="ascii")
    os.replace(temporary, target)
    return target


def _read_build_info(archive: Path) -> dict[str, str]:
    try:
        if archive.suffix == ".zip":
            with ZipFile(archive) as zipped:
                item = zipped.getinfo("BUILD-INFO.txt")
                if item.file_size > 1024:
                    raise ValueError(f"invalid build information in {archive.name}")
                payload = zipped.read(item)
        else:
            with tarfile.open(archive, "r:gz") as tar:
                item = tar.getmember("BUILD-INFO.txt")
                if item.size > 1024:
                    raise ValueError(f"invalid build information in {archive.name}")
                stream = tar.extractfile(item)
                if stream is None:
                    raise ValueError(f"missing build information in {archive.name}")
                payload = stream.read(1025)
    except (BadZipFile, KeyError, tarfile.TarError, OSError) as exc:
        raise ValueError(f"invalid release archive: {archive.name}") from exc
    fields: dict[str, str] = {}
    for line in payload.decode("ascii").splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key or key in fields:
            raise ValueError(f"invalid build information in {archive.name}")
        fields[key] = value
    return fields


def verify_artifacts(directory: str | Path, tag: str, source_commit: str) -> None:
    """Check downloaded release archives against the requested tag and source commit."""
    if not TAG.fullmatch(tag):
        raise ValueError("release tag must use vMAJOR.MINOR.PATCH SemVer format")
    if not SOURCE_COMMIT.fullmatch(source_commit):
        raise ValueError("source commit must be a full lowercase Git hash")
    version = tag[1:]
    root = Path(directory)
    expected = {
        f"wam-{version}-{target}{'.zip' if 'windows' in target else '.tar.gz'}": target
        for target in RELEASE_TARGETS
    }
    entries = list(root.iterdir())
    if any(path.is_symlink() or not path.is_file() or path.name not in expected for path in entries):
        raise ValueError("release artifacts contain an unexpected file or directory")
    archives = {path.name: path for path in entries}
    if set(archives) != set(expected):
        raise ValueError("release artifacts must contain exactly one archive for each supported target")
    for name, target in expected.items():
        archive = archives[name]
        if archive.is_symlink():
            raise ValueError(f"release archive must not be a symlink: {name}")
        fields = _read_build_info(archive)
        if fields != {"version": version, "target": target, "source_commit": source_commit}:
            raise ValueError(f"release archive metadata does not match the requested source: {name}")


def _metadata(root: Path) -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def _smoke(binary_dir: Path) -> None:
    cli_name, _ = _binary_names("x86_64-pc-windows-msvc" if os.name == "nt" else "x86_64-unknown-linux-gnu")
    completed = subprocess.run(
        [str(binary_dir / cli_name), "--help"],
        check=False,
        capture_output=True,
        text=True,
        timeout=20,
    )
    if completed.returncode != 0:
        raise RuntimeError("packaged CLI failed its local --help smoke check")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("build", help="package already-built release binaries")
    build.add_argument("--tag", required=True)
    build.add_argument("--target", required=True)
    build.add_argument("--source-commit", required=True)
    build.add_argument("--binary-dir", type=Path, required=True)
    build.add_argument("--output-dir", type=Path, required=True)
    build.add_argument("--workspace", type=Path, default=Path.cwd())
    build.add_argument("--smoke", action="store_true", help="run wam --help before packaging")
    checksums = commands.add_parser("checksums", help="write SHA256SUMS for packaged archives")
    checksums.add_argument("--directory", type=Path, required=True)
    verify = commands.add_parser("verify", help="check release archives against their tag and source commit")
    verify.add_argument("--directory", type=Path, required=True)
    verify.add_argument("--tag", required=True)
    verify.add_argument("--source-commit", required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "checksums":
            print(write_checksums(args.directory))
            return 0
        if args.command == "verify":
            verify_artifacts(args.directory, args.tag, args.source_commit)
            print("release archives verified")
            return 0
        version = release_version(args.tag, _metadata(args.workspace))
        if args.smoke:
            _smoke(args.binary_dir)
        archive = package_artifact(
            args.binary_dir, args.output_dir, version, args.target, args.source_commit
        )
        print(f"packaged {archive.name}")
        return 0
    except (OSError, ValueError, subprocess.SubprocessError, RuntimeError) as exc:
        print(f"release packaging failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
