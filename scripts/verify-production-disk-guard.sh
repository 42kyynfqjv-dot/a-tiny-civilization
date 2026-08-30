#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
guard="${project_root}/scripts/production-disk-guard.sh"
temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT
mkdir -p "${temporary_directory}/bin"

cat >"${temporary_directory}/bin/df" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\n'
printf '/dev/test 150000000 120000000 %s %s /test\n' \
  "${FAKE_AVAILABLE_KIB:-30000000}" "${FAKE_USED_PERCENT:-80%}"
EOF
chmod 0755 "${temporary_directory}/bin/df"

PATH="${temporary_directory}/bin:${PATH}" "$guard" --dry-run \
  >"${temporary_directory}/healthy.out"
grep -Fq 'Disk guard healthy' "${temporary_directory}/healthy.out"

PATH="${temporary_directory}/bin:${PATH}" FAKE_AVAILABLE_KIB=19000000 FAKE_USED_PERCENT=88% \
  "$guard" --dry-run >"${temporary_directory}/low.out"
grep -Fq 'Disk guard triggered' "${temporary_directory}/low.out"
grep -Fq 'target/debug' "${temporary_directory}/low.out"

if PATH="${temporary_directory}/bin:${PATH}" DISK_GUARD_REQUIRED_FREE_MIB=9000 \
  "$guard" --dry-run >"${temporary_directory}/invalid.out" 2>&1; then
  echo "disk guard accepted an unsafe reserve floor" >&2
  exit 1
fi

for forbidden in 'docker volume' 'docker system prune' 'source-cache' 'derived-cache' \
  'runtime-artifacts'; do
  if rg -Fq "$forbidden" "$guard"; then
    echo "disk guard gained forbidden cleanup scope: $forbidden" >&2
    exit 1
  fi
done
if rg -n '(find|delete|prune).*(postgres|hindsight)|(postgres|hindsight).*(find|delete|prune)' \
  "$guard"; then
  echo "disk guard may monitor but must never clean protected state volumes" >&2
  exit 1
fi
rg -Fq 'docker builder prune' "$guard"
rg -Fq "docker inspect --type volume" "$guard"
rg -Fq 'a-tiny-civilization-postgres-ruleset33-v1' "$guard"
rg -Fq 'a-tiny-civilization-hindsight-v1' "$guard"
rg -Fq 'find "$development_target" -mindepth 1 -delete' "$guard"

echo "Production disk guard is bounded, thresholded, and dry-run testable."
