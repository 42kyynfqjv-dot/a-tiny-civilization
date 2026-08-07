# Production runbook

This is a one-server deployment runbook for a small population. It is intentionally
not a launch authorization: although the engine library can replay a configured
full-Earth foundation, the runner exposes no canonical initializer until the scientific
bundle and first real scheduled causal process are admitted.

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
- `CLOUDFLARE_TUNNEL_TOKEN` for a dedicated remotely managed production tunnel;
- `R2_ACCOUNT_ID`, `R2_BACKUP_BUCKET`, and a bucket-scoped R2 access-key pair;
- a persistent 32-byte `WALG_LIBSODIUM_KEY` encoded as hexadecimal;
- `APP_ENV=production`.

Optional later integrations remain blank until their features are enabled: Stripe,
Google OAuth, Apple Sign in, Hindsight, and an LLM provider. No current simulation
path needs an LLM key.

Create a separate staging tunnel and production tunnel. In Cloudflare's dashboard,
create the production tunnel and public hostname, then set its service to
`http://web:3000`. A tunnel token is equivalent to permission to run that tunnel, so
rotate it if exposed and replace it only in the server's environment file. Cloudflare
documents remotely managed Docker tunnels and token rotation in its
[Tunnel documentation](https://developers.cloudflare.com/tunnel/setup/) and
[token guidance](https://developers.cloudflare.com/tunnel/advanced/tunnel-tokens/).

## Preflight and deployment

From the repository checkout, load the protected environment file into the current
shell without echoing it, then run:

```bash
./scripts/production-preflight.sh
docker compose -f compose.yaml -f compose.backup.yaml -f compose.tunnel.yaml pull --ignore-buildable
docker compose -f compose.yaml -f compose.backup.yaml -f compose.tunnel.yaml up --build -d
make smoke
```

The static preflight rejects missing settings, the documented development password, a
non-production environment, mutable `cloudflared` image references, and invalid Compose
interpolation. It does not print secrets. The default tunnel image is pinned to the
multi-architecture digest for Cloudflare's 2026.7.2 release; upgrades are deliberate
deployment checkpoints.

Verify locally that `http://127.0.0.1:3000/` and `http://127.0.0.1:8080/health/ready`
work, then verify only the intended hostname through Cloudflare. Confirm that direct
public connections to PostgreSQL and the observer API fail.

## Scheduled backup checks

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

## Required gates before public canonical-world launch

- a verified canonical full-Earth scientific bundle and unpreviewed seed procedure;
- replay, restart, partition-equivalence, and multi-year disposable-world evidence;
- a restore drill recorded under [Backup and restore](BACKUP_RESTORE.md);
- public privacy, moderation, supporter refund/transfer, and presentation policies;
- production credentials created in their respective owner accounts, with least
  privilege and rotation procedures.

Until those gates pass, this runbook can operate only an internal or proof observatory.
