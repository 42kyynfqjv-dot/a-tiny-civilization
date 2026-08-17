ALTER TABLE cancer_research_fireworks_cost_reconciliations
    DROP CONSTRAINT cancer_research_fireworks_cost_reconciliation_route_index_check;

ALTER TABLE cancer_research_fireworks_cost_reconciliations
    ADD CONSTRAINT cancer_research_fireworks_cost_reconciliation_route_index_check
    CHECK (route_index >= 0 AND route_index < 16);

COMMENT ON CONSTRAINT cancer_research_fireworks_cost_reconciliation_route_index_check
ON cancer_research_fireworks_cost_reconciliations
IS 'Fireworks remains the paid tail after at most fifteen preceding free-first route positions.';
