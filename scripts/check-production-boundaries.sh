#!/usr/bin/env bash
set -euo pipefail

# Keep the public tunnel's network reachability structurally narrow. This check uses
# the authored Compose files rather than a running stack, so CI catches a boundary
# regression before any deployment credentials or containers exist.

service_has_network() {
  local file="$1"
  local service="$2"
  local network="$3"

  awk -v service="$service" -v network="$network" '
    $0 == "  " service ":" { inside = 1; next }
    inside && /^  [[:alnum:]_-]+:$/ { inside = 0 }
    inside && $0 == "      - " network { found = 1 }
    END { exit(found ? 0 : 1) }
  ' "$file"
}

if service_has_network compose.yaml api edge; then
  echo "Production boundary violation: the observer API must not join edge." >&2
  exit 1
fi

for required in 'api backend' 'api web-api' 'web edge' 'web web-api'; do
  read -r service network <<<"$required"
  if ! service_has_network compose.yaml "$service" "$network"; then
    echo "Production boundary violation: ${service} must join ${network}." >&2
    exit 1
  fi
done

if ! service_has_network compose.tunnel.yaml cloudflared edge; then
  echo "Production boundary violation: cloudflared must join edge." >&2
  exit 1
fi

if rg --line-number --context 8 '^  cloudflared:$' compose.tunnel.yaml | rg --line-number '^      - (backend|web-api)$'; then
  echo "Production boundary violation: cloudflared must not join backend or web-api." >&2
  exit 1
fi

if ! rg --multiline --context 4 '^  web-api:$' compose.yaml | rg --line-number '^    internal: true$' >/dev/null; then
  echo "Production boundary violation: web-api must remain an internal network." >&2
  exit 1
fi

echo "Production network boundary checks passed."
