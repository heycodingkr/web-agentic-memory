CREATE TABLE IF NOT EXISTS wam_schema_migrations (
    version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS wam_app_meta (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK(singleton), document JSONB NOT NULL
);
CREATE TABLE IF NOT EXISTS wam_users (
    id TEXT PRIMARY KEY,
    document JSONB NOT NULL,
    canonical_handle TEXT GENERATED ALWAYS AS (lower(document #>> '{profile,handle}')) STORED UNIQUE,
    CHECK (canonical_handle IS NOT NULL AND length(canonical_handle) BETWEEN 3 AND 24)
);
CREATE TABLE IF NOT EXISTS wam_contributions (
    id TEXT PRIMARY KEY,
    owner_id TEXT REFERENCES wam_users(id) DEFERRABLE INITIALLY DEFERRED,
    nonce TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('queued','in_review','approved','rejected','withdrawn','expired','consent_required')),
    received_at BIGINT NOT NULL CHECK(received_at >= 0),
    document JSONB NOT NULL,
    UNIQUE(owner_id, nonce)
);
CREATE TABLE IF NOT EXISTS wam_consents (
    contribution_id TEXT PRIMARY KEY REFERENCES wam_contributions(id) DEFERRABLE INITIALLY DEFERRED,
    purge_at BIGINT NOT NULL CHECK(purge_at >= 0),
    withdrawn_at BIGINT,
    document JSONB NOT NULL
);
CREATE INDEX IF NOT EXISTS wam_consents_due ON wam_consents(purge_at) WHERE withdrawn_at IS NULL;
CREATE TABLE IF NOT EXISTS wam_memory_records (
    id TEXT PRIMARY KEY,
    site_host TEXT NOT NULL, url_pattern TEXT NOT NULL, kind TEXT NOT NULL,
    task_category TEXT NOT NULL, intent_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('active','needs_reverify','superseded','retired')),
    active_version INTEGER NOT NULL CHECK(active_version > 0),
    document JSONB NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS wam_one_active_scope ON wam_memory_records(site_host,url_pattern,kind,task_category,intent_key)
    WHERE status IN ('active','needs_reverify');
CREATE TABLE IF NOT EXISTS wam_memory_versions (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL REFERENCES wam_memory_records(id) DEFERRABLE INITIALLY DEFERRED,
    version_no INTEGER NOT NULL CHECK(version_no > 0),
    document JSONB NOT NULL,
    UNIQUE(memory_id,version_no)
);
CREATE TABLE IF NOT EXISTS wam_memory_index (
    version_id TEXT NOT NULL REFERENCES wam_memory_versions(id) ON DELETE RESTRICT,
    site_host TEXT NOT NULL, url_pattern TEXT NOT NULL, kind TEXT NOT NULL,
    task_category TEXT NOT NULL, keyword TEXT NOT NULL,
    PRIMARY KEY(version_id,site_host,url_pattern,kind,task_category,keyword)
);
CREATE INDEX IF NOT EXISTS wam_memory_scope_lookup ON wam_memory_index(site_host,url_pattern,task_category,kind,version_id);
CREATE INDEX IF NOT EXISTS wam_memory_keyword_lookup ON wam_memory_index(site_host,keyword,version_id);
CREATE TABLE IF NOT EXISTS wam_memory_authors (
    id TEXT PRIMARY KEY,
    version_id TEXT NOT NULL REFERENCES wam_memory_versions(id) DEFERRABLE INITIALLY DEFERRED,
    user_id TEXT REFERENCES wam_users(id) DEFERRABLE INITIALLY DEFERRED,
    contribution_id TEXT REFERENCES wam_contributions(id) DEFERRABLE INITIALLY DEFERRED,
    role TEXT NOT NULL CHECK(role IN ('author','support')),
    document JSONB NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS wam_one_attribution_per_account ON wam_memory_authors(version_id,user_id) WHERE user_id IS NOT NULL;
CREATE TABLE IF NOT EXISTS wam_usage_receipts (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL REFERENCES wam_users(id) DEFERRABLE INITIALLY DEFERRED,
    issued_at BIGINT NOT NULL CHECK(issued_at >= 0), expires_at BIGINT NOT NULL,
    document JSONB NOT NULL
);
CREATE TABLE IF NOT EXISTS wam_receipt_memories (
    receipt_id TEXT NOT NULL REFERENCES wam_usage_receipts(id) ON DELETE CASCADE,
    version_id TEXT NOT NULL REFERENCES wam_memory_versions(id) DEFERRABLE INITIALLY DEFERRED,
    author_id TEXT REFERENCES wam_users(id) DEFERRABLE INITIALLY DEFERRED,
    used BOOLEAN NOT NULL,
    PRIMARY KEY(receipt_id,version_id)
);
CREATE TABLE IF NOT EXISTS wam_score_events (
    id TEXT PRIMARY KEY,
    beneficiary_id TEXT REFERENCES wam_users(id) DEFERRABLE INITIALLY DEFERRED,
    consumer_id TEXT REFERENCES wam_users(id) DEFERRABLE INITIALLY DEFERRED,
    memory_id TEXT NOT NULL REFERENCES wam_memory_records(id) DEFERRABLE INITIALLY DEFERRED,
    version_id TEXT NOT NULL REFERENCES wam_memory_versions(id) DEFERRABLE INITIALLY DEFERRED,
    receipt_id TEXT REFERENCES wam_usage_receipts(id) ON DELETE SET NULL DEFERRABLE INITIALLY DEFERRED,
    event_type TEXT NOT NULL CHECK(event_type IN ('approval','successful_use','reversal')),
    points_delta BIGINT NOT NULL CHECK(points_delta <> 0),
    idempotency_key TEXT NOT NULL UNIQUE,
    created_at BIGINT NOT NULL CHECK(created_at >= 0),
    utc_day BIGINT GENERATED ALWAYS AS (created_at / 86400) STORED,
    reversal_of TEXT UNIQUE REFERENCES wam_score_events(id) DEFERRABLE INITIALLY DEFERRED,
    document JSONB NOT NULL,
    CHECK(event_type <> 'successful_use' OR points_delta = 2),
    CHECK(event_type <> 'approval' OR points_delta = 20),
    CHECK(event_type <> 'reversal' OR (points_delta < 0 AND reversal_of IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS wam_daily_consumer_version ON wam_score_events(consumer_id,version_id,utc_day) WHERE event_type = 'successful_use';
CREATE UNIQUE INDEX IF NOT EXISTS wam_daily_consumer_author ON wam_score_events(consumer_id,beneficiary_id,utc_day) WHERE event_type = 'successful_use';
CREATE INDEX IF NOT EXISTS wam_beneficiary_history ON wam_score_events(beneficiary_id,created_at DESC,id DESC);
CREATE TABLE IF NOT EXISTS wam_identity_records (
    kind TEXT NOT NULL CHECK(kind IN ('oauth_transactions','accounts','sessions','device_requests','devices','approval_attempts')),
    id TEXT NOT NULL,
    user_id TEXT REFERENCES wam_users(id) DEFERRABLE INITIALLY DEFERRED,
    secret_hash TEXT UNIQUE,
    expires_at BIGINT,
    document JSONB NOT NULL,
    PRIMARY KEY(kind,id)
);
CREATE INDEX IF NOT EXISTS wam_identity_user ON wam_identity_records(user_id,kind);
CREATE TABLE IF NOT EXISTS wam_work_items (
    id TEXT PRIMARY KEY,
    contribution_id TEXT REFERENCES wam_contributions(id) DEFERRABLE INITIALLY DEFERRED,
    memory_id TEXT REFERENCES wam_memory_records(id) DEFERRABLE INITIALLY DEFERRED,
    state TEXT NOT NULL CHECK(state IN ('queued','leased','retry','waiting_review','done','canceled')),
    generation BIGINT NOT NULL CHECK(generation >= 0),
    lease_until BIGINT,
    holder TEXT,
    document JSONB NOT NULL,
    CHECK ((state = 'leased' AND lease_until IS NOT NULL AND holder IS NOT NULL) OR
           (state <> 'leased' AND lease_until IS NULL AND holder IS NULL))
);
CREATE INDEX IF NOT EXISTS wam_pending_work ON wam_work_items(state,id) WHERE state IN ('queued','retry','leased');
CREATE TABLE IF NOT EXISTS wam_outbox (
    id TEXT PRIMARY KEY, kind TEXT NOT NULL, target TEXT NOT NULL,
    created_at BIGINT NOT NULL, delivered_at BIGINT,
    document JSONB NOT NULL
);
CREATE INDEX IF NOT EXISTS wam_pending_outbox ON wam_outbox(id) WHERE delivered_at IS NULL;
CREATE TABLE IF NOT EXISTS wam_decisions (
    id TEXT PRIMARY KEY, job_id TEXT NOT NULL, job_digest TEXT NOT NULL,
    memory_id TEXT REFERENCES wam_memory_records(id) DEFERRABLE INITIALLY DEFERRED,
    document JSONB NOT NULL, UNIQUE(job_id,job_digest)
);
CREATE TABLE IF NOT EXISTS wam_audit (
    kind TEXT NOT NULL, id TEXT NOT NULL, document JSONB NOT NULL, PRIMARY KEY(kind,id)
);
CREATE TABLE IF NOT EXISTS wam_verified_proposals (
    contribution_id TEXT PRIMARY KEY REFERENCES wam_contributions(id) DEFERRABLE INITIALLY DEFERRED,
    document JSONB NOT NULL
);

CREATE OR REPLACE FUNCTION wam_protect_version() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN RAISE EXCEPTION 'immutable memory version'; END IF;
    IF OLD.id <> NEW.id OR OLD.memory_id <> NEW.memory_id OR OLD.version_no <> NEW.version_no OR
       (OLD.document - 'status') IS DISTINCT FROM (NEW.document - 'status') THEN
        RAISE EXCEPTION 'immutable memory version';
    END IF;
    RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS wam_version_immutable ON wam_memory_versions;
CREATE TRIGGER wam_version_immutable BEFORE UPDATE OR DELETE ON wam_memory_versions FOR EACH ROW EXECUTE FUNCTION wam_protect_version();

CREATE OR REPLACE FUNCTION wam_protect_score() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN RAISE EXCEPTION 'append-only score event'; END IF;
    IF TG_OP = 'INSERT' THEN
        IF NEW.event_type = 'successful_use' AND (NEW.consumer_id IS NULL OR NEW.beneficiary_id IS NULL OR NEW.consumer_id = NEW.beneficiary_id OR NEW.receipt_id IS NULL) THEN
            RAISE EXCEPTION 'successful use requires another authenticated account and receipt';
        END IF;
        IF NEW.event_type = 'approval' AND NEW.beneficiary_id IS NULL THEN RAISE EXCEPTION 'approval requires an author'; END IF;
    ELSE
        IF (OLD.document - 'beneficiary_account_id' - 'consumer_account_id') IS DISTINCT FROM
           (NEW.document - 'beneficiary_account_id' - 'consumer_account_id') OR
           OLD.id <> NEW.id OR OLD.version_id <> NEW.version_id OR OLD.memory_id <> NEW.memory_id OR
           OLD.points_delta <> NEW.points_delta OR OLD.idempotency_key <> NEW.idempotency_key OR
           OLD.created_at <> NEW.created_at OR OLD.event_type <> NEW.event_type OR
           OLD.reversal_of IS DISTINCT FROM NEW.reversal_of THEN
            RAISE EXCEPTION 'immutable score facts';
        END IF;
        IF (NEW.beneficiary_id IS NOT NULL AND NEW.beneficiary_id IS DISTINCT FROM OLD.beneficiary_id) OR
           (NEW.consumer_id IS NOT NULL AND NEW.consumer_id IS DISTINCT FROM OLD.consumer_id) OR
           (NEW.receipt_id IS NOT NULL AND NEW.receipt_id IS DISTINCT FROM OLD.receipt_id) THEN
            RAISE EXCEPTION 'score identity and receipt links may only be erased';
        END IF;
    END IF;
    IF TG_OP = 'INSERT' AND NEW.event_type = 'successful_use' AND NOT EXISTS (
        SELECT 1 FROM wam_usage_receipts r
        JOIN wam_receipt_memories m ON m.receipt_id=r.id
        JOIN wam_memory_versions v ON v.id=m.version_id
        WHERE r.id=NEW.receipt_id AND r.owner_id=NEW.consumer_id
          AND m.author_id=NEW.beneficiary_id AND m.version_id=NEW.version_id AND m.used
          AND r.document->>'outcome'='success'
          AND (r.document->>'reported_at')::bigint=NEW.created_at
          AND r.issued_at <= NEW.created_at AND r.expires_at >= NEW.created_at
          AND v.document->>'status'<>'retired'
    ) THEN RAISE EXCEPTION 'successful use is not backed by a valid used receipt version'; END IF;
    IF TG_OP = 'INSERT' AND NEW.event_type = 'approval' AND NOT EXISTS (
        SELECT 1 FROM wam_memory_authors WHERE version_id=NEW.version_id AND user_id=NEW.beneficiary_id AND role='author'
    ) THEN RAISE EXCEPTION 'approval is not backed by version authorship'; END IF;
    IF TG_OP = 'INSERT' AND NEW.event_type = 'reversal' AND NOT EXISTS (
        SELECT 1 FROM wam_score_events original WHERE original.id=NEW.reversal_of
        AND original.points_delta > 0 AND original.points_delta = -NEW.points_delta
        AND original.version_id=NEW.version_id AND original.memory_id=NEW.memory_id
        AND original.beneficiary_id IS NOT DISTINCT FROM NEW.beneficiary_id
    ) THEN RAISE EXCEPTION 'reversal must negate its original event'; END IF;
    RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS wam_score_immutable ON wam_score_events;
CREATE TRIGGER wam_score_immutable BEFORE INSERT OR UPDATE OR DELETE ON wam_score_events FOR EACH ROW EXECUTE FUNCTION wam_protect_score();

ALTER TABLE wam_users ADD CONSTRAINT wam_user_document_id CHECK(document->>'id'=id);
ALTER TABLE wam_contributions ADD CONSTRAINT wam_contribution_document_id CHECK(document#>>'{record,id}'=id AND document#>>'{record,status}'=state);
ALTER TABLE wam_memory_records ADD CONSTRAINT wam_record_document_id CHECK(document->>'id'=id AND document->>'status'=status);
ALTER TABLE wam_memory_records ADD CONSTRAINT wam_active_version_fk FOREIGN KEY(id,active_version) REFERENCES wam_memory_versions(memory_id,version_no) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE wam_memory_versions ADD CONSTRAINT wam_version_document_number CHECK((document->>'version')::integer=version_no);
ALTER TABLE wam_usage_receipts ADD CONSTRAINT wam_receipt_document_id CHECK(document->>'id'=id AND document->>'owner_account'=owner_id AND (document->>'issued_at')::bigint=issued_at);
ALTER TABLE wam_work_items ADD CONSTRAINT wam_work_document_id CHECK(document->>'id'=id AND document->>'state'=state AND (document->>'generation')::bigint=generation);
ALTER TABLE wam_score_events ADD CONSTRAINT wam_score_document_facts CHECK(
    document->>'id'=id AND document->>'memory_id'=memory_id AND document->>'memory_version_id'=version_id
    AND document->>'event_type'=event_type AND document->>'idempotency_key'=idempotency_key
    AND (document->>'points_delta')::bigint=points_delta AND (document->>'created_at')::bigint=created_at
    AND (beneficiary_id IS NOT NULL AND document->>'beneficiary_account_id'=beneficiary_id OR beneficiary_id IS NULL AND document->>'beneficiary_account_id' IN ('deleted',''))
    AND (consumer_id IS NOT NULL AND document->>'consumer_account_id'=consumer_id OR consumer_id IS NULL AND document->>'consumer_account_id' IN ('deleted',''))
);

CREATE OR REPLACE FUNCTION wam_protect_receipt() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.id<>NEW.id OR OLD.owner_id<>NEW.owner_id OR OLD.issued_at<>NEW.issued_at OR
       OLD.document->'memory_ids' IS DISTINCT FROM NEW.document->'memory_ids' THEN
        RAISE EXCEPTION 'receipt attribution is immutable';
    END IF;
    IF OLD.document->>'outcome' IS NOT NULL AND
       (OLD.document->'outcome' IS DISTINCT FROM NEW.document->'outcome' OR
        OLD.document->'used_version_ids' IS DISTINCT FROM NEW.document->'used_version_ids' OR
        OLD.document->'reported_at' IS DISTINCT FROM NEW.document->'reported_at') THEN
        RAISE EXCEPTION 'receipt outcome is immutable';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER wam_receipt_immutable BEFORE UPDATE ON wam_usage_receipts FOR EACH ROW EXECUTE FUNCTION wam_protect_receipt();
