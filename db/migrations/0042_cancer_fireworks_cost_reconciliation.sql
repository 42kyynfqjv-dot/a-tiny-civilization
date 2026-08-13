-- A provider export may conclusively price a call that crashed after its
-- immutable dispatch but before its response receipt was recorded. Preserve
-- that original indeterminate reservation and append the external evidence.
CREATE TABLE cancer_research_fireworks_cost_reconciliations (
    reconciliation_id UUID PRIMARY KEY,
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    request_id UUID NOT NULL UNIQUE
        REFERENCES cancer_research_cost_reservations (request_id),
    route_index INTEGER NOT NULL CHECK (route_index >= 0 AND route_index < 4),
    billing_month DATE NOT NULL CHECK (
        billing_month = date_trunc('month', billing_month)::DATE
    ),
    source_format TEXT NOT NULL CHECK (
        source_format = 'fireworks_firectl_billing_export_metrics_csv_v1'
    ),
    export_sha256 BYTEA NOT NULL CHECK (octet_length(export_sha256) = 32),
    export_byte_length BIGINT NOT NULL CHECK (
        export_byte_length > 0 AND export_byte_length <= 134217728
    ),
    row_sha256 BYTEA NOT NULL UNIQUE CHECK (octet_length(row_sha256) = 32),
    row_start_offset BIGINT NOT NULL CHECK (row_start_offset >= 0),
    row_byte_length BIGINT NOT NULL CHECK (row_byte_length > 0),
    provider_started_at TIMESTAMPTZ NOT NULL,
    matched_dispatch_at TIMESTAMPTZ NOT NULL,
    requested_model TEXT NOT NULL CHECK (
        requested_model = 'accounts/fireworks/models/gpt-oss-20b'
    ),
    prompt_tokens BIGINT NOT NULL CHECK (prompt_tokens >= 0 AND prompt_tokens <= 4294967295),
    completion_tokens BIGINT NOT NULL CHECK (
        completion_tokens >= 0 AND completion_tokens <= 4294967295
    ),
    actual_micro_usd BIGINT NOT NULL CHECK (actual_micro_usd > 0),
    reserved_micro_usd BIGINT NOT NULL CHECK (
        reserved_micro_usd > 0 AND reserved_micro_usd <= 250000
    ),
    released_micro_usd BIGINT NOT NULL CHECK (released_micro_usd >= 0),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (request_id, route_index)
        REFERENCES cancer_research_route_dispatches (request_id, route_index),
    CHECK (prompt_tokens > 0 OR completion_tokens > 0),
    CHECK (actual_micro_usd + released_micro_usd = reserved_micro_usd),
    CHECK (row_start_offset + row_byte_length <= export_byte_length)
);

CREATE TABLE cancer_research_fireworks_reconciliation_exports (
    export_sha256 BYTEA PRIMARY KEY CHECK (octet_length(export_sha256) = 32),
    export_byte_length BIGINT NOT NULL CHECK (
        export_byte_length > 0 AND export_byte_length <= 134217728
    ),
    source_format TEXT NOT NULL CHECK (
        source_format = 'fireworks_firectl_billing_export_metrics_csv_v1'
    ),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE cancer_research_fireworks_reconciliation_exports
    ADD CONSTRAINT cancer_research_fireworks_exports_identity_unique
    UNIQUE (export_sha256, export_byte_length);

ALTER TABLE cancer_research_fireworks_cost_reconciliations
    ADD FOREIGN KEY (export_sha256, export_byte_length)
    REFERENCES cancer_research_fireworks_reconciliation_exports (
        export_sha256, export_byte_length
    );

CREATE FUNCTION validate_cancer_research_fireworks_cost_reconciliation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    reservation cancer_research_cost_reservations%ROWTYPE;
    dispatch cancer_research_route_dispatches%ROWTYPE;
    expected_actual BIGINT;
    matching_dispatches BIGINT;
BEGIN
    IF NEW.export_sha256 = decode(repeat('00', 32), 'hex')
       OR NEW.row_sha256 = decode(repeat('00', 32), 'hex') THEN
        RAISE EXCEPTION 'Fireworks reconciliation hashes cannot be zero';
    END IF;
    IF NEW.row_start_offset > NEW.export_byte_length
       OR NEW.row_byte_length > NEW.export_byte_length
       OR NEW.row_start_offset + NEW.row_byte_length > NEW.export_byte_length THEN
        RAISE EXCEPTION 'Fireworks reconciliation row range exceeds the bounded export';
    END IF;

    SELECT * INTO reservation
    FROM cancer_research_cost_reservations
    WHERE request_id = NEW.request_id
    FOR UPDATE;
    IF NOT FOUND
       OR reservation.status <> 'indeterminate'
       OR reservation.actual_micro_usd IS NOT NULL
       OR reservation.billing_scope <> 'cancer_research'
       OR reservation.billing_month <> NEW.billing_month
       OR reservation.reserved_micro_usd <> NEW.reserved_micro_usd THEN
        RAISE EXCEPTION 'Fireworks reconciliation requires the exact indeterminate reservation';
    END IF;

    SELECT * INTO dispatch
    FROM cancer_research_route_dispatches
    WHERE request_id = NEW.request_id AND route_index = NEW.route_index;
    IF NOT FOUND
       OR dispatch.provider_slug <> 'fireworks_cancer'
       OR dispatch.billing_class <> 'paid_approved'
       OR dispatch.requested_model <> 'accounts/fireworks/models/gpt-oss-20b'
       OR dispatch.requested_model <> NEW.requested_model
       OR dispatch.dispatched_at <> NEW.matched_dispatch_at
       OR ABS(EXTRACT(EPOCH FROM (NEW.provider_started_at - dispatch.dispatched_at))) > 5 THEN
        RAISE EXCEPTION 'Fireworks billing row does not tightly match its paid dispatch';
    END IF;

    -- The provider export has no application request ID. A timestamp match is
    -- admissible only when exactly one still-unreconciled Fireworks dispatch is
    -- in the five-second window. This deliberately refuses guessed pairing.
    SELECT COUNT(*) INTO matching_dispatches
    FROM cancer_research_route_dispatches AS candidate_dispatch
    JOIN cancer_research_cost_reservations AS candidate_reservation
      ON candidate_reservation.request_id = candidate_dispatch.request_id
    LEFT JOIN cancer_research_fireworks_cost_reconciliations AS existing
      ON existing.request_id = candidate_dispatch.request_id
    WHERE candidate_dispatch.provider_slug = 'fireworks_cancer'
      AND candidate_dispatch.billing_class = 'paid_approved'
      AND candidate_dispatch.requested_model = NEW.requested_model
      AND candidate_reservation.status = 'indeterminate'
      AND existing.request_id IS NULL
      AND ABS(EXTRACT(EPOCH FROM (
          NEW.provider_started_at - candidate_dispatch.dispatched_at
      ))) <= 5;
    IF matching_dispatches <> 1 THEN
        RAISE EXCEPTION 'Fireworks billing row has % matching indeterminate dispatches, expected exactly one', matching_dispatches;
    END IF;

    -- Same tariff as the runtime adapter: $0.07/M uncached input and $0.30/M
    -- output. Integer arithmetic rounds up to the next micro-dollar.
    expected_actual := (
        NEW.prompt_tokens * 70000
        + NEW.completion_tokens * 300000
        + 999999
    ) / 1000000;
    IF NEW.actual_micro_usd <> expected_actual
       OR NEW.actual_micro_usd > reservation.reserved_micro_usd
       OR NEW.released_micro_usd <> reservation.reserved_micro_usd - expected_actual THEN
        RAISE EXCEPTION 'Fireworks reconciliation cost disagrees with the pinned tariff';
    END IF;

    -- The account is a mutable aggregate, not raw history. Move the verified
    -- actual charge from reserved to spent; net capacity increases by exactly
    -- released_micro_usd while the original reservation stays indeterminate.
    UPDATE cognition_cost_accounts
    SET reserved_micro_usd = reserved_micro_usd - reservation.reserved_micro_usd,
        spent_micro_usd = spent_micro_usd + expected_actual,
        updated_at = NOW()
    WHERE billing_scope = 'cancer_research'
      AND billing_month = reservation.billing_month
      AND reserved_micro_usd >= reservation.reserved_micro_usd;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Cancer research cost account cannot apply Fireworks reconciliation';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_research_fireworks_reconciliations_validate
BEFORE INSERT ON cancer_research_fireworks_cost_reconciliations
FOR EACH ROW EXECUTE FUNCTION validate_cancer_research_fireworks_cost_reconciliation();

CREATE FUNCTION reject_cancer_research_fireworks_reconciliation_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'Fireworks cost reconciliations are append-only';
END
$$;

CREATE TRIGGER cancer_research_fireworks_reconciliations_are_immutable
BEFORE UPDATE OR DELETE ON cancer_research_fireworks_cost_reconciliations
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_fireworks_reconciliation_mutation();

CREATE TRIGGER cancer_research_fireworks_reconciliations_cannot_be_truncated
BEFORE TRUNCATE ON cancer_research_fireworks_cost_reconciliations
FOR EACH STATEMENT EXECUTE FUNCTION reject_cancer_research_fireworks_reconciliation_mutation();

CREATE TRIGGER cancer_research_fireworks_exports_are_immutable
BEFORE UPDATE OR DELETE ON cancer_research_fireworks_reconciliation_exports
FOR EACH ROW EXECUTE FUNCTION reject_cancer_research_fireworks_reconciliation_mutation();

CREATE TRIGGER cancer_research_fireworks_exports_cannot_be_truncated
BEFORE TRUNCATE ON cancer_research_fireworks_reconciliation_exports
FOR EACH STATEMENT EXECUTE FUNCTION reject_cancer_research_fireworks_reconciliation_mutation();

COMMENT ON TABLE cancer_research_fireworks_cost_reconciliations
IS 'Append-only proof that one exact authoritative Fireworks billing-export row resolved an otherwise immutable indeterminate Cancer World reservation.';
COMMENT ON TABLE cancer_research_fireworks_reconciliation_exports
IS 'Append-only whole-file identity and byte length for an operator-supplied authoritative Fireworks billing export; raw account data is never retained.';
COMMENT ON COLUMN cancer_research_fireworks_cost_reconciliations.row_sha256
IS 'SHA-256 of the exact raw CSV record bytes, including its original line terminator when present; no email or other account metadata is retained.';
COMMENT ON COLUMN cancer_research_fireworks_cost_reconciliations.released_micro_usd
IS 'Only this verified reserved-minus-actual difference restores future paid Cancer World capacity.';
