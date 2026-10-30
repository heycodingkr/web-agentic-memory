CREATE TABLE IF NOT EXISTS wam_diagnostic_events (
    event_id TEXT PRIMARY KEY CHECK(length(event_id)=32),
    trace_id TEXT NOT NULL CHECK(length(trace_id)=32),
    source TEXT NOT NULL CHECK(source IN ('web','cli','mcp','server')),
    operation TEXT NOT NULL CHECK(operation IN ('recall','expand','vocab','catalog','profile','dashboard','contribution','outcome','auth','device','review','usage','telemetry','unknown')),
    error_code TEXT CHECK(error_code IS NULL OR error_code IN ('network_error','http_error','invalid_response','invalid_input','unhandled_error','unhandled_rejection','panic','internal_error')),
    status_code INTEGER CHECK(status_code IS NULL OR status_code BETWEEN 100 AND 599),
    received_at BIGINT NOT NULL CHECK(received_at>=0),
    document TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS wam_diagnostic_trace ON wam_diagnostic_events(trace_id,received_at);
CREATE INDEX IF NOT EXISTS wam_diagnostic_retention ON wam_diagnostic_events(received_at);
CREATE INDEX IF NOT EXISTS wam_diagnostic_errors ON wam_diagnostic_events(error_code,received_at);
