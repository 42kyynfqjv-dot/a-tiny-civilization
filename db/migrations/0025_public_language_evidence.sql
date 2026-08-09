-- Disposable observer-only evidence for cautious signal-convention detection.
-- Canonical events remain immutable and this projection can be rebuilt at any time.
CREATE TABLE observer_language_evidence (
    projection_version SMALLINT NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds(id),
    source_event_id UUID NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_tick BIGINT NOT NULL CHECK (source_tick >= 0),
    source_event_index INTEGER NOT NULL CHECK (source_event_index >= 0),
    observer_id UUID NOT NULL,
    actor_id UUID NOT NULL,
    signal_form SMALLINT NOT NULL CHECK (signal_form BETWEEN 1 AND 32),
    action TEXT NOT NULL CHECK (action IN ('move','orient','reach','grasp','release','apply_force','bite','chew','swallow','rest','emit_signal')),
    movement_direction SMALLINT CHECK (movement_direction BETWEEN 0 AND 3),
    PRIMARY KEY (projection_version, world_id, source_event_id),
    UNIQUE (projection_version, world_id, source_sequence, source_event_index),
    CHECK ((action = 'move') OR movement_direction IS NULL)
);

CREATE INDEX observer_language_meaning_idx
    ON observer_language_evidence (
        projection_version, world_id, signal_form, action, movement_direction,
        source_sequence, source_event_index
    );
