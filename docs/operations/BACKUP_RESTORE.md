# Backup and restore policy

An archived world is an immutable historical promise, so a single server disk is not a
backup. Before public canonical-world launch, deploy PostgreSQL continuous WAL archiving
and scheduled base backups to owner-controlled offsite object storage. Use a tool with
point-in-time recovery support, such as pgBackRest or WAL-G, and keep its storage
credentials outside the repository and containers' image layers.

## Minimum operating posture

- Encrypt offsite storage and restrict its credentials to the one backup prefix.
- Archive WAL continuously; take daily base backups and retain a documented recovery
  window.
- Keep a separate, periodic exported verification bundle for every archived world.
- Monitor failed archive uploads, missing WAL segments, backup age, disk space, and
  PostgreSQL replication/archive status.
- Test restoration into an isolated PostgreSQL instance at least quarterly and after
  any backup-tool or database-major-version change.

## Restore drill acceptance

A restore drill is successful only when it restores a selected committed cursor into an
isolated database, replays the world from its stored history, and produces the recorded
state hash at that cursor. Record the date, backup identifier, requested recovery point,
resulting event hash, resulting state hash, duration, operator, and any deviations in
the private operations log. Never point a restore drill at the production database or
reuse its network-exposed service.

The chosen object-storage provider, bucket/prefix, retention duration, encryption-key
ownership, and backup tool are owner decisions because they create external billing and
data-retention commitments. This repository supplies the required boundary and
acceptance criteria; it does not invent those credentials or storage destinations.
