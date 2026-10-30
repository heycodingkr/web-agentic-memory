# Diagnostic tracing and log storage

HTTP, remote MCP, CLI, stdio MCP, and browser errors share a 32-character opaque `x-wam-trace-id`. The server preserves a valid ID and replaces malformed IDs. Every observed response includes the reference, including auth/rate-limit failures. HTTP-200 JSON-RPC/tool errors in finite JSON responses are recorded as errors too. SSE is passed through without collecting its body: diagnostics record connection status and header latency, and do not inspect streamed tool payloads or measure the lifetime of an idle connection. CLI prints the reference to stderr; MCP keeps stdout JSON-RPC framing; web API errors carry the reference.

Production startup wraps the router with `diagnostics::observe`. `WAM_LOG_DATABASE_URL` selects a separate PostgreSQL log database; otherwise `WAM_DATABASE_URL` is used. Local development writes SQLite at `WAM_DIAGNOSTICS_PATH` or `WAM_DATA_DIR/diagnostics.sqlite`. The independent, idempotent schema in `crates/wam-server/src/diagnostics/schema.sql` creates `wam_diagnostic_events` and trace/error/retention indexes. Non-loopback PostgreSQL requires verified TLS. Connection values and driver errors are not emitted.

Stored data is closed metadata: event ID, trace ID, source, operation category, error category, HTTP status/method, elapsed milliseconds and server receipt time. The JSON document contains only these fields. Tokens, cookies, account/device IDs, argv/stdin, tool arguments, request/response bodies, actual URL/route/query values, user messages and raw stack traces are excluded. Client source/status are diagnostic claims, never authorization, score, or abuse-proof evidence.

`POST /v1/telemetry/events` accepts one schema-v1 closed event, at most 2 KiB, with bounded IDs/status/duration. Extra fields and unknown enums fail without echoing the input. Event IDs deduplicate replay. The anonymous intake has an in-process 60/minute peer limit, bounded peer state, and does not trust forwarded IP headers. It is independent of the API's shared account/IP admission. No public log-reading route is exposed.

The writer runs on a dedicated thread with a 256-command channel and a bounded 512-event retry backlog. Enqueue never blocks a business response. Temporary write failures retry; closed PostgreSQL connections reconnect with timeouts. Oldest events can be dropped under sustained outage/overload; `Diagnostics::dropped_events()` exposes that count. This is bounded operational diagnostics, not a lossless financial journal. PostgreSQL statements/locks have timeouts. Graceful server shutdown flushes pending diagnostics.

Logs retain seven days and at most 100,000 rows after cleanup (cleanup every 64 queued writes and every minute). SQLite uses WAL so an operator's read transaction does not block diagnostic ingestion; files use owner-only permissions. External readers should set a bounded busy timeout. Client offline queues retain at most 100 closed events for seven days in private per-origin directories; a later successful request drains at most ten within a total 800 ms deadline. `WAM_TELEMETRY=off` disables CLI/MCP uploads and queue access. Browser uploads use a bounded in-memory queue, retain failed events while the tab stays open, and retry after a five-second cooldown on connectivity/API recovery; `localStorage['wam.telemetry.enabled']='false'` disables client capture independently from contribution consent. Necessary server request diagnostics still contain no account linkage.

Operator query, using a trace reference copied from an error:

```sql
SELECT received_at, source, operation, error_code, status_code, document
FROM wam_diagnostic_events
WHERE trace_id = '<32-character reference>'
ORDER BY received_at;
```

Verification: `crates/wam-server/tests/diagnostics.rs`, disposable-DB `postgres_diagnostics.rs`, client telemetry/spool tests, browser telemetry tests and [local smoke](../test/local-smoke.md). The smoke runs actual CLI/MCP binaries and an actual browser against an isolated fixture, then compares trace references with persisted SQLite rows. Synthetic OAuth/model/replay fixtures do not claim live external-provider acceptance.
