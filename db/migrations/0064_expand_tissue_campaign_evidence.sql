-- Campaign directive v2 permits up to ten adversarial tests. Tissue protocol
-- v1 originally retained the legacy five-test ceiling, which would reject a
-- legitimate campaign whose third supporting result arrived during escalation.
ALTER TABLE cancer_tissue_refinement_jobs
    DROP CONSTRAINT cancer_tissue_refinement_jobs_campaign_result_hashes_check;

ALTER TABLE cancer_tissue_refinement_jobs
    ADD CONSTRAINT cancer_tissue_refinement_jobs_campaign_result_hashes_check
    CHECK (cardinality(campaign_result_hashes) BETWEEN 3 AND 10);

-- An inconclusive root is deliberately campaign-eligible: three later,
-- independently varied supporting tests can promote it. Admission therefore
-- validates the exact root provenance and accepted interpretation, while the
-- survived synthesis remains the evidence threshold.
CREATE OR REPLACE FUNCTION validate_cancer_tissue_refinement_job_insert()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    root_artifact_hex TEXT := ENCODE(NEW.root_artifact_hash, 'hex');
    root_result_payload JSONB;
    synthesis_request_payload JSONB;
    synthesis_result_payload JSONB;
    result_hash BYTEA;
BEGIN
    SELECT result.result_payload
    INTO root_result_payload
    FROM cancer_research_requests AS request
    JOIN cancer_research_results AS result USING (request_id)
    JOIN cancer_virtual_experiment_results AS experiment
      ON experiment.request_id=request.request_id
     AND experiment.method_version=2
    WHERE request.request_id=NEW.root_request_id
      AND request.world_id=NEW.world_id
      AND request.stage='blind_discovery'
      AND request.completed_at IS NOT NULL
      AND experiment.artifact_hash=NEW.root_artifact_hash
      AND experiment.plan_hash=NEW.root_plan_hash
      AND experiment.result_checksum=NEW.root_result_hash
      AND experiment.result_payload->>'interpretation' IN (
          'model_supports_prediction','model_inconclusive'
      );
    IF root_result_payload IS NULL THEN
        RAISE EXCEPTION 'tissue refinement requires an exact current method-2 campaign-eligible root';
    END IF;

    SELECT request.request_payload, result.result_payload
    INTO synthesis_request_payload, synthesis_result_payload
    FROM cancer_research_requests AS request
    JOIN cancer_research_results AS result USING (request_id)
    WHERE request.request_id=NEW.survival_synthesis_request_id
      AND request.world_id=NEW.world_id
      AND request.stage='independent_replication'
      AND request.completed_at IS NOT NULL
      AND request.request_checksum=NEW.survival_synthesis_request_hash
      AND result.result_checksum=NEW.survival_synthesis_result_hash
      AND request.request_payload->'selection'->>'frozen_candidate_hash'=root_artifact_hex
      AND request.request_payload->'selection'->>'task'='interpret_replication_result';
    IF synthesis_request_payload IS NULL
       OR synthesis_result_payload->'receipt' IS NULL
       OR NOT EXISTS (
           SELECT 1
           FROM JSONB_ARRAY_ELEMENTS(synthesis_request_payload->'evidence_documents') AS document
           WHERE document->'reference'->>'source_id' LIKE 'cancer-world://campaign-directive/%'
             AND ((document->>'content')::JSONB)->>'phase'='synthesis'
             AND ((document->>'content')::JSONB)->>'outcome'='survived_replication_round'
             AND (((document->>'content')::JSONB)->>'campaign_id')::UUID=NEW.campaign_id
             AND ((document->>'content')::JSONB)->>'root_artifact_hash'=root_artifact_hex
             AND (((document->>'content')::JSONB)->>'supporting_tests')::INTEGER >= 3
             AND (((document->>'content')::JSONB)->>'falsifying_tests')::INTEGER = 0
       ) THEN
        RAISE EXCEPTION 'tissue refinement requires an exact successful survived synthesis';
    END IF;

    IF NOT (NEW.protocol_payload ?& ARRAY[
           'schema_version','method_version','refinement_id','world_id','campaign_id',
           'root_request_id','root_artifact_hash','root_plan_hash','root_result_hash',
           'survival_synthesis_request_id','survival_synthesis_request_hash',
           'survival_synthesis_result_hash','campaign_result_hashes','field_model',
           'lattice_width','lattice_height','initial_cell_count','cell_capacity',
           'maximum_steps','snapshot_every_steps','requested_exposure_hours',
           'modeled_exposure_hours','horizon_truncated','scenarios'
       ])
       OR NEW.protocol_payload->>'schema_version' <> '1'
       OR (NEW.protocol_payload->>'method_version')::INTEGER <> NEW.method_version
       OR NEW.protocol_payload->>'refinement_id' <> NEW.refinement_id::TEXT
       OR NEW.protocol_payload->>'world_id' <> NEW.world_id::TEXT
       OR NEW.protocol_payload->>'campaign_id' <> NEW.campaign_id::TEXT
       OR NEW.protocol_payload->>'root_request_id' <> NEW.root_request_id::TEXT
       OR NEW.protocol_payload->>'root_artifact_hash' <> root_artifact_hex
       OR NEW.protocol_payload->>'root_plan_hash' <> ENCODE(NEW.root_plan_hash, 'hex')
       OR NEW.protocol_payload->>'root_result_hash' <> ENCODE(NEW.root_result_hash, 'hex')
       OR NEW.protocol_payload->>'survival_synthesis_request_id'
          <> NEW.survival_synthesis_request_id::TEXT
       OR NEW.protocol_payload->>'survival_synthesis_request_hash'
          <> ENCODE(NEW.survival_synthesis_request_hash, 'hex')
       OR NEW.protocol_payload->>'survival_synthesis_result_hash'
          <> ENCODE(NEW.survival_synthesis_result_hash, 'hex')
       OR JSONB_ARRAY_LENGTH(NEW.protocol_payload->'campaign_result_hashes')
          <> CARDINALITY(NEW.campaign_result_hashes)
    THEN
        RAISE EXCEPTION 'tissue-refinement protocol columns disagree with immutable payload';
    END IF;
    FOREACH result_hash IN ARRAY NEW.campaign_result_hashes LOOP
        IF OCTET_LENGTH(result_hash) <> 32 THEN
            RAISE EXCEPTION 'tissue-refinement campaign result hashes must be SHA-256 values';
        END IF;
    END LOOP;
    IF (
        SELECT COUNT(DISTINCT hash)
        FROM UNNEST(NEW.campaign_result_hashes) AS item(hash)
    ) <> CARDINALITY(NEW.campaign_result_hashes) THEN
        RAISE EXCEPTION 'tissue-refinement campaign result hashes must be distinct';
    END IF;
    IF NEW.protocol_payload->'campaign_result_hashes' <> (
        SELECT JSONB_AGG(ENCODE(hash, 'hex') ORDER BY ordinal)
        FROM UNNEST(NEW.campaign_result_hashes) WITH ORDINALITY AS item(hash, ordinal)
    ) THEN
        RAISE EXCEPTION 'tissue-refinement campaign hashes disagree with immutable payload';
    END IF;
    RETURN NEW;
END
$$;
