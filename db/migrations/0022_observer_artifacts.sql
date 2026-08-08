CREATE TABLE observer_material_objects (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    object_id UUID NOT NULL,
    material_catalog TEXT NOT NULL CHECK (length(btrim(material_catalog)) > 0),
    material_identifier TEXT NOT NULL CHECK (length(btrim(material_identifier)) > 0),
    material_name TEXT NOT NULL CHECK (length(btrim(material_name)) > 0),
    material_source_url TEXT NOT NULL CHECK (material_source_url ~ '^https://'),
    introduced_event_id UUID NOT NULL UNIQUE,
    introduced_sequence BIGINT NOT NULL CHECK (introduced_sequence > 0),
    introduced_tick BIGINT NOT NULL CHECK (introduced_tick >= 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (projection_version, world_id, object_id)
);

CREATE TABLE observer_artifact_traces (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    object_id UUID NOT NULL,
    source_event_id UUID NOT NULL UNIQUE,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_tick BIGINT NOT NULL CHECK (source_tick >= 0),
    from_trace_units BIGINT NOT NULL CHECK (from_trace_units >= 0),
    applied_force_units INTEGER NOT NULL CHECK (applied_force_units > 0),
    to_trace_units BIGINT NOT NULL CHECK (
        to_trace_units > from_trace_units
        AND to_trace_units - from_trace_units = applied_force_units
        AND to_trace_units <= 2147483647
    ),
    provenance TEXT NOT NULL CHECK (provenance = 'world_fact'),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (projection_version, world_id, source_sequence, source_event_id),
    FOREIGN KEY (projection_version, world_id, object_id)
        REFERENCES observer_material_objects (projection_version, world_id, object_id)
);

CREATE INDEX observer_artifact_traces_world_latest
    ON observer_artifact_traces (world_id, projection_version, source_sequence DESC, source_event_id DESC);

CREATE FUNCTION reject_observer_artifact_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'observer artifact projection rows are append-only'; END
$$;

CREATE TRIGGER observer_material_objects_are_append_only
BEFORE UPDATE OR DELETE ON observer_material_objects
FOR EACH ROW EXECUTE FUNCTION reject_observer_artifact_mutation();

CREATE TRIGGER observer_artifact_traces_are_append_only
BEFORE UPDATE OR DELETE ON observer_artifact_traces
FOR EACH ROW EXECUTE FUNCTION reject_observer_artifact_mutation();

COMMENT ON TABLE observer_material_objects IS
'Safe material identity index used only to contextualize later public surface traces.';
COMMENT ON TABLE observer_artifact_traces IS
'Append-only observer filing of force-caused surface traces; never a canonical artifact label.';
