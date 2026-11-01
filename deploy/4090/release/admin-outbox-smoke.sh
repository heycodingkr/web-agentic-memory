#!/usr/bin/env bash
# Production check of the administrator link outbox. Run as root on the host. The address stays on the host:
# nothing printed here contains it, the link or any secret; only counts and status codes.
set -euo pipefail
set -a; . /etc/web-agentic-memory/admin.env; set +a
q() { docker exec wam-postgres psql -U postgres -d wam -Atc "$1"; }
count() { q "select coalesce(jsonb_array_length(document->'identity'->'admin'->'$1'),0) from wam_app_meta where singleton"; }
echo "before: deliveries=$(count deliveries) challenges=$(count challenges)"
started=$(date +%s)
code=$(printf '{"email":"%s"}' "$WAM_ADMIN_EMAIL" | curl -s -o /dev/null -w '%{http_code}' -X POST "$WAM_ADMIN_ORIGIN/v1/admin/auth/start" \
  -H "Origin: $WAM_ADMIN_ORIGIN" -H 'content-type: application/json' --data @-)
echo "start request: HTTP $code"
for i in $(seq 1 30); do
  sleep 1
  d=$(count deliveries); c=$(count challenges)
  newest=$(q "select coalesce((document->'identity'->'admin'->'sends'->-1)::text::bigint,0) from wam_app_meta where singleton")
  echo "t+${i}s deliveries=$d challenges=$c newest_send_after_request=$([ "$newest" -ge "$started" ] && echo yes || echo no)"
  if [ "$d" = "0" ] && [ "$newest" -ge "$started" ] && [ "$i" -ge 3 ]; then break; fi
done
if [ "$d" = "0" ] && [ "$c" -ge 1 ] && [ "$newest" -ge "$started" ]; then echo "RESULT: link delivered through the outbox (entry gone, challenge alive)"; else echo "RESULT: NOT CONFIRMED (deliveries=$d challenges=$c)"; exit 1; fi
journalctl -u wam-api --since "-2min" --no-pager 2>/dev/null | grep -ciE "panic|error" | xargs echo "error/panic lines in the last 2 minutes of the API log:"
