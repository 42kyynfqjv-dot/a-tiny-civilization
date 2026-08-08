CREATE TABLE stripe_webhook_events (
    event_id TEXT PRIMARY KEY CHECK (length(event_id) BETWEEN 1 AND 255),
    event_type TEXT NOT NULL CHECK (length(event_type) BETWEEN 1 AND 255),
    payload_hash BYTEA NOT NULL CHECK (octet_length(payload_hash) = 32),
    checkout_session_id TEXT CHECK (length(checkout_session_id) BETWEEN 1 AND 255),
    reservation_id UUID REFERENCES supporter_reservations (id),
    amount_minor BIGINT CHECK (amount_minor > 0),
    currency TEXT CHECK (currency ~ '^[a-z]{3}$'),
    live_mode BOOLEAN,
    outcome TEXT NOT NULL CHECK (outcome IN ('payment_recorded', 'duplicate_payment', 'ignored')),
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (
        (outcome = 'ignored'
            AND checkout_session_id IS NULL
            AND reservation_id IS NULL
            AND amount_minor IS NULL
            AND currency IS NULL
            AND live_mode IS NULL)
        OR
        (outcome IN ('payment_recorded', 'duplicate_payment')
            AND checkout_session_id IS NOT NULL
            AND reservation_id IS NOT NULL
            AND amount_minor IS NOT NULL
            AND currency IS NOT NULL
            AND live_mode IS NOT NULL)
    )
);

CREATE UNIQUE INDEX stripe_webhook_one_recorded_payment_per_reservation
    ON stripe_webhook_events (reservation_id)
    WHERE outcome = 'payment_recorded';

CREATE UNIQUE INDEX stripe_webhook_one_recorded_payment_per_checkout
    ON stripe_webhook_events (checkout_session_id)
    WHERE outcome = 'payment_recorded';

CREATE INDEX stripe_webhook_checkout_session_history
    ON stripe_webhook_events (checkout_session_id, received_at, event_id)
    WHERE checkout_session_id IS NOT NULL;

CREATE FUNCTION protect_stripe_webhook_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'Stripe webhook evidence is append-only';
END
$$;

CREATE TRIGGER stripe_webhook_events_are_append_only
BEFORE UPDATE OR DELETE ON stripe_webhook_events
FOR EACH ROW EXECUTE FUNCTION protect_stripe_webhook_history();

COMMENT ON TABLE stripe_webhook_events IS
    'Append-only observer payment evidence admitted only after exact raw-body signature verification.';
