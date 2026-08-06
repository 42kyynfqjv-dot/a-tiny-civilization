CREATE TABLE observer_finding_lives (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    organism_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('person', 'fauna')),
    PRIMARY KEY (projection_version, world_id, organism_id)
);

CREATE TABLE observer_finding_life_endings (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    organism_id UUID NOT NULL,
    PRIMARY KEY (projection_version, world_id, organism_id),
    FOREIGN KEY (projection_version, world_id, organism_id)
        REFERENCES observer_finding_lives (projection_version, world_id, organism_id)
);

CREATE TABLE observer_finding_records (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    metric TEXT NOT NULL CHECK (metric IN ('people_population', 'animal_population')),
    value BIGINT NOT NULL CHECK (value >= 0),
    PRIMARY KEY (projection_version, world_id, metric)
);

CREATE TABLE observer_findings (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    source_event_id UUID NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_tick BIGINT NOT NULL CHECK (source_tick >= 0),
    kind TEXT NOT NULL CHECK (kind IN ('first', 'record', 'streak')),
    finding_key TEXT NOT NULL CHECK (finding_key ~ '^[a-z0-9_]+$'),
    provenance TEXT NOT NULL CHECK (provenance = 'world_fact'),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 160),
    summary TEXT NOT NULL CHECK (length(summary) BETWEEN 1 AND 480),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (projection_version, world_id, finding_key)
);

CREATE INDEX observer_findings_world_order
    ON observer_findings (world_id, projection_version, source_sequence DESC, source_event_id DESC);

CREATE FUNCTION reject_observer_finding_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'observer finding rows are append-only'; END
$$;

CREATE TRIGGER observer_findings_are_append_only
BEFORE UPDATE OR DELETE ON observer_findings
FOR EACH ROW EXECUTE FUNCTION reject_observer_finding_mutation();

COMMENT ON TABLE observer_findings IS 'Versioned deterministic observer finding aids. They never affect canonical history.';
