#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
configuration="${project_root}/ops/nftables/nftables.conf"
live_rules="${project_root}/ops/nftables/atiny-cognition-egress.nft"

if [[ ${EUID} -ne 0 || ${1:-} != "--confirm-host-firewall-install" ]]; then
  echo "usage: sudo $0 --confirm-host-firewall-install" >&2
  exit 64
fi

nft --check --file "$configuration"
nft --check --file "$live_rules"
install -o root -g root -m 0644 "$configuration" /etc/nftables.conf

# Do not reload the full file on a running Docker host: its deliberate
# `flush ruleset` would momentarily remove Docker's generated chains. Apply
# only missing live rules; systemd loads the complete file on the next boot.
forward_chain="$(nft list chain inet filter forward)"
if ! grep -Fq 'a-tiny-civilization-established' <<<"$forward_chain"; then
  nft insert rule inet filter forward ct state established,related accept \
    comment 'a-tiny-civilization-established'
fi
if ! grep -Fq 'a-tiny-civilization-cognition-dns-udp' <<<"$forward_chain"; then
  nft insert rule inet filter forward iifname 'br-atiny-cog' udp dport 53 accept \
    comment 'a-tiny-civilization-cognition-dns-udp'
fi
if ! grep -Fq 'a-tiny-civilization-cognition-dns-https' <<<"$forward_chain"; then
  nft insert rule inet filter forward iifname 'br-atiny-cog' tcp dport '{ 53, 443 }' accept \
    comment 'a-tiny-civilization-cognition-dns-https'
fi

systemctl enable nftables.service >/dev/null
echo "installed persistent DNS/HTTPS egress for br-atiny-cog"
