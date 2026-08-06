# Emergent Civilization Observatory

The public, strictly out-of-world interface for observing live and archived
civilizations. It is a React 19 application built with vinext for a
Cloudflare-compatible runtime.

## Responsibilities

- show live and historical projections from the observer API;
- present evidence-backed wiki pages with explicit provenance;
- archive extinct worlds without rewriting their history;
- expose civilization-created physical artifacts if they emerge;
- provide supporter naming and account surfaces without influencing simulation.

This application never provides information to simulated agents. PostgreSQL-backed
observer projections are accessed only through the Rust API.

## Development

Requires Node.js 22.13 or newer.

```bash
npm ci
npm run dev
npm run lint
npm test
```

The status indicator reads `/api/v1/status`. It intentionally degrades to an offline
state while the Rust API is absent.

## License

Apache-2.0.
