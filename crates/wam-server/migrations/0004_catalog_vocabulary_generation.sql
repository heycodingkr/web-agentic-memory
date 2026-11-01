CREATE TABLE IF NOT EXISTS wam_catalog_vocabulary_generation (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    generation BIGINT NOT NULL DEFAULT 0 CHECK (generation >= 0)
);
INSERT INTO wam_catalog_vocabulary_generation(singleton, generation)
VALUES (TRUE, 0)
ON CONFLICT (singleton) DO NOTHING;

CREATE OR REPLACE FUNCTION wam_bump_catalog_vocabulary_generation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE wam_catalog_vocabulary_generation
    SET generation = generation + 1
    WHERE singleton;
    RETURN NULL;
END;
$$;

CREATE TRIGGER wam_memory_records_vocabulary_generation
AFTER INSERT OR UPDATE OR DELETE ON wam_memory_records
FOR EACH STATEMENT EXECUTE FUNCTION wam_bump_catalog_vocabulary_generation();

CREATE TRIGGER wam_memory_versions_vocabulary_generation
AFTER INSERT OR UPDATE OR DELETE ON wam_memory_versions
FOR EACH STATEMENT EXECUTE FUNCTION wam_bump_catalog_vocabulary_generation();

CREATE TRIGGER wam_memory_index_vocabulary_generation
AFTER INSERT OR UPDATE OR DELETE ON wam_memory_index
FOR EACH STATEMENT EXECUTE FUNCTION wam_bump_catalog_vocabulary_generation();
