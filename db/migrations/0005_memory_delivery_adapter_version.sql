ALTER TABLE memory_outbox
    ADD COLUMN adapter_version TEXT,
    ADD CONSTRAINT memory_outbox_ack_pair CHECK (
        (remote_operation_id IS NULL AND adapter_version IS NULL)
        OR (remote_operation_id IS NOT NULL AND adapter_version IS NOT NULL)
    ),
    ADD CONSTRAINT memory_outbox_adapter_requires_completion CHECK (
        adapter_version IS NULL OR completed_at IS NOT NULL
    );

COMMENT ON COLUMN memory_outbox.adapter_version
IS 'Project adapter contract version that produced the accepted Hindsight acknowledgement.';
