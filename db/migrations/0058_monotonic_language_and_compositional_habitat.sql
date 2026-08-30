-- Observer-only language milestones are immutable detector facts. The rolling
-- detector may later weaken, but a previously attained public stage must not
-- disappear from the historical record.
CREATE TABLE observer_language_milestones (
    projection_version SMALLINT NOT NULL CHECK (projection_version > 0),
    detector_version SMALLINT NOT NULL CHECK (detector_version > 0),
    world_id UUID NOT NULL REFERENCES worlds(id),
    stage TEXT NOT NULL CHECK (stage IN ('proto_lexicon', 'rudimentary_language_candidate')),
    stage_rank SMALLINT NOT NULL CHECK (stage_rank IN (1, 2)),
    attained_sequence BIGINT NOT NULL CHECK (attained_sequence > 0),
    attained_tick BIGINT NOT NULL CHECK (attained_tick >= 0),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (projection_version, detector_version, world_id, stage),
    CHECK (
        (stage = 'proto_lexicon' AND stage_rank = 1)
        OR (stage = 'rudimentary_language_candidate' AND stage_rank = 2)
    )
);

CREATE INDEX observer_language_milestones_highest
    ON observer_language_milestones (
        world_id, projection_version, detector_version, stage_rank DESC
    );

COMMENT ON TABLE observer_language_milestones IS
'Append-only observer detector milestones; current rolling evidence may weaken without erasing a stage already attained.';

-- Preserve the one version-5 crossing already published in the ordinary world's
-- operations record. This is intentionally a narrow no-op unless the immutable
-- evidence still has the exact documented identity and detector signature:
-- [27,5] -> rest, 12 events, 9 learners, 5 sources, 100% form dominance,
-- spanning ticks 3,671 through 4,772 and satisfying all original gates.
WITH boundary AS (
    SELECT 4772::BIGINT AS latest_tick, 4197::BIGINT AS recent_half_start
), eligible_evidence AS (
    SELECT evidence.*
    FROM observer_language_evidence evidence
    JOIN observer_organisms learner
      ON learner.projection_version = 1
     AND learner.world_id = evidence.world_id
     AND learner.organism_id = evidence.observer_id
     AND learner.role = 'person'
    JOIN observer_organisms source
      ON source.projection_version = 1
     AND source.world_id = evidence.world_id
     AND source.organism_id = evidence.actor_id
     AND source.role = 'person'
    WHERE evidence.projection_version = 1
      AND evidence.world_id = 'f5012e01-252d-5841-bc09-143826a20e16'::UUID
      AND evidence.action NOT IN ('bite', 'emit_signal')
      AND evidence.source_tick BETWEEN 3621 AND 4772
), meaning AS (
    SELECT
        COUNT(*)::BIGINT AS evidence_events,
        COUNT(*) FILTER (
            WHERE source_tick >= (SELECT recent_half_start FROM boundary)
        )::BIGINT AS recent_evidence_events,
        COUNT(DISTINCT observer_id)::BIGINT AS learners,
        COUNT(DISTINCT actor_id)::BIGINT AS signal_sources,
        MIN(source_tick)::BIGINT AS first_tick,
        MAX(source_tick)::BIGINT AS latest_tick,
        MAX(source_sequence)::BIGINT AS attained_sequence
    FROM eligible_evidence
    WHERE preceding_signal = 27
      AND signal_form = 5
      AND action = 'rest'
      AND movement_direction IS NULL
), form_total AS (
    SELECT
        COUNT(*)::BIGINT AS form_events,
        COUNT(*) FILTER (
            WHERE source_tick >= (SELECT recent_half_start FROM boundary)
        )::BIGINT AS recent_form_events
    FROM eligible_evidence
    WHERE preceding_signal = 27 AND signal_form = 5
), baseline AS (
    SELECT COUNT(*)::BIGINT AS baseline_events
    FROM eligible_evidence
    WHERE action = 'rest' AND movement_direction IS NULL
), eligible_total AS (
    SELECT COUNT(*)::BIGINT AS eligible_events FROM eligible_evidence
), exact_documented_crossing AS (
    SELECT meaning.attained_sequence
    FROM meaning
    CROSS JOIN form_total
    CROSS JOIN baseline
    CROSS JOIN eligible_total
    WHERE meaning.evidence_events = 12
      AND meaning.learners = 9
      AND meaning.signal_sources = 5
      AND meaning.first_tick = 3671
      AND meaning.latest_tick = 4772
      AND form_total.form_events = 12
      AND meaning.recent_evidence_events >= 4
      AND meaning.evidence_events - meaning.recent_evidence_events >= 4
      AND FLOOR(meaning.recent_evidence_events::NUMERIC * 100
            / NULLIF(form_total.recent_form_events, 0)) >= 55
      AND FLOOR((meaning.evidence_events - meaning.recent_evidence_events)::NUMERIC * 100
            / NULLIF(form_total.form_events - form_total.recent_form_events, 0)) >= 55
      AND FLOOR(meaning.evidence_events::NUMERIC * 100 / form_total.form_events) >= 60
      AND FLOOR(meaning.evidence_events::NUMERIC * 100 / form_total.form_events)
            >= FLOOR(baseline.baseline_events::NUMERIC * 100 / eligible_total.eligible_events) + 15
      AND FLOOR(meaning.evidence_events::NUMERIC * eligible_total.eligible_events * 100
            / (form_total.form_events * baseline.baseline_events)) >= 150
)
INSERT INTO observer_language_milestones (
    projection_version, detector_version, world_id, stage, stage_rank,
    attained_sequence, attained_tick
)
SELECT
    1, 6, 'f5012e01-252d-5841-bc09-143826a20e16'::UUID,
    'proto_lexicon', 1, attained_sequence, 4772
FROM exact_documented_crossing
ON CONFLICT (projection_version, detector_version, world_id, stage) DO NOTHING;

-- Habitat communication originally exposed only the final physical form. Keep
-- the preceding form recorded by compositional association events so the public
-- stream does not collapse an inhabitant-produced sequence into an atomic call.
ALTER TABLE observer_habitat_communication
    ADD COLUMN preceding_signal INTEGER
    CHECK (preceding_signal BETWEEN 1 AND 32);

UPDATE observer_habitat_communication communication
SET preceding_signal = evidence.preceding_signal
FROM observer_language_evidence evidence
WHERE communication.world_id = evidence.world_id
  AND communication.source_event_id = evidence.source_event_id
  AND communication.kind = 'associated_action'
  AND evidence.preceding_signal IS NOT NULL;

COMMENT ON COLUMN observer_habitat_communication.preceding_signal IS
'Optional first physical element of an inhabitant-produced ordered call; NULL denotes an atomic call or legacy hearing evidence.';

-- Habitat and language have independent cursors. If habitat is ahead when this
-- migration lands, the one-shot UPDATE above cannot yet find its language row.
-- Reconcile again when that evidence catches up; if language is ahead instead,
-- habitat later inserts the canonical prefix directly from the same event.
CREATE FUNCTION observer_language_evidence_reconciles_habitat_prefix()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.preceding_signal IS NOT NULL THEN
        IF EXISTS (
            SELECT 1
            FROM observer_habitat_communication
            WHERE projection_version = NEW.projection_version
              AND world_id = NEW.world_id
              AND source_event_id = NEW.source_event_id
              AND kind = 'associated_action'
              AND preceding_signal IS NOT NULL
              AND preceding_signal <> NEW.preceding_signal
        ) THEN
            RAISE EXCEPTION 'habitat communication prefix disagrees with language evidence';
        END IF;

        UPDATE observer_habitat_communication
        SET preceding_signal = NEW.preceding_signal
        WHERE projection_version = NEW.projection_version
          AND world_id = NEW.world_id
          AND source_event_id = NEW.source_event_id
          AND kind = 'associated_action'
          AND preceding_signal IS NULL;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER observer_language_evidence_reconciles_habitat_prefix
AFTER INSERT ON observer_language_evidence
FOR EACH ROW
EXECUTE FUNCTION observer_language_evidence_reconciles_habitat_prefix();
