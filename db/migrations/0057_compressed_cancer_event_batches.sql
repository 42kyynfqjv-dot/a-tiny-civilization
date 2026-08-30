ALTER TABLE event_batches
    ADD COLUMN payload_encoding TEXT NOT NULL DEFAULT 'jsonb-v1',
    ADD COLUMN compressed_payload BYTEA,
    ADD COLUMN uncompressed_payload_bytes INTEGER,
    ADD CONSTRAINT event_batches_payload_storage_shape CHECK (
        (
            payload_encoding = 'jsonb-v1'
            AND compressed_payload IS NULL
            AND uncompressed_payload_bytes IS NULL
        )
        OR (
            payload_encoding = 'zlib-json-v1'
            AND payload = 'null'::JSONB
            AND compressed_payload IS NOT NULL
            AND octet_length(compressed_payload) > 0
            AND uncompressed_payload_bytes BETWEEN 1 AND 33554432
        )
    );

COMMENT ON COLUMN event_batches.payload_encoding IS
'Immutable storage codec only; canonical EventBatch bytes and checksums are codec-independent.';
COMMENT ON COLUMN event_batches.compressed_payload IS
'Zlib-compressed canonical JSON for large Cancer World batches; decoded and checksum-verified before replay.';
