-- Read-side indexes for the observer memory stream. These add no simulation
-- inputs and do not alter retained memories or cognition history.
CREATE INDEX memory_outbox_public_stream
    ON memory_outbox (world_id, agent_id, source_sequence DESC, operation_id)
    WHERE completed_at IS NOT NULL;

CREATE INDEX cognition_requests_public_memory_stream
    ON cognition_requests (world_id, selected_tick DESC, request_id);

COMMENT ON INDEX memory_outbox_public_stream IS
    'Bounded observer lookup over immutable, successfully delivered subjective memories.';

COMMENT ON INDEX cognition_requests_public_memory_stream IS
    'Bounded observer lookup over immutable cognition request and recall provenance.';
