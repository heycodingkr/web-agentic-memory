CREATE TABLE IF NOT EXISTS wam_gatekeeper_memories (
    id TEXT PRIMARY KEY,
    site_host TEXT NOT NULL,
    url_pattern TEXT NOT NULL,
    memory_kind TEXT NOT NULL,
    task_category TEXT NOT NULL,
    intent_key TEXT NOT NULL,
    support_count BIGINT NOT NULL DEFAULT 0 CHECK (support_count >= 0),
    lifecycle_status TEXT NOT NULL CHECK (lifecycle_status IN ('active', 'needs_reverify', 'superseded', 'retired')),
    active_version INTEGER NOT NULL CHECK (active_version > 0),
    confidence_band SMALLINT NOT NULL CHECK (confidence_band BETWEEN 0 AND 2),
    UNIQUE (id, site_host)
);

CREATE UNIQUE INDEX IF NOT EXISTS wam_gatekeeper_one_active_scope
    ON wam_gatekeeper_memories (site_host, url_pattern, memory_kind, task_category, intent_key)
    WHERE lifecycle_status IN ('active', 'needs_reverify');

CREATE TABLE IF NOT EXISTS wam_gatekeeper_memory_versions (
    memory_id TEXT NOT NULL REFERENCES wam_gatekeeper_memories(id) ON DELETE RESTRICT,
    version_no INTEGER NOT NULL CHECK (version_no > 0),
    payload JSONB NOT NULL,
    fingerprint TEXT NOT NULL,
    verified_at BIGINT NOT NULL CHECK (verified_at >= 0),
    lifecycle_status TEXT NOT NULL CHECK (lifecycle_status IN ('active', 'needs_reverify', 'superseded', 'retired')),
    evidence JSONB NOT NULL,
    job_id TEXT NOT NULL,
    PRIMARY KEY (memory_id, version_no)
);

CREATE INDEX IF NOT EXISTS wam_gatekeeper_versions_by_job
    ON wam_gatekeeper_memory_versions (job_id, memory_id, version_no);

CREATE TABLE IF NOT EXISTS wam_gatekeeper_decisions (
    job_id TEXT NOT NULL,
    job_digest TEXT NOT NULL,
    decision_kind TEXT NOT NULL,
    reason_codes TEXT[] NOT NULL,
    confidence_band SMALLINT,
    memory_id TEXT,
    version_no INTEGER,
    decided_at BIGINT NOT NULL CHECK (decided_at >= 0),
    PRIMARY KEY (job_id, job_digest)
);

CREATE TABLE IF NOT EXISTS wam_gatekeeper_audits (
    audit_id BIGSERIAL PRIMARY KEY,
    job_id TEXT NOT NULL,
    job_digest TEXT NOT NULL,
    outcome TEXT NOT NULL,
    audit JSONB NOT NULL,
    audited_at BIGINT NOT NULL CHECK (audited_at >= 0)
);

CREATE INDEX IF NOT EXISTS wam_gatekeeper_audits_by_job
    ON wam_gatekeeper_audits (job_id, audited_at DESC);

CREATE TABLE IF NOT EXISTS wam_gatekeeper_queue_config (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    settings JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS wam_gatekeeper_queue_meta (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    fairness_tick BIGINT NOT NULL DEFAULT 0 CHECK (fairness_tick >= 0)
);

INSERT INTO wam_gatekeeper_queue_meta (singleton, fairness_tick)
VALUES (TRUE, 0)
ON CONFLICT (singleton) DO NOTHING;

CREATE TABLE IF NOT EXISTS wam_gatekeeper_queue_fairness (
    contributor_id TEXT PRIMARY KEY,
    last_served_tick BIGINT NOT NULL CHECK (last_served_tick >= 0)
);

CREATE TABLE IF NOT EXISTS wam_gatekeeper_jobs (
    id BIGSERIAL PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('contribution', 'reverify', 'withdrawal', 'rollback')),
    scope_key TEXT NOT NULL,
    contributor_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    account_class TEXT NOT NULL CHECK (account_class IN ('established', 'new')),
    nonce TEXT NOT NULL,
    contribution_id TEXT,
    priority TEXT NOT NULL CHECK (priority IN ('p0', 'p1')),
    enqueued_at BIGINT NOT NULL CHECK (enqueued_at >= 0),
    state TEXT NOT NULL CHECK (state IN ('queued', 'leased', 'waiting_scope', 'waiting_budget', 'retry', 'done', 'dead', 'canceled')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    not_before BIGINT NOT NULL CHECK (not_before >= 0),
    lease_generation BIGINT NOT NULL DEFAULT 0 CHECK (lease_generation >= 0),
    lease_expires_at BIGINT,
    lease_holder TEXT,
    attribution_withdrawn BOOLEAN NOT NULL DEFAULT FALSE,
    finished_at BIGINT,
    last_error TEXT,
    UNIQUE (contributor_id, nonce),
    CHECK (
        (state = 'leased' AND lease_expires_at IS NOT NULL AND lease_holder IS NOT NULL)
        OR (state <> 'leased' AND lease_expires_at IS NULL AND lease_holder IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS wam_gatekeeper_jobs_pending
    ON wam_gatekeeper_jobs (priority, not_before, enqueued_at, id)
    WHERE state IN ('queued', 'waiting_scope', 'retry');

CREATE INDEX IF NOT EXISTS wam_gatekeeper_jobs_scope_state
    ON wam_gatekeeper_jobs (scope_key, state, id);

CREATE INDEX IF NOT EXISTS wam_gatekeeper_jobs_active_contributor
    ON wam_gatekeeper_jobs (contributor_id, state)
    WHERE state NOT IN ('done', 'dead', 'canceled');

CREATE INDEX IF NOT EXISTS wam_gatekeeper_jobs_active_device
    ON wam_gatekeeper_jobs (device_id, state)
    WHERE state NOT IN ('done', 'dead', 'canceled');
