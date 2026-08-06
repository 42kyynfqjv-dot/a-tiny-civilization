DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM worlds)
       OR EXISTS (SELECT 1 FROM event_batches)
       OR EXISTS (SELECT 1 FROM snapshots) THEN
        RAISE EXCEPTION 'deterministic history migration requires an empty foundation ledger';
    END IF;
END
$$;

ALTER TABLE worlds
    ADD COLUMN manifest JSONB NOT NULL,
    ADD COLUMN manifest_checksum BYTEA NOT NULL,
    ADD COLUMN last_event_checksum BYTEA NOT NULL,
    ADD COLUMN current_state_checksum BYTEA NOT NULL,
    ADD CONSTRAINT worlds_manifest_checksum_size CHECK (octet_length(manifest_checksum) = 32),
    ADD CONSTRAINT worlds_last_event_checksum_size CHECK (octet_length(last_event_checksum) = 32),
    ADD CONSTRAINT worlds_current_state_checksum_size CHECK (octet_length(current_state_checksum) = 32);

CREATE UNIQUE INDEX worlds_unique_seed ON worlds (seed);

ALTER TABLE event_batches
    ADD COLUMN previous_checksum BYTEA NOT NULL,
    ADD COLUMN post_state_checksum BYTEA NOT NULL,
    ADD CONSTRAINT event_batches_checksum_size CHECK (octet_length(checksum) = 32),
    ADD CONSTRAINT event_batches_previous_checksum_size CHECK (octet_length(previous_checksum) = 32),
    ADD CONSTRAINT event_batches_post_state_checksum_size CHECK (octet_length(post_state_checksum) = 32);

ALTER TABLE snapshots
    ADD COLUMN last_event_checksum BYTEA NOT NULL,
    ADD CONSTRAINT snapshots_checksum_size CHECK (octet_length(checksum) = 32),
    ADD CONSTRAINT snapshots_last_event_checksum_size CHECK (octet_length(last_event_checksum) = 32);

CREATE FUNCTION reject_event_batch_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'canonical event batches are append-only';
END
$$;

CREATE TRIGGER event_batches_are_append_only
BEFORE UPDATE OR DELETE ON event_batches
FOR EACH ROW EXECUTE FUNCTION reject_event_batch_mutation();

CREATE FUNCTION reject_archived_world_mutation()
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
    RETURN NEW;
END
$$;

CREATE TRIGGER archived_worlds_are_immutable
BEFORE UPDATE OR DELETE ON worlds
FOR EACH ROW EXECUTE FUNCTION reject_archived_world_mutation();

CREATE FUNCTION require_writable_event_world()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM worlds
        WHERE id = NEW.world_id
          AND status <> 'archived'
    ) THEN
        RAISE EXCEPTION 'cannot append events to an archived or missing world';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER event_batches_require_writable_world
BEFORE INSERT ON event_batches
FOR EACH ROW EXECUTE FUNCTION require_writable_event_world();

CREATE FUNCTION require_archived_predecessor()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.predecessor_world_id IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM worlds
           WHERE id = NEW.predecessor_world_id
             AND status = 'archived'
       ) THEN
        RAISE EXCEPTION 'a predecessor world must already be archived';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER worlds_require_archived_predecessor
BEFORE INSERT OR UPDATE OF predecessor_world_id ON worlds
FOR EACH ROW EXECUTE FUNCTION require_archived_predecessor();

COMMENT ON COLUMN worlds.manifest IS 'Immutable pre-genesis ruleset and scientific data commitment.';
COMMENT ON COLUMN event_batches.previous_checksum IS 'SHA-256 hash of the preceding batch, or zero for genesis.';
COMMENT ON COLUMN event_batches.post_state_checksum IS 'SHA-256 hash of causal engine state after this batch.';
