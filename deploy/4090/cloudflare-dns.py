#!/usr/bin/env python3
"""Move only the WAM apex/API address records to a direct 4090 origin."""
import argparse
import base64
import ipaddress
import json
import re
import urllib.error
import urllib.request
from pathlib import Path

DOMAIN = "webagenticmemory.com"
NAMES = [DOMAIN, "api." + DOMAIN]
parser = argparse.ArgumentParser()
action = parser.add_mutually_exclusive_group()
action.add_argument("--apply", action="store_true")
action.add_argument("--restore", action="store_true")
parser.add_argument("--ip", required=True)
parser.add_argument("--backup", type=Path, default=Path("build/deploy-4090/dns-before.json"))
args = parser.parse_args()
address = ipaddress.IPv4Address(args.ip)
if not address.is_global:
    raise SystemExit("The direct origin must be a public IPv4 address.")
pem = (Path.home() / ".cloudflared/cert.pem").read_text()
match = re.search(r"-----BEGIN ARGO TUNNEL TOKEN-----\s*(.*?)\s*-----END ARGO TUNNEL TOKEN-----", pem, re.S)
auth = json.loads(base64.b64decode("".join(match.group(1).split())))

def api(path, method="GET", data=None):
    request = urllib.request.Request(
        "https://api.cloudflare.com/client/v4" + path,
        data=None if data is None else json.dumps(data).encode(),
        headers={"Authorization": "Bearer " + auth["apiToken"], "Content-Type": "application/json"},
        method=method,
    )
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            value = json.load(response)
    except urllib.error.HTTPError as error:
        raise SystemExit("Cloudflare request failed with HTTP " + str(error.code)) from None
    if not value.get("success"):
        raise SystemExit("Cloudflare API did not accept the operation.")
    return value["result"]

base = "/zones/" + auth["zoneID"]
assert api(base)["name"] == DOMAIN, "Credential belongs to a different zone."
before = {name: api(base + "/dns_records?name=" + name) for name in NAMES}
if not args.backup.exists():
    args.backup.parent.mkdir(parents=True, exist_ok=True)
    with args.backup.open("x") as file:
        json.dump(before, file, indent=2)
    args.backup.chmod(0o600)
mail_before = {name: [r for r in records if r["type"] in ("MX", "TXT", "CAA")] for name, records in before.items()}
if args.restore:
    saved = json.loads(args.backup.read_text())
    for name in NAMES:
        previous = [r for r in saved[name] if r["type"] in ("A", "AAAA", "CNAME")]
        current = [r for r in before[name] if r["type"] in ("A", "AAAA", "CNAME")]
        if len(previous) != 1 or len(current) != 1:
            raise SystemExit("Ambiguous rollback records for " + name)
        record = previous[0]
        payload = {key: record[key] for key in ("type", "name", "content", "proxied", "ttl")}
        payload["comment"] = record.get("comment") or ""
        api(base + "/dns_records/" + current[0]["id"], "PATCH", payload)
if args.apply:
    for name in NAMES:
        records = [r for r in before[name] if r["type"] in ("A", "AAAA", "CNAME")]
        if len(records) != 1:
            raise SystemExit("Expected exactly one existing address record for " + name)
        result = api(base + "/dns_records/" + records[0]["id"], "PATCH", {
            "type": "A", "name": name, "content": str(address), "proxied": False, "ttl": 60,
            "comment": "Direct 4090 origin",
        })
        assert result["type"] == "A" and result["content"] == str(address) and not result["proxied"]
after = {name: api(base + "/dns_records?name=" + name) for name in NAMES}
mail_after = {name: [r for r in records if r["type"] in ("MX", "TXT", "CAA")] for name, records in after.items()}
assert mail_before == mail_after, "Unexpected mail-record change."
for name, records in after.items():
    print(json.dumps({"name": name, "records": [
        {key: record.get(key) for key in ("type", "content", "proxied", "ttl")}
        for record in records if record["type"] in ("A", "AAAA", "CNAME")
    ]}))
print("Existing apex mail/TXT/CAA records preserved.")
