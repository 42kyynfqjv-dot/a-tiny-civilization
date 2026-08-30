# ADR 0176: Cancer console uses stale-while-revalidate

Date: 2026-08-30

Status: Accepted

## Context

The protected Cancer World view verifies and reconstructs every successful research
receipt, duplicate relationship, campaign, experiment, qualification, memory state,
and provenance checksum before publishing a bounded page. At the current history
size, that cold reconstruction takes about five seconds. The API already cached the
view for sixty seconds, but the first observer request after every expiry synchronously
paid the full rebuild cost. The console therefore appeared to pause even while the
research workers and canonical world were healthy.

## Decision

- The first request after an API process start still performs the complete read-only
  reconstruction and fails closed if it cannot verify the view.
- Once one verified view exists, expiry never removes it. The next authorized request
  receives that stale view immediately and starts exactly one background refresh.
- Concurrent requests share the stale view while the refresh is running. A successful
  refresh atomically replaces it and starts a new sixty-second freshness window.
- A failed refresh preserves the last verified view and waits ten seconds before
  another request may trigger a retry. Failures remain visible in structured logs.
- Authorization is checked before either cached or refreshed data is returned. The
  cache is process-local, read-side only, and cannot affect Cancer World scheduling,
  research memory, canonical history, or replay.

## Consequences

Warm console reads remain millisecond-scale even when the provenance reconstruction
takes seconds. The displayed totals may be up to roughly one refresh interval plus
one rebuild duration behind the database, which is acceptable for an observer status
surface and is preferable to periodic UI stalls. A process restart still has one
intentional cold read rather than serving unverified or persisted cache data.
