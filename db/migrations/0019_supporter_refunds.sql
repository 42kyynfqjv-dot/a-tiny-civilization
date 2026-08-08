CREATE TABLE supporter_refunds (
    reservation_id UUID PRIMARY KEY REFERENCES supporter_reservations (id),
    payment_intent_id TEXT NOT NULL UNIQUE CHECK (payment_intent_id ~ '^pi_[A-Za-z0-9_]+$'),
    reason TEXT NOT NULL CHECK (reason IN (
        'moderation_rejection', 'world_extinction', 'supporter_cancellation',
        'duplicate_charge', 'service_failure'
    )),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    stripe_refund_id TEXT UNIQUE CHECK (
        stripe_refund_id IS NULL OR stripe_refund_id ~ '^re_[A-Za-z0-9_]+$'
    ),
    completed_at TIMESTAMPTZ,
    CHECK (
        (stripe_refund_id IS NULL AND completed_at IS NULL)
        OR (stripe_refund_id IS NOT NULL AND completed_at IS NOT NULL)
    )
);

CREATE FUNCTION protect_supporter_refund_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'supporter refund history cannot be deleted';
    END IF;
    IF NEW.reservation_id IS DISTINCT FROM OLD.reservation_id
       OR NEW.payment_intent_id IS DISTINCT FROM OLD.payment_intent_id
       OR NEW.reason IS DISTINCT FROM OLD.reason
       OR NEW.requested_at IS DISTINCT FROM OLD.requested_at THEN
        RAISE EXCEPTION 'supporter refund request evidence is immutable';
    END IF;
    IF OLD.stripe_refund_id IS NOT NULL
       AND (NEW.stripe_refund_id IS DISTINCT FROM OLD.stripe_refund_id
            OR NEW.completed_at IS DISTINCT FROM OLD.completed_at) THEN
        RAISE EXCEPTION 'completed supporter refund evidence is immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER supporter_refunds_preserve_history
BEFORE UPDATE OR DELETE ON supporter_refunds
FOR EACH ROW EXECUTE FUNCTION protect_supporter_refund_history();

COMMENT ON TABLE supporter_refunds IS
    'Durable operator refund intents and Stripe completion evidence; never consumed by the simulation.';
