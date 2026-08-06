CREATE OR REPLACE FUNCTION reject_archived_world_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'world records cannot be deleted';
    END IF;
    IF OLD.status = 'archived' THEN
        RAISE EXCEPTION 'archived worlds are immutable';
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
        OR (OLD.status = 'running' AND NEW.status IN ('running', 'extinct', 'archived'))
        OR (OLD.status = 'extinct' AND NEW.status = 'archived')
    ) THEN
        RAISE EXCEPTION 'invalid world lifecycle transition from % to %', OLD.status, NEW.status;
    END IF;
    RETURN NEW;
END
$$;

COMMENT ON FUNCTION reject_archived_world_mutation()
IS 'Enforces immutable provenance, monotonic cursors, and one-way lifecycle transitions.';
