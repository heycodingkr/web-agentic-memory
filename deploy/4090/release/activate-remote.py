#!/usr/bin/env python3
"""Activate a verified release (API binary and static web) without touching private configuration.

Holds the shared deploy lock, refuses to run if another release became active, backs up the database, keeps the
previous release, switches `current`, restarts the API, checks it and the public site, and switches back on any failure.
Run as root: sudo -n python3 activate-remote.py <release>
"""
import fcntl, hashlib, json, os, re, shutil, subprocess, sys, time, urllib.request, urllib.error
from pathlib import Path

release = sys.argv[1]
assert re.fullmatch(r"[0-9]{8}T[0-9]{6}Z-main-[0-9a-f]{7,40}", release), "unexpected release id"
base = Path("/srv/web-agentic-memory")
work = base / "builds" / release
target = base / "releases" / release
expected = (work / "expected-previous.txt").read_text().strip()
info = json.loads((work / "release-info.json").read_text())

lock = open(base / ".deploy.lock", "a")
fcntl.flock(lock, fcntl.LOCK_EX)
previous = (base / "current").resolve()
assert str(previous) == expected, f"Active release changed ({previous}); do not overwrite another deployment."
assert not target.exists(), "Release already exists."
assert (work / "public/200.html").is_file() and (work / "bin/wam-server").is_file()
api_sha = hashlib.sha256((work / "bin/wam-server").read_bytes()).hexdigest()
assert api_sha == (work / "bin.sha256").read_text().split()[0], "binary changed after the verified build"

backup = Path("/var/backups/web-agentic-memory") / release
backup.mkdir(mode=0o700, parents=True, exist_ok=False)
with (backup / "production-before.dump").open("xb") as out:
    os.fchmod(out.fileno(), 0o600)
    subprocess.run(["docker", "exec", "wam-postgres", "pg_dump", "-U", "postgres", "-d", "wam", "-Fc"], stdout=out, check=True)
assert (backup / "production-before.dump").stat().st_size > 0

target.mkdir(mode=0o755)
(target / "bin").mkdir(mode=0o755)
shutil.copy2(work / "bin/wam-server", target / "bin/wam-server")
shutil.copytree(work / "public", target / "public")
# Content-addressed assets of the previous build stay available to pages that are already open during the switch.
shutil.copytree(previous / "public/_app/immutable", target / "public/_app/immutable", dirs_exist_ok=True)
shutil.copy2(work / "source-hashes.json", target / "source-hashes.json")
metadata = {
    "release": release, "previous": str(previous), "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "api_sha256": api_sha, "source_commit": info["source_commit"], "source_base": info["base"],
    "scope": info.get("scope", ""),
}
(target / "deployment.json").write_text(json.dumps(metadata, indent=2) + "\n")


def switch(path):
    temporary = base / ("current-" + release)
    if temporary.is_symlink():
        temporary.unlink()
    temporary.symlink_to(path, target_is_directory=True)
    temporary.replace(base / "current")


def api(path, host=None):
    request = urllib.request.Request("http://127.0.0.1:4090" + path, headers={} if host is None else {"Host": host})
    try:
        with urllib.request.urlopen(request, timeout=8) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


def site(path, host="webagenticmemory.com"):
    result = subprocess.run(
        ["curl", "-s", "--max-time", "15", "--resolve", f"{host}:443:127.0.0.1", "-w", "\n%{http_code}", f"https://{host}{path}"],
        capture_output=True, check=True)
    body, _, code = result.stdout.rpartition(b"\n")
    return int(code), body


def must(condition, message):
    if not condition:
        raise AssertionError(message)


try:
    must((base / "current").resolve() == previous, "Active release changed during staging.")
    switch(target)
    subprocess.run(["systemctl", "restart", "wam-api"], check=True)
    for attempt in range(40):
        try:
            if api("/healthz")[0] == 200:
                break
        except OSError:
            pass
        time.sleep(1)
    else:
        raise RuntimeError("API failed readiness")
    # API boundaries that must not have moved.
    must(api("/v1/admin/auth/session", "admin.webagenticmemory.com")[0] == 401, "admin session boundary")
    must(api("/v1/admin/contributions", "admin.webagenticmemory.com")[0] == 401, "admin list boundary")
    must(api("/v1/review/contributions", "webagenticmemory.com")[0] == 403, "review boundary")
    # Official authorship stays consistent: the official profile earns 20 points per authored version, so the total is
    # a multiple of 20 and at least 20 per authored memory (a refreshed example adds a version, not a memory).
    status, body = api("/v1/memories")
    must(status == 200, "catalog")
    memories = json.loads(body)["memories"]
    authored = [m for m in memories if m.get("author_handle") == "wam_official"]
    must(len(authored) >= 25, f"official memories: {len(authored)}")
    status, body = api("/v1/leaderboard")
    must(status == 200, "leaderboard")
    official = next(e for e in json.loads(body)["entries"] if e["handle"] == "wam_official")
    must(official["total_points"] % 20 == 0 and official["total_points"] >= 20 * len(authored),
         f"official points {official['total_points']} for {len(authored)} memories")
    status, body = api("/v1/profiles/wam_official")
    must(status == 200 and json.loads(body)["public_opt_in"], "official profile")
    # The static site through the real front door.
    for path, phrases in [
        ("/", ["Start with a browser your agent can use.", "Find memories for your website"]),
        ("/ko", ["에이전트가 쓸 브라우저부터", "사이트 URL로 메모리 찾기"]),
        ("/guide", ["browser-tools", "Download Arise Browser"]),
        ("/ko/guide", ["browser-tools", "Arise Browser 다운로드"]),
        ("/leaderboard", ["HOW POINTS ARE EARNED"]),
        ("/robots.txt", ["Sitemap: https://webagenticmemory.com/sitemap.xml"]),
        ("/sitemap.xml", ["https://webagenticmemory.com/"]),
        ("/llms.txt", ["Web Agentic Memory"]),
    ]:
        code, text = site(path)
        must(code == 200, f"{path} -> {code}")
        text = text.decode("utf-8", "replace")
        for phrase in phrases:
            must(phrase in text, f"{path} lacks {phrase!r}")
    must(site("/memories")[0] == 200, "/memories")
    metadata.update({"activated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                     "official_memories": len(authored), "official_points": official["total_points"]})
    (target / "deployment.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(json.dumps({"release": str(target), "previous": str(previous), "backup": str(backup),
                      "official_memories": len(authored), "official_points": official["total_points"], "api_ready": True}))
except BaseException:
    if (base / "current").resolve() == target:
        switch(previous)
        subprocess.run(["systemctl", "restart", "wam-api"], check=True)
    raise
