-- An observer world and its isolated experimental twin may intentionally share
-- the same public seed and therefore the same content-addressed Earth inputs.
-- Seed uniqueness remains enforced within each manifest domain.
DROP INDEX IF EXISTS worlds_unique_seed;
DROP INDEX IF EXISTS worlds_unique_seed_observer;
DROP INDEX IF EXISTS worlds_unique_seed_experimental;

CREATE UNIQUE INDEX worlds_unique_seed_observer
ON worlds (seed)
WHERE manifest->'experiment' IS NULL
  AND status IN ('initializing', 'running', 'extinct');

CREATE UNIQUE INDEX worlds_unique_seed_experimental
ON worlds (seed)
WHERE manifest->'experiment' IS NOT NULL
  AND status IN ('initializing', 'running', 'extinct');

COMMENT ON INDEX worlds_unique_seed_observer IS
'Observer-world seeds are unique among observer worlds.';

COMMENT ON INDEX worlds_unique_seed_experimental IS
'Experimental-world seeds are unique among experimental worlds.';
