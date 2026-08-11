CREATE TABLE cancer_research_literature (
    evidence_id UUID PRIMARY KEY,
    world_id UUID NOT NULL REFERENCES worlds(id),
    source_id TEXT NOT NULL,
    title TEXT NOT NULL,
    license TEXT NOT NULL CHECK (license IN ('cc by', 'cc0')),
    published_at DATE,
    content TEXT NOT NULL,
    content_hash BYTEA NOT NULL CHECK (octet_length(content_hash) = 32),
    source_payload JSONB NOT NULL,
    retrieved_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (world_id, source_id, content_hash)
);

CREATE INDEX cancer_research_literature_world_recent_idx
    ON cancer_research_literature (world_id, published_at DESC NULLS LAST, evidence_id);

COMMENT ON TABLE cancer_research_literature IS
    'Immutable, content-addressed CC BY/CC0 literature snapshots for observer-side Cancer World audits.';
