CREATE TABLE cancer_tcga_gbm_target_context_qualifications (
    qualification_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    request_id UUID NOT NULL REFERENCES cancer_research_results (request_id),
    method_version INTEGER NOT NULL CHECK (
        method_version > 0 AND method_version <= 65535
    ),
    artifact_hash BYTEA NOT NULL CHECK (octet_length(artifact_hash) = 32),
    source_artifact_hash BYTEA NOT NULL CHECK (
        octet_length(source_artifact_hash) = 32
        AND source_artifact_hash = decode(
            'f523989c2bec5ee14c0ff2c6dc30d193fb324e1dd234aba524bef179553294da',
            'hex'
        )
    ),
    result_payload JSONB NOT NULL,
    result_checksum BYTEA NOT NULL CHECK (octet_length(result_checksum) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (request_id, method_version)
);

CREATE INDEX cancer_tcga_gbm_target_context_world_created
    ON cancer_tcga_gbm_target_context_qualifications (
        world_id, created_at DESC, qualification_id
    );

CREATE FUNCTION validate_cancer_tcga_gbm_target_context_qualification_insert()
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
          AND JSONB_TYPEOF(
              result.result_payload->'receipt'->'contribution'->'molecular_targets'
          ) = 'array'
          AND JSONB_ARRAY_LENGTH(
              result.result_payload->'receipt'->'contribution'->'molecular_targets'
          ) > 0
    ) THEN
        RAISE EXCEPTION 'TCGA target context must reference a molecularly targeted artifact in the same world';
    END IF;
    IF NOT (NEW.result_payload ?& ARRAY[
           'schema_version','method_version','qualification_id','world_id','request_id',
           'artifact_hash','source','baseline_id','data_release',
           'calibration_profiled_patient_count','held_out_profiled_patient_count',
           'feature_selection','target_observations','limitations'
       ])
       OR NOT (NEW.result_payload->'source' ?& ARRAY['kind','source_id','content_hash'])
       OR NEW.result_payload->>'schema_version' <> '1'
       OR (NEW.result_payload->>'method_version')::INTEGER <> NEW.method_version
       OR NEW.result_payload->>'qualification_id' <> NEW.qualification_id::TEXT
       OR NEW.result_payload->>'world_id' <> NEW.world_id::TEXT
       OR NEW.result_payload->>'request_id' <> NEW.request_id::TEXT
       OR NEW.result_payload->>'artifact_hash' <> ENCODE(NEW.artifact_hash, 'hex')
       OR NEW.result_payload->'source'->>'content_hash'
          <> ENCODE(NEW.source_artifact_hash, 'hex')
       OR NEW.result_payload->'source'->>'kind' <> 'raw_dataset'
       OR NEW.result_payload->'source'->>'source_id'
          <> 'gdc://TCGA-GBM/DR46/open-aggregate'
       OR NEW.result_payload->>'baseline_id' <> 'tcga-gbm-dr46-patient-baseline-v1'
       OR NEW.result_payload->>'data_release'
          <> 'Data Release 46.0 - August 10, 2026'
       OR (NEW.result_payload->>'calibration_profiled_patient_count')::INTEGER <> 303
       OR (NEW.result_payload->>'held_out_profiled_patient_count')::INTEGER <> 71
       OR NEW.result_payload->>'feature_selection'
          <> 'top 25 protein-altering genes selected using calibration patients only'
       OR JSONB_TYPEOF(NEW.result_payload->'target_observations') <> 'array'
       OR JSONB_ARRAY_LENGTH(NEW.result_payload->'target_observations') = 0
       OR JSONB_TYPEOF(NEW.result_payload->'limitations') <> 'array'
       OR JSONB_ARRAY_LENGTH(NEW.result_payload->'limitations') <> 5
    THEN
        RAISE EXCEPTION 'TCGA target-context columns disagree with its immutable payload';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_tcga_gbm_target_context_qualifications_validate_insert
BEFORE INSERT ON cancer_tcga_gbm_target_context_qualifications
FOR EACH ROW EXECUTE FUNCTION validate_cancer_tcga_gbm_target_context_qualification_insert();

CREATE TRIGGER cancer_tcga_gbm_target_context_qualifications_are_immutable
BEFORE UPDATE OR DELETE ON cancer_tcga_gbm_target_context_qualifications
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

COMMENT ON TABLE cancer_tcga_gbm_target_context_qualifications IS
'Immutable observer-side exact-target context against a patient-disjoint TCGA-GBM aggregate; somatic-variant prevalence only, never intervention response or clinical evidence.';
