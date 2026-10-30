CREATE TABLE IF NOT EXISTS wam_memory_usage_aggregates (
    memory_id TEXT NOT NULL REFERENCES wam_memory_records(id) ON DELETE CASCADE,
    version_id TEXT NOT NULL REFERENCES wam_memory_versions(id) ON DELETE CASCADE,
    utc_date DATE NOT NULL,
    anonymous_recall_hits BIGINT NOT NULL DEFAULT 0 CHECK (anonymous_recall_hits >= 0),
    other_account_recall_hits BIGINT NOT NULL DEFAULT 0 CHECK (other_account_recall_hits >= 0),
    anonymous_detail_views BIGINT NOT NULL DEFAULT 0 CHECK (anonymous_detail_views >= 0),
    other_account_detail_views BIGINT NOT NULL DEFAULT 0 CHECK (other_account_detail_views >= 0),
    success_outcomes BIGINT NOT NULL DEFAULT 0 CHECK (success_outcomes >= 0),
    partial_outcomes BIGINT NOT NULL DEFAULT 0 CHECK (partial_outcomes >= 0),
    failed_outcomes BIGINT NOT NULL DEFAULT 0 CHECK (failed_outcomes >= 0),
    PRIMARY KEY (version_id, utc_date)
);
CREATE INDEX IF NOT EXISTS wam_memory_usage_retention ON wam_memory_usage_aggregates(utc_date);
