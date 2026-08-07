CREATE TABLE cognition_requests (
    request_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    agent_id UUID NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_event_id UUID NOT NULL,
    source_event_index BIGINT NOT NULL CHECK (
        source_event_index >= 0 AND source_event_index <= 4294967295
    ),
    selected_tick BIGINT NOT NULL CHECK (selected_tick >= 0),
    deadline_tick BIGINT NOT NULL CHECK (deadline_tick > selected_tick),
    ordinal BIGINT NOT NULL CHECK (ordinal >= 0 AND ordinal <= 4294967295),
    selection_schema_version INTEGER NOT NULL CHECK (
        selection_schema_version > 0 AND selection_schema_version <= 65535
    ),
    selection JSONB NOT NULL,
    selection_checksum BYTEA NOT NULL CHECK (octet_length(selection_checksum) = 32),
    available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_by TEXT,
    claimed_at TIMESTAMPTZ,
    claim_count BIGINT NOT NULL DEFAULT 0 CHECK (claim_count >= 0),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (world_id, source_sequence)
        REFERENCES event_batches (world_id, sequence),
    UNIQUE (world_id, source_event_id),
    CHECK (
        (claimed_by IS NULL AND claimed_at IS NULL)
        OR (claimed_by IS NOT NULL AND claimed_at IS NOT NULL)
    )
);

CREATE INDEX cognition_requests_claimable
    ON cognition_requests (available_at, created_at, request_id);

CREATE TABLE cognition_recall_outcomes (
    request_id UUID PRIMARY KEY REFERENCES cognition_requests (request_id),
    recall_request JSONB NOT NULL,
    recall_request_checksum BYTEA NOT NULL CHECK (
        octet_length(recall_request_checksum) = 32
    ),
    recall_outcome JSONB NOT NULL,
    recall_outcome_checksum BYTEA NOT NULL CHECK (
        octet_length(recall_outcome_checksum) = 32
    ),
    admitted_memory_inputs JSONB NOT NULL,
    admitted_memory_inputs_checksum BYTEA NOT NULL CHECK (
        octet_length(admitted_memory_inputs_checksum) = 32
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE cognition_route_attempts (
    request_id UUID NOT NULL REFERENCES cognition_requests (request_id),
    route_index INTEGER NOT NULL CHECK (route_index >= 0 AND route_index <= 255),
    provider_slug TEXT NOT NULL CHECK (provider_slug ~ '^[a-z0-9_-]{1,64}$'),
    requested_model TEXT NOT NULL CHECK (
        requested_model = BTRIM(requested_model)
        AND length(requested_model) BETWEEN 1 AND 256
    ),
    billing_class TEXT NOT NULL CHECK (
        billing_class IN ('free_allocation', 'trial_credit', 'development_only', 'paid_approved')
    ),
    dispatch_state TEXT NOT NULL CHECK (
        dispatch_state IN ('skipped', 'dispatched', 'completed', 'abandoned')
    ),
    network_dispatched BOOLEAN NOT NULL,
    normalized_status TEXT CHECK (
        normalized_status IN (
            'succeeded',
            'unavailable',
            'rejected',
            'invalid_response',
            'skipped_unconfigured',
            'skipped_cooldown',
            'skipped_quota_exhausted',
            'skipped_disabled',
            'skipped_paid_unauthorized',
            'stopped_attempt_limit'
        )
    ),
    attempt_payload JSONB,
    attempt_checksum BYTEA CHECK (
        attempt_checksum IS NULL OR octet_length(attempt_checksum) = 32
    ),
    receipt_payload JSONB,
    dispatched_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (request_id, route_index),
    CHECK (
        (
            dispatch_state = 'dispatched'
            AND network_dispatched
            AND dispatched_at IS NOT NULL
            AND completed_at IS NULL
            AND normalized_status IS NULL
            AND attempt_payload IS NULL
            AND attempt_checksum IS NULL
            AND receipt_payload IS NULL
        )
        OR (
            dispatch_state IN ('completed', 'abandoned')
            AND network_dispatched
            AND dispatched_at IS NOT NULL
            AND completed_at IS NOT NULL
            AND normalized_status IN (
                'succeeded', 'unavailable', 'rejected', 'invalid_response'
            )
            AND attempt_payload IS NOT NULL
            AND attempt_checksum IS NOT NULL
            AND (
                (normalized_status = 'succeeded' AND receipt_payload IS NOT NULL)
                OR (normalized_status <> 'succeeded' AND receipt_payload IS NULL)
            )
        )
        OR (
            dispatch_state = 'skipped'
            AND NOT network_dispatched
            AND dispatched_at IS NULL
            AND completed_at IS NOT NULL
            AND normalized_status IN (
                'skipped_unconfigured',
                'skipped_cooldown',
                'skipped_quota_exhausted',
                'skipped_disabled',
                'skipped_paid_unauthorized',
                'stopped_attempt_limit'
            )
            AND attempt_payload IS NOT NULL
            AND attempt_checksum IS NOT NULL
            AND receipt_payload IS NULL
        )
    )
);

CREATE TABLE cognition_results (
    request_id UUID PRIMARY KEY REFERENCES cognition_requests (request_id),
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

CREATE TABLE cognition_cost_accounts (
    billing_month DATE PRIMARY KEY CHECK (
        billing_month = date_trunc('month', billing_month)::DATE
    ),
    target_micro_usd BIGINT NOT NULL CHECK (
        target_micro_usd >= 0 AND target_micro_usd <= 1000000000000
    ),
    hard_stop_micro_usd BIGINT NOT NULL CHECK (
        hard_stop_micro_usd >= target_micro_usd
        AND hard_stop_micro_usd <= 1000000000000
    ),
    reserved_micro_usd BIGINT NOT NULL DEFAULT 0 CHECK (
        reserved_micro_usd >= 0 AND reserved_micro_usd <= 1000000000000
    ),
    spent_micro_usd BIGINT NOT NULL DEFAULT 0 CHECK (
        spent_micro_usd >= 0 AND spent_micro_usd <= 1000000000000
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (reserved_micro_usd + spent_micro_usd <= hard_stop_micro_usd)
);

CREATE TABLE cognition_cost_reservations (
    request_id UUID PRIMARY KEY REFERENCES cognition_requests (request_id),
    billing_month DATE NOT NULL REFERENCES cognition_cost_accounts (billing_month),
    reserved_micro_usd BIGINT NOT NULL CHECK (reserved_micro_usd > 0),
    status TEXT NOT NULL CHECK (
        status IN ('reserved', 'settled', 'released', 'indeterminate')
    ),
    actual_micro_usd BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    CHECK (
        (status = 'reserved' AND actual_micro_usd IS NULL AND resolved_at IS NULL)
        OR (
            status = 'settled'
            AND actual_micro_usd IS NOT NULL
            AND actual_micro_usd >= 0
            AND actual_micro_usd <= reserved_micro_usd
            AND resolved_at IS NOT NULL
        )
        OR (
            status IN ('released', 'indeterminate')
            AND actual_micro_usd IS NULL
            AND resolved_at IS NOT NULL
        )
    )
);

CREATE TABLE cognition_deadline_latches (
    request_id UUID PRIMARY KEY REFERENCES cognition_requests (request_id),
    world_id UUID NOT NULL REFERENCES worlds (id),
    deadline_tick BIGINT NOT NULL CHECK (deadline_tick >= 0),
    target_sequence BIGINT NOT NULL CHECK (target_sequence > 0),
    latch_kind TEXT NOT NULL CHECK (latch_kind IN ('model_result', 'unavailable')),
    latch_payload JSONB NOT NULL,
    latch_checksum BYTEA NOT NULL CHECK (octet_length(latch_checksum) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (world_id, target_sequence, request_id)
);

CREATE TABLE cognition_latch_consumptions (
    request_id UUID PRIMARY KEY REFERENCES cognition_deadline_latches (request_id),
    world_id UUID NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_event_id UUID NOT NULL,
    source_event_index BIGINT NOT NULL CHECK (
        source_event_index >= 0 AND source_event_index <= 4294967295
    ),
    latch_checksum BYTEA NOT NULL CHECK (octet_length(latch_checksum) = 32),
    consumed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (world_id, source_sequence)
        REFERENCES event_batches (world_id, sequence),
    UNIQUE (world_id, source_event_id)
);

CREATE FUNCTION protect_cognition_request_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'cognition request history cannot be deleted';
    END IF;
    IF NEW.request_id IS DISTINCT FROM OLD.request_id
       OR NEW.world_id IS DISTINCT FROM OLD.world_id
       OR NEW.agent_id IS DISTINCT FROM OLD.agent_id
       OR NEW.source_sequence IS DISTINCT FROM OLD.source_sequence
       OR NEW.source_event_id IS DISTINCT FROM OLD.source_event_id
       OR NEW.source_event_index IS DISTINCT FROM OLD.source_event_index
       OR NEW.selected_tick IS DISTINCT FROM OLD.selected_tick
       OR NEW.deadline_tick IS DISTINCT FROM OLD.deadline_tick
       OR NEW.ordinal IS DISTINCT FROM OLD.ordinal
       OR NEW.selection_schema_version IS DISTINCT FROM OLD.selection_schema_version
       OR NEW.selection IS DISTINCT FROM OLD.selection
       OR NEW.selection_checksum IS DISTINCT FROM OLD.selection_checksum
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'cognition request provenance and selection are immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cognition_requests_preserve_history
BEFORE UPDATE OR DELETE ON cognition_requests
FOR EACH ROW EXECUTE FUNCTION protect_cognition_request_history();

CREATE FUNCTION reject_cognition_immutable_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'completed cognition inputs and results are immutable';
END
$$;

CREATE TRIGGER cognition_recall_outcomes_are_immutable
BEFORE UPDATE OR DELETE ON cognition_recall_outcomes
FOR EACH ROW EXECUTE FUNCTION reject_cognition_immutable_mutation();

CREATE TRIGGER cognition_results_are_immutable
BEFORE UPDATE OR DELETE ON cognition_results
FOR EACH ROW EXECUTE FUNCTION reject_cognition_immutable_mutation();

CREATE TRIGGER cognition_deadline_latches_are_immutable
BEFORE UPDATE OR DELETE ON cognition_deadline_latches
FOR EACH ROW EXECUTE FUNCTION reject_cognition_immutable_mutation();

CREATE TRIGGER cognition_latch_consumptions_are_immutable
BEFORE UPDATE OR DELETE ON cognition_latch_consumptions
FOR EACH ROW EXECUTE FUNCTION reject_cognition_immutable_mutation();

CREATE FUNCTION validate_cognition_attempt_insert()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM 1
    FROM cognition_requests
    WHERE request_id = NEW.request_id
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'cognition request does not exist';
    END IF;
    IF NEW.route_index > 0 AND NOT EXISTS (
        SELECT 1
        FROM cognition_route_attempts
        WHERE request_id = NEW.request_id
          AND route_index = NEW.route_index - 1
          AND dispatch_state IN ('skipped', 'completed', 'abandoned')
    ) THEN
        RAISE EXCEPTION 'cognition route attempts must be a completed prefix';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM cognition_route_attempts
        WHERE request_id = NEW.request_id
          AND normalized_status IN ('succeeded', 'stopped_attempt_limit')
    ) THEN
        RAISE EXCEPTION 'cognition route attempts already terminated';
    END IF;
    IF NEW.network_dispatched AND (
        SELECT COUNT(*)
        FROM cognition_route_attempts
        WHERE request_id = NEW.request_id
          AND network_dispatched
    ) >= 16 THEN
        RAISE EXCEPTION 'cognition network attempt limit reached';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cognition_route_attempts_validate_insert
BEFORE INSERT ON cognition_route_attempts
FOR EACH ROW EXECUTE FUNCTION validate_cognition_attempt_insert();

CREATE FUNCTION protect_cognition_attempt_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'cognition route attempt history cannot be deleted';
    END IF;
    IF NEW.request_id IS DISTINCT FROM OLD.request_id
       OR NEW.route_index IS DISTINCT FROM OLD.route_index
       OR NEW.provider_slug IS DISTINCT FROM OLD.provider_slug
       OR NEW.requested_model IS DISTINCT FROM OLD.requested_model
       OR NEW.billing_class IS DISTINCT FROM OLD.billing_class
       OR NEW.network_dispatched IS DISTINCT FROM OLD.network_dispatched
       OR NEW.dispatched_at IS DISTINCT FROM OLD.dispatched_at
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'cognition route attempt provenance is immutable';
    END IF;
    IF OLD.dispatch_state <> 'dispatched'
       OR NEW.dispatch_state NOT IN ('completed', 'abandoned') THEN
        RAISE EXCEPTION 'terminal cognition route attempts are immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cognition_route_attempts_preserve_history
BEFORE UPDATE OR DELETE ON cognition_route_attempts
FOR EACH ROW EXECUTE FUNCTION protect_cognition_attempt_history();

CREATE FUNCTION protect_cognition_cost_reservation_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'cognition cost reservation history cannot be deleted';
    END IF;
    IF NEW.request_id IS DISTINCT FROM OLD.request_id
       OR NEW.billing_month IS DISTINCT FROM OLD.billing_month
       OR NEW.reserved_micro_usd IS DISTINCT FROM OLD.reserved_micro_usd
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'cognition cost reservation provenance is immutable';
    END IF;
    IF OLD.status <> 'reserved'
       OR NEW.status NOT IN ('settled', 'released', 'indeterminate') THEN
        RAISE EXCEPTION 'resolved cognition cost reservations are immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cognition_cost_reservations_preserve_history
BEFORE UPDATE OR DELETE ON cognition_cost_reservations
FOR EACH ROW EXECUTE FUNCTION protect_cognition_cost_reservation_history();

COMMENT ON TABLE cognition_requests
IS 'Canonical simulated-time cognition selections inserted atomically with their source event batch.';
COMMENT ON TABLE cognition_route_attempts
IS 'Durable route-by-route prefix; network dispatch is recorded before the external call.';
COMMENT ON TABLE cognition_deadline_latches
IS 'Immutable exact external input or absence chosen at the canonical simulation deadline.';
COMMENT ON COLUMN cognition_requests.claimed_at
IS 'Wall-clock worker lease only; never a simulation input.';
