ALTER TABLE cancer_research_route_dispatches
    DROP CONSTRAINT cancer_research_route_dispatches_route_index_check;

ALTER TABLE cancer_research_route_dispatches
    ADD CONSTRAINT cancer_research_route_dispatches_route_index_check
    CHECK (route_index >= 0 AND route_index < 256);

ALTER TABLE cancer_research_route_dispatches
    DROP CONSTRAINT cancer_research_route_dispatches_provider_slug_check;

ALTER TABLE cancer_research_route_dispatches
    ADD CONSTRAINT cancer_research_route_dispatches_provider_slug_check
    CHECK (
        provider_slug IN (
            'openrouter_cancer',
            'local_openai',
            'cloudflare_workers_ai',
            'groq',
            'cerebras',
            'fireworks_cancer'
        )
    );

COMMENT ON CONSTRAINT cancer_research_route_dispatches_route_index_check
ON cancer_research_route_dispatches
IS 'Versioned Cancer World ladders may address any code-approved route within the global 256-route contract bound.';

COMMENT ON CONSTRAINT cancer_research_route_dispatches_provider_slug_check
ON cancer_research_route_dispatches
IS 'Cancer World may reuse code-approved free-first providers while retaining its dedicated OpenRouter/Fireworks identities, research memory, receipts, and treasury.';
