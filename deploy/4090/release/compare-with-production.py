#!/usr/bin/env python3
"""Before deploying a commit, find what runs in production that this commit does not contain.

Production releases record `source-hashes.json` (path -> sha256 of the source files they were built from). Releases have
been built from working-tree snapshots, so a live file can exist in no commit at all; deploying a clean commit over it
would silently remove that behaviour (this happened with the official-authorship change of 20261101T123749Z).

    ./compare-with-production.py <commit> [ssh-host]      (run inside the repository)

Prints, for every file whose production version differs from <commit>: whether the production version is an older or
other version that some commit in the repository history contains (safe: the commit is newer), or whether it exists in no
commit (resolve it before deploying: commit it, or merge it into the tree that will be built). Exit status 1 when any
file exists only in production.
"""
import hashlib, json, subprocess, sys

commit = sys.argv[1]
host = sys.argv[2] if len(sys.argv) > 2 else "4090"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(*args, **kw):
    return subprocess.run(args, capture_output=True, **kw)


current = run("ssh", "-o", "BatchMode=yes", host, "readlink -f /srv/web-agentic-memory/current").stdout.decode().strip()
manifest = json.loads(run("ssh", "-o", "BatchMode=yes", host, f"cat {current}/source-hashes.json").stdout)
print(f"production release: {current} ({len(manifest)} source files)")


def at(ref, path):
    result = run("git", "show", f"{ref}:{path}")
    return sha(result.stdout) if result.returncode == 0 else None


def in_history(path, target):
    seen = set()
    for rev in run("git", "log", "--all", "--format=%H", "--", path).stdout.decode().split():
        blob = run("git", "rev-parse", f"{rev}:{path}")
        if blob.returncode:
            continue
        blob_id = blob.stdout.decode().strip()
        if blob_id in seen:
            continue
        seen.add(blob_id)
        if sha(run("git", "cat-file", "blob", blob_id).stdout) == target:
            return rev[:7]
    return None


same, older, only = 0, [], []
for path, digest in sorted(manifest.items()):
    if at(commit, path) == digest:
        same += 1
    elif (rev := in_history(path, digest)):
        older.append((path, rev))
    else:
        only.append(path)
print(f"identical in {commit}: {same}")
print(f"production has an older or other committed version (the commit is newer): {len(older)}")
for path, rev in older:
    print(f"   {path}  (production = {rev})")
print(f"ONLY IN PRODUCTION, in no commit: {len(only)}")
for path in only:
    print(f"   {path}")
sys.exit(1 if only else 0)
