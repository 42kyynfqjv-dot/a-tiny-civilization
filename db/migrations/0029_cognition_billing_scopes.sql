ALTER TABLE cognition_cost_accounts
    ADD COLUMN billing_scope TEXT NOT NULL DEFAULT 'production'
    CHECK (billing_scope IN ('production', 'cancer_research'));

ALTER TABLE cognition_cost_reservations
    ADD COLUMN billing_scope TEXT NOT NULL DEFAULT 'production'
    CHECK (billing_scope IN ('production', 'cancer_research'));

ALTER TABLE cognition_cost_reservations
    DROP CONSTRAINT cognition_cost_reservations_billing_month_fkey;

ALTER TABLE cognition_cost_accounts
    DROP CONSTRAINT cognition_cost_accounts_pkey;

ALTER TABLE cognition_cost_accounts
    ADD PRIMARY KEY (billing_scope, billing_month);

ALTER TABLE cognition_cost_reservations
    ADD CONSTRAINT cognition_cost_reservations_billing_account_fkey
    FOREIGN KEY (billing_scope, billing_month)
    REFERENCES cognition_cost_accounts (billing_scope, billing_month);

COMMENT ON COLUMN cognition_cost_accounts.billing_scope IS
'Hard budget isolation boundary. Cancer research can never consume the production-world cognition treasury.';
