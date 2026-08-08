# Public incident log

Status: pre-launch. No public production world has been activated, so no public-production incident
has occurred.

Material incidents will be appended newest-first using the policy in
[Incident response and public disclosure](INCIDENT_RESPONSE.md). Pre-launch development failures
remain in qualification and host-preflight evidence when they materially changed a gate; they are
not relabelled as public incidents.

## Entry template

- Incident ID: `ATC-YYYY-NNN`
- Severity: `SEV-1 | SEV-2 | SEV-3`
- Status: `investigating | monitoring | resolved`
- Discovered (UTC): `YYYY-MM-DDTHH:MM:SSZ`
- Recovered (UTC): `none | YYYY-MM-DDTHH:MM:SSZ`
- Affected boundary: `service or invariant`
- Committed world cursor: `none | world / tick / sequence / hash commitment`
- Public impact: `none | factual description`
- Canonical-history impact: `none | factual description`
- Personal/payment-data impact: `none | factual description`
- Root cause: `pending | factual description`
- Remediation: `pending | factual description`
- Verification evidence: `pending | public-safe links or hash commitments`
- Follow-up owner: `stable non-secret operator identifier`
