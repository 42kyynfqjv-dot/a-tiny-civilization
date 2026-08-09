# Visitor privacy baseline

The public observatory is deliberately usable without third-party visitor measurement. This is a
release invariant, not a future preference.

## Prohibited on public pages

- advertising pixels or conversion tags;
- visitor analytics, session replay, heatmaps, or interaction recording;
- cross-site identity, device fingerprinting, or social-media tracking;
- third-party fonts, scripts, chat widgets, tag managers, or client error-reporting beacons; and
- exporting observer requests, headers, account data, or payment data to simulation cognition.

Adding any of these requires an explicit privacy/security ADR, updated public notice, a legal review,
and any legally required prior consent or opt-out mechanism before deployment. A vendor's free tier
or standard contract does not bypass this gate.

## Current necessary processors

- Cloudflare terminates TLS, routes requests, mitigates abuse, and necessarily handles IP addresses,
  requested paths, timestamps, headers, and security signals.
- The origin serves the observatory and its same-origin API. Anonymous browsing creates no server-side
  observer account or project-level persistent visitor identifier.
- Apple/Google and Stripe remain choice-triggered and configuration-disabled until their corresponding
  account/payment launch gates pass.

Client-side life following uses local browser storage and remains on the device. External scientific
source sites receive a request only after the visitor deliberately follows a source link.

## Technical enforcement

- Content Security Policy limits scripts, connections, fonts, forms, frames, and objects to this origin.
- Referrer data is suppressed, DNS prefetching is disabled, and advertising-attribution/browser-topic
  APIs are denied through response policy.
- Web tests require every rendered public route, verify the privacy disclosure, and reject known
  third-party tracking markers.
- Live release verification must inspect rendered HTML and response headers. Cloudflare Web Analytics,
  Browser Insights, Zaraz, and other script-injection features must remain disabled unless the full
  change gate above is completed.

This engineering baseline reduces exposure; it is not a guarantee against claims and is not a
substitute for advice from counsel licensed in the operator's jurisdiction.

