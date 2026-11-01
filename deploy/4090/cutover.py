#!/usr/bin/env python3
"""Quiesce the previous API, restore its DB, and switch only WAM's direct DNS."""
import json
import os
import subprocess
import sys
import urllib.parse
from pathlib import Path

root = Path("build/deploy-4090")
previous_label = "gui/" + str(os.getuid()) + "/io.webagenticmemory.api"
previous_plist = Path.home() / "Library/LaunchAgents/io.webagenticmemory.api.plist"
source = json.loads((Path.home() / ".local/share/web-agentic-memory/config/api-env.json").read_text())
url = urllib.parse.urlparse(source["WAM_DATABASE_URL"])
environment = {**os.environ, "PGHOST": url.hostname or "127.0.0.1", "PGPORT": str(url.port or 5432),
               "PGUSER": urllib.parse.unquote(url.username or ""), "PGPASSWORD": urllib.parse.unquote(url.password or ""),
               "PGDATABASE": url.path.lstrip("/")}
previous_stopped = False
dns_attempted = False
# This migration is deliberately one-time. Refuse before touching the new API.
if (root / "production-final.dump").exists():
    raise SystemExit("Cutover already completed or attempted; use the recorded recovery procedure.")
subprocess.run(["launchctl", "print", previous_label], check=True, capture_output=True)

def remote(command):
    return subprocess.run(["ssh", "-o", "BatchMode=yes", "4090", command], check=True)

try:
    remote("sudo systemctl stop wam-api")
    subprocess.run(["launchctl", "bootout", previous_label], check=True, capture_output=True)
    previous_stopped = True
    snapshot = root / "production-final.dump"
    descriptor = os.open(snapshot, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        subprocess.run(["/opt/homebrew/bin/pg_dump", "--format=custom", "--no-owner", "--no-privileges"],
                       env=environment, stdout=output, check=True)
    subprocess.run(["scp", "-q", str(snapshot), "4090:/home/jason/build/wam-deploy-4090/production-final.dump"], check=True)
    remote("cat /home/jason/build/wam-deploy-4090/production-final.dump | sudo docker exec -i wam-postgres pg_restore --exit-on-error --clean --if-exists -U postgres --role=wam_app --no-owner --no-privileges --dbname=wam")
    remote("sudo systemctl start wam-api; for attempt in 1 2 3 4 5; do curl -fsS http://127.0.0.1:4090/healthz && exit 0; sleep 1; done; exit 1")
    dns_attempted = True
    subprocess.run([sys.executable, "deploy/4090/cloudflare-dns.py", "--apply", "--ip", "121.167.167.102"], check=True)
    print("Final database restored and both direct DNS A records applied.")
except Exception:
    if dns_attempted:
        subprocess.run([sys.executable, "deploy/4090/cloudflare-dns.py", "--restore", "--ip", "121.167.167.102"])
    if previous_stopped:
        subprocess.run(["launchctl", "bootstrap", "gui/" + str(os.getuid()), str(previous_plist)], capture_output=True)
    subprocess.run(["ssh", "-o", "BatchMode=yes", "4090", "sudo systemctl start wam-api"], capture_output=True)
    raise
