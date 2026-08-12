# Production runbook

This is a one-server deployment runbook for the first bounded population. It is
intentionally not a launch authorization: the runner can initialize the provisional
full-Earth path, but genesis remains closed until its complete seed-specific artifact
set and accelerated integration gates pass.

## Network boundary

`compose.yaml` publishes the web and observer API only to loopback. The optional
`compose.tunnel.yaml` joins `cloudflared` only to the `edge` Docker network, which can
reach `web`. The web-to-API network is separate and internal, so the tunnel cannot
reach the observer API; PostgreSQL, the runner, projector, and migrations remain on
the separate `backend` network. Configure the remotely managed Cloudflare Tunnel public
hostname to use `http://web:3000`; do not route to `api`, `db`, or a host port.

Keep the host firewall closed to public inbound HTTP and database traffic. Use SSH with
key-based access for the host and Cloudflare Access for any administrative route that
is later added. The observatory itself remains read-only.

The web origin applies the browser security policy to rendered pages, assets, error
responses, and proxied observer JSON. Dynamic pages and `/api/` responses are
`no-store`; content-hashed framework assets remain immutable. Verify these origin
headers through the public hostname after each web deployment rather than relying only
on dashboard settings.

### Docker bridge prerequisite

This deployment deliberately separates `edge`, `web-api`, and `backend` onto Docker
bridges. On this host, bridge netfilter must not send that internal container-to-
container traffic through host iptables: it prevents the web container from reaching
the observer API even though both services and Docker network membership are healthy.

Before deployment, ensure the dedicated host has this persistent sysctl setting:

```ini
# /etc/sysctl.d/60-atiny-docker-bridge.conf
net.bridge.bridge-nf-call-iptables = 0
```

Apply and verify it with `sudo sysctl -p /etc/sysctl.d/60-atiny-docker-bridge.conf`
and `sysctl net.bridge.bridge-nf-call-iptables`. This is a host-level networking
choice: retain the loopback-only published ports and Compose network boundaries above;
do not compensate by placing `cloudflared` or PostgreSQL on the web/API network.

### Cancer World outbound research workers

The production host keeps Docker forwarding closed, so the two processes that need
outbound HTTPS run as hardened, unprivileged system services rather than weakening the
container firewall. Install the checked-in units after building the release runner:

```bash
cargo build --locked --release -p civilization-runner
sudo install -o root -g root -m 0644 \
  ops/systemd/atiny-cancer-evidence.service /etc/systemd/system/
sudo install -o root -g root -m 0644 \
  ops/systemd/atiny-cancer-research.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now \
  atiny-cancer-evidence.service atiny-cancer-research.service
```

Both units load the root-protected production environment through systemd, run as the
unprivileged project owner, expose no listener, and retain read-only host/filesystem
hardening. On every evidence-worker start, its `ExecStartPre` verifies the pinned
NCI-60 catalogue and ignored answer key, then copies only the answer key into the
unit-private `/run/atiny-cancer-evidence/` staging tree. A hash mismatch or missing
key fails the unit before any qualification can run. Container-profile operators
must first run `bash scripts/stage-cancer-nci60-qualification-key.sh`; Compose mounts
that content-addressed, mode-`0444` file only into `cancer-evidence-worker` and refuses
to auto-create a missing source path.
The host model-worker unit also makes both the ignored source key and the evidence
worker's `/run` staging tree inaccessible inside its own mount namespace.

The evidence worker retrieves at most 24 current, CC BY/CC0 glioblastoma
records from Europe PMC every six hours. The model worker uses dedicated Cancer World
OpenRouter and optional Fireworks keys. Exploration tries the pinned free GPT-OSS route
before the metered Fireworks GPT-OSS overflow; literature audits retain their separate
DeepSeek ladder. The opportunistic free attempt is bounded to 30 seconds while the
treasury-capped Fireworks attempt retains the normal 120-second timeout, so shared-pool
congestion cannot halve the deterministic research cadence. All paid calls share the
independent durable $2.85 monthly circuit breaker. Their Compose
equivalents are behind the `container-research` profile for development hosts whose
Docker bridges already have outbound HTTPS; do not run both copies concurrently.

## Owner-controlled setup

On the Ubuntu server, clone a tagged repository revision and create a root-readable
environment file outside the checkout. Do not put credentials in `.env`, Git, shell
history, browser screenshots, or chat.

The account-by-account boundary, timing, and secret-handling rules are in
[Owner-controlled production handoffs](EXTERNAL_HANDOFFS.md). No current integration
requires a screen share or a credential in this repository.

Required runtime values are:

- `POSTGRES_DB`, `POSTGRES_USER`, and a unique `POSTGRES_PASSWORD`;
- `LOCAL_COGNITION_BASE_URL=http://local-cognition:11434/v1` for the standard private CPU model.
  Remote provider keys are optional; free routes are attempted first and paid cognition is disabled
  unless `COGNITION_PAID_ENABLED=true` is explicitly set;
- `COGNITION_EXTERNAL_EXPORT_APPROVED=true` only when a remote provider key is configured, and only
  after approving that provider's receipt of private cognition and recalled-memory context;
- `APP_ENV=production`.

The local Hindsight service is keyless inside its private Docker network. Stripe and
Google OAuth stay disabled unless their complete paired settings are supplied; Apple
Sign in remains blank until that adapter is enabled. This host currently runs Cloudflare Tunnel as a separate system service, so
the application environment does not need `CLOUDFLARE_TUNNEL_TOKEN`. A Compose-managed
tunnel requires it and sets `ATINY_REQUIRE_COMPOSE_TUNNEL=1` during preflight.

Encrypted WAL-G/R2 settings are supported but explicitly deferred for the first
genesis by owner decision. Supplying any backup value requires the complete set;
backup and restore commands additionally set `ATINY_REQUIRE_OFFSITE_BACKUP=1` and fail
closed when it is absent.

Create a separate staging tunnel and production tunnel. In Cloudflare's dashboard,
create the production tunnel and public hostname, then set its service to
`http://web:3000`. A tunnel token is equivalent to permission to run that tunnel, so
rotate it if exposed and replace it only in the server's environment file. Cloudflare
documents remotely managed Docker tunnels and token rotation in its
[Tunnel documentation](https://developers.cloudflare.com/tunnel/setup/) and
[token guidance](https://developers.cloudflare.com/tunnel/advanced/tunnel-tokens/).

## Preflight and deployment

### Current host-managed tunnel deployment

This production host runs its Cloudflare Tunnel as a separately managed systemd
service, rather than as the optional Docker tunnel profile. Deploy the application
containers with the checked-in helper; it loads the root-protected application
environment itself and never removes unrelated containers such as Hindsight.

```bash
sudo env POSTGRES_PORT=55432 ./scripts/deploy-production-app.sh \
  --env-file /etc/a-tiny-civilization-production.env \
  --genesis-directory "$QUALIFICATION_GENESIS_DIRECTORY" \
  --evidence-directory "$QUALIFICATION_EVIDENCE_DIRECTORY" \
  --admission-file "$QUALITY_WORLD_ADMISSION_FILE" \
  --observatory-admission-file "$PUBLIC_OBSERVATORY_ADMISSION_FILE" \
  --runtime-root "$QUALIFIED_RUNTIME_DIRECTORY" \
  --confirm-public-deployment
```

The helper first runs the complete composed public-genesis preflight against the exact qualified
genesis and evidence directories and the root-protected env file that Compose will consume. The
file must be an absolute, regular non-symlink owned by root (or the invoking operator for a manual
preflight), with no group or other permissions. Before Compose changes service state, the helper
revalidates the candidate and both admissions, retransverses the staged full-Earth composition,
rejects mutable or symlinked paths, rechecks both DE441 segments by byte length and SHA-256, and
reruns the complete host-portable read-only production operations suite: supporter isolation,
image/log/durability/shutdown/privilege/volume policies, deployment ordering, live
genesis verification structure, backend monitoring, systemd unit rendering, incident fields, and
the private Hindsight/local-model contract. Cargo dependency-tree architecture analysis remains an
immutable CI and source-admission gate rather than becoming a production-host toolchain dependency.
It then
builds the application and web images, starts only the private database, migrations, pinned local
model, and Hindsight foundation, and then requires that PostgreSQL already contain exactly the
running world and ruleset named by the admitted evidence. Only after that check does it start the
API, projector, runner, memory worker, and cognition worker with `APP_ENV=production`, then update
the web container
without allowing Compose defaults to recreate its API dependency. It waits for web and API
responses, Hindsight health, and fresh durable heartbeats from the runner, projector, memory
worker, and cognition worker. Before it checks the public edge, it also requires the exact world to
advance through tick one, drain initial Hindsight delivery without error, expose current
privacy-safe projections, and replay exactly from canonical history. It deliberately does **not** configure a tunnel or
off-site backups; those remain separate operational changes.
The literal confirmation argument is required even for root and has no environment-variable
equivalent, preventing a copied preflight command from becoming a deployment by implication.
Every production mutation helper also requires a clean Git checkout with no staged, unstaged, or
untracked files and reports its exact 40-character commit before proceeding. Reviewed source-tree
admissions and operational scripts therefore cannot be mixed with local uncommitted changes.
The helper also refuses to mutate Compose when any Compose-resolved web, database, or API loopback
port is held by a container outside the `a-tiny-civilization` production project. On the long-term
host this intentionally requires the legacy development stack to be stopped during the deliberate
cutover; its separate volume and development world are retained, not promoted or deleted.
The same guard rejects foreign running containers attached to any production data volume or the
shared `atiny-ollama` model volume. This prevents concurrent local-model writers during cutover.

For this first genesis, use host port 55432 consistently through every private and public production
command. This lets the private preparation helper start only `db` and `migrate` while the legacy
observatory remains live on 5432:

```bash
sudo env POSTGRES_PORT=55432 ./scripts/prepare-production-genesis-database.sh \
  --env-file /etc/a-tiny-civilization-production.env \
  --genesis-directory "$QUALIFICATION_GENESIS_DIRECTORY" \
  --evidence-directory "$QUALIFICATION_EVIDENCE_DIRECTORY" \
  --admission-file "$QUALITY_WORLD_ADMISSION_FILE" \
  --observatory-admission-file "$PUBLIC_OBSERVATORY_ADMISSION_FILE" \
  --runtime-root "$QUALIFIED_RUNTIME_DIRECTORY" \
  --confirm-private-database-preparation
```

It runs the same complete candidate/admission/runtime preflight before changing service state and
refuses to start any public or canonical process. Its narrow conflict guard checks only the
production PostgreSQL loopback port resolved from Compose and the protected production database
volume; web, API, and local cognition remain untouched. Assigning production a distinct
`POSTGRES_PORT` therefore allows this private preparation while the legacy observatory stays up.
The full deployment guard still requires every legacy listener and shared-model consumer to stop
before public cutover. Commit the admitted world while the database is still private:

```bash
sudo env POSTGRES_PORT=55432 ./scripts/activate-production-genesis.sh \
  --env-file /etc/a-tiny-civilization-production.env \
  --genesis-directory "$QUALIFICATION_GENESIS_DIRECTORY" \
  --evidence-directory "$QUALIFICATION_EVIDENCE_DIRECTORY" \
  --admission-file "$QUALITY_WORLD_ADMISSION_FILE" \
  --observatory-admission-file "$PUBLIC_OBSERVATORY_ADMISSION_FILE" \
  --runtime-root "$QUALIFIED_RUNTIME_DIRECTORY" \
  --confirm-experimental-genesis
```

This wrapper re-runs the full composed preflight, loads the protected literal environment through
the production parser, percent-encodes the loopback PostgreSQL URL in memory, and invokes the
unchanged qualified activation boundary. It commits only tick zero and starts no service. Record
its replay hashes, and only then invoke the deployment helper above. The helper now refuses an
empty database instead of publishing a pre-genesis shell, and it refuses a different world,
ruleset, terminal status, zero sequence, or multiple worlds before any canonical service or web
container starts. Private preparation and deployment use the same strict world-row validator;
empty state is accepted only in preparation mode, while deployment requires the one exact running
world with a positive committed sequence.

Before the first deployment, populate the external local-model volume. The provisioner uses a
pinned Ollama image, host networking only while downloading, a loopback-bound API, and verifies the
expected Qwen model digest. Stop any process already listening on loopback port 11434 first.

```bash
sudo ./scripts/provision-local-cognition.sh
```

Compose subsequently mounts that external volume into an unexposed `local-cognition` service on the
private backend network. Runtime cloud access is disabled for that service.

The deployment helper installs the same complete check as a periodic host monitor after every
successful public-edge verification. The installer writes a systemd drop-in bound to the exact
clean checkout and protected environment, so this host's `/home/shmuel/codex/emergent-civilization`
path does not inherit the portable unit template's `/opt/a-tiny-civilization` default. It enables
the two-minute timer and fails deployment unless systemd reports the timer both enabled and active.

To repair or reinstall it independently after an operating-system change:

```bash
sudo ./scripts/install-production-backend-monitor.sh \
  --env-file /etc/a-tiny-civilization-production.env \
  --confirm-production-monitor-install
sudo systemctl start a-tiny-civilization-backend-status.service
systemctl status a-tiny-civilization-backend-status.service
```

It fails when the project filesystem has less than 20 GiB free or is at least 95% full, when the web
edge, API, Hindsight, or exact pinned local model is unreachable, or when any required Rust-service
heartbeat is older than `BACKEND_HEARTBEAT_MAX_AGE_SECONDS` (60 seconds by default). The free-space
floor may be changed with bounded `BACKEND_MIN_FREE_MIB`; configure host alerting for the failed unit
before genesis. The installer now wires every backend-check failure to the checked-in, rate-limited
`a-tiny-civilization-operations-alert@.service`. Set
`ATINY_OPERATIONS_ALERT_WEBHOOK_URL` to an HTTPS receiver in the protected environment and,
optionally, `ATINY_OPERATIONS_ALERT_BEARER_TOKEN`. The notifier sends only the project, failed unit,
failure state, schema version, and wall-clock occurrence time; it never sends world events, journal
text, environment values, host inventory, or agent memory. Redirects and plaintext external URLs
are rejected, delivery is attempted three times, and the failed source unit remains visible in
systemd whether or not a receiver is configured.

The installer also enables `a-tiny-civilization-disk-guard.timer` every five minutes. At less than
23 GiB free or 90% used, its bounded cleanup first prunes unused Docker build cache older than seven
days, then clears only the reproducible Rust `target/debug` tree when no Cargo or Rust compiler is
active, and finally prunes remaining unused Docker build cache only if the 20 GiB reserve is still
not restored. It never deletes PostgreSQL volumes, world history, runtime artifacts, retained
scientific sources, derived scientific data, release binaries, or repository files. If a build is
active or those safe caches cannot restore the reserve, the guard fails visibly through the same
operations-alert unit rather than expanding its deletion scope. `DISK_GUARD_TRIGGER_FREE_MIB`,
`DISK_GUARD_REQUIRED_FREE_MIB`, and `DISK_GUARD_MAX_USED_PERCENT` provide bounded host overrides.

The serving runner holds a PostgreSQL session advisory lock for its lifetime. A second
runner refuses startup instead of racing the canonical cursor; process or connection
loss releases the lock automatically so the configured restart policy can recover.

Every checked-in Compose service uses Docker's local `json-file` driver with a 10 MiB file and
five-file rotation limit. CI composes every supported profile and rejects any service that omits
that bound. Canonical PostgreSQL history remains durable in its named volume; container logs are
operational diagnostics and are intentionally bounded so they cannot silently consume the history
disk.

A newly created PostgreSQL volume is initialized with page checksums. The complete backend health
gate also reads the live server settings and refuses readiness unless `data_checksums`, `fsync`,
`synchronous_commit`, and `full_page_writes` are all `on`. The initialization flag cannot repair an
older volume: if a pre-genesis development volume fails this check, create a fresh production
volume through an explicitly scoped operator procedure rather than weakening the gate. Never
replace a volume after canonical tick zero.

Docker stops services with `SIGTERM`. The API, runner, projector, memory worker, and cognition
worker all install an explicit termination handler, and the canonical writers plus PostgreSQL have
a 60-second shutdown grace period. An in-flight database transaction is therefore allowed to
commit or roll back cleanly before the container runtime may force termination. CI verifies both
the signal handlers and the composed grace periods.

Every first-party runtime process uses an unprivileged image account, drops all Linux capabilities,
forbids privilege escalation, and runs with a read-only root filesystem. A bounded, non-executable
`/tmp` tmpfs is the only general scratch space. The web runtime directs Wrangler/Miniflare scratch
state there; the runner's staged Earth inputs remain a separate read-only mount. CI verifies the
fully composed service policy rather than trusting Dockerfile intent alone.
The `Production images` CI job additionally builds both final images, inspects their configured
runtime users, executes the Rust image under the production restrictions, and boots plus HTTP-probes
the web image on a random loopback port with a read-only root filesystem.

After a deployment passes the private backend and observer smoke gates, the deploy helper also
checks the real `atinycivilization.com` edge. HTTP must preserve paths and query strings while
redirecting to the one canonical HTTPS origin. The homepage, wiki, every public policy route, and
observer status must return 200 with the complete security/no-store header contract and their
admitted content markers; the returned status must parse as JSON. A healthy loopback origin behind
a broken tunnel, missing policy route, or weakened edge policy is not reported as a successful
deployment.

The Compose project name and canonical PostgreSQL/Hindsight volume names are explicit and do not
depend on the checkout directory. Those volumes are external to Compose, so `docker compose down
-v` cannot delete them. `make up`, `make hindsight-up`, and the production deploy helper create
missing volumes idempotently and require project ownership/schema labels before reuse. Provisioning
never deletes or replaces a volume; an unexpected pre-existing volume fails closed for operator
review.

### Docker tunnel and backup profile

From the repository checkout, load the protected environment file into the current
shell without echoing it, then run:

```bash
./scripts/production-preflight.sh
docker compose -f compose.yaml -f compose.backup.yaml -f compose.tunnel.yaml pull --ignore-buildable
docker compose -f compose.yaml -f compose.backup.yaml -f compose.tunnel.yaml up --build -d
make smoke
```

For a provisional full-Earth integration world, stage a service-readable copy of
exactly its pinned inputs before starting the runner. The original retained `data/`
tree stays private; the staging tool verifies every source file's length and SHA-256,
never replaces a staging directory, and produces files readable only by root and the
runner service group.

```bash
sudo ./scripts/stage-provisional-runner-artifacts.sh ./runtime-artifacts
```

The staging command traverses all six unique global tile trees, rather than copying only their root
indexes, and re-verifies the complete staged closure. For an explicit pre-genesis source audit run:

```bash
ATINY_VERIFY_FULL_PROVISIONAL_CLOSURE=1 \
ATINY_CIVILIZATION_DATA_EXECUTABLE=target/release/civilization-data \
  ./scripts/verify-provisional-genesis-pins.sh
```

The runner mounts the exact directory carried through `--runtime-root` read-only at `/runtime`;
the production wrappers export that validated absolute path for Compose rather than silently
falling back to a different staging tree. The command is deliberately
for the provisional integration path only; it does not authorize canonical genesis.

### Prepare and initialize the canonical provisional world

After publishing and resolving the future-beacon commitment in
[Public canonical seed procedure](PUBLIC_SEED.md), build the two
one-time operator binaries and derive the entire seed-bound chain into a new directory.
The preparation script resolves the selected L10 centre itself, queries the pinned
iNaturalist range release, and refuses to replace any output.

```bash
cargo build --release --locked -p civilization-data -p civilization-runner
read -r WORLD_ID WORLD_SEED < <(
  target/release/civilization-data seed verify \
    --commitment docs/operations/CANONICAL_SEED_COMMITMENT.json \
    --resolution docs/operations/CANONICAL_SEED_RESOLUTION.json
)
./scripts/acquire-inaturalist-origin-observations.py \
  --latitude-e7 236449522 \
  --longitude-e7 -1034974258 \
  --radius-kilometers 75 \
  --output-directory "/var/lib/a-tiny-civilization/sources/$WORLD_ID-local-fauna"
ATINY_LOCAL_OCCURRENCE_SOURCE_DIRECTORY="/var/lib/a-tiny-civilization/sources/$WORLD_ID-local-fauna" \
  ./scripts/prepare-canonical-genesis.sh \
  docs/operations/CANONICAL_SEED_COMMITMENT.json \
  docs/operations/CANONICAL_SEED_RESOLUTION.json \
  "/var/lib/a-tiny-civilization/genesis/$WORLD_ID" 32
```

Do not initialize production from freshly prepared inputs. First run the isolated qualification
world through the required bounded evidence path below and create its launch-evidence bundle. With
`DATABASE_URL` then loaded from the root-protected production environment, the qualified activation
wrapper reruns every offline evidence check before appending the tick-zero founders and reservoirs.
It refuses a database containing any different world, so use a fresh production database rather
than the development/proof database. The underlying initializer applies the same idempotent
embedded migration set used by the deployment migration service, allowing a genuinely empty
canonical database without weakening the exclusive-world check:

Before creating that isolated database, the same complete bundle can be checked through the exact
genesis constructor with no persistence side effect:

```bash
target/release/civilization-runner verify-provisional-genesis \
  --world-id "$WORLD_ID" \
  --seed "$WORLD_SEED" \
  --genesis-directory "/var/lib/a-tiny-civilization/genesis/$WORLD_ID" \
  --ruleset-version 32
```

This verifies the portable input manifest, every full-Earth composition artifact, genesis
construction, event-zero replay, and snapshot replay. It is an early fail-fast proof, not a
substitute for database-backed bounded qualification or launch admission.

```bash
./scripts/activate-qualified-canonical-world.sh activate \
  docs/operations/CANONICAL_SEED_COMMITMENT.json \
  docs/operations/CANONICAL_SEED_RESOLUTION.json \
  "/var/lib/a-tiny-civilization/genesis/$WORLD_ID" \
  "$QUALIFICATION_EVIDENCE_DIRECTORY" \
  docs/operations/QUALITY_WORLD_ADMISSION_RULESET32_2026-08-09.json \
  --confirm-experimental-genesis
```

The second command is retry-safe only for byte-identical inputs. Do not enable or
restart the long-running runner until the printed replay hashes and the public seed
commitment have been recorded in the launch evidence.

The deployment helper runs the live-genesis verifier automatically. It remains available as a
read-only operator check after restarts or incident recovery: it waits for tick 1, drains the
initial Hindsight outbox, requires exact current privacy-safe projections, replays the live world,
and reruns complete backend health:

```bash
sudo ./scripts/verify-live-genesis.sh \
  --env-file /etc/a-tiny-civilization-production.env \
  --world-id "$WORLD_ID"
```

If it fails, repair or restart the failed service and rerun the verifier. Never reinitialize or
replace the committed tick-zero world.

For disposable pre-genesis evidence only, an exact bounded runner path avoids relying on process
signal timing. It uses the real writer lock, snapshot resume, cognition scheduling, and DE441
driver, but refuses `APP_ENV=production` and cannot change a public world's pace:

```bash
# Start the local memory and cognition workers first. This wrapper advances one
# tick, waits for a strictly free model result without advancing simulation time, then
# completes the exact bounded run.
./scripts/advance-cognition-qualified-world.sh "$QUALIFICATION_WORLD_ID" 1000
# After the bounded advance, a finite isolated-database drain avoids waiting on
# the continuous service's deliberately conservative idle poll cadence.
civilization-runner memory-worker \
  --hindsight-base-url http://127.0.0.1:8888 --drain
civilization-runner verify-world --world-id "$QUALIFICATION_WORLD_ID"
civilization-projector once --world-id "$QUALIFICATION_WORLD_ID"
./scripts/qualification-status.sh "$QUALIFICATION_WORLD_ID" > qualification-status.json
./scripts/create-qualification-evidence.sh \
  "$QUALIFICATION_WORLD_ID" "$QUALIFICATION_GENESIS_DIRECTORY" \
  "$QUALIFICATION_EVIDENCE_DIRECTORY"
./scripts/observer-candidate-smoke.sh \
  http://127.0.0.1:8080 "$QUALIFICATION_WORLD_ID" 1018
```

Use a separate disposable database and a non-public world ID. The command accepts at most one
million ticks per invocation and stops early with an error if the world reaches a terminal state.
The cognition-qualified wrapper requires the exact tick-zero cursor, refuses resume from any other
cursor, and fails rather than crossing the first fixed deadline without a durable model receipt.
It does not start workers or alter the simulation clock while waiting. The receipt may come from
the loopback model or a separately approved external free-allocation route, but its recorded billed
cost must be exactly zero; the wrapper never enables the paid tail.
`memory-worker --drain` forces a one-millisecond work cadence, exits successfully only after no
ready outbox entry remains, and treats a delivery or store failure as fatal. It is for an isolated
qualification database after bounded advancement; the normal production worker remains continuous
and retains its configured idle poll interval. The status command is read-only apart from the
runner's replay verification reads. It exits nonzero
unless canonical replay, snapshots, projections, memory delivery, cognition deadlines, one actual
Hindsight-backed cognition result, and observer content all pass; its single JSON object is suitable
for the retained launch-evidence bundle.
The evidence command requires a clean committed worktree, verifies the original genesis checksums,
reruns the qualification report, and atomically creates a checksum-covered directory containing no
canonical event payloads. It never replaces prior evidence.
The observer smoke command independently requires a running disclosed world, zero lag across all
five projections, nonempty timeline/finding/organism/artifact/wiki views, hash-only audit
commitments, and no private or explicit mechanism vocabulary in the public payloads. Supply the
actual expected sequence for each retained candidate rather than copying the example cursor.
Before accepting a retained bundle into launch review, independently bind all of its offline claims:

```bash
./scripts/verify-launch-candidate-evidence.py \
  --world-id "$QUALIFICATION_WORLD_ID" \
  --genesis-directory "$QUALIFICATION_GENESIS_DIRECTORY" \
  --evidence-directory "$QUALIFICATION_EVIDENCE_DIRECTORY" \
  --expected-ruleset 32 --minimum-tick 1000
```

This verifies every bundle checksum, the external genesis-manifest binding, absence declaration for
canonical payloads, exact world/ruleset identity, replay and qualification status, minimum history,
all five projections, complete Hindsight delivery, person-only model receipts, every boolean gate,
and that the recorded source commit is an ancestor of the checked-out code.

Use the qualified wrapper—not the low-level initializer—for a deliberate production genesis. Its
read-only mode can be repeated without a database:

```bash
./scripts/activate-qualified-canonical-world.sh verify \
  docs/operations/CANONICAL_SEED_COMMITMENT.json \
  docs/operations/CANONICAL_SEED_RESOLUTION.json \
  "$QUALIFICATION_GENESIS_DIRECTORY" "$QUALIFICATION_EVIDENCE_DIRECTORY" \
  docs/operations/QUALITY_WORLD_ADMISSION_RULESET32_2026-08-09.json
```

Both modes also verify the exact experimental quality-world admission and its binding to the two
checksum manifests. The write mode requires `DATABASE_URL`, reruns every offline check, re-verifies
the public seed, and accepts the literal final argument `--confirm-experimental-genesis` before
invoking the exclusive canonical initializer. It commits tick zero only; it does not deploy
containers or expose a site. This keeps quality admission, world creation, and public deployment as
three separate operations.

For the standard private Compose Qwen2.5 1.5B service, set
`LOCAL_COGNITION_BASE_URL=http://local-cognition:11434/v1`. A host-run worker may use a loopback URL.
The runner rejects every other host and HTTP redirects; this route needs no external-export approval
because private context never leaves the host. See ADR 0103.

Provider authentication and response compatibility can be tested separately without a database or
world-data export. This sends only the fixed synthetic request specified by ADR 0088 and still
requires the provider key in the protected process environment:

```bash
civilization-runner probe-openrouter-free
```

A successful synthetic probe does not authorize live cognition export; the long-running cognition
worker still requires `COGNITION_EXTERNAL_EXPORT_APPROVED=true` whenever any provider is configured.

The static preflight rejects missing core settings, an absent cognition route, remote providers
without separate external-export approval, the
documented development password, a non-production environment, partial paid, backup,
or tunnel configuration, mutable `cloudflared` image references, and invalid Compose
interpolation. It does not print secrets. The default tunnel image is pinned to the
multi-architecture digest for Cloudflare's 2026.7.2 release; upgrades are deliberate
deployment checkpoints.

Verify locally that `http://127.0.0.1:3000/` and `http://127.0.0.1:8080/health/ready`
work, then verify only the intended hostname through Cloudflare. Confirm that direct
public connections to PostgreSQL and the observer API fail.

`backend-status.sh` defaults to at most 100 sequences of observer-projection lag and five minutes
for incomplete memory delivery or a stuck cognition dispatch. Override those only with bounded
`BACKEND_PROJECTION_MAX_LAG_SEQUENCES` and `BACKEND_ASYNC_MAX_AGE_SECONDS` values in the monitor
environment; changing them does not alter canonical history.

The deployment helper additionally resolves the committed cursor for the required running world and runs
the full observer-candidate smoke gate through the API's loopback binding. A deploy fails if more
than one world is running, the API is not bound only to IPv4 loopback, a projection is behind or
empty, private/explicit mechanism data appears publicly, or the audit endpoint exposes event
payloads rather than commitments. A pre-genesis deployment is rejected; the world-specific smoke
gate is mandatory rather than conditional.

Immediately before the first public production deployment, stop the superseded proof stack with
the checked helper instead of manually naming containers:

```bash
sudo env POSTGRES_PORT=55432 ./scripts/stop-legacy-public-stack-for-cutover.sh --check
sudo env POSTGRES_PORT=55432 ./scripts/stop-legacy-public-stack-for-cutover.sh \
  --confirm-legacy-public-cutover
```

The helper discovers only the `emergent-civilization` Compose project, validates each service and
its checkout working-directory label, allows only the already-prepared private production database
and migration sentinel, refuses any already-running production runner, worker, cognition, API, or
web service, and prints the exact ordered set without mutation in `--check` mode. The confirmed mode
stops canonical writers before public and persistence services, then reruns the production
port/volume preflight. That preflight resolves the database, API, and web loopback ports from the
protected Compose configuration; alternate preparation ports are never checked against hardcoded
defaults. It never removes a container, network, image, or volume. Its printed exact
container identities are the incident rollback set if deployment fails before production becomes
public; do not start them while a production container owns the same ports or protected volumes.

## Deferred offsite backup checks

After the first successful encrypted base backup, install the checked-in systemd units
on the host. They assume the repository is deployed at `/opt/a-tiny-civilization` and
the root-readable secret environment is `/etc/a-tiny-civilization-production.env`;
change both paths in the copied unit files if the deployment uses another location.

```bash
sudo install -d -m 0755 /etc/systemd/system
sudo install -m 0644 ops/systemd/a-tiny-civilization-backup.service /etc/systemd/system/
sudo install -m 0644 ops/systemd/a-tiny-civilization-backup.timer /etc/systemd/system/
sudo install -m 0644 ops/systemd/a-tiny-civilization-backup-status.service /etc/systemd/system/
sudo install -m 0644 ops/systemd/a-tiny-civilization-backup-status.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now a-tiny-civilization-backup.timer a-tiny-civilization-backup-status.timer
systemctl list-timers 'a-tiny-civilization-*'
```

The base backup runs daily with a bounded random delay; freshness and WAL archival are
checked hourly. A failed unit is intentionally visible in `systemctl` and the journal
rather than being silently retried as if the backup were current. Both services route failure to
the same rate-limited operations-alert unit installed with the backend monitor; configure its HTTPS
receiver before relying on unattended backups.

## Required gates before public genesis

Run the composed read-only gate as root before either canonical activation or deployment. It reads
the protected environment, verifies the exact candidate, experimental quality admission, and
source-bound public-observatory admission, then streams the staged runtime inputs without creating
a world or changing a service:

```bash
sudo ./scripts/public-genesis-preflight.sh \
  --env-file /etc/a-tiny-civilization-production.env \
  --genesis-directory "$QUALIFICATION_GENESIS_DIRECTORY" \
  --evidence-directory "$QUALIFICATION_EVIDENCE_DIRECTORY" \
  --admission-file "$QUALITY_WORLD_ADMISSION_FILE" \
  --observatory-admission-file "$PUBLIC_OBSERVATORY_ADMISSION_FILE" \
  --runtime-root "$QUALIFIED_RUNTIME_DIRECTORY"
```

- a complete, hash-pinned provisional full-Earth artifact set and unpreviewed public
  seed procedure, with assumptions disclosed for later scientific review;
- a ruleset-32 genesis chain produced by `prepare-canonical-genesis.sh` from the
  independently verified public seed resolution and required range-plus-local-occurrence fauna
  evidence plus the complete point-scoped 1981–2010 ERA5 evidence and its deterministic fixed-point
  monthly summaries, embedded as the schema-5 local weather input and exposed only as
  label-free temperature, water-flux, and air-motion sensations, verified by `SHA256SUMS`,
  independently rederived during initialization, admitted by the offline qualification-evidence
  verifier, atomically initialized by `activate-qualified-canonical-world.sh`, and replayed before
  the long-running runner is enabled;
- the exact ruleset-32 genesis and qualification manifest digests admitted by
  `QUALITY_WORLD_ADMISSION_RULESET32_2026-08-09.json`; that experimental quality admission does not
  by itself authorize deployment or claim scientific admission;
- the exact reviewed web and policy trees admitted by
  `PUBLIC_OBSERVATORY_ADMISSION_RULESET32_2026-08-09.json`; that review also cannot authorize deployment;
- accelerated replay, restart, cognition-deadline, provider-failure,
  partition-equivalence, reproduction, and load evidence;
- local PostgreSQL durability and restart/replay evidence. The offsite restore drill is
  deferred for this genesis by explicit owner decision;
- public privacy, moderation, supporter refund/transfer, and presentation policies;
- Hindsight plus at least one configured free-first cognition provider, with paid
  fallback disabled unless deliberately enabled; enabling a remote provider also requires explicit
  approval to export private cognition requests and recalled-memory context to that provider;
- production credentials for features actually enabled at launch, with least privilege
  and rotation procedures.

Until those gates pass, this runbook can operate only an internal or proof observatory.

## Moderation monitoring before enabling payments

Stripe-enabled preflight requires `ATINY_MODERATOR_ID`, using a stable operator identifier made only
from letters, digits, `.`, `_`, `:`, `@`, `/`, or `-`. It is audit identity, not a credential. Set
`MODERATION_MAX_AGE_MINUTES` if the default 60-minute review threshold is unsuitable, then install
and start the checked-in queue timer:

```bash
sudo install -m 0644 ops/systemd/a-tiny-civilization-moderation-status.service /etc/systemd/system/
sudo install -m 0644 ops/systemd/a-tiny-civilization-moderation-status.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now a-tiny-civilization-moderation-status.timer
sudo systemctl start a-tiny-civilization-moderation-status.service
systemctl status a-tiny-civilization-moderation-status.service
```

The service uses `/etc/a-tiny-civilization-production.env` and assumes the repository is deployed at
`/opt/a-tiny-civilization`, matching the current host deployment helper. Change both paths in the
copied unit if the host differs. Configure host alerting for a failed service before accepting a
purchase; this service is already wired to the shared rate-limited operations-alert unit. Failure
means the API/container is unavailable or at least one paid label exceeded the review threshold.
