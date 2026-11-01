#!/usr/bin/env python3
"""Create/update only the administrator A record from the existing direct apex."""
import argparse
import base64
import ipaddress
import json
import re
import urllib.error
import urllib.request
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--apply", action="store_true")
parser.add_argument("--backup", type=Path, default=Path("build/deploy-admin/dns-before.json"))
args = parser.parse_args()
pem = (Path.home() / ".cloudflared/cert.pem").read_text()
match = re.search(r"-----BEGIN ARGO TUNNEL TOKEN-----\s*(.*?)\s*-----END ARGO TUNNEL TOKEN-----", pem, re.S)
if not match:
    raise SystemExit("Cloudflare certificate credential is unavailable.")
auth = json.loads(base64.b64decode("".join(match.group(1).split())))

def api(path, method="GET", data=None):
    request = urllib.request.Request("https://api.cloudflare.com/client/v4" + path,
        data=None if data is None else json.dumps(data).encode(),
        headers={"Authorization": "Bearer " + auth["apiToken"], "Content-Type": "application/json"},
        method=method)
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            value = json.load(response)
    except urllib.error.HTTPError as error:
        raise SystemExit("Cloudflare request failed with HTTP " + str(error.code)) from None
    if not value.get("success"):
        raise SystemExit("Cloudflare did not accept the request.")
    return value["result"]

base = "/zones/" + auth["zoneID"]
assert api(base)["name"] == "webagenticmemory.com", "Wrong DNS zone."
apex = [r for r in api(base + "/dns_records?name=webagenticmemory.com") if r["type"] in ("A","AAAA","CNAME")]
assert len(apex) == 1 and apex[0]["type"] == "A" and not apex[0]["proxied"], "Apex must use direct IPv4."
assert ipaddress.IPv4Address(apex[0]["content"]).is_global
name = "admin.webagenticmemory.com"
before = api(base + "/dns_records?name=" + name)
records = [r for r in before if r["type"] in ("A","AAAA","CNAME")]
assert len(records) <= 1, "Ambiguous administrator DNS records."
if records:
    assert records[0]["type"] == "A" and not records[0]["proxied"], "Refuse to overwrite another DNS mode."
if not args.backup.exists():
    args.backup.parent.mkdir(parents=True, exist_ok=True)
    with args.backup.open("x") as file:
        json.dump(before, file, indent=2)
    args.backup.chmod(0o600)
payload = {"type":"A","name":name,"content":apex[0]["content"],"ttl":60,"proxied":False,"comment":"WAM administrator direct origin"}
if args.apply and (not records or records[0]["content"] != apex[0]["content"]):
    if records:
        api(base + "/dns_records/" + records[0]["id"], "PATCH", payload)
    else:
        api(base + "/dns_records", "POST", payload)
print(json.dumps({"name":name,"address":apex[0]["content"],"applied":args.apply}))
