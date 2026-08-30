ALTER TABLE cancer_research_requests
    ADD COLUMN claim_token UUID;

-- A deployment or crash can leave an immutable dispatch without an outcome.
-- Release every unfinished pre-generation claim so a replacement worker can
-- immediately record that call as unavailable and continue the remaining
-- closed route ladder without issuing the same call twice.
UPDATE cancer_research_requests
SET claimed_by = NULL,
    claimed_at = NULL,
    claim_token = NULL
WHERE completed_at IS NULL
  AND claimed_by IS NOT NULL;

-- Completed rows retain their historical operational owner metadata. Their
-- token is inert, but keeps the claim triple structurally well formed. The
-- history trigger predates this operational column and correctly rejects every
-- update to completed rows, so suspend it only around this one metadata
-- backfill and restore it before adding the invariant.
ALTER TABLE cancer_research_requests
    DISABLE TRIGGER cancer_research_requests_preserve_history;

UPDATE cancer_research_requests
SET claim_token = gen_random_uuid()
WHERE completed_at IS NOT NULL
  AND claimed_by IS NOT NULL;

ALTER TABLE cancer_research_requests
    ENABLE TRIGGER cancer_research_requests_preserve_history;

ALTER TABLE cancer_research_requests
    ADD CONSTRAINT cancer_research_requests_claim_generation CHECK (
        (claimed_by IS NULL AND claimed_at IS NULL AND claim_token IS NULL)
        OR
        (claimed_by IS NOT NULL AND claimed_at IS NOT NULL AND claim_token IS NOT NULL)
    );

COMMENT ON COLUMN cancer_research_requests.claim_token
IS 'Opaque generation for one worker lease; prevents same-worker ABA after crash recovery.';
