#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "${temporary_directory}"' EXIT

cat >"${temporary_directory}/docker" <<'FAKE_DOCKER'
#!/usr/bin/env bash
exit 0
FAKE_DOCKER
chmod 0755 "${temporary_directory}/docker"

common=(
  env
  -u ATINY_MODERATOR_ID
  PATH="${temporary_directory}:${PATH}"
  APP_ENV=production
  POSTGRES_DB=ci
  POSTGRES_USER=ci
  POSTGRES_PASSWORD=ci-production-check-only
  OPENROUTER_API_KEY=ci-free-route-check-only
  COGNITION_EXTERNAL_EXPORT_APPROVED=true
  STRIPE_SECRET_KEY=sk_live_ci_only
  STRIPE_SUPPORTER_PRICE_ID=price_ci_only
  STRIPE_WEBHOOK_SECRET=whsec_ci_only
  STRIPE_LIVE_MODE=true
  GOOGLE_OAUTH_CLIENT_ID=google-ci-only
  GOOGLE_OAUTH_CLIENT_SECRET=google-ci-secret-only
)

failure_output="${temporary_directory}/missing-moderator.txt"
if "${common[@]}" "${project_root}/scripts/production-preflight.sh" \
  >"${failure_output}" 2>&1; then
  echo "production preflight accepted Stripe without a moderator identity" >&2
  exit 1
fi
if ! grep -q 'stable ATINY_MODERATOR_ID' "${failure_output}"; then
  echo "production preflight failed for the wrong missing-moderator reason" >&2
  exit 1
fi

"${common[@]}" ATINY_MODERATOR_ID=operator:ci \
  "${project_root}/scripts/production-preflight.sh" >/dev/null

wrong_callback_output="${temporary_directory}/wrong-callback.txt"
if "${common[@]}" ATINY_MODERATOR_ID=operator:ci \
  GOOGLE_OAUTH_REDIRECT_URI=https://example.invalid/callback \
  "${project_root}/scripts/production-preflight.sh" >"$wrong_callback_output" 2>&1; then
  echo "production preflight accepted a foreign OAuth callback" >&2
  exit 1
fi
if ! grep -q 'exact atinycivilization.com HTTPS route' "$wrong_callback_output"; then
  echo "production preflight rejected the foreign callback for the wrong reason" >&2
  exit 1
fi

test_mode_output="${temporary_directory}/test-mode.txt"
if "${common[@]}" ATINY_MODERATOR_ID=operator:ci STRIPE_LIVE_MODE=false \
  "${project_root}/scripts/production-preflight.sh" >"$test_mode_output" 2>&1; then
  echo "production preflight accepted test-mode Stripe Checkout" >&2
  exit 1
fi
if ! grep -q 'requires STRIPE_LIVE_MODE=true' "$test_mode_output"; then
  echo "production preflight rejected test-mode Stripe for the wrong reason" >&2
  exit 1
fi

echo "Supporter production policy preflight is enforced."
