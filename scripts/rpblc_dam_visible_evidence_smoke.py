#!/usr/bin/env python3
"""Visible evidence smoke test for DAM using loopback-only synthetic data.

Starts a deterministic fake OpenAI-compatible upstream plus dam-proxy and dam-web
against temporary SQLite stores, sends one synthetic request through DAM, and
verifies the user-visible local evidence APIs update without exposing raw
synthetic sensitive values outside allowed control surfaces.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

SYNTHETIC_EMAIL = "alex.sandbox@example.test"
SYNTHETIC_SSN = "123-45-6789"
DEFAULT_PROXY_LISTEN = "127.0.0.1:7831"
DEFAULT_WEB_ADDR = "127.0.0.1:2896"
REFERENCE_PREFIXES = ("[email:", "[ssn:")


class SmokeBlocked(RuntimeError):
    """Concrete environment blocker, not a DAM verification failure."""


def chat_request() -> dict[str, Any]:
    return {
        "model": "local",
        "temperature": 0,
        "max_tokens": 48,
        "stream": False,
        "messages": [
            {
                "role": "system",
                "content": "Reply with the single word ok and nothing else.",
            },
            {
                "role": "user",
                "content": (
                    "Synthetic local DAM test. Keep private values protected. "
                    f"email={SYNTHETIC_EMAIL} ssn={SYNTHETIC_SSN}"
                ),
            },
        ],
    }


def get_json(url: str, *, timeout: float) -> dict[str, Any]:
    with urllib.request.urlopen(url, timeout=timeout) as response:  # nosec: loopback only
        return json.loads(response.read().decode("utf-8"))


def get_bytes(url: str, *, timeout: float) -> bytes:
    with urllib.request.urlopen(url, timeout=timeout) as response:  # nosec: loopback only
        return response.read()


def post_json(url: str, payload: dict[str, Any], *, timeout: float) -> dict[str, Any]:
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"content-type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:  # nosec: loopback only
        return json.loads(response.read().decode("utf-8"))


def post_empty_json(url: str, *, timeout: float) -> dict[str, Any]:
    parsed = urllib.parse.urlparse(url)
    origin = f"{parsed.scheme}://{parsed.netloc}"
    request = urllib.request.Request(
        url,
        data=b"{}",
        headers={
            "content-type": "application/json",
            "origin": origin,
            "referer": origin + "/",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:  # nosec: loopback only
        return json.loads(response.read().decode("utf-8"))


def assert_no_raw_values(label: str, data: bytes) -> None:
    leaks = [value for value in (SYNTHETIC_EMAIL, SYNTHETIC_SSN) if value.encode() in data]
    if leaks:
        raise AssertionError(f"{label} leaked raw synthetic values: {', '.join(leaks)}")


def assert_reference_tokens(label: str, data: bytes) -> None:
    compact = b"".join(data.lower().split())
    if not any(prefix.encode() in compact for prefix in REFERENCE_PREFIXES):
        raise AssertionError(f"{label} did not contain DAM reference tokens")


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


def wait_for_json(url: str, *, timeout: float, interval: float = 0.2) -> dict[str, Any]:
    deadline = time.time() + timeout
    last_error: BaseException | None = None
    while time.time() < deadline:
        try:
            return get_json(url, timeout=1)
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
            last_error = error
            time.sleep(interval)
    raise SmokeBlocked(f"timed out waiting for {url}: {last_error}")


def process_exit_summary(process: subprocess.Popen[str], label: str) -> str:
    stdout, stderr = process.communicate(timeout=0.1)
    lines = []
    if stderr.strip():
        lines.append(f"stderr: {stderr.strip()[-500:]}")
    if stdout.strip():
        lines.append(f"stdout: {stdout.strip()[-500:]}")
    output = "; ".join(lines) if lines else "no captured output"
    return f"{label} exited early with code {process.returncode}; {output}"


def wait_for_process_json(
    url: str,
    *,
    timeout: float,
    process: subprocess.Popen[str],
    label: str,
) -> dict[str, Any]:
    deadline = time.time() + timeout
    last_error: BaseException | None = None
    while time.time() < deadline:
        if process.poll() is not None:
            raise SmokeBlocked(process_exit_summary(process, label))
        try:
            return get_json(url, timeout=1)
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
            last_error = error
            time.sleep(0.1)
    raise SmokeBlocked(f"{label} did not become ready within {timeout}s: {last_error}")


def free_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


class FakeUpstreamState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.last_body: bytes = b""
        self.request_count = 0


class FakeUpstreamHandler(BaseHTTPRequestHandler):
    server_version = "dam-fake-openai/0.1"

    def do_GET(self) -> None:  # noqa: N802
        if self.path == "/v1/models":
            self._write_json({"object": "list", "data": [{"id": "local", "object": "model"}]})
            return
        self.send_error(404)

    def do_POST(self) -> None:  # noqa: N802
        if self.path != "/v1/chat/completions":
            self.send_error(404)
            return
        length = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(length)
        state: FakeUpstreamState = self.server.state  # type: ignore[attr-defined]
        with state.lock:
            state.last_body = body
            state.request_count += 1
        self._write_json(
            {
                "id": "chatcmpl-local",
                "object": "chat.completion",
                "choices": [
                    {"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}
                ],
            }
        )

    def log_message(self, format: str, *args: object) -> None:  # noqa: A003
        return

    def _write_json(self, payload: dict[str, Any]) -> None:
        encoded = json.dumps(payload).encode("utf-8")
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)


class FakeUpstream:
    def __init__(self) -> None:
        self.port = free_loopback_port()
        self.state = FakeUpstreamState()
        self.server = ThreadingHTTPServer(("127.0.0.1", self.port), FakeUpstreamHandler)
        self.server.state = self.state  # type: ignore[attr-defined]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    def start(self) -> None:
        self.thread.start()

    def stop(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)

    def last_body(self) -> bytes:
        with self.state.lock:
            return self.state.last_body

    def request_count(self) -> int:
        with self.state.lock:
            return self.state.request_count


def infer_web_binary(proxy_binary: str) -> str:
    proxy_path = Path(proxy_binary)
    return str(proxy_path.with_name("dam-web"))


def start_process(command: list[str], *, cwd: Path) -> subprocess.Popen[str]:
    return subprocess.Popen(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )


def stop_process(process: subprocess.Popen[str]) -> None:
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def poll_connect(base_url: str, before: int, *, timeout: float) -> tuple[dict[str, Any], bytes]:
    deadline = time.time() + timeout
    last_payload = b""
    while time.time() < deadline:
        payload = get_bytes(f"{base_url}/api/v1/connect", timeout=1)
        last_payload = payload
        assert_no_raw_values("connect api", payload)
        data = json.loads(payload.decode("utf-8"))
        redacted_today = int(data["data"]["counts"]["redacted_today"])
        if redacted_today > before:
            return data, payload
        time.sleep(0.2)
    raise AssertionError("connect redacted_today did not increase within timeout")


def poll_activity(base_url: str, *, timeout: float) -> tuple[dict[str, Any], bytes, dict[str, Any]]:
    deadline = time.time() + timeout
    last_error = ""
    while time.time() < deadline:
        payload = get_bytes(f"{base_url}/api/v1/activity?since=0", timeout=1)
        assert_no_raw_values("activity api", payload)
        data = json.loads(payload.decode("utf-8"))
        events = data["data"]["events"]
        sealed = [event for event in events if event.get("decision") == "sealed"]
        if sealed:
            detail_payload = get_bytes(
                f"{base_url}/api/v1/activity/{sealed[0]['id']}",
                timeout=1,
            )
            assert_no_raw_values("activity detail api", detail_payload)
            detail = json.loads(detail_payload.decode("utf-8"))
            return data, payload, detail
        last_error = payload.decode("utf-8", errors="replace")
        time.sleep(0.2)
    raise AssertionError(f"activity feed did not surface a sealed event within timeout: {last_error[-500:]}")


def sanitize_wallet_add_result(result: dict[str, Any]) -> dict[str, Any]:
    data = result.get("data")
    if not isinstance(data, dict):
        return {"ok": bool(result.get("ok"))}

    raw_item = data.get("item")
    item = raw_item if isinstance(raw_item, dict) else {}
    sanitized_item: dict[str, Any] = {}
    for key in ("id", "kind", "state"):
        if key in item:
            sanitized_item[key] = item[key]

    sanitized: dict[str, Any] = {"ok": bool(result.get("ok"))}
    sanitized_data: dict[str, Any] = {}
    if sanitized_item:
        sanitized_data["item"] = sanitized_item
    for key in ("reference", "first_seen"):
        if key in data:
            sanitized_data[key] = data[key]
    if "meta" in data:
        sanitized_data["meta"] = data["meta"]
    if sanitized_data:
        sanitized["data"] = sanitized_data
    return sanitized


def run_smoke(args: argparse.Namespace) -> dict[str, Any]:
    root = Path(__file__).resolve().parents[1]
    proxy_binary = Path(args.binary)
    web_binary = Path(args.web_binary or infer_web_binary(args.binary))
    if args.build:
        subprocess.run(["cargo", "build", "-p", "dam-proxy", "-p", "dam-web"], check=True, cwd=root)
    if not proxy_binary.exists():
        raise SmokeBlocked(f"dam-proxy binary not found: {proxy_binary}")
    if not web_binary.exists():
        raise SmokeBlocked(f"dam-web binary not found: {web_binary}")

    temp_dir = Path(tempfile.mkdtemp(prefix="dam-visible-evidence-smoke-"))
    vault_db = temp_dir / "vault.sqlite"
    log_db = temp_dir / "activity.sqlite"
    consent_db = temp_dir / "consent.sqlite"

    upstream = FakeUpstream()
    proxy_process: subprocess.Popen[str] | None = None
    web_process: subprocess.Popen[str] | None = None
    try:
        upstream.start()
        proxy_command = [
            str(proxy_binary),
            "--listen",
            args.listen,
            "--upstream",
            upstream.base_url,
            "--provider",
            "openai-compatible",
            "--resolve-inbound",
            "--no-api-key-env",
            "--db",
            str(vault_db),
            "--log",
            str(log_db),
        ]
        web_command = [
            str(web_binary),
            "--addr",
            args.web_addr,
            "--db",
            str(vault_db),
            "--log",
            str(log_db),
            "--consent-db",
            str(consent_db),
        ]
        proxy_process = start_process(proxy_command, cwd=root)
        wait_for_process_json(
            f"http://{args.listen}/health",
            timeout=args.startup_timeout,
            process=proxy_process,
            label="dam-proxy",
        )
        web_process = start_process(web_command, cwd=root)
        wait_for_process_json(
            f"http://{args.web_addr}/api/v1/connect",
            timeout=args.startup_timeout,
            process=web_process,
            label="dam-web",
        )

        connect_before = get_json(f"http://{args.web_addr}/api/v1/connect", timeout=args.http_timeout)
        redacted_before = int(connect_before["data"]["counts"]["redacted_today"])

        post_json(
            f"http://{args.listen}/v1/chat/completions",
            chat_request(),
            timeout=args.http_timeout,
        )

        if upstream.request_count() < 1:
            raise AssertionError("fake upstream did not receive the proxied request")
        upstream_body = upstream.last_body()
        assert_no_raw_values("upstream payload", upstream_body)
        assert_reference_tokens("upstream payload", upstream_body)

        connect_after, connect_payload = poll_connect(
            f"http://{args.web_addr}",
            redacted_before,
            timeout=args.poll_timeout,
        )
        activity_after, activity_payload, detail_after = poll_activity(
            f"http://{args.web_addr}",
            timeout=args.poll_timeout,
        )

        event = next(
            item
            for item in activity_after["data"]["events"]
            if item.get("decision") == "sealed"
        )

        if raw_values_in_file(log_db):
            raise AssertionError(
                "non-vault log database leaked raw synthetic values: "
                + ", ".join(raw_values_in_file(log_db))
            )

        if not event.get("can_add_to_wallet"):
            raise AssertionError(
                "sealed activity event did not allow add-to-wallet; visible-evidence smoke "
                "requires exercising the guarded wallet path"
            )

        add_result = post_empty_json(
            f"http://{args.web_addr}/api/v1/activity/{event['id']}/add-to-wallet",
            timeout=args.http_timeout,
        )
        sanitized_add_result = sanitize_wallet_add_result(add_result)

        return {
            "upstream": upstream.base_url,
            "proxy": f"http://{args.listen}",
            "web": f"http://{args.web_addr}",
            "redacted_today_before": redacted_before,
            "redacted_today_after": connect_after["data"]["counts"]["redacted_today"],
            "activity_event_id": event["id"],
            "activity_value": event.get("value"),
            "activity_reference": event.get("reference"),
            "activity_can_add_to_wallet": event.get("can_add_to_wallet"),
            "activity_summary": activity_after["data"]["summary"],
            "detail_labels": [item["label"] for item in detail_after["data"]["items"]],
            "upstream_payload": upstream_body.decode("utf-8", errors="replace"),
            "connect_payload": connect_payload.decode("utf-8", errors="replace"),
            "activity_payload": activity_payload.decode("utf-8", errors="replace"),
            "log_rows": count_log_rows(log_db),
            "raw_synthetic_values_in_log_db": raw_values_in_file(log_db),
            "wallet_add_result": sanitized_add_result,
            "cleanup": "kept" if args.keep_temp else "removed",
            "temp_dir": str(temp_dir),
        }
    finally:
        if web_process is not None:
            stop_process(web_process)
        if proxy_process is not None:
            stop_process(proxy_process)
        upstream.stop()
        if not args.keep_temp:
            shutil.rmtree(temp_dir, ignore_errors=True)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--listen", default=DEFAULT_PROXY_LISTEN)
    parser.add_argument("--web-addr", default=DEFAULT_WEB_ADDR)
    parser.add_argument("--binary", default="target/debug/dam-proxy")
    parser.add_argument("--web-binary", default="")
    parser.add_argument("--build", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--keep-temp", action="store_true")
    parser.add_argument("--startup-timeout", type=float, default=20)
    parser.add_argument("--http-timeout", type=float, default=30)
    parser.add_argument("--poll-timeout", type=float, default=10)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        result = run_smoke(args)
    except SmokeBlocked as error:
        print(json.dumps({"status": "blocked", "reason": str(error)}, indent=2), file=sys.stderr)
        return 2
    except Exception as error:  # noqa: BLE001
        print(json.dumps({"status": "failed", "reason": str(error)}, indent=2), file=sys.stderr)
        return 1

    print(json.dumps({"status": "passed", **result}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
