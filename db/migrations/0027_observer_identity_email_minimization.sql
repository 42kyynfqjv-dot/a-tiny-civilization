UPDATE observer_identities SET email = NULL, email_verified = FALSE
WHERE email IS NOT NULL OR email_verified;

ALTER TABLE observer_identities
    ADD CONSTRAINT observer_identities_do_not_retain_email
    CHECK (email IS NULL AND NOT email_verified);

COMMENT ON COLUMN observer_identities.email IS
    'Reserved for schema compatibility; always NULL because newsletter providers retain subscriber email.';
