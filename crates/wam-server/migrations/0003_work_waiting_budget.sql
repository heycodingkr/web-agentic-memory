-- A contribution job that meets a spent provider quota or credit waits as `waiting_budget` instead of failing or
-- using an attempt (docs/spec/queue.md, QUE-08). The state check of 0001 predates that state, so it is replaced.
DO $$
DECLARE
    constraint_name text;
BEGIN
    FOR constraint_name IN
        SELECT c.conname
        FROM pg_constraint c
        WHERE c.conrelid = 'wam_work_items'::regclass
          AND c.contype = 'c'
          AND pg_get_constraintdef(c.oid) LIKE '%waiting_review%'
    LOOP
        EXECUTE format('ALTER TABLE wam_work_items DROP CONSTRAINT %I', constraint_name);
    END LOOP;
END $$;
ALTER TABLE wam_work_items
    ADD CONSTRAINT wam_work_state_check
    CHECK (state IN ('queued','leased','retry','waiting_budget','waiting_review','done','canceled'));
DROP INDEX IF EXISTS wam_pending_work;
CREATE INDEX wam_pending_work ON wam_work_items(state,id)
    WHERE state IN ('queued','retry','leased','waiting_budget');
