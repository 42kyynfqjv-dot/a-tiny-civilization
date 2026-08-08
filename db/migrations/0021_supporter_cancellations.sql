CREATE TABLE supporter_cancellations (
    reservation_id UUID PRIMARY KEY REFERENCES supporter_reservations (id),
    supporter_subject TEXT NOT NULL CHECK (
        supporter_subject = btrim(supporter_subject)
        AND length(supporter_subject) BETWEEN 1 AND 256
        AND supporter_subject !~ '[[:cntrl:]]'
    ),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE FUNCTION protect_supporter_cancellation_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'supporter cancellation evidence is append-only';
END
$$;

CREATE TRIGGER supporter_cancellations_are_append_only
BEFORE UPDATE OR DELETE ON supporter_cancellations
FOR EACH ROW EXECUTE FUNCTION protect_supporter_cancellation_history();

COMMENT ON TABLE supporter_cancellations IS
    'Immutable account-owned cancellation requests for unmatched observer labels.';
