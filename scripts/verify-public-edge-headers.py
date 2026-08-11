#!/usr/bin/env python3
"""Validate the exact public response-header contract for one edge path."""

from __future__ import annotations

import sys


def verify(path: str, raw_headers: str) -> None:
    raw = raw_headers.replace("\r\n", "\n")
    blocks = [block for block in raw.split("\n\n") if block.strip()]
    if not blocks:
        raise SystemExit(f"{path}: response headers are absent")
    lines = blocks[-1].splitlines()
    status = lines[0] if lines else "none"
    status_parts = status.split()
    if not status.startswith("HTTP/") or len(status_parts) < 2 or status_parts[1] != "200":
        raise SystemExit(f"{path}: expected HTTP 200, found {status}")
    headers: dict[str, str] = {}
    for line in lines[1:]:
        if ":" in line:
            name, value = line.split(":", 1)
            headers[name.strip().lower()] = value.strip()
    expected = {
        "cache-control": "no-store",
        "cross-origin-opener-policy": "same-origin",
        "cross-origin-resource-policy": "same-origin",
        "origin-agent-cluster": "?1",
        "referrer-policy": "no-referrer",
        "strict-transport-security": "max-age=31536000; includeSubDomains",
        "x-content-type-options": "nosniff",
        "x-frame-options": "DENY",
        "x-permitted-cross-domain-policies": "none",
    }
    for name, value in expected.items():
        if headers.get(name) != value:
            raise SystemExit(f"{path}: {name} is {headers.get(name)!r}, expected {value!r}")
    content_security_policy = headers.get("content-security-policy", "")
    for directive in (
        "default-src 'self'",
        "frame-ancestors 'none'",
        "object-src 'none'",
        "upgrade-insecure-requests",
    ):
        if directive not in content_security_policy:
            raise SystemExit(f"{path}: content-security-policy omits {directive!r}")
    permissions_policy = headers.get("permissions-policy", "")
    for feature in ("camera=()", "geolocation=()", "microphone=()", "payment=()"):
        if feature not in permissions_policy:
            raise SystemExit(f"{path}: permissions-policy omits {feature!r}")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: verify-public-edge-headers.py /path")
    verify(sys.argv[1], sys.stdin.read())


if __name__ == "__main__":
    main()
