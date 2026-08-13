# Cancer World Fireworks cost reconciliation

Use this only for Fireworks Cancer World reservations whose immutable status is
`indeterminate`. Normal successful responses settle from their recorded runtime
receipt and never use this path.

## Export

Export no more than one UTC billing month with the official Fireworks CLI. Keep
the CSV outside the repository and do not edit, sort, normalize line endings, or
combine it with another export after download: the entire byte stream and exact
matched records are the evidence.

```sh
firectl billing export-metrics \
  --start-time "2026-08-01" \
  --end-time "2026-09-01" \
  --filename /run/operator/fireworks-2026-08.csv
```

The Fireworks command supports at most 31 days. The runner intentionally never
accepts an API key and never contacts Fireworks; acquisition and admission are
separate operator actions.

## Verify without writing

Run the production runner image or binary with its normal protected
`DATABASE_URL`. Omit the confirmation flag first:

```sh
civilization-runner reconcile-cancer-fireworks-billing \
  --billing-month 2026-08-01 \
  --export /run/operator/fireworks-2026-08.csv
```

Expected status is `verified_no_write`. Review:

- the month;
- number of matched indeterminate requests;
- verified actual and released micro-dollar totals; and
- the SHA-256 of the unmodified whole export.

The command fails without writing if a request has no matching row, more than
one row is within five seconds, a row is reused, a required/known header is
missing or ambiguous, the model differs, the tariff
cost exceeds its reservation, or any database provenance check fails.

## Append verified evidence

Re-run the exact file and arguments with explicit confirmation:

```sh
civilization-runner reconcile-cancer-fireworks-billing \
  --billing-month 2026-08-01 \
  --export /run/operator/fireworks-2026-08.csv \
  --confirm-operator-reconciliation
```

Expected status is `recorded`. Repeating the exact command is safe and makes no
second aggregate adjustment. The same request with any different export hash,
row hash, timestamp, token count, or amount conflicts.

The database retains whole-file length/hash, exact matched row byte ranges/hashes,
and bounded billing facts, not the CSV's email or other account metadata. Retain or remove the operator CSV according to the
account owner's secure billing-record policy; it is not an application runtime
artifact and must never be mounted into the research worker.
