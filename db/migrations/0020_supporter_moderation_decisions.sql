CREATE TABLE supporter_moderation_decisions (
    reservation_id UUID PRIMARY KEY REFERENCES supporter_reservations (id),
    decision TEXT NOT NULL CHECK (decision IN ('approved', 'rejected')),
    moderator_subject TEXT NOT NULL CHECK (
        moderator_subject = btrim(moderator_subject)
        AND length(moderator_subject) BETWEEN 1 AND 256
        AND moderator_subject !~ '[[:cntrl:]]'
    ),
    automatic_policy_version SMALLINT NOT NULL CHECK (automatic_policy_version >= 0),
    decided_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE FUNCTION protect_supporter_moderation_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'supporter moderation evidence is append-only';
END
$$;

CREATE TRIGGER supporter_moderation_decisions_are_append_only
BEFORE UPDATE OR DELETE ON supporter_moderation_decisions
FOR EACH ROW EXECUTE FUNCTION protect_supporter_moderation_history();

COMMENT ON TABLE supporter_moderation_decisions IS
    'Immutable human-review decisions for paid observer labels; never visible to or consumed by the simulation.';
