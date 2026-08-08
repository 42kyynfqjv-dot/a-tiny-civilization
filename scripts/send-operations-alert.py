#!/usr/bin/env python3
"""Deliver one privacy-minimized systemd failure alert to an operator webhook."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request


UNIT = re.compile(r"^[A-Za-z0-9_.@:-]{1,200}\.service$")
LOOPBACK_HOSTS = {"127.0.0.1", "::1", "localhost"}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, file_pointer, code, message, headers, new_url):
        return None


def validated_url(raw: str, allow_http_loopback: bool) -> str:
    if len(raw) > 2048 or any(character.isspace() for character in raw):
        raise ValueError("operations alert webhook URL is malformed")
    parsed = urllib.parse.urlsplit(raw)
    if parsed.username is not None or parsed.password is not None or parsed.fragment:
        raise ValueError("operations alert webhook URL contains forbidden components")
    if not parsed.hostname:
        raise ValueError("operations alert webhook URL has no host")
    if parsed.scheme == "https":
        return raw
    if (
        parsed.scheme == "http"
        and allow_http_loopback
        and parsed.hostname.lower() in LOOPBACK_HOSTS
    ):
        return raw
    raise ValueError("operations alert webhook must use HTTPS")


def alert_payload(unit: str) -> bytes:
    document = {
        "alert_schema_version": 1,
        "occurred_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "project": "a-tiny-civilization",
        "state": "failed",
        "unit": unit,
    }
    return json.dumps(document, sort_keys=True, separators=(",", ":")).encode("utf-8")


def deliver(url: str, unit: str, bearer_token: str) -> None:
    if len(bearer_token) > 4096 or "\r" in bearer_token or "\n" in bearer_token:
        raise ValueError("operations alert bearer token is malformed")
    headers = {
        "Content-Type": "application/json",
        "User-Agent": "a-tiny-civilization-operations-alert/1",
    }
    if bearer_token:
        headers["Authorization"] = f"Bearer {bearer_token}"
    opener = urllib.request.build_opener(NoRedirect())
    payload = alert_payload(unit)
    last_error: Exception | None = None
    for attempt in range(1, 4):
        request = urllib.request.Request(url, data=payload, headers=headers, method="POST")
        try:
            with opener.open(request, timeout=10) as response:
                if not 200 <= response.status < 300:
                    raise RuntimeError(f"alert receiver returned HTTP {response.status}")
                response.read(1024)
                return
        except (OSError, RuntimeError, urllib.error.HTTPError) as error:
            last_error = error
            if attempt < 3:
                time.sleep(attempt)
    raise RuntimeError(f"operations alert delivery failed after three attempts: {last_error}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--unit", required=True)
    args = parser.parse_args()
    if not UNIT.fullmatch(args.unit):
        print("operations alert unit identity is invalid", file=sys.stderr)
        return 2

    url = os.environ.get("ATINY_OPERATIONS_ALERT_WEBHOOK_URL", "").strip()
    if not url:
        print(
            f"operations alert destination is not configured; {args.unit} remains failed in systemd",
            file=sys.stderr,
        )
        return 0
    allow_http_loopback = os.environ.get("ATINY_OPERATIONS_ALERT_ALLOW_HTTP_LOOPBACK", "") == "1"
    try:
        url = validated_url(url, allow_http_loopback)
        deliver(url, args.unit, os.environ.get("ATINY_OPERATIONS_ALERT_BEARER_TOKEN", ""))
    except (OSError, RuntimeError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 1
    print(f"operations alert delivered for {args.unit}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
