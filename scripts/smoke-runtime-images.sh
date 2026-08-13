#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
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

catalogue_digest="$(docker run --rm \
  --user 10001:10001 \
  --read-only \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  "$app_image" sha256sum \
    /app/data/cancer-research/nci-cellminer-2-15-cns-challenge-catalogue-v1.json \
  | awk '{print $1}')"
if [[ "$catalogue_digest" != 'ab9f8087135aeb6a62c1d351d088a492b3dafb1c01dd4c37af0d0659be5362a5' ]]; then
  echo "Rust runtime image does not expose the pinned prompt-safe NCI catalogue to uid 10001" >&2
  exit 1
fi

# A clean CI checkout intentionally has no held-out answer key. Hosts that have
# derived it exercise the real bind and uid boundary as part of the image smoke.
nci60_answer_source="${CANCER_NCI60_ANSWER_KEY_SOURCE_PATH:-${project_root}/data/source-cache/nci-cellminer-2026-08-12/nci-cellminer-2-15-cns-challenge-answer-key-v1.json}"
if [[ -f "$nci60_answer_source" ]]; then
  ATINY_APP_IMAGE="$app_image" \
    CANCER_NCI60_ANSWER_KEY_SOURCE_PATH="$nci60_answer_source" \
    bash "${project_root}/scripts/smoke-cancer-nci60-qualification-key.sh"
fi

# Patient-derived evidence is intentionally absent from clean CI checkouts. A
# host with the derived pair must expose both files or neither; a half-staged
# provenance pair is always a release failure.
pdc000711_proteome_source="${CANCER_PDC000711_PROTEOME_SOURCE_PATH:-${project_root}/data/derived-cache/pdc000711-hcmi-gbm-proteome/pdc000711-gbm-proteome.tsv}"
pdc000711_metadata_source="${CANCER_PDC000711_PROTEOME_METADATA_SOURCE_PATH:-${project_root}/data/derived-cache/pdc000711-hcmi-gbm-proteome/pdc000711-gbm-proteome.metadata.json}"
if [[ -f "$pdc000711_proteome_source" && -f "$pdc000711_metadata_source" ]]; then
  ATINY_APP_IMAGE="$app_image" \
    CANCER_PDC000711_PROTEOME_SOURCE_PATH="$pdc000711_proteome_source" \
    CANCER_PDC000711_PROTEOME_METADATA_SOURCE_PATH="$pdc000711_metadata_source" \
    bash "${project_root}/scripts/smoke-cancer-pdc000711-evidence.sh"
elif [[ -e "$pdc000711_proteome_source" || -e "$pdc000711_metadata_source" ]]; then
  echo "PDC000711 runtime smoke found an incomplete matrix/metadata pair" >&2
  exit 1
fi

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
