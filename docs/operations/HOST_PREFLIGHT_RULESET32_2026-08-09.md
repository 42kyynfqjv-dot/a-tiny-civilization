# Ruleset-32 production-host preflight — 2026-08-09

The long-term project host passed the complete read-only public-genesis preflight for world
`b3ea736d-7a5a-5161-a74b-fa8c4302d333` and ruleset 32. The run did not create or activate a
canonical world, change a service, or deploy the public site.

The successful run verified:

- the protected production environment and its explicitly deferred offsite-backup state;
- all static production, container, PostgreSQL-durability, privacy, shutdown, monitoring,
  Hindsight, local-cognition, and deployment-order gates;
- the portable database-free genesis manifest whose `SHA256SUMS` digest is
  `76d54b0749bd9602c625c73d9f6eac78c21ca06865ece796976e49284e06a725`;
- isolated PostgreSQL/Hindsight qualification through tick 1,000 and sequence 1,018, bound by
  evidence digest `a2ff86bfdede7daa6cec451f01df558d20f54e970c2e8c618f88064e9a865990`;
- the seven-dimension experimental quality admission, file digest
  `1f6750a373c4d3029638091073361860ed974b3ebde7195510e986579aafd358`;
- the source-bound public-observatory admission, file digest
  `c035bbd57b0cac4d5c2ed535c5fe6f026aeb7ea706e5b558c3ec524942f1c9b7`; and
- a fresh composition-0.1.2 runtime stage containing 147,469 regular files, no symbolic links,
  the complete admitted full-Earth closure, and both exact JPL DE441 segments.

The first attempted host run correctly failed closed because retaining both the active legacy
composition-0.1.1 stage and the fresh 13 GB composition-0.1.2 stage reduced free capacity below
the checked-in 10 GiB backend floor. Only Cargo's reproducible development-profile build output
was reclaimed (26.1 GiB across 50,158 generated files). Neither runtime stage, canonical inputs,
genesis artifacts, qualification evidence, images, containers, nor database volumes were removed.
The host then had about 30 GiB free at 80% filesystem use, and the identical preflight passed.

The production Compose project intentionally remains stopped and empty. Public deployment and
canonical-world activation remain separate literal-confirmation operations, and both admissions
continue to record `public_deployment_authorized: false`. This checkpoint proves readiness of the
inputs and operations path; it is not an activation record.
