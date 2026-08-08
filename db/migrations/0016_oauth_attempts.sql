CREATE TABLE observer_oauth_attempts (
    state_digest BYTEA PRIMARY KEY CHECK (octet_length(state_digest) = 32),
    provider TEXT NOT NULL CHECK (provider IN ('apple', 'google')),
    nonce_digest BYTEA NOT NULL CHECK (octet_length(nonce_digest) = 32),
    verifier_digest BYTEA NOT NULL CHECK (octet_length(verifier_digest) = 32),
    browser_binding_digest BYTEA NOT NULL CHECK (octet_length(browser_binding_digest) = 32),
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    CHECK (expires_at > created_at),
    CHECK (consumed_at IS NULL OR consumed_at >= created_at),
    CHECK (state_digest <> nonce_digest AND state_digest <> verifier_digest
        AND state_digest <> browser_binding_digest AND nonce_digest <> verifier_digest
        AND nonce_digest <> browser_binding_digest AND verifier_digest <> browser_binding_digest)
);

CREATE INDEX observer_oauth_attempts_pending_expiry
    ON observer_oauth_attempts (expires_at) WHERE consumed_at IS NULL;

CREATE FUNCTION protect_observer_oauth_attempt_history()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'OAuth attempt history cannot be deleted';
    END IF;
    IF NEW.state_digest IS DISTINCT FROM OLD.state_digest
       OR NEW.provider IS DISTINCT FROM OLD.provider
       OR NEW.nonce_digest IS DISTINCT FROM OLD.nonce_digest
       OR NEW.verifier_digest IS DISTINCT FROM OLD.verifier_digest
       OR NEW.browser_binding_digest IS DISTINCT FROM OLD.browser_binding_digest
       OR NEW.created_at IS DISTINCT FROM OLD.created_at
       OR NEW.expires_at IS DISTINCT FROM OLD.expires_at
       OR OLD.consumed_at IS NOT NULL THEN
        RAISE EXCEPTION 'OAuth attempt evidence is immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER observer_oauth_attempts_preserve_history
BEFORE UPDATE OR DELETE ON observer_oauth_attempts
FOR EACH ROW EXECUTE FUNCTION protect_observer_oauth_attempt_history();
