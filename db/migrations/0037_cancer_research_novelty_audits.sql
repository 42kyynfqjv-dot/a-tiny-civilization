-- Observer-only, reproducible literature-overlap assessments. These records are
-- finding aids and never feed the blind research collective or canonical world.
CREATE TABLE cancer_research_novelty_audits (
    audit_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    request_id UUID NOT NULL REFERENCES cancer_research_results (request_id),
    method_version INTEGER NOT NULL CHECK (
        method_version > 0 AND method_version <= 65535
    ),
    artifact_hash BYTEA NOT NULL CHECK (octet_length(artifact_hash) = 32),
    normalized_status TEXT NOT NULL CHECK (
        normalized_status IN (
            'known_overlap',
            'new_combination',
            'no_close_match_found',
            'possible_error'
        )
    ),
    audit_payload JSONB NOT NULL,
    audit_checksum BYTEA NOT NULL CHECK (octet_length(audit_checksum) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (request_id, method_version)
);

CREATE INDEX cancer_research_novelty_audits_world_created
    ON cancer_research_novelty_audits (world_id, created_at DESC, audit_id);

CREATE FUNCTION validate_cancer_research_novelty_audit_insert()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM cancer_research_requests AS request
        JOIN cancer_research_results AS result USING (request_id)
        WHERE request.request_id = NEW.request_id
          AND request.world_id = NEW.world_id
          AND result.result_payload->'receipt' <> 'null'::JSONB
    ) THEN
        RAISE EXCEPTION 'novelty audit must reference a successful artifact in the same world';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_research_novelty_audits_validate_insert
BEFORE INSERT ON cancer_research_novelty_audits
FOR EACH ROW EXECUTE FUNCTION validate_cancer_research_novelty_audit_insert();

CREATE TRIGGER cancer_research_novelty_audits_are_immutable
BEFORE UPDATE OR DELETE ON cancer_research_novelty_audits
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

COMMENT ON TABLE cancer_research_novelty_audits IS
'Immutable observer-side literature-overlap triage; never a proof of scientific novelty and never research-world input.';
