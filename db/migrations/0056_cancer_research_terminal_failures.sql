CREATE TABLE cancer_research_terminal_failures (
    request_id UUID PRIMARY KEY
        REFERENCES cancer_research_requests (request_id),
    request_checksum BYTEA NOT NULL
        CHECK (octet_length(request_checksum) = 32),
    failure_class TEXT NOT NULL CHECK (
        failure_class IN (
            'store_migration',
            'store_conflict',
            'store_not_found',
            'store_corrupt',
            'worker_corrupt'
        )
    ),
    failure_text TEXT NOT NULL CHECK (
        length(failure_text) > 0 AND length(failure_text) <= 2048
    ),
    claim_count BIGINT NOT NULL CHECK (claim_count > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE FUNCTION protect_cancer_research_terminal_failure_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'cancer research terminal failure history is immutable';
END
$$;

CREATE TRIGGER cancer_research_terminal_failures_preserve_history
BEFORE UPDATE OR DELETE ON cancer_research_terminal_failures
FOR EACH ROW EXECUTE FUNCTION protect_cancer_research_terminal_failure_history();

COMMENT ON TABLE cancer_research_terminal_failures IS
'Append-only dead-letter evidence for non-transient research worker failures; these are infrastructure failures, never scientific outcomes.';
