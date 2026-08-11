CREATE TABLE cancer_research_requests (
    request_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    resident_id UUID NOT NULL,
    selected_tick BIGINT NOT NULL CHECK (selected_tick >= 0),
    deadline_tick BIGINT NOT NULL CHECK (deadline_tick > selected_tick),
    ordinal BIGINT NOT NULL CHECK (ordinal >= 0 AND ordinal <= 4294967295),
    stage TEXT NOT NULL CHECK (
        stage IN ('blind_discovery', 'literature_audit', 'independent_replication')
    ),
    inference_tier TEXT NOT NULL CHECK (
        inference_tier IN ('exploration', 'escalation')
    ),
    request_payload JSONB NOT NULL,
    request_checksum BYTEA NOT NULL CHECK (octet_length(request_checksum) = 32),
    available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_by TEXT,
    claimed_at TIMESTAMPTZ,
    claim_count BIGINT NOT NULL DEFAULT 0 CHECK (claim_count >= 0),
    last_error TEXT,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (
        (claimed_by IS NULL AND claimed_at IS NULL)
        OR (claimed_by IS NOT NULL AND claimed_at IS NOT NULL)
    )
);

CREATE INDEX cancer_research_requests_claimable
    ON cancer_research_requests (available_at, created_at, request_id)
    WHERE completed_at IS NULL;

CREATE TABLE cancer_research_route_dispatches (
    request_id UUID NOT NULL REFERENCES cancer_research_requests (request_id),
    route_index INTEGER NOT NULL CHECK (route_index >= 0 AND route_index < 4),
    provider_slug TEXT NOT NULL CHECK (provider_slug = 'openrouter_cancer'),
    requested_model TEXT NOT NULL CHECK (
        requested_model = BTRIM(requested_model)
        AND length(requested_model) BETWEEN 1 AND 256
    ),
    billing_class TEXT NOT NULL CHECK (
        billing_class IN ('free_allocation', 'paid_approved')
    ),
    route_payload JSONB NOT NULL,
    route_checksum BYTEA NOT NULL CHECK (octet_length(route_checksum) = 32),
    dispatched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (request_id, route_index)
);

CREATE TABLE cancer_research_route_outcomes (
    request_id UUID NOT NULL,
    route_index INTEGER NOT NULL,
    normalized_status TEXT NOT NULL CHECK (
        normalized_status IN ('succeeded', 'unavailable', 'rejected', 'invalid_response')
    ),
    attempt_payload JSONB NOT NULL,
    attempt_checksum BYTEA NOT NULL CHECK (octet_length(attempt_checksum) = 32),
    receipt_payload JSONB,
    receipt_checksum BYTEA CHECK (
        receipt_checksum IS NULL OR octet_length(receipt_checksum) = 32
    ),
    completed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (request_id, route_index),
    FOREIGN KEY (request_id, route_index)
        REFERENCES cancer_research_route_dispatches (request_id, route_index),
    CHECK (
        (normalized_status = 'succeeded' AND receipt_payload IS NOT NULL AND receipt_checksum IS NOT NULL)
        OR
        (normalized_status <> 'succeeded' AND receipt_payload IS NULL AND receipt_checksum IS NULL)
    )
);

CREATE TABLE cancer_research_results (
    request_id UUID PRIMARY KEY REFERENCES cancer_research_requests (request_id),
    route_policy_version INTEGER NOT NULL CHECK (
        route_policy_version > 0 AND route_policy_version <= 65535
    ),
    route_registry_checksum BYTEA NOT NULL CHECK (
        octet_length(route_registry_checksum) = 32
    ),
    result_payload JSONB NOT NULL,
    result_checksum BYTEA NOT NULL CHECK (octet_length(result_checksum) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE FUNCTION protect_cancer_research_request_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'cancer research request history cannot be deleted';
    END IF;
    IF NEW.request_id IS DISTINCT FROM OLD.request_id
       OR NEW.world_id IS DISTINCT FROM OLD.world_id
       OR NEW.resident_id IS DISTINCT FROM OLD.resident_id
       OR NEW.selected_tick IS DISTINCT FROM OLD.selected_tick
       OR NEW.deadline_tick IS DISTINCT FROM OLD.deadline_tick
       OR NEW.ordinal IS DISTINCT FROM OLD.ordinal
       OR NEW.stage IS DISTINCT FROM OLD.stage
       OR NEW.inference_tier IS DISTINCT FROM OLD.inference_tier
       OR NEW.request_payload IS DISTINCT FROM OLD.request_payload
       OR NEW.request_checksum IS DISTINCT FROM OLD.request_checksum
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'cancer research request provenance is immutable';
    END IF;
    IF OLD.completed_at IS NOT NULL THEN
        RAISE EXCEPTION 'completed cancer research requests are immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_research_requests_preserve_history
BEFORE UPDATE OR DELETE ON cancer_research_requests
FOR EACH ROW EXECUTE FUNCTION protect_cancer_research_request_history();

CREATE FUNCTION reject_cancer_research_immutable_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'cancer research dispatches, outcomes, and results are immutable';
END
$$;

CREATE TRIGGER cancer_research_dispatches_are_immutable
BEFORE UPDATE OR DELETE ON cancer_research_route_dispatches
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

CREATE TRIGGER cancer_research_outcomes_are_immutable
BEFORE UPDATE OR DELETE ON cancer_research_route_outcomes
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

CREATE TRIGGER cancer_research_results_are_immutable
BEFORE UPDATE OR DELETE ON cancer_research_results
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_immutable_mutation();

CREATE FUNCTION validate_cancer_research_dispatch_insert()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM 1
    FROM cancer_research_requests
    WHERE request_id = NEW.request_id
      AND completed_at IS NULL
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'claimable cancer research request does not exist';
    END IF;
    IF NEW.route_index > 0 AND NOT EXISTS (
        SELECT 1
        FROM cancer_research_route_outcomes
        WHERE request_id = NEW.request_id
          AND route_index = NEW.route_index - 1
          AND normalized_status <> 'succeeded'
    ) THEN
        RAISE EXCEPTION 'cancer research route dispatches must be a failed prefix';
    END IF;
    IF EXISTS (
        SELECT 1 FROM cancer_research_route_outcomes
        WHERE request_id = NEW.request_id AND normalized_status = 'succeeded'
    ) THEN
        RAISE EXCEPTION 'cancer research route dispatches already succeeded';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_research_dispatches_validate_insert
BEFORE INSERT ON cancer_research_route_dispatches
FOR EACH ROW EXECUTE FUNCTION validate_cancer_research_dispatch_insert();

COMMENT ON TABLE cancer_research_requests
IS 'Exact content-addressed Cancer World research turns; operational claims may change but research inputs may not.';
COMMENT ON TABLE cancer_research_route_dispatches
IS 'Immutable proof recorded before each external Cancer World model call.';
COMMENT ON TABLE cancer_research_route_outcomes
IS 'Immutable terminal outcome paired with a prior external-call dispatch.';
COMMENT ON TABLE cancer_research_results
IS 'Immutable final route-ladder result and successful research contribution, if any.';
