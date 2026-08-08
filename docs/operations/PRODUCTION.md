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
- at least one cognition-provider key: Cloudflare Workers AI, Groq, Cerebras, or
  OpenRouter. Free routes are attempted first and paid cognition is disabled unless
  `COGNITION_PAID_ENABLED=true` is explicitly set;
- `APP_ENV=production`.

The local Hindsight service is keyless inside its private Docker network. Stripe,
Google OAuth, and Apple Sign in remain blank until those observer products are
enabled. This host currently runs Cloudflare Tunnel as a separate system service, so
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

The helper builds the application and web images, starts the database, migrations,
API, projector, and runner with `APP_ENV=production`, then updates the web container
without allowing Compose defaults to recreate its API dependency. It waits for the
observer API readiness check. It deliberately does **not** configure a tunnel or
off-site backups; those remain separate operational changes.

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

The runner mounts that directory read-only at `/runtime`. The command is deliberately
for the provisional integration path only; it does not authorize canonical genesis.

### Prepare and initialize the canonical provisional world

After publishing the seed by the committed, unpreviewed procedure, build the two
one-time operator binaries and derive the entire seed-bound chain into a new directory.
The preparation script resolves the selected L10 centre itself, queries the pinned
iNaturalist range release, and refuses to replace any output.

```bash
cargo build --release --locked -p civilization-data -p civilization-runner
./scripts/prepare-provisional-genesis.sh "$WORLD_SEED" \
  "/var/lib/a-tiny-civilization/genesis/$WORLD_ID" 32
```

With `DATABASE_URL` loaded from the root-protected production environment, initialize
all founders and material reservoirs in one append and immediately replay-verify it:

```bash
./scripts/initialize-provisional-world.sh \
  "$WORLD_ID" "$WORLD_SEED" "/var/lib/a-tiny-civilization/genesis/$WORLD_ID"
```

The second command is retry-safe only for byte-identical inputs. Do not enable or
restart the long-running runner until the printed replay hashes and the public seed
commitment have been recorded in the launch evidence.

The static preflight rejects missing core settings, an absent cognition provider, the
documented development password, a non-production environment, partial paid, backup,
or tunnel configuration, mutable `cloudflared` image references, and invalid Compose
interpolation. It does not print secrets. The default tunnel image is pinned to the
multi-architecture digest for Cloudflare's 2026.7.2 release; upgrades are deliberate
deployment checkpoints.

Verify locally that `http://127.0.0.1:3000/` and `http://127.0.0.1:8080/health/ready`
work, then verify only the intended hostname through Cloudflare. Confirm that direct
public connections to PostgreSQL and the observer API fail.

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
- a ruleset-18 genesis chain produced by `prepare-provisional-genesis.sh`, verified by
  `SHA256SUMS`, atomically initialized by `initialize-provisional-world.sh`, and replayed
  before the long-running runner is enabled;
- accelerated replay, restart, cognition-deadline, provider-failure,
  partition-equivalence, reproduction, and load evidence;
- local PostgreSQL durability and restart/replay evidence. The offsite restore drill is
  deferred for this genesis by explicit owner decision;
- public privacy, moderation, supporter refund/transfer, and presentation policies;
- Hindsight plus at least one configured free-first cognition provider, with paid
  fallback disabled unless deliberately enabled;
- production credentials for features actually enabled at launch, with least privilege
  and rotation procedures.

Until those gates pass, this runbook can operate only an internal or proof observatory.
