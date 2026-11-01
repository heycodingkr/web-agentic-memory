#!/usr/bin/env python3
"""Install only the marked WAM hosts, preserving and backing up shared ingress."""
import hashlib
import json
import os
import subprocess
import time
from pathlib import Path

path = Path("/etc/caddy/Caddyfile")
original = path.read_text()
begin, end = "# BEGIN WAM DIRECT 4090", "# END WAM DIRECT 4090"
fragment = Path("/srv/web-agentic-memory/ops/Caddyfile.fragment").read_text()
if begin in original:
    if original.count(begin) != 1 or original.count(end) != 1:
        raise SystemExit("Ambiguous WAM config markers.")
    prefix, rest = original.split(begin, 1)
    _, suffix = rest.split(end, 1)
    desired = prefix + begin + "\n" + fragment + "\n" + end + suffix
else:
    if "webagenticmemory.com {" in original:
        raise SystemExit("Unmarked WAM site exists; inspect before replacing it.")
    desired = original.rstrip() + "\n\n" + begin + "\n" + fragment + "\n" + end + "\n"
backup = path.with_name("Caddyfile.pre-wam-" + time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()))
with backup.open("xb") as file:
    file.write(path.read_bytes())
backup.chmod(0o640)
candidate = path.with_name("Caddyfile.wam-candidate")
candidate.write_text(desired)
candidate.chmod(0o640)
auth = json.loads(Path("/etc/web-agentic-memory/cloudflare.json").read_text())
environment = {**os.environ, "WAM_DNS_API_TOKEN": auth["apiToken"]}
caddy = "/usr/local/bin/caddy-front"
subprocess.run([caddy, "fmt", "--overwrite", str(candidate)], check=True, stdout=subprocess.DEVNULL)
subprocess.run([caddy, "validate", "--config", str(candidate), "--adapter", "caddyfile"], check=True, stdout=subprocess.DEVNULL, env=environment)
if hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256(original.encode()).digest():
    raise SystemExit("Shared Caddyfile changed; no replacement applied.")
credential = Path("/etc/web-agentic-memory/caddy.env")
credential.write_text("WAM_DNS_API_TOKEN=" + json.dumps(auth["apiToken"]) + "\n")
credential.chmod(0o600)
override = Path("/etc/systemd/system/caddy-front.service.d/wam-dns.conf")
override.parent.mkdir(parents=True, exist_ok=True)
override.write_text("[Service]\nEnvironmentFile=/etc/web-agentic-memory/caddy.env\n")
subprocess.run(["systemctl", "daemon-reload"], check=True)
candidate.replace(path)
subprocess.run(["chown", "root:caddy", str(path)], check=True)
try:
    subprocess.run([caddy, "reload", "--config", str(path), "--adapter", "caddyfile"], check=True, env=environment)
except subprocess.CalledProcessError:
    path.write_text(original)
    subprocess.run(["chown", "root:caddy", str(path)], check=True)
    path.chmod(0o640)
    raise SystemExit("Caddy rejected reload; original config restored on disk.") from None
print(json.dumps({"backup": str(backup), "added_hosts": ["webagenticmemory.com", "api.webagenticmemory.com", "admin.webagenticmemory.com"]}))
