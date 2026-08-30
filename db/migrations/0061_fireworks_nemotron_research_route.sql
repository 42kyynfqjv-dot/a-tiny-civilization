-- Fireworks removed serverless support for the historical GPT-OSS 20B route.
-- Admit the replacement pinned serverless model without mutating old dispatches,
-- receipts, or schema-v1 reconciliation evidence.
ALTER TABLE cancer_research_fireworks_cost_reconciliations
    DROP CONSTRAINT cancer_research_fireworks_cost_reconciliat_schema_version_check;

ALTER TABLE cancer_research_fireworks_cost_reconciliations
    ADD CONSTRAINT cancer_research_fireworks_reconciliation_schema_version_check
    CHECK (schema_version IN (1, 2));

ALTER TABLE cancer_research_fireworks_cost_reconciliations
    DROP CONSTRAINT cancer_research_fireworks_cost_reconcilia_requested_model_check;

ALTER TABLE cancer_research_fireworks_cost_reconciliations
    ADD CONSTRAINT cancer_research_fireworks_reconciliation_model_check
    CHECK (requested_model IN (
        'accounts/fireworks/models/gpt-oss-20b',
        'accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b'
    ));

CREATE OR REPLACE FUNCTION validate_cancer_research_fireworks_cost_reconciliation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    reservation cancer_research_cost_reservations%ROWTYPE;
    dispatch cancer_research_route_dispatches%ROWTYPE;
    expected_actual BIGINT;
    matching_dispatches BIGINT;
BEGIN
    IF NEW.schema_version NOT IN (1, 2)
       OR (
            NEW.schema_version = 1
            AND NEW.requested_model <> 'accounts/fireworks/models/gpt-oss-20b'
       ) THEN
        RAISE EXCEPTION 'Fireworks reconciliation schema does not admit this model';
    END IF;
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
       OR dispatch.requested_model NOT IN (
            'accounts/fireworks/models/gpt-oss-20b',
            'accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b'
       )
       OR dispatch.requested_model <> NEW.requested_model
       OR dispatch.dispatched_at <> NEW.matched_dispatch_at
       OR ABS(EXTRACT(EPOCH FROM (NEW.provider_started_at - dispatch.dispatched_at))) > 5 THEN
        RAISE EXCEPTION 'Fireworks billing row does not tightly match its paid dispatch';
    END IF;

    -- The provider export has no application request ID. Match an exact model
    -- identity and timestamp window, and refuse to guess when cardinality is
    -- not exactly one.
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

    -- Cached input is deliberately priced as uncached input so the local hard
    -- stop never understates the provider account's maximum charge.
    expected_actual := CASE NEW.requested_model
        WHEN 'accounts/fireworks/models/gpt-oss-20b' THEN (
            NEW.prompt_tokens * 70000
            + NEW.completion_tokens * 300000
            + 999999
        ) / 1000000
        WHEN 'accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b' THEN (
            NEW.prompt_tokens * 50000
            + NEW.completion_tokens * 200000
            + 999999
        ) / 1000000
        ELSE NULL
    END;
    IF expected_actual IS NULL
       OR NEW.actual_micro_usd <> expected_actual
       OR NEW.actual_micro_usd > reservation.reserved_micro_usd
       OR NEW.released_micro_usd <> reservation.reserved_micro_usd - expected_actual THEN
        RAISE EXCEPTION 'Fireworks reconciliation cost disagrees with the pinned tariff';
    END IF;

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

COMMENT ON CONSTRAINT cancer_research_fireworks_reconciliation_schema_version_check
ON cancer_research_fireworks_cost_reconciliations
IS 'Version 1 preserves historical GPT-OSS-only evidence; version 2 admits the pinned Nemotron replacement.';

COMMENT ON CONSTRAINT cancer_research_fireworks_reconciliation_model_check
ON cancer_research_fireworks_cost_reconciliations
IS 'Closed set of historical and current Fireworks research models; model-specific validation remains in the insert trigger.';
