UPDATE cognition_cost_accounts
SET target_micro_usd = 7500000,
    hard_stop_micro_usd = 8000000,
    updated_at = NOW()
WHERE billing_scope = 'cancer_research'
  AND target_micro_usd = 2500000
  AND hard_stop_micro_usd = 3000000;
