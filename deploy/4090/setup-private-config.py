#!/usr/bin/env python3
"""Convert private deployment JSON into root-owned runtime configuration."""
import json
import os
from pathlib import Path

root = Path("/etc/web-agentic-memory")
settings = json.loads((root / "deployment-private.json").read_text())
environment = json.loads((root / "api-env.json").read_text())
environment.setdefault("WAM_DIAGNOSTICS_EMAIL_TO", "easycodingkr@gmail.com")
files = {
    "postgres.env": "POSTGRES_USER=postgres\nPOSTGRES_DB=postgres\nPOSTGRES_PASSWORD=" + settings["postgres_password"] + "\n",
    "redis.conf": "bind 0.0.0.0\nprotected-mode yes\nport 6379\ndir /data\nappendonly yes\nrequirepass " + settings["redis_password"] + "\n",
    "api.env": "".join(key + "=" + json.dumps(str(value), ensure_ascii=False) + "\n" for key, value in environment.items()),
}
for name, value in files.items():
    path = root / name
    path.write_text(value)
    path.chmod(0o644 if name == "redis.conf" else 0o600)
    os.chown(path, 0, 0)
for path in root.glob("*.json"):
    path.chmod(0o600)
    os.chown(path, 0, 0)
root.chmod(0o700)
os.chown(root, 0, 0)
print("Private runtime configuration installed; values omitted.")
