CREATE TABLE cancer_virtual_experiment_results (
    experiment_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    request_id UUID NOT NULL REFERENCES cancer_research_results (request_id),
    method_version INTEGER NOT NULL CHECK (
        method_version > 0 AND method_version <= 65535
    ),
    artifact_hash BYTEA NOT NULL CHECK (octet_length(artifact_hash) = 32),
    plan_hash BYTEA NOT NULL CHECK (octet_length(plan_hash) = 32),
    result_payload JSONB NOT NULL,
    result_checksum BYTEA NOT NULL CHECK (octet_length(result_checksum) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (request_id, method_version)
);

CREATE INDEX cancer_virtual_experiment_results_world_created
    ON cancer_virtual_experiment_results (world_id, created_at DESC, experiment_id);

CREATE FUNCTION validate_cancer_virtual_experiment_result_insert()
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
          AND result.result_payload->'receipt'->'contribution'->'virtual_experiment_plan' IS NOT NULL
    ) THEN
        RAISE EXCEPTION 'virtual experiment must reference a planned successful artifact in the same world';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_virtual_experiment_results_validate_insert
BEFORE INSERT ON cancer_virtual_experiment_results
FOR EACH ROW EXECUTE FUNCTION validate_cancer_virtual_experiment_result_insert();

CREATE TRIGGER cancer_virtual_experiment_results_are_immutable
BEFORE UPDATE OR DELETE ON cancer_virtual_experiment_results
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

COMMENT ON TABLE cancer_virtual_experiment_results IS
'Deterministic observer-side model projections for closed Cancer World experiment plans; never wet-lab or clinical evidence.';
