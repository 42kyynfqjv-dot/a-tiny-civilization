CREATE TABLE observer_timeline_items (
    projection_version INTEGER NOT NULL CHECK (projection_version > 0),
    world_id UUID NOT NULL REFERENCES worlds (id),
    source_event_id UUID NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    source_tick BIGINT NOT NULL CHECK (source_tick >= 0),
    source_event_index INTEGER NOT NULL CHECK (source_event_index >= 0),
    kind TEXT NOT NULL CHECK (kind IN (
        'world_began', 'initial_person_present', 'initial_animal_present', 'person_born',
        'animal_born', 'life_ended', 'people_extinct', 'world_archived'
    )),
    provenance TEXT NOT NULL CHECK (provenance = 'world_fact'),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 160),
    summary TEXT NOT NULL CHECK (length(summary) BETWEEN 1 AND 480),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (projection_version, source_event_id),
    UNIQUE (projection_version, world_id, source_sequence, source_event_index)
);

CREATE INDEX observer_timeline_items_world_order
    ON observer_timeline_items (world_id, projection_version, source_sequence DESC, source_event_index DESC);

CREATE FUNCTION reject_observer_timeline_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'observer timeline projection rows are append-only';
END
$$;

CREATE TRIGGER observer_timeline_items_are_append_only
BEFORE UPDATE OR DELETE ON observer_timeline_items
FOR EACH ROW EXECUTE FUNCTION reject_observer_timeline_mutation();

COMMENT ON TABLE observer_timeline_items IS 'One-way public projection from committed canonical event facts; never simulation input.';
