# ADR 0091: Backend readiness includes data-plane progress

## Status

Accepted.

## Decision

The continuous `backend-status.sh` gate checks more than fresh process heartbeats. When one active
world exists it also requires all four observer projection cursors, no projection more than 100
sequences behind by default, no incomplete Hindsight delivery older than five minutes, and no
external cognition dispatch or unlatched worker claim stuck beyond five minutes. The projection and
asynchronous-age thresholds are operator-configurable within bounded ranges.

A deployment with no active world remains healthy, allowing the complete stack to be brought up
before genesis. More than one active world is always unhealthy and independently contradicts the
database lifecycle constraint.

Provider unavailability by itself is not an operations failure: deterministic unavailable latches
are valid world inputs. The monitor targets stuck durable work, not model success, so it cannot turn
provider behavior into a hidden history intervention.

## Consequences

A worker that continues writing heartbeats while its durable queue stalls can no longer leave the
backend green indefinitely. Projection lag remains tolerant of the intentional asynchronous polling
interval, and the thresholds are measured against durable database state rather than observer load.
