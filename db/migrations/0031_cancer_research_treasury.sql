CREATE TABLE cancer_research_cost_reservations (
    request_id UUID PRIMARY KEY REFERENCES cancer_research_requests (request_id),
    billing_scope TEXT NOT NULL DEFAULT 'cancer_research'
        CHECK (billing_scope = 'cancer_research'),
    billing_month DATE NOT NULL CHECK (
        billing_month = date_trunc('month', billing_month)::DATE
    ),
    reserved_micro_usd BIGINT NOT NULL CHECK (
        reserved_micro_usd > 0 AND reserved_micro_usd <= 250000
    ),
    status TEXT NOT NULL CHECK (
        status IN ('reserved', 'settled', 'released', 'indeterminate')
    ),
    actual_micro_usd BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    FOREIGN KEY (billing_scope, billing_month)
        REFERENCES cognition_cost_accounts (billing_scope, billing_month),
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

CREATE FUNCTION protect_cancer_research_cost_reservation_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'cancer research cost reservation history cannot be deleted';
    END IF;
    IF NEW.request_id IS DISTINCT FROM OLD.request_id
       OR NEW.billing_scope IS DISTINCT FROM OLD.billing_scope
       OR NEW.billing_month IS DISTINCT FROM OLD.billing_month
       OR NEW.reserved_micro_usd IS DISTINCT FROM OLD.reserved_micro_usd
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'cancer research cost reservation provenance is immutable';
    END IF;
    IF OLD.status <> 'reserved'
       OR NEW.status NOT IN ('settled', 'released', 'indeterminate') THEN
        RAISE EXCEPTION 'resolved cancer research cost reservations are immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_research_cost_reservations_preserve_history
BEFORE UPDATE OR DELETE ON cancer_research_cost_reservations
FOR EACH ROW EXECUTE FUNCTION protect_cancer_research_cost_reservation_history();

CREATE FUNCTION require_cancer_research_paid_authorization()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.billing_class = 'paid_approved' AND NOT EXISTS (
        SELECT 1
        FROM cancer_research_cost_reservations
        WHERE request_id = NEW.request_id AND status = 'reserved'
    ) THEN
        RAISE EXCEPTION 'paid cancer research dispatch requires an active reservation';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER cancer_research_dispatches_require_paid_authorization
BEFORE INSERT ON cancer_research_route_dispatches
FOR EACH ROW EXECUTE FUNCTION require_cancer_research_paid_authorization();

COMMENT ON TABLE cancer_research_cost_reservations
IS 'Per-turn paid Cancer World authorization isolated inside the cancer_research monthly treasury.';
