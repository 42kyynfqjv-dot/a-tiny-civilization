-- The Cancer research console joins durable Hindsight delivery state by the
-- ordinal embedded in its immutable outbox payload. Without these partial
-- expression indexes, every console poll repeatedly scans and decodes the full
-- cross-world memory ledger.
CREATE INDEX IF NOT EXISTS memory_outbox_cancer_artifact_ordinal
ON memory_outbox (world_id, agent_id, ((payload->>'ordinal')::BIGINT))
INCLUDE (completed_at)
WHERE payload->>'context' = 'Cancer World research artifact';

CREATE INDEX IF NOT EXISTS memory_outbox_cancer_experiment_ordinal
ON memory_outbox (world_id, agent_id, ((payload->>'ordinal')::BIGINT))
INCLUDE (completed_at)
WHERE payload->>'context' = 'Cancer World virtual experiment result';
