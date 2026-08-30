ALTER TABLE cognition_requests
    ADD COLUMN claim_token UUID;

-- A deployment can interrupt a request after its dispatch row commits. Any
-- nonterminal lease held by pre-generation code is intentionally invalidated so
-- the replacement worker can immediately audit and abandon that in-flight
-- attempt instead of waiting for a long whole-ladder lease.
UPDATE cognition_requests AS request
SET claimed_by = NULL,
    claimed_at = NULL,
    claim_token = NULL
WHERE request.claimed_by IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM cognition_results AS result
      WHERE result.request_id = request.request_id
  )
  AND (
      NOT EXISTS (
          SELECT 1 FROM cognition_deadline_latches AS latch
          WHERE latch.request_id = request.request_id
      )
      OR EXISTS (
          SELECT 1 FROM cognition_route_attempts AS attempt
          WHERE attempt.request_id = request.request_id
            AND attempt.dispatch_state = 'dispatched'
      )
      OR EXISTS (
          SELECT 1 FROM cognition_cost_reservations AS reservation
          WHERE reservation.request_id = request.request_id
            AND reservation.status = 'reserved'
      )
  );

-- Terminal historical requests retain their operational owner metadata. Give
-- those inert rows a well-formed generation without changing canonical history.
UPDATE cognition_requests
SET claim_token = gen_random_uuid()
WHERE claimed_by IS NOT NULL;

ALTER TABLE cognition_requests
    ADD CONSTRAINT cognition_requests_claim_generation CHECK (
        (claimed_by IS NULL AND claimed_at IS NULL AND claim_token IS NULL)
        OR
        (claimed_by IS NOT NULL AND claimed_at IS NOT NULL AND claim_token IS NOT NULL)
    );

COMMENT ON COLUMN cognition_requests.claim_token
IS 'Opaque generation for one worker lease; protects late route responses from same-worker ABA after reclaim.';
