CREATE TABLE worlds (
    id UUID PRIMARY KEY,
    seed TEXT NOT NULL CHECK (seed ~ '^[0-9]{1,20}$'),
    status TEXT NOT NULL CHECK (status IN ('initializing', 'running', 'extinct', 'archived')),
    ruleset_version INTEGER NOT NULL CHECK (ruleset_version > 0),
    current_tick BIGINT NOT NULL DEFAULT 0 CHECK (current_tick >= 0),
    current_sequence BIGINT NOT NULL DEFAULT 0 CHECK (current_sequence >= 0),
    predecessor_world_id UUID REFERENCES worlds (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    extinct_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    CHECK (predecessor_world_id IS NULL OR predecessor_world_id <> id)
);

CREATE UNIQUE INDEX worlds_one_unarchived
    ON worlds ((true))
    WHERE status IN ('initializing', 'running', 'extinct');

CREATE TABLE event_batches (
    world_id UUID NOT NULL REFERENCES worlds (id),
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    tick BIGINT NOT NULL CHECK (tick >= 0),
    event_schema_version INTEGER NOT NULL CHECK (event_schema_version > 0),
    ruleset_version INTEGER NOT NULL CHECK (ruleset_version > 0),
    payload JSONB NOT NULL,
    checksum BYTEA NOT NULL,
    appended_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (world_id, sequence)
);

CREATE INDEX event_batches_world_tick ON event_batches (world_id, tick);

CREATE TABLE snapshots (
    world_id UUID NOT NULL REFERENCES worlds (id),
    through_sequence BIGINT NOT NULL CHECK (through_sequence >= 0),
    tick BIGINT NOT NULL CHECK (tick >= 0),
    snapshot_schema_version INTEGER NOT NULL CHECK (snapshot_schema_version > 0),
    ruleset_version INTEGER NOT NULL CHECK (ruleset_version > 0),
    state JSONB NOT NULL,
    checksum BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (world_id, through_sequence)
);

CREATE TABLE outbox (
    id UUID PRIMARY KEY,
    world_id UUID REFERENCES worlds (id),
    source_sequence BIGINT,
    topic TEXT NOT NULL,
    payload JSONB NOT NULL,
    available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_by TEXT,
    claimed_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX outbox_pending ON outbox (available_at, created_at) WHERE completed_at IS NULL;

CREATE TABLE memory_outbox (
    operation_id UUID PRIMARY KEY,
    document_id UUID NOT NULL,
    world_id UUID NOT NULL REFERENCES worlds (id),
    agent_id UUID NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    bank_id TEXT NOT NULL,
    payload_version INTEGER NOT NULL CHECK (payload_version > 0),
    payload JSONB NOT NULL,
    available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (bank_id, document_id, payload_version)
);

CREATE INDEX memory_outbox_pending
    ON memory_outbox (available_at, created_at)
    WHERE completed_at IS NULL;

CREATE TABLE projection_offsets (
    projection_name TEXT NOT NULL,
    world_id UUID NOT NULL REFERENCES worlds (id),
    through_sequence BIGINT NOT NULL DEFAULT 0 CHECK (through_sequence >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (projection_name, world_id)
);

CREATE TABLE service_heartbeats (
    service_name TEXT NOT NULL,
    instance_id UUID NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    PRIMARY KEY (service_name, instance_id)
);

COMMENT ON TABLE event_batches IS 'Authoritative append-only simulation history, batched by committed transition.';
COMMENT ON TABLE snapshots IS 'Replaceable replay acceleration cache; never the sole source of history.';
COMMENT ON TABLE memory_outbox IS 'Eventually consistent delivery of subjective memories to Hindsight.';
