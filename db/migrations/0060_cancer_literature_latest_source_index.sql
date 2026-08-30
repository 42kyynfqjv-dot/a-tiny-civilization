CREATE INDEX cancer_research_literature_latest_source_idx
    ON cancer_research_literature (
        world_id,
        source_id,
        retrieved_at DESC,
        evidence_id DESC
    );

COMMENT ON INDEX cancer_research_literature_latest_source_idx IS
    'Supports deterministic latest immutable snapshot selection per external literature source.';
