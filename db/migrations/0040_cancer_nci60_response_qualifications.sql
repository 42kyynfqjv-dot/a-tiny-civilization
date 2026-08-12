CREATE TABLE cancer_nci60_response_qualifications (
    qualification_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    request_id UUID NOT NULL REFERENCES cancer_research_results (request_id),
    method_version INTEGER NOT NULL CHECK (
        method_version > 0 AND method_version <= 65535
    ),
    artifact_hash BYTEA NOT NULL CHECK (octet_length(artifact_hash) = 32),
    prediction_hash BYTEA NOT NULL CHECK (octet_length(prediction_hash) = 32),
    challenge_id UUID NOT NULL,
    answer_key_hash BYTEA NOT NULL CHECK (octet_length(answer_key_hash) = 32),
    result_payload JSONB NOT NULL,
    result_checksum BYTEA NOT NULL CHECK (octet_length(result_checksum) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (request_id, method_version)
);

CREATE INDEX cancer_nci60_response_qualifications_world_created
    ON cancer_nci60_response_qualifications (world_id, created_at DESC, qualification_id);

CREATE FUNCTION validate_cancer_nci60_response_qualification_insert()
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
          AND result.result_payload->'receipt'->'contribution'->'nci60_response_prediction' IS NOT NULL
          AND result.result_payload->'receipt'->'contribution'->'nci60_response_prediction'->>'challenge_id'
              = NEW.challenge_id::TEXT
          AND result.result_payload->'receipt'->'contribution'->'nci60_response_prediction'->'intervention'
              = NEW.result_payload->'intervention'
    ) THEN
        RAISE EXCEPTION 'NCI-60 qualification must reference a predicted successful artifact in the same world';
    END IF;
    IF NOT (NEW.result_payload ?& ARRAY[
           'schema_version','method_version','qualification_id','world_id','request_id',
           'artifact_hash','prediction_hash','challenge_id','intervention','answer_key',
           'pairwise_comparison_count','concordant_pair_count',
           'pairwise_concordance_per_mille','most_responsive_line_correct',
           'least_responsive_line_correct'
       ])
       OR NOT (NEW.result_payload->'answer_key' ?& ARRAY['kind','content_hash'])
       OR NEW.result_payload->>'schema_version' <> '1'
       OR (NEW.result_payload->>'method_version')::INTEGER <> NEW.method_version
       OR NEW.result_payload->>'qualification_id' <> NEW.qualification_id::TEXT
       OR NEW.result_payload->>'world_id' <> NEW.world_id::TEXT
       OR NEW.result_payload->>'request_id' <> NEW.request_id::TEXT
       OR NEW.result_payload->>'artifact_hash' <> ENCODE(NEW.artifact_hash, 'hex')
       OR NEW.result_payload->>'prediction_hash' <> ENCODE(NEW.prediction_hash, 'hex')
       OR NEW.result_payload->>'challenge_id' <> NEW.challenge_id::TEXT
       OR NEW.result_payload->'answer_key'->>'content_hash' <> ENCODE(NEW.answer_key_hash, 'hex')
       OR NEW.result_payload->'answer_key'->>'kind' <> 'assay_observation'
       OR (NEW.result_payload->>'pairwise_comparison_count')::INTEGER NOT BETWEEN 0 AND 15
       OR (NEW.result_payload->>'concordant_pair_count')::INTEGER NOT BETWEEN 0
          AND (NEW.result_payload->>'pairwise_comparison_count')::INTEGER
       OR (
           (NEW.result_payload->>'pairwise_comparison_count')::INTEGER = 0
           AND (
               NEW.result_payload->'pairwise_concordance_per_mille' <> 'null'::JSONB
               OR NEW.result_payload->'most_responsive_line_correct' <> 'null'::JSONB
               OR NEW.result_payload->'least_responsive_line_correct' <> 'null'::JSONB
           )
       )
       OR (
           (NEW.result_payload->>'pairwise_comparison_count')::INTEGER > 0
           AND (
               (NEW.result_payload->>'pairwise_concordance_per_mille')::INTEGER
                   <> ((NEW.result_payload->>'concordant_pair_count')::INTEGER * 1000)
                      / NULLIF((NEW.result_payload->>'pairwise_comparison_count')::INTEGER, 0)
               OR NEW.result_payload->'most_responsive_line_correct' NOT IN ('true'::JSONB, 'false'::JSONB)
               OR NEW.result_payload->'least_responsive_line_correct' NOT IN ('true'::JSONB, 'false'::JSONB)
           )
       )
    THEN
        RAISE EXCEPTION 'NCI-60 qualification columns or score disagree with its immutable payload';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_nci60_response_qualifications_validate_insert
BEFORE INSERT ON cancer_nci60_response_qualifications
FOR EACH ROW EXECUTE FUNCTION validate_cancer_nci60_response_qualification_insert();

CREATE TRIGGER cancer_nci60_response_qualifications_are_immutable
BEFORE UPDATE OR DELETE ON cancer_nci60_response_qualifications
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

COMMENT ON TABLE cancer_nci60_response_qualifications IS
'Immutable observer-side rank benchmark against a runtime-isolated public NCI-60/ALMANAC CNS answer key; in-vitro assay evidence only, never a treatment verdict or patient efficacy.';
