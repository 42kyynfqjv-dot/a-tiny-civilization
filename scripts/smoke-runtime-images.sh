#!/usr/bin/env bash
set -euo pipefail

app_image="${ATINY_APP_IMAGE:-a-tiny-civilization-app:ci}"
web_image="${ATINY_WEB_IMAGE:-a-tiny-civilization-web:ci}"

if [[ "$(docker image inspect --format '{{.Config.User}}' "$app_image")" != "civilization" ]]; then
  echo "Rust runtime image does not execute as civilization" >&2
  exit 1
fi
if [[ "$(docker image inspect --format '{{.Config.User}}' "$web_image")" != "node" ]]; then
  echo "web runtime image does not execute as node" >&2
  exit 1
fi

docker run --rm \
  --read-only \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --tmpfs /tmp:rw,noexec,nosuid,size=16m,mode=1777 \
  "$app_image" /app/civilization-runner --help >/dev/null

web_container=""
cleanup() {
  if [[ -n "$web_container" ]]; then
    docker stop --time 10 "$web_container" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

web_container="$(docker run --detach --rm \
  --read-only \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --tmpfs /tmp:rw,noexec,nosuid,size=16m,mode=1777 \
  --publish 127.0.0.1::3000 \
  "$web_image")"

web_address="$(docker port "$web_container" 3000/tcp)"
if [[ ! "$web_address" =~ ^127\.0\.0\.1:[0-9]{1,5}$ ]]; then
  echo "web runtime did not publish one loopback address: ${web_address:-none}" >&2
  exit 1
fi

web_ready=0
for _ in $(seq 1 30); do
  if curl --fail --silent --show-error "http://${web_address}/" >/dev/null; then
    web_ready=1
    break
  fi
  if ! docker inspect "$web_container" >/dev/null 2>&1; then
    echo "web runtime exited before becoming ready" >&2
    exit 1
  fi
  sleep 1
done

if ((web_ready != 1)); then
  docker logs "$web_container" >&2 || true
  echo "web runtime did not become ready within 30 seconds" >&2
  exit 1
fi

# Exercise the container server rather than only the source-level Worker test. Cloudflare's
# zone-level redirect remains preferred, but a released origin must also preserve the canonical
# target when a plaintext request reaches it with the public Host header.
redirect="$(curl --silent --show-error --head \
  --header 'Host: atinycivilization.com' \
  --write-out '%{http_code}|%{redirect_url}' --output /dev/null \
  "http://${web_address}/wiki?edge-check=container")"
IFS='|' read -r redirect_status redirect_url <<<"$redirect"
if [[ "$redirect_status" != "308" \
   || "$redirect_url" != "https://atinycivilization.com/wiki?edge-check=container" ]]; then
  echo "web runtime did not preserve the canonical HTTPS target: ${redirect_status:-none} ${redirect_url:-none}" >&2
  exit 1
fi

echo "Production images run as non-root on read-only filesystems and preserve canonical HTTPS."
