# Incident response and public disclosure

This policy covers service and integrity incidents in A Tiny Civilization. Simulation events such
as injury, death, extinction, resource scarcity, or an uneventful period are world history, not
operations incidents. An incident is a failure of the promised boundary: unavailable public
observation, missed target advancement caused by infrastructure, projection lag or corruption,
failed memory/cognition delivery beyond its declared bound, unauthorized access, lost durability,
payment/moderation failure, or an operator action that did not follow the public runbook.

## Response order

1. Preserve canonical history and stop additional damage. A capacity pause occurs only after a
   committed boundary; no operator creates compensating world events or edits history.
2. Record UTC discovery time, affected service, committed world cursor, symptoms, and the checks
   that failed. Preserve bounded logs and payload-free hash evidence.
3. Restore the declared service boundary using retry-safe commands. Never reinitialize a world,
   reroll its seed, or replace an extinct/archived history.
4. Publish an initial notice in `INCIDENTS.md` within 24 hours for a material availability,
   integrity, privacy, payment, or history-verification incident. Security notices may temporarily
   withhold exploit-enabling detail but still disclose scope and user action.
5. Close the entry with the UTC recovery time, impact, root cause, remediation, verification, and
   follow-up owner. A later correction is appended and dated; the original disclosure is retained.

## Severity

- **SEV-1:** suspected history loss/tampering, unauthorized private-data access, payment credential
  exposure, or inability to prove the canonical cursor. Public access is closed while evidence is
  preserved and the world pauses only at a committed boundary.
- **SEV-2:** extended public/API outage, unhealthy canonical writer, materially stale projections,
  missed memory/cognition delivery bounds, or supporter fulfilment/refund failure.
- **SEV-3:** degraded but trustworthy observation, delayed noncanonical features, or a near miss
  caught before public impact.

## Required public entry fields

Every material entry has a stable incident ID, severity, UTC discovery and recovery timestamps,
status, affected boundary, committed world cursor (when applicable), public impact, canonical-history
impact, personal/payment-data impact, root cause, remediation, verification evidence, and follow-up
owner. `none` is an explicit value; fields are never silently omitted.

The public entry contains no credentials, OAuth/payment identifiers, private cognition or memory
payloads, individual positions, reproductive mechanisms, or graphic world detail. Operational
honesty does not weaken the observer/privacy boundary.
