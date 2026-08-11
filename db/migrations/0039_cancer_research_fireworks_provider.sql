ALTER TABLE cancer_research_route_dispatches
    DROP CONSTRAINT cancer_research_route_dispatches_provider_slug_check;

ALTER TABLE cancer_research_route_dispatches
    ADD CONSTRAINT cancer_research_route_dispatches_provider_slug_check
    CHECK (provider_slug IN ('openrouter_cancer', 'fireworks_cancer'));

COMMENT ON CONSTRAINT cancer_research_route_dispatches_provider_slug_check
ON cancer_research_route_dispatches
IS 'Closed Cancer World provider boundary: dedicated OpenRouter plus metered Fireworks overflow.';
