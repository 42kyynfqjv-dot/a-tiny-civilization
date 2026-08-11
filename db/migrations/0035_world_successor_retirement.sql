-- A populated legacy world may be deliberately closed for an explicitly recorded
-- successor without fabricating extinction. The canonical event log remains the
-- authority for this state; the database value is only its replay-checked cursor.
ALTER TABLE worlds DROP CONSTRAINT worlds_status_check;

ALTER TABLE worlds
    ADD CONSTRAINT worlds_status_check
    CHECK (status IN ('initializing', 'running', 'extinct', 'archived', 'retired'));

COMMENT ON CONSTRAINT worlds_status_check ON worlds IS
'Retired is a populated, event-recorded successor cutover and is never equivalent to extinction.';

CREATE OR REPLACE FUNCTION reject_archived_world_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'world records cannot be deleted';
    END IF;
    IF OLD.status IN ('archived', 'retired') THEN
        RAISE EXCEPTION 'archived and retired worlds are immutable';
    END IF;
    IF NEW.id IS DISTINCT FROM OLD.id
       OR NEW.seed IS DISTINCT FROM OLD.seed
       OR NEW.ruleset_version IS DISTINCT FROM OLD.ruleset_version
       OR NEW.predecessor_world_id IS DISTINCT FROM OLD.predecessor_world_id
       OR NEW.manifest IS DISTINCT FROM OLD.manifest
       OR NEW.manifest_checksum IS DISTINCT FROM OLD.manifest_checksum THEN
        RAISE EXCEPTION 'world provenance fields are immutable after creation';
    END IF;
    IF NEW.current_sequence <> OLD.current_sequence + 1 THEN
        RAISE EXCEPTION 'world event sequence must advance exactly once per update';
    END IF;
    IF NEW.current_tick < OLD.current_tick OR NEW.current_tick > OLD.current_tick + 1 THEN
        RAISE EXCEPTION 'world tick must remain fixed or advance exactly once';
    END IF;
    IF NEW.last_event_checksum = OLD.last_event_checksum THEN
        RAISE EXCEPTION 'world event hash must advance with its sequence';
    END IF;
    IF NOT (
        (OLD.status = 'initializing' AND NEW.status = 'running')
        OR (OLD.status = 'running' AND NEW.status IN ('running', 'extinct', 'archived', 'retired'))
        OR (OLD.status = 'extinct' AND NEW.status = 'archived')
    ) THEN
        RAISE EXCEPTION 'invalid world lifecycle transition from % to %', OLD.status, NEW.status;
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION require_writable_event_world()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM worlds
        WHERE id = NEW.world_id
          AND status NOT IN ('archived', 'retired')
    ) THEN
        RAISE EXCEPTION 'cannot append events to an archived, retired, or missing world';
    END IF;
    RETURN NEW;
END
$$;

CREATE OR REPLACE FUNCTION require_archived_predecessor()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.predecessor_world_id IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM worlds
           WHERE id = NEW.predecessor_world_id
             AND status IN ('archived', 'retired')
       ) THEN
        RAISE EXCEPTION 'a predecessor world must already be archived or retired';
    END IF;
    RETURN NEW;
END
$$;

ALTER TABLE observer_timeline_items
    DROP CONSTRAINT observer_timeline_items_kind_check;

ALTER TABLE observer_timeline_items
    ADD CONSTRAINT observer_timeline_items_kind_check
    CHECK (kind IN (
        'world_began', 'initial_person_present', 'initial_animal_present', 'person_born',
        'animal_born', 'life_ended', 'people_extinct', 'world_archived', 'world_retired'
    ));
