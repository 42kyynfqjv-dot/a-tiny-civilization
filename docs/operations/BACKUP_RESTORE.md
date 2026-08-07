# Backup and restore policy

An archived world is an immutable historical promise, so a single server disk is not a
backup. Production uses WAL-G 3.0.8 for continuous PostgreSQL WAL archiving and base
backups to a private, owner-controlled Cloudflare R2 bucket. The container build pins
the upstream binary checksum. Backup objects are encrypted client-side with a dedicated
libsodium key before upload; R2 also encrypts all objects at rest.

## Minimum operating posture

- Keep the R2 bucket private and issue an Object Read & Write token scoped only to that
  bucket. The application uses only the `a-tiny-civilization/postgres` prefix.
- Keep `WALG_LIBSODIUM_KEY` in a second secure location independent of the host and R2
  account. Losing it makes every backup unrecoverable; exposing it together with the R2
  credential exposes the backups.
- Archive WAL continuously; take daily base backups and retain a documented recovery
  window.
- Keep a separate, periodic exported verification bundle for every archived world.
- Monitor failed archive uploads, missing WAL segments, backup age, disk space, and
  PostgreSQL replication/archive status.
- Test restoration into an isolated PostgreSQL instance at least quarterly and after
  any backup-tool or database-major-version change.

Cloudflare's S3-compatible endpoint is
`https://<ACCOUNT_ID>.r2.cloudflarestorage.com`, with region `auto`. Create a dedicated
private bucket and a bucket-scoped Object Read & Write token as described in Cloudflare's
[R2 S3 setup](https://developers.cloudflare.com/r2/get-started/s3/). R2's documented
[data security](https://developers.cloudflare.com/r2/reference/data-security/) includes
automatic AES-256 encryption at rest and TLS in transit; client-side encryption remains
required here so possession of an R2 credential alone is insufficient to read history.

## Production configuration

The protected production environment must define:

- `R2_ACCOUNT_ID` and `R2_BACKUP_BUCKET`;
- the bucket-scoped `R2_ACCESS_KEY_ID` and `R2_SECRET_ACCESS_KEY`;
- `WALG_LIBSODIUM_KEY`, generated once as 32 random bytes encoded in hexadecimal;
- the normal PostgreSQL and Cloudflare Tunnel settings.

Generate the client-side encryption key directly into the protected secret store. Do
not regenerate it during redeployments. The key format accepted by preflight is the
output of `openssl rand -hex 32`.

Production must include the backup overlay:

```bash
./scripts/production-preflight.sh
docker compose \
  -f compose.yaml \
  -f compose.backup.yaml \
  -f compose.tunnel.yaml \
  up --build -d
```

The overlay enables `archive_mode`, sends completed WAL segments with `wal-g wal-push`,
and forces a segment switch at least every 60 seconds on a quiet database. Check
`pg_stat_archiver` and container logs after startup; a failed archive command causes
PostgreSQL to retain the WAL locally rather than silently discard it.

## Base backup

Run the checked-in wrapper from the protected production environment:

```bash
./scripts/backup-postgres.sh
```

It runs `wal-g backup-push` as the PostgreSQL user and then prints the remote backup
inventory. Schedule it daily with the host's service manager. Do not add automatic
retention deletion until the recovery window and legal/operational retention policy
are explicitly recorded; WAL-G deletion is deliberately absent from this repository.

Monitor the remote base-backup age and PostgreSQL archiver state with:

```bash
./scripts/backup-status.sh
```

It fails when archive mode is off, the latest archive attempt failed, no remote base
backup exists, or the newest base backup exceeds 26 hours. Override that last threshold
only through a documented `BACKUP_MAX_AGE_SECONDS` operating-policy change.

## Restore drill acceptance

A restore drill is successful only when it restores a selected committed cursor into an
isolated database, replays the world from its stored history, and produces the recorded
state hash at that cursor. Record the date, backup identifier, requested recovery point,
resulting event hash, resulting state hash, duration, operator, and any deviations in
the private operations log. Never point a restore drill at the production database or
reuse its network-exposed service.

Choose a fresh lowercase drill identifier and the world UUID to verify:

```bash
RESTORE_DRILL_ID=quarterly-2026q4 \
RESTORE_WORLD_ID=<world-uuid> \
RESTORE_BACKUP_NAME=LATEST \
./scripts/restore-drill.sh
```

The wrapper creates a separate Compose project, network, PostgreSQL volume, and
loopback-only port. It fetches the selected base backup, replays archived WAL, starts
PostgreSQL, and runs `civilization-runner verify-world`. That verifier rebuilds the
world from genesis, independently checks snapshot-plus-tail, and compares the event and
state hashes with the committed cursor. It retains the stopped drill volume for
inspection and prints an explicit cleanup command; cleanup is never automatic.

The bucket, retention duration, billing, credential lifecycle, independent encryption-
key escrow, and drill record remain owner-controlled operational commitments.
