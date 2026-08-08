#!/usr/bin/env bash
set -euo pipefail

readonly image='ollama/ollama@sha256:b88c73ace3e115f8ec53dc8761ae1c0aabfa675406e3681786b98757ce050f42'
readonly model='qwen2.5:1.5b'
readonly expected_model_digest='65ec06548149b04c096a120e4a6da9d4017ea809c91734ea5631e89f96ddc57b'
readonly volume='atiny-ollama'
readonly container='atiny-local-cognition-provisioner'
readonly endpoint='http://127.0.0.1:11434'

cleanup() {
  docker stop "${container}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

if docker container inspect "${container}" >/dev/null 2>&1; then
  echo "refusing to replace existing container: ${container}" >&2
  exit 2
fi

docker volume inspect "${volume}" >/dev/null 2>&1 || docker volume create "${volume}" >/dev/null
docker run --rm --detach \
  --name "${container}" \
  --network host \
  --env OLLAMA_HOST=127.0.0.1:11434 \
  --volume "${volume}:/root/.ollama" \
  "${image}" >/dev/null

for _attempt in $(seq 1 60); do
  if curl --fail --silent "${endpoint}/api/version" >/dev/null; then
    break
  fi
  sleep 1
done
curl --fail --silent --show-error "${endpoint}/api/version" >/dev/null
curl --fail --silent --show-error --max-time 900 \
  "${endpoint}/api/pull" \
  --header 'Content-Type: application/json' \
  --data "{\"model\":\"${model}\",\"stream\":false}" >/dev/null

actual_digest="$(
  curl --fail --silent --show-error "${endpoint}/api/tags" \
    | sed -n 's/.*"name":"qwen2.5:1.5b"[^}]*"digest":"\([0-9a-f]*\)".*/\1/p'
)"
if [[ "${actual_digest}" != "${expected_model_digest}" ]]; then
  echo "local cognition model digest mismatch: ${actual_digest:-missing}" >&2
  exit 1
fi

echo "Provisioned ${model} (${actual_digest}) in Docker volume ${volume}."
