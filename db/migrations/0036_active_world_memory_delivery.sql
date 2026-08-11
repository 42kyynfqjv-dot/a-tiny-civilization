-- A large archived or experimental perception backlog must not prevent the
-- currently observed open-ended world from receiving its own subjective memory.
-- The claim query still falls back to the global immutable queue whenever the
-- active ordinary world has no eligible delivery.
CREATE INDEX memory_outbox_world_pending
    ON memory_outbox (world_id, available_at, created_at, operation_id)
    WHERE completed_at IS NULL;

COMMENT ON INDEX memory_outbox_world_pending IS
'Supports bounded preference for the active ordinary world without deleting or rewriting older memory deliveries.';
