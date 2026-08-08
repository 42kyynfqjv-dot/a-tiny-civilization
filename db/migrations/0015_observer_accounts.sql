CREATE TABLE observer_accounts (
    id UUID PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL,
    disabled_at TIMESTAMPTZ,
    CHECK (disabled_at IS NULL OR disabled_at >= created_at)
);

CREATE TABLE observer_identities (
    provider TEXT NOT NULL CHECK (provider IN ('apple', 'google')),
    provider_subject TEXT NOT NULL CHECK (length(provider_subject) BETWEEN 1 AND 255),
    account_id UUID NOT NULL REFERENCES observer_accounts (id),
    email TEXT CHECK (email IS NULL OR length(email) BETWEEN 3 AND 320),
    email_verified BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    last_authenticated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (provider, provider_subject),
    CHECK (email IS NOT NULL OR NOT email_verified),
    CHECK (last_authenticated_at >= created_at)
);

CREATE TABLE observer_sessions (
    session_digest BYTEA PRIMARY KEY CHECK (octet_length(session_digest) = 32),
    csrf_digest BYTEA NOT NULL CHECK (octet_length(csrf_digest) = 32),
    account_id UUID NOT NULL REFERENCES observer_accounts (id),
    provider TEXT NOT NULL,
    provider_subject TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    FOREIGN KEY (provider, provider_subject)
        REFERENCES observer_identities (provider, provider_subject),
    CHECK (expires_at > created_at),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at),
    CHECK (session_digest <> csrf_digest)
);

CREATE INDEX observer_sessions_active_account
    ON observer_sessions (account_id, expires_at)
    WHERE revoked_at IS NULL;

CREATE FUNCTION protect_observer_session_identity()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'observer session history cannot be deleted';
    END IF;
    IF NEW.session_digest IS DISTINCT FROM OLD.session_digest
       OR NEW.csrf_digest IS DISTINCT FROM OLD.csrf_digest
       OR NEW.account_id IS DISTINCT FROM OLD.account_id
       OR NEW.provider IS DISTINCT FROM OLD.provider
       OR NEW.provider_subject IS DISTINCT FROM OLD.provider_subject
       OR NEW.created_at IS DISTINCT FROM OLD.created_at
       OR NEW.expires_at IS DISTINCT FROM OLD.expires_at
       OR OLD.revoked_at IS NOT NULL THEN
        RAISE EXCEPTION 'observer session identity is immutable';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER observer_sessions_preserve_history
BEFORE UPDATE OR DELETE ON observer_sessions
FOR EACH ROW EXECUTE FUNCTION protect_observer_session_identity();

COMMENT ON TABLE observer_accounts IS 'Observer-only accounts; never canonical organism identities.';
COMMENT ON COLUMN observer_sessions.session_digest IS 'SHA-256 digest; the browser bearer secret is never stored.';
COMMENT ON COLUMN observer_sessions.csrf_digest IS 'Independent SHA-256 digest for state-changing request proof.';
