CREATE TABLE supporter_reservations (
    id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds (id),
    supporter_subject TEXT NOT NULL CHECK (length(btrim(supporter_subject)) BETWEEN 1 AND 256),
    observer_label TEXT NOT NULL CHECK (
        observer_label = btrim(observer_label)
        AND length(observer_label) BETWEEN 1 AND 80
        AND observer_label !~ '[[:cntrl:]]'
    ),
    target_role TEXT NOT NULL CHECK (target_role IN ('person', 'fauna')),
    species_catalog TEXT,
    species_identifier TEXT,
    species_scientific_name TEXT,
    species_source_url TEXT,
    birth_category TEXT NOT NULL CHECK (birth_category ~ '^[a-z_]+$'),
    state TEXT NOT NULL CHECK (state IN (
        'pending_payment', 'pending_moderation', 'active', 'matched', 'rejected',
        'cancelled_by_supporter', 'expired'
    )),
    payment_reference TEXT UNIQUE,
    payment_verified_at TIMESTAMPTZ,
    activated_at TIMESTAMPTZ,
    matched_birth_event_id UUID UNIQUE,
    matched_event_sequence BIGINT CHECK (matched_event_sequence > 0),
    matched_tick BIGINT CHECK (matched_tick >= 0),
    matched_organism_id UUID UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (
        (target_role = 'person'
            AND species_catalog IS NULL
            AND species_identifier IS NULL
            AND species_scientific_name IS NULL
            AND species_source_url IS NULL)
        OR
        (target_role = 'fauna'
            AND length(btrim(species_catalog)) > 0
            AND length(btrim(species_identifier)) > 0
            AND length(btrim(species_scientific_name)) > 0
            AND species_source_url ~ '^https://')
    ),
    CHECK (
        (payment_reference IS NULL AND payment_verified_at IS NULL)
        OR (payment_reference IS NOT NULL AND payment_verified_at IS NOT NULL)
    ),
    CHECK (
        (state = 'matched'
            AND matched_birth_event_id IS NOT NULL
            AND matched_event_sequence IS NOT NULL
            AND matched_tick IS NOT NULL
            AND matched_organism_id IS NOT NULL)
        OR
        (state <> 'matched'
            AND matched_birth_event_id IS NULL
            AND matched_event_sequence IS NULL
            AND matched_tick IS NULL
            AND matched_organism_id IS NULL)
    )
);

CREATE INDEX supporter_reservations_active_match
    ON supporter_reservations (world_id, target_role, birth_category, activated_at, id)
    WHERE state = 'active';

CREATE FUNCTION protect_supporter_reservation_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'supporter reservation history cannot be deleted';
    END IF;
    IF NEW.id IS DISTINCT FROM OLD.id
       OR NEW.world_id IS DISTINCT FROM OLD.world_id
       OR NEW.supporter_subject IS DISTINCT FROM OLD.supporter_subject
       OR NEW.observer_label IS DISTINCT FROM OLD.observer_label
       OR NEW.target_role IS DISTINCT FROM OLD.target_role
       OR NEW.species_catalog IS DISTINCT FROM OLD.species_catalog
       OR NEW.species_identifier IS DISTINCT FROM OLD.species_identifier
       OR NEW.species_scientific_name IS DISTINCT FROM OLD.species_scientific_name
       OR NEW.species_source_url IS DISTINCT FROM OLD.species_source_url
       OR NEW.birth_category IS DISTINCT FROM OLD.birth_category
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'supporter request fields are immutable';
    END IF;
    IF OLD.state = 'matched' THEN
        RAISE EXCEPTION 'matched supporter reservations are immutable';
    END IF;
    IF OLD.payment_reference IS NOT NULL
       AND (NEW.payment_reference IS DISTINCT FROM OLD.payment_reference
            OR NEW.payment_verified_at IS DISTINCT FROM OLD.payment_verified_at) THEN
        RAISE EXCEPTION 'verified payment evidence is immutable';
    END IF;
    IF NOT (
        (OLD.state = 'pending_payment' AND NEW.state IN ('pending_payment', 'pending_moderation', 'cancelled_by_supporter'))
        OR (OLD.state = 'pending_moderation' AND NEW.state IN ('pending_moderation', 'active', 'rejected', 'cancelled_by_supporter', 'expired'))
        OR (OLD.state = 'active' AND NEW.state IN ('active', 'matched', 'cancelled_by_supporter', 'expired'))
        OR (OLD.state IN ('rejected', 'cancelled_by_supporter', 'expired') AND NEW.state = OLD.state)
    ) THEN
        RAISE EXCEPTION 'invalid supporter reservation transition from % to %', OLD.state, NEW.state;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER supporter_reservations_preserve_history
BEFORE UPDATE OR DELETE ON supporter_reservations
FOR EACH ROW EXECUTE FUNCTION protect_supporter_reservation_history();

COMMENT ON TABLE supporter_reservations IS 'Observer-only paid-label queue. It cannot create, delay, or modify canonical births.';
COMMENT ON COLUMN supporter_reservations.matched_birth_event_id IS 'Canonical event ID already committed before observer-side matching.';
