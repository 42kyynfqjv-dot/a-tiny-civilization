CREATE TABLE observer_world_telemetry (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    through_sequence BIGINT NOT NULL CHECK (through_sequence >= 0),
    committed_events BIGINT NOT NULL CHECK (committed_events >= 0),
    canonical_payload_bytes BIGINT NOT NULL CHECK (canonical_payload_bytes >= 0),
    PRIMARY KEY (projection_version, world_id)
);

COMMENT ON TABLE observer_world_telemetry IS
'Disposable observer-side counters derived from immutable event batches; never a simulation input.';
