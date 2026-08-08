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
sudo ./scripts/deploy-production-app.sh \
  --env-file /etc/a-tiny-civilization-production.env
```

The helper first runs the complete production preflight against the exact root-protected env file
that Compose will consume. The file must be an absolute, regular non-symlink owned by root (or the
invoking operator for a manual preflight), with no group or other permissions. The helper then builds
the application and web images, starts the database, migrations, pinned local model,
API, projector, and runner with `APP_ENV=production`, then updates the web container
without allowing Compose defaults to recreate its API dependency. It waits for web and API
responses, Hindsight health, and fresh durable heartbeats from the runner, projector, memory
worker, and cognition worker. It deliberately does **not** configure a tunnel or
off-site backups; those remain separate operational changes.

Before the first deployment, populate the external local-model volume. The provisioner uses a
pinned Ollama image, host networking only while downloading, a loopback-bound API, and verifies the
expected Qwen model digest. Stop any process already listening on loopback port 11434 first.

```bash
sudo ./scripts/provision-local-cognition.sh
```

Compose subsequently mounts that external volume into an unexposed `local-cognition` service on the
private backend network. Runtime cloud access is disabled for that service.

Install the same complete check as a periodic host monitor after deployment:

```bash
sudo install -m 0644 ops/systemd/a-tiny-civilization-backend-status.service /etc/systemd/system/
sudo install -m 0644 ops/systemd/a-tiny-civilization-backend-status.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now a-tiny-civilization-backend-status.timer
sudo systemctl start a-tiny-civilization-backend-status.service
systemctl status a-tiny-civilization-backend-status.service
```

It fails when the web edge, API, Hindsight, or exact pinned local model is unreachable, or when any
required Rust-service heartbeat is older than `BACKEND_HEARTBEAT_MAX_AGE_SECONDS` (60 seconds by default). Configure
host alerting for the failed unit before genesis.

The serving runner holds a PostgreSQL session advisory lock for its lifetime. A second
runner refuses startup instead of racing the canonical cursor; process or connection
loss releases the lock automatically so the configured restart policy can recover.

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

The runner mounts that directory read-only at `/runtime`. The command is deliberately
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
./scripts/prepare-canonical-genesis.sh \
  docs/operations/CANONICAL_SEED_COMMITMENT.json \
  docs/operations/CANONICAL_SEED_RESOLUTION.json \
  "/var/lib/a-tiny-civilization/genesis/$WORLD_ID" 32
```

With `DATABASE_URL` loaded from the root-protected production environment, initialize
all founders and material reservoirs in one append and immediately replay-verify it:

```bash
./scripts/initialize-canonical-world.sh \
  docs/operations/CANONICAL_SEED_COMMITMENT.json \
  docs/operations/CANONICAL_SEED_RESOLUTION.json \
  "/var/lib/a-tiny-civilization/genesis/$WORLD_ID"
```

The second command is retry-safe only for byte-identical inputs. Do not enable or
restart the long-running runner until the printed replay hashes and the public seed
commitment have been recorded in the launch evidence.

For disposable pre-genesis evidence only, an exact bounded runner path avoids relying on process
signal timing. It uses the real writer lock, snapshot resume, cognition scheduling, and DE441
driver, but refuses `APP_ENV=production` and cannot change a public world's pace:

```bash
APP_ENV=development civilization-runner advance-qualification \
  --world-id "$QUALIFICATION_WORLD_ID" --ticks 1000
civilization-runner verify-world --world-id "$QUALIFICATION_WORLD_ID"
civilization-projector once --world-id "$QUALIFICATION_WORLD_ID"
./scripts/qualification-status.sh "$QUALIFICATION_WORLD_ID" > qualification-status.json
./scripts/create-qualification-evidence.sh \
  "$QUALIFICATION_WORLD_ID" "$QUALIFICATION_GENESIS_DIRECTORY" \
  "$QUALIFICATION_EVIDENCE_DIRECTORY"
```

Use a separate disposable database and a non-public world ID. The command accepts at most one
million ticks per invocation and stops early with an error if the world reaches a terminal state.
The status command is read-only apart from the runner's replay verification reads. It exits nonzero
unless canonical replay, snapshots, projections, memory delivery, cognition deadlines, one actual
Hindsight-backed cognition result, and observer content all pass; its single JSON object is suitable
for the retained launch-evidence bundle.
The evidence command requires a clean committed worktree, verifies the original genesis checksums,
reruns the qualification report, and atomically creates a checksum-covered directory containing no
canonical event payloads. It never replaces prior evidence.

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

## Deferred offsite backup checks

After the first successful encrypted base backup, install the checked-in systemd units
on the host. They assume the repository is deployed at `/opt/a-tiny-civilization` and
the root-readable secret environment is `/etc/a-tiny-civilization/production.env`;
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
rather than being silently retried as if the backup were current. Configure host-level
alerting to notify an operator for either failed service.

## Required gates before public genesis

- a complete, hash-pinned provisional full-Earth artifact set and unpreviewed public
  seed procedure, with assumptions disclosed for later scientific review;
- a ruleset-25 genesis chain produced by `prepare-canonical-genesis.sh` from the
  independently verified public seed resolution, verified by `SHA256SUMS`, atomically
  initialized by `initialize-canonical-world.sh`, and replayed before the long-running
  runner is enabled;
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
purchase; failure means the API/container is unavailable or at least one paid label exceeded the
review threshold.
