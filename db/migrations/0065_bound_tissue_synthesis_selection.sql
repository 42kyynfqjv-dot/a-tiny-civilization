-- Tissue admission polls continuously, but only successful campaign-synthesis
-- requests can ever become candidates. Keep that operational scan proportional
-- to the handful of synthesis rows instead of the complete replication ledger.
CREATE INDEX IF NOT EXISTS cancer_research_tissue_synthesis_candidates
    ON cancer_research_requests (world_id, ordinal, request_id)
    WHERE stage = 'independent_replication'
      AND completed_at IS NOT NULL
      AND request_payload->'selection'->>'task' = 'interpret_replication_result';

COMMENT ON INDEX cancer_research_tissue_synthesis_candidates IS
'Bounds the observer-only tissue admission poll to completed campaign syntheses; it does not select, score, or alter research outcomes.';
