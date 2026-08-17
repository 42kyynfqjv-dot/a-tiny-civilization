-- Route policy v10 contains optional direct-provider positions. An absent
-- adapter is recorded as skipped_unconfigured in the signed terminal ladder
-- result, but no network dispatch row exists for it. Preserve the important
-- invariant here: every earlier route that was actually dispatched must have a
-- terminal failed outcome before a later configured route can be dispatched.
CREATE OR REPLACE FUNCTION validate_cancer_research_dispatch_insert()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM 1
    FROM cancer_research_requests
    WHERE request_id = NEW.request_id
      AND completed_at IS NULL
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'claimable cancer research request does not exist';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM cancer_research_route_dispatches AS earlier
        LEFT JOIN cancer_research_route_outcomes AS outcome
          ON outcome.request_id = earlier.request_id
         AND outcome.route_index = earlier.route_index
        WHERE earlier.request_id = NEW.request_id
          AND earlier.route_index < NEW.route_index
          AND (
              outcome.route_index IS NULL
              OR outcome.normalized_status = 'succeeded'
          )
    ) THEN
        RAISE EXCEPTION 'earlier dispatched cancer research routes must form a failed prefix';
    END IF;
    IF EXISTS (
        SELECT 1 FROM cancer_research_route_outcomes
        WHERE request_id = NEW.request_id AND normalized_status = 'succeeded'
    ) THEN
        RAISE EXCEPTION 'cancer research route dispatches already succeeded';
    END IF;
    RETURN NEW;
END
$$;

COMMENT ON FUNCTION validate_cancer_research_dispatch_insert()
IS 'Allows signed skipped-unconfigured policy positions while requiring every earlier actual network dispatch to have failed.';
