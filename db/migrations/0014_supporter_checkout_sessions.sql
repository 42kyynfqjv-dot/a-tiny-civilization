CREATE TABLE supporter_checkout_sessions (
    reservation_id UUID PRIMARY KEY REFERENCES supporter_reservations (id),
    stripe_session_id TEXT NOT NULL UNIQUE CHECK (length(stripe_session_id) BETWEEN 4 AND 255),
    checkout_url TEXT NOT NULL CHECK (checkout_url ~ '^https://'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE FUNCTION protect_supporter_checkout_session_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'supporter Checkout session history is immutable';
END
$$;

CREATE TRIGGER supporter_checkout_sessions_are_immutable
BEFORE UPDATE OR DELETE ON supporter_checkout_sessions
FOR EACH ROW EXECUTE FUNCTION protect_supporter_checkout_session_history();

COMMENT ON TABLE supporter_checkout_sessions IS
    'Immutable observer-only correlation for reservation-idempotent Stripe Checkout creation.';
