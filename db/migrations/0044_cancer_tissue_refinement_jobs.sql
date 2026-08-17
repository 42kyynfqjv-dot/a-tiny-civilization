CREATE TABLE cancer_tissue_refinement_jobs (
    refinement_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    campaign_id UUID NOT NULL,
    root_request_id UUID NOT NULL REFERENCES cancer_research_results (request_id),
    method_version INTEGER NOT NULL CHECK (
        method_version > 0 AND method_version <= 65535
    ),
    root_artifact_hash BYTEA NOT NULL CHECK (octet_length(root_artifact_hash) = 32),
    root_plan_hash BYTEA NOT NULL CHECK (octet_length(root_plan_hash) = 32),
    root_result_hash BYTEA NOT NULL CHECK (octet_length(root_result_hash) = 32),
    survival_synthesis_request_id UUID NOT NULL REFERENCES cancer_research_results (request_id),
    survival_synthesis_request_hash BYTEA NOT NULL CHECK (
        octet_length(survival_synthesis_request_hash) = 32
    ),
    survival_synthesis_result_hash BYTEA NOT NULL CHECK (
        octet_length(survival_synthesis_result_hash) = 32
    ),
    campaign_result_hashes BYTEA[] NOT NULL CHECK (
        cardinality(campaign_result_hashes) BETWEEN 3 AND 5
    ),
    protocol_payload JSONB NOT NULL,
    protocol_checksum BYTEA NOT NULL CHECK (octet_length(protocol_checksum) = 32),
    available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_by TEXT,
    claimed_at TIMESTAMPTZ,
    lease_until TIMESTAMPTZ,
    claim_token UUID,
    claim_count BIGINT NOT NULL DEFAULT 0 CHECK (claim_count >= 0),
    last_error TEXT,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (campaign_id, method_version),
    CHECK (
        (claimed_by IS NULL AND claimed_at IS NULL AND lease_until IS NULL AND claim_token IS NULL)
        OR
        (claimed_by IS NOT NULL AND claimed_at IS NOT NULL AND lease_until IS NOT NULL
         AND claim_token IS NOT NULL AND lease_until > claimed_at)
    )
);

CREATE TABLE cancer_tissue_refinement_results (
    refinement_id UUID PRIMARY KEY REFERENCES cancer_tissue_refinement_jobs (refinement_id),
    world_id UUID NOT NULL REFERENCES worlds (id),
    method_version INTEGER NOT NULL CHECK (
        method_version > 0 AND method_version <= 65535
    ),
    protocol_checksum BYTEA NOT NULL CHECK (octet_length(protocol_checksum) = 32),
    result_payload JSONB NOT NULL,
    result_checksum BYTEA NOT NULL CHECK (octet_length(result_checksum) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX cancer_tissue_refinement_jobs_claimable
    ON cancer_tissue_refinement_jobs (available_at, created_at, refinement_id)
    WHERE completed_at IS NULL;

CREATE INDEX cancer_tissue_refinement_results_world_created
    ON cancer_tissue_refinement_results (world_id, created_at DESC, refinement_id);

CREATE UNIQUE INDEX cancer_tissue_refinement_one_live_claim
    ON cancer_tissue_refinement_jobs ((true))
    WHERE claimed_by IS NOT NULL AND completed_at IS NULL;

CREATE FUNCTION protect_cancer_tissue_refinement_job_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'cancer tissue-refinement job history cannot be deleted';
    END IF;
    IF NEW.refinement_id IS DISTINCT FROM OLD.refinement_id
       OR NEW.world_id IS DISTINCT FROM OLD.world_id
       OR NEW.campaign_id IS DISTINCT FROM OLD.campaign_id
       OR NEW.root_request_id IS DISTINCT FROM OLD.root_request_id
       OR NEW.method_version IS DISTINCT FROM OLD.method_version
       OR NEW.root_artifact_hash IS DISTINCT FROM OLD.root_artifact_hash
       OR NEW.root_plan_hash IS DISTINCT FROM OLD.root_plan_hash
       OR NEW.root_result_hash IS DISTINCT FROM OLD.root_result_hash
       OR NEW.survival_synthesis_request_id IS DISTINCT FROM OLD.survival_synthesis_request_id
       OR NEW.survival_synthesis_request_hash IS DISTINCT FROM OLD.survival_synthesis_request_hash
       OR NEW.survival_synthesis_result_hash IS DISTINCT FROM OLD.survival_synthesis_result_hash
       OR NEW.campaign_result_hashes IS DISTINCT FROM OLD.campaign_result_hashes
       OR NEW.protocol_payload IS DISTINCT FROM OLD.protocol_payload
       OR NEW.protocol_checksum IS DISTINCT FROM OLD.protocol_checksum
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'cancer tissue-refinement protocol provenance is immutable';
    END IF;
    IF OLD.completed_at IS NOT NULL THEN
        RAISE EXCEPTION 'completed cancer tissue-refinement jobs are immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_tissue_refinement_jobs_preserve_history
BEFORE UPDATE OR DELETE ON cancer_tissue_refinement_jobs
FOR EACH ROW EXECUTE FUNCTION protect_cancer_tissue_refinement_job_history();

CREATE TRIGGER cancer_tissue_refinement_results_are_immutable
BEFORE UPDATE OR DELETE ON cancer_tissue_refinement_results
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

CREATE FUNCTION validate_cancer_tissue_refinement_job_insert()
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
      AND experiment.result_payload->>'interpretation'='model_supports_prediction';
    IF root_result_payload IS NULL THEN
        RAISE EXCEPTION 'tissue refinement requires an exact current method-2 supporting root';
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

CREATE TRIGGER cancer_tissue_refinement_jobs_validate_insert
BEFORE INSERT ON cancer_tissue_refinement_jobs
FOR EACH ROW EXECUTE FUNCTION validate_cancer_tissue_refinement_job_insert();

CREATE FUNCTION validate_cancer_tissue_refinement_result_insert()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM cancer_tissue_refinement_jobs AS job
        WHERE job.refinement_id=NEW.refinement_id
          AND job.world_id=NEW.world_id
          AND job.method_version=NEW.method_version
          AND job.protocol_checksum=NEW.protocol_checksum
          AND job.claimed_by IS NOT NULL
          AND job.claim_token IS NOT NULL
          AND job.lease_until > NOW()
          AND job.completed_at IS NULL
    ) THEN
        RAISE EXCEPTION 'tissue result requires the exact active unexpired protocol claim';
    END IF;
    IF NOT (NEW.result_payload ?& ARRAY[
           'schema_version','method_version','refinement_id','world_id','protocol_hash',
           'scenario_summaries','snapshots','uncertainty','evidence_class','caveats'
       ])
       OR NEW.result_payload->>'schema_version' <> '1'
       OR (NEW.result_payload->>'method_version')::INTEGER <> NEW.method_version
       OR NEW.result_payload->>'refinement_id' <> NEW.refinement_id::TEXT
       OR NEW.result_payload->>'world_id' <> NEW.world_id::TEXT
       OR NEW.result_payload->>'protocol_hash' <> ENCODE(NEW.protocol_checksum, 'hex')
       OR NEW.result_payload->>'evidence_class'
          <> 'uncalibrated_deterministic_tissue_projection'
       OR JSONB_ARRAY_LENGTH(NEW.result_payload->'scenario_summaries') <> 3
       OR JSONB_ARRAY_LENGTH(NEW.result_payload->'snapshots') > 48
       OR JSONB_ARRAY_LENGTH(NEW.result_payload->'caveats') <> 4
    THEN
        RAISE EXCEPTION 'tissue-refinement result columns disagree with immutable payload';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_tissue_refinement_results_validate_insert
BEFORE INSERT ON cancer_tissue_refinement_results
FOR EACH ROW EXECUTE FUNCTION validate_cancer_tissue_refinement_result_insert();

COMMENT ON TABLE cancer_tissue_refinement_jobs IS
'Immutable preregistered inputs plus an operational singleton lease for bounded Cancer World tissue projections.';
COMMENT ON TABLE cancer_tissue_refinement_results IS
'Immutable uncalibrated deterministic tissue projections; never research memory, wet-lab evidence, animal evidence, clinical efficacy, or cure claims.';
