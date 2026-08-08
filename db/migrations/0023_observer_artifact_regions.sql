ALTER TABLE observer_artifact_traces
    ADD COLUMN contact_region SMALLINT CHECK (contact_region >= 0 AND contact_region < 8);

COMMENT ON COLUMN observer_artifact_traces.contact_region IS
'Optional label-free physical contact region; NULL for ruleset-19 aggregate traces.';
