-- Earth Genesis remains the single active observer world, while one isolated
-- experimental world may run beside it. Experiment membership is committed in
-- the immutable manifest at genesis, so this split cannot be changed later.
DROP INDEX worlds_one_unarchived;

CREATE UNIQUE INDEX worlds_one_unarchived_observer_world
ON worlds ((TRUE))
WHERE status IN ('initializing', 'running', 'extinct')
  AND manifest->'experiment' IS NULL;

CREATE UNIQUE INDEX worlds_one_unarchived_experimental_world
ON worlds ((TRUE))
WHERE status IN ('initializing', 'running', 'extinct')
  AND manifest->'experiment' IS NOT NULL;

COMMENT ON INDEX worlds_one_unarchived_observer_world IS
'At most one non-archived public observer world may exist.';

COMMENT ON INDEX worlds_one_unarchived_experimental_world IS
'At most one non-archived manifest-declared experimental world may exist.';
