ALTER TABLE supporter_reservations
    ADD COLUMN automatic_policy_version SMALLINT NOT NULL DEFAULT 0
    CHECK (automatic_policy_version >= 0);

ALTER TABLE supporter_reservations ALTER COLUMN automatic_policy_version DROP DEFAULT;

CREATE FUNCTION protect_supporter_label_policy_version()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.automatic_policy_version IS DISTINCT FROM OLD.automatic_policy_version THEN
        RAISE EXCEPTION 'supporter label policy evidence is immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER supporter_label_policy_version_is_immutable
BEFORE UPDATE ON supporter_reservations
FOR EACH ROW EXECUTE FUNCTION protect_supporter_label_policy_version();

COMMENT ON COLUMN supporter_reservations.automatic_policy_version IS
    'Version of the deterministic pre-payment label policy; 0 identifies legacy rows.';
