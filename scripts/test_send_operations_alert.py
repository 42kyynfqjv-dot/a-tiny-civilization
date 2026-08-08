#!/usr/bin/env python3
from __future__ import annotations

import http.server
import json
import os
import pathlib
import subprocess
import threading
import unittest


ROOT = pathlib.Path(__file__).resolve().parent.parent
NOTIFIER = ROOT / "scripts/send-operations-alert.py"


class Receiver(http.server.BaseHTTPRequestHandler):
    payload: dict | None = None
    authorization: str | None = None

    def do_POST(self):
        length = int(self.headers["Content-Length"])
        type(self).payload = json.loads(self.rfile.read(length))
        type(self).authorization = self.headers.get("Authorization")
        self.send_response(204)
        self.end_headers()

    def log_message(self, _format, *_arguments):
        return


class OperationsAlertTests(unittest.TestCase):
    def run_notifier(self, environment: dict[str, str]):
        return subprocess.run(
            [str(NOTIFIER), "--unit", "a-tiny-civilization-backend-status.service"],
            cwd=ROOT,
            env={**os.environ, **environment},
            text=True,
            capture_output=True,
            timeout=20,
        )

    def test_absent_destination_keeps_the_failed_unit_visible_without_cascading(self):
        environment = os.environ.copy()
        environment.pop("ATINY_OPERATIONS_ALERT_WEBHOOK_URL", None)
        result = subprocess.run(
            [str(NOTIFIER), "--unit", "a-tiny-civilization-backend-status.service"],
            cwd=ROOT,
            env=environment,
            text=True,
            capture_output=True,
            timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("remains failed in systemd", result.stderr)

    def test_plaintext_non_loopback_destination_is_rejected(self):
        result = self.run_notifier(
            {"ATINY_OPERATIONS_ALERT_WEBHOOK_URL": "http://example.com/alert"}
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must use HTTPS", result.stderr)

    def test_posts_only_minimized_failure_metadata_with_optional_bearer(self):
        Receiver.payload = None
        Receiver.authorization = None
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Receiver)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            result = self.run_notifier(
                {
                    "ATINY_OPERATIONS_ALERT_WEBHOOK_URL": (
                        f"http://127.0.0.1:{server.server_port}/alert"
                    ),
                    "ATINY_OPERATIONS_ALERT_ALLOW_HTTP_LOOPBACK": "1",
                    "ATINY_OPERATIONS_ALERT_BEARER_TOKEN": "test-token",
                }
            )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(Receiver.authorization, "Bearer test-token")
        self.assertEqual(
            set(Receiver.payload or {}),
            {"alert_schema_version", "occurred_at", "project", "state", "unit"},
        )
        self.assertEqual(Receiver.payload["state"], "failed")


if __name__ == "__main__":
    unittest.main()
