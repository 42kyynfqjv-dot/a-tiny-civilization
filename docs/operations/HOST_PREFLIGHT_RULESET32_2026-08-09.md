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

After the production wrappers were corrected to carry one explicit validated runtime root through
preflight and the runner's read-only Compose mount, the exact commit `33e59634d736fb7c1d334eee0b1eda051be9c0c6`
was built without starting services. The resulting locally tagged candidates are:

- `a-tiny-civilization-app:candidate-33e5963` —
  `sha256:09c8afcce93865c3653da02322fd406d3de435373832a4b288f4d1a65f503bc0`, user
  `civilization`; and
- `a-tiny-civilization-web:candidate-33e5963` —
  `sha256:abbfcfdd22fc335f0a8abf00eb72c439a93a53416f50b4e7120be927bf3f2262`, user `node`.

Both passed the checked-in isolated image smoke with a read-only root filesystem, all capabilities
dropped, no-new-privileges, a bounded no-exec temporary filesystem, and only an ephemeral
loopback binding for the web process. The container smoke also proved that a plaintext request
carrying the canonical public Host receives an exact method-preserving `308` to the same HTTPS
target. The production Compose render independently confirmed that the ruleset-32 v24 stage is the
runner's single read-only `/runtime` mount.

The deployment build was also reduced to one Rust image build and one observatory image build;
the API, migration, projector, and runner services all consume that one Rust tag instead of racing
four equivalent builds onto it. Repeating the deduplicated protected-environment build reproduced
the two exact image IDs recorded above.

The read-only cutover inventory reports exactly four legacy-development conflicts: web on loopback
port 3000, PostgreSQL on 5432, the observer API on 8080, and the local-cognition container's use of
the shared `atiny-ollama` volume. No unexpected production-volume consumer exists. The improved
preflight reports the complete set in one pass; it does not stop, remove, or recreate any of those
containers. Their deliberate stop remains part of the separately authorized public cutover, and
the legacy development PostgreSQL volume is retained rather than promoted.

After qualification and evidence verification completed, the two ruleset-32 qualification worker
processes and five isolated qualification database/Hindsight containers were stopped. Their named
ruleset-32, v19, Hindsight, and r18 database volumes remain attached to stopped restartable
containers. One older superseded `atiny-quality-cc8f863-db` probe had Docker auto-remove enabled;
stopping it also removed its anonymous 1.16 GB volume. That probe volume is not recoverable, but it
was not canonical history or retained launch evidence. The external v20 evidence, v24 genesis,
current ruleset-32 qualification database, development-world volume, production volumes, and every
running development service remain intact.

The private-database preparation guard is now scoped to the only resources that phase mutates: the
Compose-resolved PostgreSQL loopback port and `a-tiny-civilization-postgres-v1`. Against the
current default it reports exactly the legacy database on port 5432; a read-only protected Compose
render with production port 55432 passed while the legacy web, API, and local-model services kept
running. No protected environment value or production service was changed during that proof.

Every mutation wrapper now accepts, validates, and forwards the exact quality-world and
public-observatory admission files alongside genesis, qualification evidence, and runtime root.
Defaults remain ruleset 32 for operator convenience, but the documented release commands name both
admissions explicitly so a later default cannot change a copied historical cutover command.

The legacy-stop phase is now an explicit non-destructive operation rather than a manual container
list. Its read-only host inspection validated nine running containers in the exact safe stop order:
runner, memory worker, projector, web, API, migration sentinel, Hindsight, local cognition, and
PostgreSQL. Every container carried the expected legacy project, known service, and this checkout's
working-directory labels; no production container was running and no state changed. The confirmed
operation stops those exact identities and removes nothing.

The full cutover guard now derives all three published loopback ports from the protected Compose
render. It reported the expected four conflicts at the default ports. With `POSTGRES_PORT=55432`,
the identical read-only check omitted only the legacy 5432 database conflict and retained the web,
API, and shared local-model conflicts. This proves private database preparation and final cutover
cannot silently reason about different PostgreSQL ports.
