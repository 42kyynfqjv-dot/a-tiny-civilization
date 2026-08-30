-- A provider-reported paid receipt is authoritative evidence of cost. The
-- original reservation estimate was only 15,000 micro-USD; one valid 19,822
-- micro-USD receipt therefore could not settle even though the global account
-- had ample headroom. Permit that already-incurred overage up to the immutable
-- per-call exposure cap. Future workers reserve the full cap before dispatch.
DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    SELECT conname
    INTO constraint_name
    FROM pg_constraint
    WHERE conrelid = 'cancer_research_cost_reservations'::REGCLASS
      AND contype = 'c'
      AND pg_get_constraintdef(oid) LIKE '%actual_micro_usd <= reserved_micro_usd%';

    IF constraint_name IS NULL THEN
        RAISE EXCEPTION 'paid-research actual-cost reservation constraint was not found';
    END IF;

    EXECUTE FORMAT(
        'ALTER TABLE cancer_research_cost_reservations DROP CONSTRAINT %I',
        constraint_name
    );
END
$$;

ALTER TABLE cancer_research_cost_reservations
ADD CONSTRAINT cancer_research_cost_reservations_actual_cost_bound CHECK (
    (status = 'reserved' AND actual_micro_usd IS NULL AND resolved_at IS NULL)
    OR (
        status = 'settled'
        AND actual_micro_usd IS NOT NULL
        AND actual_micro_usd >= 0
        AND actual_micro_usd <= 250000
        AND resolved_at IS NOT NULL
    )
    OR (
        status IN ('released', 'indeterminate')
        AND actual_micro_usd IS NULL
        AND resolved_at IS NOT NULL
    )
);

COMMENT ON CONSTRAINT cancer_research_cost_reservations_actual_cost_bound
ON cancer_research_cost_reservations
IS 'Settles authoritative provider cost up to the pre-approved per-call cap; reserved amount remains immutable provenance.';

-- The monthly hard stop authorizes calls before dispatch; it cannot make an
-- already-incurred provider charge disappear. Allow Cancer research accounting
-- to record a bounded legacy overage truthfully. The reservation path still
-- refuses every future call whenever reserved + spent is at or above the stop.
DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    SELECT conname
    INTO constraint_name
    FROM pg_constraint
    WHERE conrelid = 'cognition_cost_accounts'::REGCLASS
      AND contype = 'c'
      AND pg_get_constraintdef(oid) LIKE '%reserved_micro_usd%'
      AND pg_get_constraintdef(oid) LIKE '%spent_micro_usd%'
      AND pg_get_constraintdef(oid) LIKE '%hard_stop_micro_usd%';

    IF constraint_name IS NULL THEN
        RAISE EXCEPTION 'cognition account hard-stop constraint was not found';
    END IF;

    EXECUTE FORMAT(
        'ALTER TABLE cognition_cost_accounts DROP CONSTRAINT %I',
        constraint_name
    );
END
$$;

ALTER TABLE cognition_cost_accounts
ADD CONSTRAINT cognition_cost_accounts_pre_dispatch_hard_stop CHECK (
    reserved_micro_usd + spent_micro_usd <= hard_stop_micro_usd
    OR (
        billing_scope = 'cancer_research'
        AND reserved_micro_usd + spent_micro_usd
            <= hard_stop_micro_usd + 250000
    )
);

COMMENT ON CONSTRAINT cognition_cost_accounts_pre_dispatch_hard_stop
ON cognition_cost_accounts
IS 'Production commitments remain inside the hard stop; Cancer research may record at most one per-call cap of already-incurred overage, after which new reservations fail closed.';
