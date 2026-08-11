#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).with_name("verify-public-edge-headers.py")
SPEC = importlib.util.spec_from_file_location("verify_public_edge_headers", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("could not load public edge header verifier")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


VALID = """HTTP/2 200
cache-control: no-store
content-security-policy: default-src 'self'; frame-ancestors 'none'; object-src 'none'; upgrade-insecure-requests
cross-origin-opener-policy: same-origin
cross-origin-resource-policy: same-origin
origin-agent-cluster: ?1
permissions-policy: camera=(), geolocation=(), microphone=(), payment=()
referrer-policy: no-referrer
strict-transport-security: max-age=31536000; includeSubDomains
x-content-type-options: nosniff
x-frame-options: DENY
x-permitted-cross-domain-policies: none

"""


class PublicEdgeHeaderTests(unittest.TestCase):
    def test_accepts_exact_contract(self) -> None:
        MODULE.verify("/", VALID)

    def test_rejects_missing_or_weakened_header(self) -> None:
        with self.assertRaises(SystemExit):
            MODULE.verify("/wiki", VALID.replace("x-frame-options: DENY\n", ""))
        with self.assertRaises(SystemExit):
            MODULE.verify("/api/v1/status", VALID.replace("camera=()", "camera=(self)"))


if __name__ == "__main__":
    unittest.main()
