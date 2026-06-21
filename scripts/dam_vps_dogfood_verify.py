#!/usr/bin/env python3
"""Verify a low-risk DAM VPS dogfooding path with synthetic values only.

This script is intentionally stdlib-only. It starts loopback `dam-proxy` and
`dam-web` against a shared state directory, sends OpenAI-compatible traffic
through DAM to prove tokenization/resolution, checks the Activity API, and
exercises the local pending-consent request flow.
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
from typing import Any, NamedTuple

SYNTHETIC_EMAIL = "alex.sandbox@example.test"
SYNTHETIC_SSN = "123-45-6789"
DEFAULT_UPSTREAM = "http://127.0.0.1:8080"
DEFAULT_LISTEN = "127.0.0.1:7828"
DEFAULT_WEB_ADDR = "127.0.0.1:2896"
DEFAULT_STATE_DIR = Path.home() / ".dam-hermes"


class SmokeBlocked(RuntimeError):
    """Concrete environment blocker, not a DAM verification failure."""


class RuntimePaths(NamedTuple):
    state_dir: Path
    vault_db: Path
    log_db: Path
    consent_db: Path


def runtime_paths(state_dir: Path) -> RuntimePaths:
    state_dir = state_dir.expanduser()
    return RuntimePaths(
        state_dir=state_dir,
        vault_db=state_dir / "vault.db",
        log_db=state_dir / "log.db",
        consent_db=state_dir / "consent.db",
    )


def proxy_command(*, binary: Path, listen: str, upstream: str, paths: RuntimePaths) -> list[str]:
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
        str(paths.vault_db),
        "--log",
        str(paths.log_db),
    ]


def web_command(*, binary: Path, addr: str, paths: RuntimePaths) -> list[str]:
    return [
        str(binary),
        "--addr",
        addr,
        "--db",
        str(paths.vault_db),
        "--log",
        str(paths.log_db),
        "--consent-db",
        str(paths.consent_db),
    ]


def proxy_env(listen: str) -> dict[str, str]:
    proxy_url = f"http://{listen}"
    return {
        "HTTP_PROXY": proxy_url,
        "HTTPS_PROXY": proxy_url,
        "ALL_PROXY": proxy_url,
        "NO_PROXY": "127.0.0.1,localhost",
    }


def pending_request_payload() -> dict[str, Any]:
    return {
        "actor": "codex",
        "value_label": "synthetic email",
        "value_preview": SYNTHETIC_EMAIL,
        "purpose": "verify DAM consent handling on the VPS without revealing raw values upstream",
        "expires_in_sec": 600,
    }


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
        "For a VPS synthetic test, reply with exactly this text and nothing else: "
        f"alpha={SYNTHETIC_EMAIL}; beta={SYNTHETIC_SSN}"
    )


def transform_token_prompt() -> str:
    return (
        "For a VPS synthetic test, copy the final text below exactly. If the "
        "text contains DAM reference tokens like [email:abc] or [ssn:abc], "
        "write them with one space after the opening bracket, like [ email:abc] "
        "or [ ssn:abc]. Do not change any other characters. Text: "
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


def envelope_data(data: dict[str, Any]) -> Any:
    if data.get("ok") is True and "data" in data:
        return data["data"]
    return data


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


def assert_no_raw_values_in_text(text: str, *, surface: str) -> None:
    leaks = [value for value in (SYNTHETIC_EMAIL, SYNTHETIC_SSN) if value in text]
    if leaks:
        raise AssertionError(f"{surface} leaked raw synthetic values: {', '.join(leaks)}")


def post_json(
    url: str,
    payload: dict[str, Any],
    *,
    timeout: float,
    headers: dict[str, str] | None = None,
) -> dict[str, Any]:
    request_headers = {"content-type": "application/json"}
    if headers:
        request_headers.update(headers)
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers=request_headers,
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:  # nosec: local/explicit URLs
        return json.loads(response.read().decode("utf-8"))


def get_json(url: str, *, timeout: float) -> dict[str, Any]:
    with urllib.request.urlopen(url, timeout=timeout) as response:  # nosec: local/explicit URLs
        return json.loads(response.read().decode("utf-8"))


def upstream_available(upstream: str, *, timeout: float) -> bool:
    try:
        get_json(f"{upstream.rstrip('/')}/v1/models", timeout=timeout)
        return True
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        return False


def process_exit_summary(process: subprocess.Popen[str], *, label: str) -> str:
    stdout, stderr = process.communicate(timeout=0.1)
    lines = []
    if stderr.strip():
        lines.append(f"stderr: {stderr.strip()[-500:]}")
    if stdout.strip():
        lines.append(f"stdout: {stdout.strip()[-500:]}")
    output = "; ".join(lines) if lines else "no captured output"
    return f"{label} exited early with code {process.returncode}; {output}"


def wait_for_proxy(
    base_url: str,
    *,
    timeout: float,
    process: subprocess.Popen[str] | None = None,
) -> dict[str, Any]:
    deadline = time.time() + timeout
    last_error: BaseException | None = None
    while time.time() < deadline:
        if process is not None and process.poll() is not None:
            raise SmokeBlocked(process_exit_summary(process, label="dam-proxy"))
        try:
            return get_json(f"{base_url}/health", timeout=1)
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
            last_error = error
            time.sleep(0.1)
    raise SmokeBlocked(f"dam-proxy did not become healthy within {timeout}s: {last_error}")


def wait_for_web(
    base_url: str,
    *,
    timeout: float,
    process: subprocess.Popen[str] | None = None,
) -> dict[str, Any]:
    deadline = time.time() + timeout
    last_error: BaseException | None = None
    while time.time() < deadline:
        if process is not None and process.poll() is not None:
            raise SmokeBlocked(process_exit_summary(process, label="dam-web"))
        try:
            return get_json(f"{base_url}/api/v1/activity?since=0", timeout=1)
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
            last_error = error
            time.sleep(0.1)
    raise SmokeBlocked(f"dam-web did not become ready within {timeout}s: {last_error}")


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


def assert_activity_feed(feed: dict[str, Any]) -> int:
    events = feed.get("events")
    if not isinstance(events, list) or not events:
        raise AssertionError(f"activity feed did not contain any events: {feed!r}")
    assert_no_raw_values_in_text(json.dumps(feed), surface="activity API")
    return len(events)


def run_pending_request_flow(web_base_url: str, *, timeout: float) -> dict[str, Any]:
    headers = {
        "Origin": web_base_url,
        "Referer": f"{web_base_url}/connect",
    }
    triggered = envelope_data(
        post_json(
            f"{web_base_url}/api/v1/requests/trigger",
            pending_request_payload(),
            timeout=timeout,
            headers=headers,
        )
    )
    request_id = triggered.get("id")
    if not request_id:
        raise AssertionError(f"pending request trigger did not return an id: {triggered!r}")

    pending_before = envelope_data(
        get_json(f"{web_base_url}/api/v1/requests/pending", timeout=timeout)
    )
    pending_items = pending_before.get("items") if isinstance(pending_before, dict) else None
    if not isinstance(pending_items, list) or not any(item.get("id") == request_id for item in pending_items):
        raise AssertionError(f"pending request store did not list {request_id}: {pending_before!r}")

    resolved = envelope_data(
        post_json(
            f"{web_base_url}/api/v1/requests/{request_id}/allow-once",
            {},
            timeout=timeout,
            headers=headers,
        )
    )
    remaining = resolved.get("items") if isinstance(resolved, dict) else None
    if not isinstance(remaining, list):
        raise AssertionError(f"allow-once response did not return pending requests: {resolved!r}")
    if any(item.get("id") == request_id for item in remaining):
        raise AssertionError(f"pending request {request_id} was not removed after allow-once")

    return {
        "request_id": request_id,
        "pending_before": len(pending_items),
        "pending_after": len(remaining),
    }


def terminate_process(process: subprocess.Popen[str] | None) -> None:
    if process is None or process.poll() is not None:
        return
    os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)


def run_verify(args: argparse.Namespace) -> dict[str, Any]:
    upstream = args.upstream.rstrip("/")
    if not upstream_available(upstream, timeout=args.http_timeout):
        raise SmokeBlocked(
            f"OpenAI-compatible upstream is unavailable at {upstream}; "
            "start a loopback fake/local upstream or pass --upstream"
        )

    proxy_binary = Path(args.proxy_binary)
    web_binary = Path(args.web_binary)
    if args.build:
        subprocess.run(["cargo", "build", "-p", "dam-proxy", "-p", "dam-web"], check=True)
    if not proxy_binary.exists():
        raise SmokeBlocked(f"dam-proxy binary not found: {proxy_binary}; rerun with --build")
    if not web_binary.exists():
        raise SmokeBlocked(f"dam-web binary not found: {web_binary}; rerun with --build")

    temp_dir: Path | None = None
    if args.state_dir:
        state_dir = Path(args.state_dir).expanduser()
        cleanup = "kept"
    else:
        temp_dir = Path(tempfile.mkdtemp(prefix="dam-vps-dogfood-"))
        state_dir = temp_dir
        cleanup = "kept" if args.keep_state else "removed"
    state_dir.mkdir(parents=True, exist_ok=True)
    paths = runtime_paths(state_dir)

    proxy_process: subprocess.Popen[str] | None = None
    web_process: subprocess.Popen[str] | None = None
    proxy_base = f"http://{args.listen}"
    web_base = f"http://{args.web_addr}"
    try:
        proxy_process = subprocess.Popen(
            proxy_command(binary=proxy_binary, listen=args.listen, upstream=upstream, paths=paths),
            cwd=Path.cwd(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        proxy_health = wait_for_proxy(proxy_base, timeout=args.startup_timeout, process=proxy_process)

        web_process = subprocess.Popen(
            web_command(binary=web_binary, addr=args.web_addr, paths=paths),
            cwd=Path.cwd(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        wait_for_web(web_base, timeout=args.startup_timeout, process=web_process)

        exact_text = response_text(
            post_json(
                f"{proxy_base}/v1/chat/completions",
                chat_request(exact_echo_prompt()),
                timeout=args.http_timeout,
            )
        )
        assert_exact_echo_resolved(exact_text)

        transformed_text = response_text(
            post_json(
                f"{proxy_base}/v1/chat/completions",
                chat_request(transform_token_prompt()),
                timeout=args.http_timeout,
            )
        )
        assert_transformed_token_only(transformed_text)

        activity_feed = envelope_data(
            get_json(f"{web_base}/api/v1/activity?since=0", timeout=args.http_timeout)
        )
        activity_event_count = assert_activity_feed(activity_feed)
        consent = run_pending_request_flow(web_base, timeout=args.http_timeout)

        assert_no_raw_values_in_activity_log(paths.log_db)
        raw_in_log = raw_values_in_file(paths.log_db)

        return {
            "upstream": upstream,
            "proxy": proxy_base,
            "web": web_base,
            "state_dir": str(paths.state_dir),
            "vault_db": str(paths.vault_db),
            "log_db": str(paths.log_db),
            "consent_db": str(paths.consent_db),
            "proxy_pid": proxy_process.pid,
            "web_pid": web_process.pid,
            "proxy_health": proxy_health,
            "proxy_env": proxy_env(args.listen),
            "log_rows": count_log_rows(paths.log_db),
            "raw_synthetic_values_in_local_activity_log": raw_in_log,
            "exact_echo_response": exact_text,
            "transformed_token_response": transformed_text,
            "activity_event_count": activity_event_count,
            "consent": consent,
            "cleanup": cleanup,
        }
    finally:
        terminate_process(web_process)
        terminate_process(proxy_process)
        if temp_dir is not None and not args.keep_state:
            shutil.rmtree(temp_dir, ignore_errors=True)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command")

    env_parser = subparsers.add_parser("env", help="print proxy exports for explicit DAM routing")
    env_parser.add_argument("--listen", default=DEFAULT_LISTEN)

    verify_parser = subparsers.add_parser("verify", help="run proxy/activity/consent verification")
    verify_parser.add_argument("--upstream", default=DEFAULT_UPSTREAM)
    verify_parser.add_argument("--listen", default=DEFAULT_LISTEN)
    verify_parser.add_argument("--web-addr", default=DEFAULT_WEB_ADDR)
    verify_parser.add_argument("--state-dir", default=None)
    verify_parser.add_argument("--proxy-binary", default="target/debug/dam-proxy")
    verify_parser.add_argument("--web-binary", default="target/debug/dam-web")
    verify_parser.add_argument("--build", action=argparse.BooleanOptionalAction, default=True)
    verify_parser.add_argument("--keep-state", action="store_true")
    verify_parser.add_argument("--startup-timeout", type=float, default=15)
    verify_parser.add_argument("--http-timeout", type=float, default=30)

    args = parser.parse_args(argv)
    if not args.command:
        args.command = "verify"
        args.upstream = DEFAULT_UPSTREAM
        args.listen = DEFAULT_LISTEN
        args.web_addr = DEFAULT_WEB_ADDR
        args.state_dir = None
        args.proxy_binary = "target/debug/dam-proxy"
        args.web_binary = "target/debug/dam-web"
        args.build = True
        args.keep_state = False
        args.startup_timeout = 15
        args.http_timeout = 30
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.command == "env":
        for key, value in proxy_env(args.listen).items():
            print(f"export {key}={value}")
        print(f"# canonical state dir: {DEFAULT_STATE_DIR}")
        return 0

    try:
        result = run_verify(args)
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
