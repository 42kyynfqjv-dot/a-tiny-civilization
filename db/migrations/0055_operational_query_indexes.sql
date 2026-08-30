-- Bound the observer's recent language-window query by tick before it groups
-- meanings. The existing meaning-first index remains useful for convention
-- grouping; this complementary index prevents a full world-evidence scan.
CREATE INDEX observer_language_evidence_world_tick
    ON observer_language_evidence (projection_version, world_id, source_tick DESC);

-- Heartbeats are disposable operational presence, not canonical history.
-- record_heartbeat removes entries older than 30 days per service; this index
-- keeps that bounded cleanup independent of accumulated restart identities.
CREATE INDEX service_heartbeats_service_last_seen
    ON service_heartbeats (service_name, last_seen_at);

COMMENT ON INDEX observer_language_evidence_world_tick IS
'Bounds recent observer language detection by world and source tick.';

COMMENT ON INDEX service_heartbeats_service_last_seen IS
'Supports bounded removal of stale operational heartbeat instances; heartbeats are not canonical history.';
