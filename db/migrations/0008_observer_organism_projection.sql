CREATE TABLE observer_organisms (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    organism_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('person', 'fauna')),
    species_catalog TEXT NOT NULL CHECK (length(btrim(species_catalog)) > 0),
    species_identifier TEXT NOT NULL CHECK (length(btrim(species_identifier)) > 0),
    species_scientific_name TEXT NOT NULL CHECK (length(btrim(species_scientific_name)) > 0),
    species_source_url TEXT NOT NULL CHECK (species_source_url ~ '^https://'),
    provenance TEXT NOT NULL CHECK (provenance = 'world_fact'),
    introduced_event_id UUID NOT NULL UNIQUE,
    introduced_sequence BIGINT NOT NULL CHECK (introduced_sequence > 0),
    introduced_tick BIGINT NOT NULL CHECK (introduced_tick >= 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (projection_version, world_id, organism_id)
);

CREATE INDEX observer_organisms_world_introduced
    ON observer_organisms (world_id, projection_version, introduced_sequence DESC, organism_id ASC);

CREATE TABLE observer_organism_endings (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    organism_id UUID NOT NULL,
    source_event_id UUID NOT NULL UNIQUE,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_tick BIGINT NOT NULL CHECK (source_tick >= 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (projection_version, world_id, organism_id),
    FOREIGN KEY (projection_version, world_id, organism_id)
        REFERENCES observer_organisms (projection_version, world_id, organism_id)
);

CREATE FUNCTION reject_observer_organism_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'observer organism projection rows are append-only';
END
$$;

CREATE TRIGGER observer_organisms_are_append_only
BEFORE UPDATE OR DELETE ON observer_organisms
FOR EACH ROW EXECUTE FUNCTION reject_observer_organism_mutation();

CREATE TRIGGER observer_organism_endings_are_append_only
BEFORE UPDATE OR DELETE ON observer_organism_endings
FOR EACH ROW EXECUTE FUNCTION reject_observer_organism_mutation();

COMMENT ON TABLE observer_organisms IS 'Safe observer index of canonical organism introduction facts; never simulation input.';
COMMENT ON TABLE observer_organism_endings IS 'Safe append-only observer index of canonical life-ending events, without mechanism detail.';
