ALTER TABLE cancer_research_route_dispatches
    DROP CONSTRAINT cancer_research_route_dispatches_provider_slug_check;

ALTER TABLE cancer_research_route_dispatches
    ADD CONSTRAINT cancer_research_route_dispatches_provider_slug_check
    CHECK (
        provider_slug IN (
            'openrouter_cancer',
            'hetzner_experiments',
            'deterministic_research',
            'local_openai',
            'cloudflare_workers_ai',
            'groq',
            'cerebras',
            'fireworks_cancer'
        )
    );

COMMENT ON CONSTRAINT cancer_research_route_dispatches_provider_slug_check
ON cancer_research_route_dispatches
IS 'Cancer World admits the code-approved Hetzner-first, deterministic-continuity, shared-free, and isolated paid provider identities.';
