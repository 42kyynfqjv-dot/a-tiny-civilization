ALTER TABLE memory_outbox
    ADD COLUMN claimed_by TEXT,
    ADD COLUMN claimed_at TIMESTAMPTZ,
    ADD COLUMN remote_operation_id TEXT,
    ADD CONSTRAINT memory_outbox_claim_pair CHECK (
        (claimed_by IS NULL AND claimed_at IS NULL)
        OR (claimed_by IS NOT NULL AND claimed_at IS NOT NULL)
    ),
    ADD CONSTRAINT memory_outbox_remote_requires_completion CHECK (
        remote_operation_id IS NULL OR completed_at IS NOT NULL
    );

CREATE FUNCTION protect_memory_outbox_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'memory delivery history cannot be deleted';
    END IF;
    IF OLD.completed_at IS NOT NULL THEN
        RAISE EXCEPTION 'accepted memory deliveries are immutable';
    END IF;
    IF NEW.operation_id IS DISTINCT FROM OLD.operation_id
       OR NEW.document_id IS DISTINCT FROM OLD.document_id
       OR NEW.world_id IS DISTINCT FROM OLD.world_id
       OR NEW.agent_id IS DISTINCT FROM OLD.agent_id
       OR NEW.source_sequence IS DISTINCT FROM OLD.source_sequence
       OR NEW.bank_id IS DISTINCT FROM OLD.bank_id
       OR NEW.payload_version IS DISTINCT FROM OLD.payload_version
       OR NEW.payload IS DISTINCT FROM OLD.payload
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'memory delivery provenance and payload are immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER memory_outbox_preserves_history
BEFORE UPDATE OR DELETE ON memory_outbox
FOR EACH ROW EXECUTE FUNCTION protect_memory_outbox_history();

COMMENT ON COLUMN memory_outbox.claimed_at
IS 'Wall-clock worker lease only; never a simulation input.';
COMMENT ON COLUMN memory_outbox.remote_operation_id
IS 'Hindsight acknowledgement for the idempotent client-supplied operation.';
