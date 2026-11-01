#!/usr/bin/env bash
# Applies the revised built-in example memories to production by starting the API once with WAM_EXAMPLES=refresh.
#   sudo -n bash refresh-examples.sh dry-run   restores a copy of the production database into a disposable PostgreSQL,
#                                              runs the same binary against it and shows what would change. Production
#                                              is only read.
#   sudo -n bash refresh-examples.sh apply     backs up the database, then restarts the API once with the setting.
# The refresh appends a version only to untouched built-in rows whose bundled text changed; rows someone else authored,
# reviewed or withdrew are protected, and the official author gets +20 per new version (docs/spec/api.md, #127, #205).
set -euo pipefail
mode="${1:?usage: refresh-examples.sh dry-run|apply}"
base=/srv/web-agentic-memory
bin="$base/current/bin/wam-server"
seeds="$base/builds/$(basename "$(readlink -f "$base/current")")/source/crates/wam-server/seed"

catalog_report() {  # <api base url>: the catalog compared with the bundled text of the same release, as a table
  python3 - "$1" "$seeds" <<'PY'
import json, sys, urllib.request
api, seeds = sys.argv[1:3]
catalog = {m["memory_id"]: m for m in json.load(urllib.request.urlopen(api + "/v1/memories", timeout=20))["memories"]}
bundled = {}
for name in ("example_memories.json", "productivity_memories.json"):
    data = json.load(open(f"{seeds}/{name}"))
    for entry in data if isinstance(data, list) else data.get("examples") or data.get("memories"):
        bundled[entry["id"]] = entry["payload"]["l1"]
stale = [i for i, l1 in bundled.items() if i in catalog and (catalog[i]["summary"] != l1["summary"] or catalog[i]["title"] != l1["title"])]
board = json.load(urllib.request.urlopen(api + "/v1/leaderboard", timeout=20))["entries"]
print(f"  catalog {len(catalog)} memories, {len(bundled)} bundled, official points {[e['total_points'] for e in board]}")
print(f"  {len(stale)} differ from the bundled text:")
for i in stale:
    print(f"    {i}  {catalog[i]['version_id'][:16]}")
print(json.dumps({i: catalog[i]["version_id"] for i in catalog}), file=open("/tmp/wam-refresh-versions-" + api.rsplit(":", 1)[1], "w"))
PY
}

wait_http() {  # <url> <tries>
  for _ in $(seq 1 "$2"); do curl -fsS -m 3 -o /dev/null "$1" 2>/dev/null && return 0; sleep 1; done
  return 1
}

if [ "$mode" = "dry-run" ]; then
  db="wam-refresh-dryrun-$$"; log="$(mktemp)"; pid=""
  cleanup() { [ -z "$pid" ] || kill "$pid" 2>/dev/null || true; docker rm -f "$db" >/dev/null 2>&1 || true; rm -f "$log"; }
  trap cleanup EXIT
  docker run -d --name "$db" -e POSTGRES_PASSWORD=dryrun -p 127.0.0.1::5432 postgres:16 >/dev/null
  for _ in $(seq 1 60); do docker exec "$db" psql -U postgres -tAc 'select 1' >/dev/null 2>&1 && break; sleep 1; done
  sleep 2
  port=$(docker inspect "$db" --format '{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}')
  docker exec wam-postgres pg_dump -U postgres -d wam -Fc --no-owner --no-privileges \
    | docker exec -i "$db" pg_restore -U postgres -d postgres --no-owner --no-privileges
  echo "production (read only):"; catalog_report http://127.0.0.1:4090
  runuser -u wam -- env -i PATH=/usr/bin:/bin WAM_DATABASE_URL="postgres://postgres:dryrun@127.0.0.1:$port/postgres" \
    WAM_EXAMPLES=refresh WAM_BIND=127.0.0.1:0 "$bin" >"$log" 2>&1 &
  pid=$!
  for _ in $(seq 1 60); do grep -q "listening on" "$log" && break; kill -0 "$pid" 2>/dev/null || { cat "$log"; exit 1; }; sleep 1; done
  echo "refresh on the copy: $(grep 'built-in example memories' "$log" | tail -1)"
  addr=$(grep -o 'listening on [0-9.:]*' "$log" | head -1 | cut -d' ' -f3)
  wait_http "http://$addr/healthz" 20
  echo "the copy after the refresh:"; catalog_report "http://$addr"
  python3 - "$addr" <<'PY'
import json, sys, urllib.request
before = json.load(open("/tmp/wam-refresh-versions-4090"))
after = json.load(open("/tmp/wam-refresh-versions-" + sys.argv[1].rsplit(":", 1)[1]))
changed = sorted(i for i in after if before.get(i) != after[i])
print(f"  new current versions on the copy: {len(changed)}")
for i in changed: print("    " + i)
PY
  exit 0
fi

[ "$mode" = "apply" ] || { echo "unknown mode $mode" >&2; exit 2; }
ts="$(date -u +%Y%m%dT%H%M%SZ)"
backup="/var/backups/web-agentic-memory/refresh-examples-$ts"
install -d -m 700 "$backup"
( umask 077; docker exec wam-postgres pg_dump -U postgres -d wam -Fc > "$backup/production-before.dump" )
test -s "$backup/production-before.dump"
echo "backup: $backup/production-before.dump"
exec 9>"$base/.deploy.lock"; flock -x 9           # no release is switched while this runs
dropin=/etc/systemd/system/wam-api.service.d/zz-refresh-examples-once.conf
envfile=/run/wam-refresh-examples.env
revert() { rm -f "$dropin" "$envfile"; systemctl daemon-reload; }
trap revert EXIT                                  # whatever happens, the setting does not stay
( umask 077; printf 'WAM_EXAMPLES=refresh\n' > "$envfile" )
printf '[Service]\nEnvironmentFile=%s\n' "$envfile" > "$dropin"
systemctl daemon-reload
since="$(date +%s)"
systemctl restart wam-api
wait_http http://127.0.0.1:4090/healthz 60 || { echo "API did not come up"; exit 1; }
line="$(journalctl -u wam-api --since "@$since" --no-pager | grep 'built-in example memories' | tail -1 | sed -E 's/^.*(built-in example memories)/\1/')"
echo "$line"
revert; trap - EXIT                               # the next start is a normal one again (WAM_EXAMPLES=on)
echo "setting removed; the running process keeps serving"
