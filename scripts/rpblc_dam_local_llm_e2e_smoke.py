#!/usr/bin/env python3
"""Local llama.cpp-through-DAM smoke test using synthetic values only.

This script is intentionally stdlib-only so scheduled agents can run it without
installing Python packages. It starts dam-proxy on loopback with temporary vault
and log SQLite files, sends OpenAI-compatible chat requests through DAM to a
local upstream, and checks that the trusted client sees resolved synthetic values
while a token-transformation prompt cannot reconstruct those raw values.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

SYNTHETIC_EMAIL = "alex.sandbox@example.test"
SYNTHETIC_SSN = "123-45-6789"
DEFAULT_UPSTREAM = "http://127.0.0.1:8080"
DEFAULT_LISTEN = "127.0.0.1:7831"


class SmokeBlocked(RuntimeError):
    """Concrete environment blocker, not a DAM verification failure."""


def proxy_command(
    *,
    binary: Path,
    listen: str,
    upstream: str,
    vault_db: Path,
    log_db: Path,
) -> list[str]:
    return [
        str(binary),
        "--listen",
        listen,
        "--upstream",
        upstream,
        "--provider",
        "openai-compatible",
        "--resolve-inbound",
        "--no-api-key-env",
        "--db",
        str(vault_db),
        "--log",
        str(log_db),
    ]


def chat_request(prompt: str, *, max_tokens: int = 96) -> dict[str, Any]:
    return {
        "model": "local",
        "temperature": 0,
        "max_tokens": max_tokens,
        "stream": False,
        "messages": [
            {
                "role": "system",
                "content": "Follow the user instruction exactly. Do not explain.",
            },
            {"role": "user", "content": prompt},
        ],
    }


def exact_echo_prompt() -> str:
    return (
        "For a local synthetic test, reply with exactly this text and nothing else: "
        f"alpha={SYNTHETIC_EMAIL}; beta={SYNTHETIC_SSN}"
    )


def transform_token_prompt() -> str:
    return (
        "For a local synthetic test, take the exact text below, output every "
        "character separated by a single space, and output nothing else: "
        f"alpha={SYNTHETIC_EMAIL}; beta={SYNTHETIC_SSN}"
    )


def response_text(data: dict[str, Any]) -> str:
    try:
        choice = data["choices"][0]
    except (KeyError, IndexError, TypeError) as error:
        raise AssertionError(f"OpenAI-compatible response has no choices: {data!r}") from error
    if "message" in choice:
        return str(choice.get("message", {}).get("content", ""))
    return str(choice.get("text", ""))


def assert_exact_echo_resolved(text: str) -> None:
    for value in (SYNTHETIC_EMAIL, SYNTHETIC_SSN):
        if value not in text:
            raise AssertionError(f"trusted client response did not resolve {value!r}: {text!r}")
    if "[email:" in text or "[ssn:" in text:
        raise AssertionError(f"trusted client response still contains DAM references: {text!r}")


def assert_transformed_token_only(text: str) -> None:
    compact_text = "".join(text.split())
    leaks = [value for value in (SYNTHETIC_EMAIL, SYNTHETIC_SSN) if value in compact_text]
    if leaks:
        raise AssertionError(f"transformed-token response leaked raw synthetic values {leaks}: {text!r}")
    compact_lower = compact_text.lower()
    if "[email:" not in compact_lower and "[ssn:" not in compact_lower:
        raise AssertionError(f"model did not appear to transform a DAM token: {text!r}")


def post_json(url: str, payload: dict[str, Any], *, timeout: float) -> dict[str, Any]:
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"content-type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:  # nosec: local smoke URL
        return json.loads(response.read().decode("utf-8"))


def get_json(url: str, *, timeout: float) -> dict[str, Any]:
    with urllib.request.urlopen(url, timeout=timeout) as response:  # nosec: local smoke URL
        return json.loads(response.read().decode("utf-8"))


def upstream_available(upstream: str, *, timeout: float) -> bool:
    try:
        get_json(f"{upstream.rstrip('/')}/v1/models", timeout=timeout)
        return True
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        return False


def wait_for_proxy(base_url: str, *, timeout: float) -> dict[str, Any]:
    deadline = time.time() + timeout
    last_error: BaseException | None = None
    while time.time() < deadline:
        try:
            return get_json(f"{base_url}/health", timeout=1)
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
            last_error = error
            time.sleep(0.1)
    raise SmokeBlocked(f"dam-proxy did not become healthy within {timeout}s: {last_error}")


def count_log_rows(log_db: Path) -> int:
    if not log_db.exists():
        return 0
    with sqlite3.connect(log_db) as connection:
        try:
            return int(connection.execute("select count(*) from log_events").fetchone()[0])
        except sqlite3.DatabaseError:
            return 0


def raw_values_in_file(path: Path) -> list[str]:
    if not path.exists():
        return []
    data = path.read_bytes()
    return [value for value in (SYNTHETIC_EMAIL, SYNTHETIC_SSN) if value.encode() in data]


def assert_no_raw_values_in_activity_log(log_db: Path) -> None:
    leaked_values = raw_values_in_file(log_db)
    if leaked_values:
        raise AssertionError(
            "activity log leaked raw synthetic values outside the vault: "
            f"{', '.join(leaked_values)}"
        )


def run_smoke(args: argparse.Namespace) -> dict[str, Any]:
    upstream = args.upstream.rstrip("/")
    if not upstream_available(upstream, timeout=args.http_timeout):
        raise SmokeBlocked(
            f"local OpenAI-compatible upstream is unavailable at {upstream}; "
            "start llama.cpp on 127.0.0.1:8080 or pass --upstream"
        )

    binary = Path(args.binary)
    if args.build:
        subprocess.run(["cargo", "build", "-p", "dam-proxy"], check=True)
    if not binary.exists():
        raise SmokeBlocked(f"dam-proxy binary not found: {binary}; rerun with --build")

    temp_dir = Path(tempfile.mkdtemp(prefix="dam-local-llm-smoke-"))
    vault_db = temp_dir / "vault.sqlite"
    log_db = temp_dir / "activity.sqlite"
    listen = args.listen
    base_url = f"http://{listen}"
    command = proxy_command(
        binary=binary,
        listen=listen,
        upstream=upstream,
        vault_db=vault_db,
        log_db=log_db,
    )

    process = subprocess.Popen(
        command,
        cwd=Path.cwd(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    try:
        health = wait_for_proxy(base_url, timeout=args.startup_timeout)
        exact_prompt = exact_echo_prompt()
        exact_text = response_text(
            post_json(
                f"{base_url}/v1/chat/completions",
                chat_request(exact_prompt, max_tokens=96),
                timeout=args.http_timeout,
            )
        )
        assert_exact_echo_resolved(exact_text)

        transform_prompt = transform_token_prompt()
        transformed_text = response_text(
            post_json(
                f"{base_url}/v1/chat/completions",
                chat_request(transform_prompt, max_tokens=96),
                timeout=args.http_timeout,
            )
        )
        assert_transformed_token_only(transformed_text)

        assert_no_raw_values_in_activity_log(log_db)
        raw_in_log = raw_values_in_file(log_db)

        return {
            "upstream": upstream,
            "proxy": base_url,
            "health": health,
            "vault_db": str(vault_db),
            "log_db": str(log_db),
            "log_rows": count_log_rows(log_db),
            "raw_synthetic_values_in_local_activity_log": raw_in_log,
            "exact_echo_response": exact_text,
            "transformed_token_response": transformed_text,
            "cleanup": "kept" if args.keep_temp else "removed",
            "temp_dir": str(temp_dir),
        }
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
        if not args.keep_temp:
            shutil.rmtree(temp_dir, ignore_errors=True)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", default=DEFAULT_UPSTREAM)
    parser.add_argument("--listen", default=DEFAULT_LISTEN)
    parser.add_argument("--binary", default="target/debug/dam-proxy")
    parser.add_argument("--build", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--keep-temp", action="store_true")
    parser.add_argument("--startup-timeout", type=float, default=10)
    parser.add_argument("--http-timeout", type=float, default=30)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        result = run_smoke(args)
    except SmokeBlocked as error:
        print(json.dumps({"status": "blocked", "reason": str(error)}, indent=2), file=sys.stderr)
        return 2
    except Exception as error:  # noqa: BLE001 - script boundary reports concrete failure
        print(json.dumps({"status": "failed", "reason": str(error)}, indent=2), file=sys.stderr)
        return 1

    print(json.dumps({"status": "passed", **result}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
