ALTER TABLE memory_outbox
    ADD COLUMN claim_token UUID;

-- This field is operational lease state, not retained memory provenance. Populate
-- historical acknowledgements so the strict claim tuple can cover every row;
-- accepted rows are otherwise (correctly) protected from all updates.
ALTER TABLE memory_outbox DISABLE TRIGGER memory_outbox_preserves_history;

UPDATE memory_outbox
SET claim_token = gen_random_uuid()
WHERE completed_at IS NOT NULL
  AND claimed_by IS NOT NULL;

-- Invalidate leases issued by pre-generation workers at the deployment boundary.
-- Their late acknowledgements no longer match claimed_by after this transaction.
UPDATE memory_outbox
SET claimed_by = NULL,
    claimed_at = NULL,
    claim_token = NULL
WHERE completed_at IS NULL
  AND claimed_by IS NOT NULL;

ALTER TABLE memory_outbox ENABLE TRIGGER memory_outbox_preserves_history;

ALTER TABLE memory_outbox
    ADD CONSTRAINT memory_outbox_claim_generation CHECK (
        (claimed_by IS NULL AND claimed_at IS NULL AND claim_token IS NULL)
        OR
        (claimed_by IS NOT NULL AND claimed_at IS NOT NULL AND claim_token IS NOT NULL)
    );

COMMENT ON COLUMN memory_outbox.claim_token
IS 'Opaque generation for one worker lease; never sent to Hindsight or used by simulation replay.';
