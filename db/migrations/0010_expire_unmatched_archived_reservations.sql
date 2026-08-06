-- An archived world can never produce a later matching birth. Preserve every request
-- and payment record, but allow its unmatched pending-payment reservation to become
-- terminal alongside pending moderation and active reservations.
CREATE OR REPLACE FUNCTION protect_supporter_reservation_history()
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
        (OLD.state = 'pending_payment' AND NEW.state IN ('pending_payment', 'pending_moderation', 'cancelled_by_supporter', 'expired'))
        OR (OLD.state = 'pending_moderation' AND NEW.state IN ('pending_moderation', 'active', 'rejected', 'cancelled_by_supporter', 'expired'))
        OR (OLD.state = 'active' AND NEW.state IN ('active', 'matched', 'cancelled_by_supporter', 'expired'))
        OR (OLD.state IN ('rejected', 'cancelled_by_supporter', 'expired') AND NEW.state = OLD.state)
    ) THEN
        RAISE EXCEPTION 'invalid supporter reservation transition from % to %', OLD.state, NEW.state;
    END IF;
    RETURN NEW;
END
$$;

COMMENT ON FUNCTION protect_supporter_reservation_history() IS
    'Preserves supporter history while permitting all unmatched states to expire after world archival.';
