#!/usr/bin/env python3
"""Initialize only the dedicated WAM database and a non-superuser app role."""
import json
import subprocess
from pathlib import Path

settings = json.loads(Path("/etc/web-agentic-memory/deployment-private.json").read_text())
password = settings["app_password"].replace("'", "''")
sql = "CREATE ROLE wam_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD '" + password + "';\nCREATE DATABASE wam OWNER wam_app;\n"
result = subprocess.run(
    ["docker", "exec", "-i", "wam-postgres", "psql", "-X", "-v", "ON_ERROR_STOP=1", "-U", "postgres"],
    input=sql, text=True, capture_output=True,
)
if result.returncode:
    raise SystemExit("Dedicated database initialization failed; inspect database state privately.")
print("Dedicated WAM database and non-superuser app role initialized.")
