CREATE TABLE observer_habitat_communication (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    source_event_id UUID NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_tick BIGINT NOT NULL CHECK (source_tick >= 0),
    source_event_index INTEGER NOT NULL CHECK (source_event_index >= 0),
    kind TEXT NOT NULL CHECK (kind IN ('heard_signal', 'associated_action')),
    source_organism_id UUID NOT NULL,
    observer_organism_id UUID NOT NULL,
    signal_form INTEGER NOT NULL CHECK (signal_form BETWEEN 1 AND 32),
    associated_action TEXT CHECK (associated_action IN (
        'move','orient','reach','grasp','release','apply_force','chew','swallow','rest','emit_signal'
    )),
    PRIMARY KEY (projection_version, world_id, source_event_id),
    CHECK (
        (kind = 'heard_signal' AND associated_action IS NULL)
        OR (kind = 'associated_action' AND associated_action IS NOT NULL)
    ),
    CHECK (source_organism_id <> observer_organism_id)
);

CREATE INDEX observer_habitat_communication_latest
    ON observer_habitat_communication (
        world_id, projection_version, source_sequence DESC, source_event_index DESC
    );

COMMENT ON TABLE observer_habitat_communication IS
'Bounded observer-only projection of directly heard signals and private signal/action associations; it never enters simulation state and makes no claim of language or intent.';
