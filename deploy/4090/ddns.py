#!/usr/bin/env python3
"""Maintain only the three direct WAM A records when the 4090 public IPv4 changes."""
import ipaddress
import json
import os
import urllib.request
from pathlib import Path

credential_directory = os.environ.get("CREDENTIALS_DIRECTORY")
path = Path(credential_directory) / "cloudflare" if credential_directory else Path("/etc/web-agentic-memory/cloudflare.json")
auth = json.loads(path.read_text())
request = urllib.request.Request("https://1.1.1.1/cdn-cgi/trace", headers={"User-Agent": "WAM-direct-DDNS/1"})
with urllib.request.urlopen(request, timeout=15) as response:
    address = dict(line.split("=", 1) for line in response.read().decode().splitlines() if "=" in line)["ip"]
if not ipaddress.IPv4Address(address).is_global:
    raise SystemExit("Public IPv4 check failed.")

def api(path, payload=None):
    request = urllib.request.Request(
        "https://api.cloudflare.com/client/v4" + path,
        data=None if payload is None else json.dumps(payload).encode(),
        method="GET" if payload is None else "PATCH",
        headers={"Authorization": "Bearer " + auth["apiToken"], "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=20) as response:
        value = json.load(response)
    if not value.get("success"):
        raise RuntimeError("Cloudflare DDNS request was rejected.")
    return value["result"]

base = "/zones/" + auth["zoneID"]
assert api(base)["name"] == "webagenticmemory.com"
for name in ("webagenticmemory.com", "api.webagenticmemory.com", "admin.webagenticmemory.com"):
    records = [r for r in api(base + "/dns_records?name=" + name) if r["type"] in ("A", "AAAA", "CNAME")]
    if len(records) != 1 or records[0]["type"] != "A" or records[0]["proxied"]:
        raise RuntimeError("Direct DNS mode changed; DDNS refuses to overwrite it.")
    record = records[0]
    if record["content"] != address:
        api(base + "/dns_records/" + record["id"], {"content": address, "ttl": 60})
        print(name + ": direct origin address updated.")
    else:
        print(name + ": direct origin address unchanged.")
