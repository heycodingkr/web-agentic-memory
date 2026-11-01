#!/usr/bin/env bash
# Package a pinned commit for the host: the source for the Linux build, the production web build and the hash manifest.
#   ./package.sh <commit> <out-dir>        (run inside a clean checkout of <commit>; nothing is uploaded)
set -euo pipefail
commit="$(git rev-parse "$1")"; out="$2"
[ -z "$(git status --porcelain)" ] || { echo "the checkout is not clean: deploy a commit, not a working tree" >&2; exit 1; }
[ "$(git rev-parse HEAD)" = "$commit" ] || { echo "check out $commit first" >&2; exit 1; }
mkdir -p "$out"
release="$(date -u +%Y%m%dT%H%M%SZ)-main-$(git rev-parse --short=7 "$commit")"
echo "$release" > "$out/release-id.txt"
git archive --format=tar.gz -o "$out/source.tar.gz" "$commit" Cargo.lock Cargo.toml crates docs tests guides
pnpm --dir web install --frozen-lockfile
VITE_SITE_URL=https://webagenticmemory.com VITE_WAM_MCP_ENDPOINT=https://webagenticmemory.com/mcp pnpm --dir web run build
test -f web/build/200.html
tar czf "$out/web-public.tar.gz" -C web/build .
python3 - "$out" "$commit" "$release" <<'PY'
import hashlib, json, os, subprocess, sys
out, commit, release = sys.argv[1:4]
names = subprocess.run(["git", "ls-files", "-z", "Cargo.lock", "Cargo.toml", "crates", "docs", "web", "guides", "tests"],
                       capture_output=True).stdout.split(b"\0")
manifest = {n.decode(): hashlib.sha256(open(n, "rb").read()).hexdigest() for n in names if n and os.path.isfile(n)}
json.dump(manifest, open(f"{out}/source-hashes.json", "w"), indent=0, sort_keys=True)
json.dump({"release": release, "source_commit": commit, "base": f"commit {commit[:7]}"}, open(f"{out}/release-info.json", "w"), indent=2)
print(release, len(manifest), "files")
PY
here="$(cd "$(dirname "$0")" && pwd)"
cp "$here/build-remote.sh" "$here/activate-remote.py" "$out"/
