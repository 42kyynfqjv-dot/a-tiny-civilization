CREATE TABLE observer_habitat_entities (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    organism_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('person', 'fauna')),
    species_catalog TEXT NOT NULL CHECK (length(btrim(species_catalog)) > 0),
    species_identifier TEXT NOT NULL CHECK (length(btrim(species_identifier)) > 0),
    species_scientific_name TEXT NOT NULL CHECK (length(btrim(species_scientific_name)) > 0),
    species_source_url TEXT NOT NULL CHECK (species_source_url ~ '^https://'),
    embodied_patch CHAR(16) NOT NULL CHECK (embodied_patch ~ '^[0-9a-f]{16}$'),
    latitude_e7 INTEGER NOT NULL CHECK (latitude_e7 BETWEEN -900000000 AND 900000000),
    longitude_e7 INTEGER NOT NULL CHECK (longitude_e7 >= -1800000000 AND longitude_e7 < 1800000000),
    previous_latitude_e7 INTEGER NOT NULL CHECK (previous_latitude_e7 BETWEEN -900000000 AND 900000000),
    previous_longitude_e7 INTEGER NOT NULL CHECK (previous_longitude_e7 >= -1800000000 AND previous_longitude_e7 < 1800000000),
    last_movement_sequence BIGINT NOT NULL CHECK (last_movement_sequence > 0),
    last_movement_tick BIGINT NOT NULL CHECK (last_movement_tick >= 0),
    last_action TEXT CHECK (last_action IN ('move','orient','reach','grasp','release','apply_force','bite','chew','swallow','rest','emit_signal')),
    signal_form INTEGER CHECK (signal_form BETWEEN 1 AND 32),
    alive BOOLEAN NOT NULL DEFAULT TRUE,
    updated_sequence BIGINT NOT NULL CHECK (updated_sequence > 0),
    PRIMARY KEY (projection_version, world_id, organism_id),
    CHECK (last_action = 'emit_signal' OR signal_form IS NULL)
);

CREATE INDEX observer_habitat_entities_viewport
    ON observer_habitat_entities (world_id, projection_version, longitude_e7, latitude_e7)
    WHERE alive;
CREATE INDEX observer_habitat_entities_role
    ON observer_habitat_entities (world_id, projection_version, role, organism_id)
    WHERE alive;

CREATE TABLE observer_habitat_activity (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    source_event_id UUID NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_tick BIGINT NOT NULL CHECK (source_tick >= 0),
    source_event_index INTEGER NOT NULL CHECK (source_event_index >= 0),
    organism_id UUID NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('move','orient','reach','grasp','release','apply_force','bite','chew','swallow','rest','emit_signal')),
    signal_form INTEGER CHECK (signal_form BETWEEN 1 AND 32),
    PRIMARY KEY (projection_version, world_id, source_event_id),
    CHECK (action = 'emit_signal' OR signal_form IS NULL)
);

CREATE INDEX observer_habitat_activity_latest
    ON observer_habitat_activity (world_id, projection_version, source_sequence DESC, source_event_index DESC);

COMMENT ON TABLE observer_habitat_entities IS
'Disposable bounded current-position projection for the public habitat renderer; never simulation input.';
COMMENT ON TABLE observer_habitat_activity IS
'Bounded recent primitive activity projection used by the public habitat ticker; never simulation input.';
