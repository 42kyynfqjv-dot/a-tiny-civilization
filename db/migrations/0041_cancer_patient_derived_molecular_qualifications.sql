CREATE TABLE cancer_patient_derived_molecular_qualifications (
    qualification_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    request_id UUID NOT NULL REFERENCES cancer_research_results (request_id),
    method_version INTEGER NOT NULL CHECK (
        method_version > 0 AND method_version <= 65535
    ),
    artifact_hash BYTEA NOT NULL CHECK (octet_length(artifact_hash) = 32),
    source_artifact_hash BYTEA NOT NULL CHECK (octet_length(source_artifact_hash) = 32),
    result_payload JSONB NOT NULL,
    result_checksum BYTEA NOT NULL CHECK (octet_length(result_checksum) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (request_id, method_version)
);

CREATE INDEX cancer_patient_derived_molecular_qualifications_world_created
    ON cancer_patient_derived_molecular_qualifications (
        world_id, created_at DESC, qualification_id
    );

CREATE FUNCTION validate_cancer_patient_derived_molecular_qualification_insert()
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
        RAISE EXCEPTION 'patient-derived qualification must reference a molecularly targeted artifact in the same world';
    END IF;
    IF NOT (NEW.result_payload ?& ARRAY[
           'schema_version','method_version','qualification_id','world_id','request_id',
           'artifact_hash','source','pdc_study_id','study_version_id','source_file_id',
           'source_file_md5','cohort_model_count','target_observations','limitations'
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
       OR NEW.result_payload->>'pdc_study_id' <> 'PDC000711'
       OR (NEW.result_payload->>'cohort_model_count')::INTEGER <= 0
       OR JSONB_TYPEOF(NEW.result_payload->'target_observations') <> 'array'
       OR JSONB_ARRAY_LENGTH(NEW.result_payload->'target_observations') = 0
       OR JSONB_TYPEOF(NEW.result_payload->'limitations') <> 'array'
    THEN
        RAISE EXCEPTION 'patient-derived qualification columns disagree with its immutable payload';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_patient_derived_molecular_qualifications_validate_insert
BEFORE INSERT ON cancer_patient_derived_molecular_qualifications
FOR EACH ROW EXECUTE FUNCTION validate_cancer_patient_derived_molecular_qualification_insert();

CREATE TRIGGER cancer_patient_derived_molecular_qualifications_are_immutable
BEFORE UPDATE OR DELETE ON cancer_patient_derived_molecular_qualifications
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

COMMENT ON TABLE cancer_patient_derived_molecular_qualifications IS
'Immutable observer-side exact-target lookup against a patient-derived GBM proteome; molecular presence only, never treatment response, efficacy, safety, or clinical evidence.';
