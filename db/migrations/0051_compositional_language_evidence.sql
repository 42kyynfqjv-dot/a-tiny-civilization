-- Observer-only address for ordered call transitions. Historical evidence keeps
-- NULL and therefore remains the exact one-element form recorded before ruleset 41.
ALTER TABLE observer_language_evidence
    ADD COLUMN preceding_signal SMALLINT
    CHECK (preceding_signal BETWEEN 1 AND 32);

DROP INDEX observer_language_meaning_idx;

CREATE INDEX observer_language_meaning_idx
    ON observer_language_evidence (
        projection_version, world_id, preceding_signal, signal_form, action,
        movement_direction, source_sequence, source_event_index
    );
