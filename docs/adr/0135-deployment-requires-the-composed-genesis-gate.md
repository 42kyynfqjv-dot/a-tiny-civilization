# ADR 0135: Deployment requires the composed genesis gate

## Decision

`deploy-production-app.sh` requires absolute qualified genesis and qualification-evidence
directories in addition to the literal public-deployment confirmation. Before any volume,
image, container, or service mutation, it invokes `public-genesis-preflight.sh` with those
directories, the exact protected environment, and the fixed runtime staging root.

That composed read-only gate validates production configuration, candidate evidence, the
experimental quality-world admission, the source-bound observatory admission, and every staged
full-Earth and DE441 byte. A standalone deployment-time subset is no longer accepted.

## Consequences

An operator cannot deploy a merely buildable checkout while accidentally omitting the reviewed
world or site. Repeated deployments pay the cost of retransversing immutable runtime inputs; this
is intentional for the single-host genesis. Passing still does not activate a world or authorize
deployment—the exact confirmation flag remains independently required.
