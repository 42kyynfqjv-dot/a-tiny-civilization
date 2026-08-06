# Production runbook

This is a one-server deployment runbook for a small population. It is intentionally
not a launch authorization: the runner continues to reject canonical full-Earth genesis
until the scientific bundle and embodied execution milestones are complete.

## Network boundary

`compose.yaml` publishes the web and observer API only to loopback. The optional
`compose.tunnel.yaml` joins `cloudflared` only to the `edge` Docker network, which can
reach `web`. It cannot reach PostgreSQL, the runner, projector, or migrations because
they are on the separate `backend` network. Configure the remotely managed Cloudflare
Tunnel public hostname to use `http://web:3000`; do not route to `api`, `db`, or a host
port.

Keep the host firewall closed to public inbound HTTP and database traffic. Use SSH with
key-based access for the host and Cloudflare Access for any administrative route that
is later added. The observatory itself remains read-only.

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
docker compose -f compose.yaml -f compose.tunnel.yaml pull
docker compose -f compose.yaml -f compose.tunnel.yaml up --build -d
make smoke
```

The static preflight rejects missing settings, the documented development password, a
non-production environment, and invalid Compose interpolation. It does not print
secrets. Before the first use, record the resolved `cloudflared` image digest in the
deployment change log; do not confuse a mutable image tag with a provenance pin.

Verify locally that `http://127.0.0.1:3000/` and `http://127.0.0.1:8080/health/ready`
work, then verify only the intended hostname through Cloudflare. Confirm that direct
public connections to PostgreSQL and the observer API fail.

## Required gates before public canonical-world launch

- a verified canonical full-Earth scientific bundle and unpreviewed seed procedure;
- replay, restart, partition-equivalence, and multi-year disposable-world evidence;
- a restore drill recorded under [Backup and restore](BACKUP_RESTORE.md);
- public privacy, moderation, supporter refund/transfer, and presentation policies;
- production credentials created in their respective owner accounts, with least
  privilege and rotation procedures.

Until those gates pass, this runbook can operate only an internal or proof observatory.
