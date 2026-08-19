CREATE INDEX IF NOT EXISTS cancer_research_requests_world_ordinal
    ON cancer_research_requests (world_id, ordinal, request_id);

CREATE INDEX IF NOT EXISTS cancer_research_campaign_children
    ON cancer_research_requests (
        world_id,
        ((request_payload->'selection'->>'frozen_candidate_hash')),
        ordinal
    )
    WHERE stage = 'independent_replication';

COMMENT ON INDEX cancer_research_campaign_children IS
'Bounds continuation lookup for an immutable Cancer World replication lineage; it does not select or alter research outcomes.';
