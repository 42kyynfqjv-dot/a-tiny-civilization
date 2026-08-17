-- Cancer World was originally given a $2.85 hard stop despite the public
-- operating decision being a strict $3 monthly ceiling. Preserve every
-- reservation and settlement while making the final $0.15 available.
UPDATE cognition_cost_accounts
SET hard_stop_micro_usd = 3000000
WHERE billing_scope = 'cancer_research'
  AND hard_stop_micro_usd = 2850000;
