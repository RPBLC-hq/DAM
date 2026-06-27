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
from typing import Any, NamedTuple

SYNTHETIC_EMAIL = "alex.sandbox@example.test"
SYNTHETIC_PHONE = "+14155550123"
SYNTHETIC_SSN = "123-45-6789"
SYNTHETIC_ENV_SECRET = "SandboxLocalSecretValue0001"
AGENT_SESSION_FIXTURE_NAME = "agent_session_mixed_pii_secret_v1"
DEFAULT_UPSTREAM = "http://127.0.0.1:8080"
DEFAULT_LISTEN = "127.0.0.1:7831"


def synthetic_github_token() -> str:
    """Return a synthetic GitHub-shaped token without storing one as a repo literal."""

    return "ghp" + "_" + ("A" * 30)


def raw_synthetic_values() -> tuple[str, ...]:
    return (
        SYNTHETIC_EMAIL,
        SYNTHETIC_PHONE,
        SYNTHETIC_SSN,
        SYNTHETIC_ENV_SECRET,
        synthetic_github_token(),
    )


def leak_summary(leaks: list[str]) -> str:
    """Describe leaked synthetic values without printing the values themselves."""

    return f"{len(set(leaks))} raw synthetic value(s)"


class SmokeRouteCase(NamedTuple):
    """Representative MVP route target exercised through the proxy smoke."""

    route_id: str
    target_name: str
    provider: str
    label: str


DEFAULT_ROUTE_CASES = (
    SmokeRouteCase("openai-api", "openai", "openai-compatible", "OpenAI API HTTP route"),
    SmokeRouteCase("anthropic-api", "anthropic", "anthropic", "Anthropic API HTTP route"),
    SmokeRouteCase("claude-web", "claude-web", "generic-http", "Claude web-profile HTTP route"),
    SmokeRouteCase(
        "anthropic-console",
        "anthropic-console",
        "generic-http",
        "Anthropic Console HTTP route",
    ),
    SmokeRouteCase(
        "claude-mcp-proxy",
        "claude-mcp-proxy",
        "generic-http",
        "Claude MCP proxy HTTP route",
    ),
    SmokeRouteCase(
        "claude-platform",
        "claude-platform",
        "generic-http",
        "Claude Platform HTTP route",
    ),
    SmokeRouteCase(
        "openai-platform",
        "openai-platform",
        "generic-http",
        "OpenAI Platform HTTP route",
    ),
)


class SmokeBlocked(RuntimeError):
    """Concrete environment blocker, not a DAM verification failure."""


def proxy_command(
    *,
    binary: Path,
    listen: str,
    upstream: str,
    vault_db: Path,
    log_db: Path,
    route_case: SmokeRouteCase = DEFAULT_ROUTE_CASES[0],
) -> list[str]:
    return [
        str(binary),
        "--listen",
        listen,
        "--upstream",
        upstream,
        "--target-name",
        route_case.target_name,
        "--provider",
        route_case.provider,
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
        "For a local synthetic test, copy the final text below exactly. If the "
        "text contains DAM reference tokens like [email:abc] or [ssn:abc], "
        "write them with one space after the opening bracket, like [ email:abc] "
        "or [ ssn:abc]. Do not change any other characters. Text: "
        f"alpha={SYNTHETIC_EMAIL}; beta={SYNTHETIC_SSN}"
    )


def agent_session_prompt() -> str:
    github_token = synthetic_github_token()
    return "\n".join(
        [
            "For a local synthetic DAM product proof, summarize this agent session without exposing secrets.",
            f"Fixture: {AGENT_SESSION_FIXTURE_NAME}",
            "",
            "## User prompt",
            f"Please debug onboarding for {SYNTHETIC_EMAIL} and call {SYNTHETIC_PHONE} only if needed.",
            f"Temporary test SSN-like identifier: {SYNTHETIC_SSN}",
            "",
            "```dotenv",
            f"OPENAI_API_KEY={SYNTHETIC_ENV_SECRET}",
            f"GITHUB_TOKEN={github_token}",
            "FEATURE_FLAG=synthetic-local-only",
            "```",
            "",
            "Tool output:",
            "- repo: example/local-agent-sandbox",
            "- command: npm test -- --runInBand",
            "- result: failed because test fixtures were not updated",
        ]
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
    leaks = [value for value in raw_synthetic_values() if value in compact_text]
    if leaks:
        raise AssertionError(
            f"transformed-token response leaked {leak_summary(leaks)}; response redacted"
        )
    compact_lower = compact_text.lower()
    if "[email:" not in compact_lower and "[ssn:" not in compact_lower:
        raise AssertionError(f"model did not appear to transform a DAM token: {text!r}")


def assert_agent_session_response_safe(text: str) -> None:
    compact_text = "".join(text.split())
    leaks = [value for value in raw_synthetic_values() if value in compact_text]
    if leaks:
        raise AssertionError(
            f"agent-session response leaked {leak_summary(leaks)}; response redacted"
        )


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


def upstream_transcript(upstream: str, *, timeout: float) -> dict[str, Any] | None:
    transcript_url = f"{upstream.rstrip('/')}/__dam/transcript"
    try:
        return get_json(transcript_url, timeout=timeout)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise AssertionError(
            f"fake upstream transcript endpoint failed closed with HTTP {error.code}: {transcript_url}"
        ) from error
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as error:
        raise AssertionError(
            f"fake upstream transcript endpoint was advertised but unreadable: {transcript_url}: {error}"
        ) from error


def transcript_requests(transcript: dict[str, Any] | None) -> list[Any]:
    if transcript is None:
        return []
    requests = transcript.get("requests")
    if not isinstance(requests, list):
        raise AssertionError(f"fake upstream transcript returned malformed requests: {transcript!r}")
    return requests


def assert_upstream_transcript_protected(transcript: dict[str, Any] | None) -> list[str]:
    if transcript is None:
        return []
    requests = transcript_requests(transcript)
    if not requests:
        raise AssertionError(f"fake upstream transcript did not record any requests: {transcript!r}")
    path_results = []
    payload_positions_checked = False
    leaks: list[str] = []
    for request in requests:
        if not isinstance(request, dict):
            continue
        path_results.append(str(request.get("path", "")))
        surfaces = [str(request.get("body", "")), str(request.get("user_content", ""))]
        joined = "\n".join(surfaces)
        leaks.extend(value for value in raw_synthetic_values() if value in joined)
        compact_lower = "".join(joined.split()).lower()
        if "alpha=[email:" in compact_lower and "beta=[ssn:" in compact_lower:
            payload_positions_checked = True
    if leaks:
        raise AssertionError(
            f"fake upstream transcript leaked {leak_summary(leaks)}; transcript redacted"
        )
    if not payload_positions_checked:
        raise AssertionError(
            "fake upstream transcript did not contain DAM references in the synthetic payload positions "
            "alpha=[email:...] and beta=[ssn:...]: "
            f"{transcript!r}"
        )
    return path_results


def assert_agent_session_transcript_protected(transcript: dict[str, Any] | None) -> dict[str, str]:
    if transcript is None:
        return {}
    requests = transcript_requests(transcript)
    fixture_requests = []
    for request in requests:
        if not isinstance(request, dict):
            continue
        joined = "\n".join([str(request.get("body", "")), str(request.get("user_content", ""))])
        if AGENT_SESSION_FIXTURE_NAME in joined:
            fixture_requests.append(joined)
    if not fixture_requests:
        raise AssertionError(f"fake upstream transcript did not include {AGENT_SESSION_FIXTURE_NAME}")

    joined_fixture = "\n".join(fixture_requests)
    leaks = [value for value in raw_synthetic_values() if value in joined_fixture]
    if leaks:
        raise AssertionError(
            f"agent-session transcript leaked {leak_summary(leaks)}; transcript redacted"
        )

    compact_lower = "".join(joined_fixture.split()).lower()
    expected_kinds = ("email", "phone", "ssn", "api_key")
    missing = [
        kind for kind in expected_kinds if f"[{kind}:" not in compact_lower and f"[{kind}]" not in compact_lower
    ]
    if missing:
        raise AssertionError(
            "agent-session transcript did not contain expected DAM references/redactions "
            f"for {missing}: {transcript!r}"
        )
    return {kind: "reference_or_redaction_observed" for kind in expected_kinds}


def process_exit_summary(process: subprocess.Popen[str]) -> str:
    stdout, stderr = process.communicate(timeout=0.1)
    lines = []
    if stderr.strip():
        lines.append(f"stderr: {stderr.strip()[-500:]}")
    if stdout.strip():
        lines.append(f"stdout: {stdout.strip()[-500:]}")
    output = "; ".join(lines) if lines else "no captured output"
    return f"dam-proxy exited early with code {process.returncode}; {output}"


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
            raise SmokeBlocked(process_exit_summary(process))
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


def max_log_event_id(log_db: Path) -> int:
    if not log_db.exists():
        return 0
    with sqlite3.connect(log_db) as connection:
        try:
            return int(connection.execute("select coalesce(max(id), 0) from log_events").fetchone()[0])
        except sqlite3.DatabaseError:
            return 0


def raw_values_in_file(path: Path) -> list[str]:
    if not path.exists():
        return []
    data = path.read_bytes()
    return [value for value in raw_synthetic_values() if value.encode() in data]


def assert_no_raw_values_in_activity_log(log_db: Path) -> None:
    leaked_values = raw_values_in_file(log_db)
    if leaked_values:
        raise AssertionError(
            "activity log leaked raw synthetic values outside the vault: "
            f"{leak_summary(leaked_values)}; activity log redacted"
        )


def assert_health_route_matches(
    health: dict[str, Any],
    *,
    route_case: SmokeRouteCase,
    upstream: str,
) -> None:
    actual_target = health.get("target")
    if actual_target != route_case.target_name:
        raise AssertionError(
            "dam-proxy health target did not match route smoke target: "
            f"expected {route_case.target_name!r}, got {actual_target!r}; health={health!r}"
        )

    actual_upstream = health.get("upstream")
    if str(actual_upstream).rstrip("/") != upstream.rstrip("/"):
        raise AssertionError(
            "dam-proxy health upstream did not match route smoke upstream: "
            f"expected {upstream!r}, got {actual_upstream!r}; health={health!r}"
        )


def provider_forward_messages(log_db: Path) -> list[str]:
    if not log_db.exists():
        return []
    with sqlite3.connect(log_db) as connection:
        try:
            rows = connection.execute(
                "select message from log_events where action = ? order by id",
                ("provider_forward_start",),
            ).fetchall()
        except sqlite3.DatabaseError:
            return []
    return [str(row[0]) for row in rows]


def detector_kind_action_counts(
    log_db: Path,
    *,
    after_id: int | None = None,
    before_id: int | None = None,
    event_type: str | None = None,
    action: str | None = None,
) -> dict[str, int]:
    if not log_db.exists():
        return {}

    clauses = ["kind is not null", "action is not null"]
    params: list[Any] = []
    if after_id is not None:
        clauses.append("id > ?")
        params.append(after_id)
    if before_id is not None:
        clauses.append("id < ?")
        params.append(before_id)
    if event_type is not None:
        clauses.append("event_type = ?")
        params.append(event_type)
    if action is not None:
        clauses.append("action = ?")
        params.append(action)

    query = f"""
        select kind, action, count(*)
        from log_events
        where {' and '.join(clauses)}
        group by kind, action
        order by kind, action
    """
    with sqlite3.connect(log_db) as connection:
        try:
            rows = connection.execute(query, tuple(params)).fetchall()
        except sqlite3.DatabaseError:
            return {}
    return {f"{row[0]}:{row[1]}": int(row[2]) for row in rows}


def first_provider_forward_id_after(log_db: Path, after_id: int) -> int:
    if not log_db.exists():
        raise AssertionError("activity log did not record provider_forward_start for agent-session request")
    with sqlite3.connect(log_db) as connection:
        try:
            row = connection.execute(
                """
                select id
                from log_events
                where id > ? and action = ?
                order by id
                limit 1
                """,
                (after_id, "provider_forward_start"),
            ).fetchone()
        except sqlite3.DatabaseError as error:
            raise AssertionError(f"activity log unreadable: {error}") from error
    if row is None:
        raise AssertionError("activity log did not record provider_forward_start for agent-session request")
    return int(row[0])


def assert_agent_session_detector_kinds_observed(counts: dict[str, int]) -> dict[str, str]:
    """Require detector evidence for the mixed agent-session fixture without raw values."""

    expected_kinds = ("email", "phone", "ssn", "api_key")
    observed_kinds = {key.split(":", 1)[0] for key, count in counts.items() if count > 0}
    missing = [kind for kind in expected_kinds if kind not in observed_kinds]
    if missing:
        raise AssertionError(
            "agent-session detector log did not contain expected protected kinds: "
            f"missing={missing}; observed={sorted(observed_kinds)}"
        )
    return {kind: "detector_log_observed" for kind in expected_kinds}


def assert_provider_forward_route_matches(
    log_db: Path,
    route_case: SmokeRouteCase,
    *,
    expected_count: int,
) -> list[str]:
    messages = provider_forward_messages(log_db)
    if not messages:
        raise AssertionError("activity log did not record any provider_forward_start route lines")
    if len(messages) != expected_count:
        raise AssertionError(
            "activity log did not record one provider_forward_start route line per proof request: "
            f"expected {expected_count}, got {len(messages)}; messages={messages!r}"
        )

    expected_target = f"target={route_case.target_name}"
    expected_provider = f"provider={route_case.provider}"
    if not all(expected_target in message and expected_provider in message for message in messages):
        raise AssertionError(
            "provider_forward_start route line did not match route smoke case: "
            f"expected {expected_target!r} and {expected_provider!r}; messages={messages!r}"
        )
    return messages


def selected_route_cases(route_ids: list[str] | None) -> list[SmokeRouteCase]:
    if not route_ids:
        return list(DEFAULT_ROUTE_CASES)
    by_id = {route.route_id: route for route in DEFAULT_ROUTE_CASES}
    unknown = sorted(set(route_ids) - set(by_id))
    if unknown:
        supported = ", ".join(sorted(by_id))
        raise SmokeBlocked(f"unknown --route value(s) {', '.join(unknown)}; supported: {supported}")
    return [by_id[route_id] for route_id in route_ids]


def upstream_transcript_request_count(upstream: str, *, timeout: float) -> int | None:
    transcript = upstream_transcript(upstream, timeout=timeout)
    if transcript is None:
        return None
    return len(transcript_requests(transcript))


def route_scoped_transcript(upstream: str, baseline: int | None, *, timeout: float) -> dict[str, Any] | None:
    transcript = upstream_transcript(upstream, timeout=timeout)
    if transcript is None or baseline is None:
        return transcript
    return {"requests": transcript_requests(transcript)[baseline:]}


def stop_process(process: subprocess.Popen[str]) -> None:
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def run_route_smoke(
    args: argparse.Namespace,
    *,
    binary: Path,
    upstream: str,
    route_case: SmokeRouteCase,
) -> dict[str, Any]:
    temp_dir = Path(tempfile.mkdtemp(prefix=f"dam-{route_case.route_id}-smoke-"))
    vault_db = temp_dir / "vault.sqlite"
    log_db = temp_dir / "activity.sqlite"
    listen = args.listen
    base_url = f"http://{listen}"
    baseline_transcript_count = upstream_transcript_request_count(upstream, timeout=args.http_timeout)
    command = proxy_command(
        binary=binary,
        listen=listen,
        upstream=upstream,
        vault_db=vault_db,
        log_db=log_db,
        route_case=route_case,
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
        health = wait_for_proxy(base_url, timeout=args.startup_timeout, process=process)
        assert_health_route_matches(health, route_case=route_case, upstream=upstream)
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

        agent_session_log_baseline = max_log_event_id(log_db)
        agent_session_text = response_text(
            post_json(
                f"{base_url}/v1/chat/completions",
                chat_request(agent_session_prompt(), max_tokens=160),
                timeout=args.http_timeout,
            )
        )
        assert_agent_session_response_safe(agent_session_text)

        assert_no_raw_values_in_activity_log(log_db)
        raw_in_log = raw_values_in_file(log_db)
        provider_forward_route_messages = assert_provider_forward_route_matches(
            log_db,
            route_case,
            expected_count=3,
        )
        detector_counts = detector_kind_action_counts(log_db)
        agent_session_forward_id = first_provider_forward_id_after(
            log_db,
            agent_session_log_baseline,
        )
        agent_session_detector_counts = detector_kind_action_counts(
            log_db,
            after_id=agent_session_log_baseline,
            before_id=agent_session_forward_id,
            event_type="redaction",
            action="tokenized",
        )
        agent_session_detector_kinds = assert_agent_session_detector_kinds_observed(
            agent_session_detector_counts
        )
        transcript = route_scoped_transcript(
            upstream,
            baseline_transcript_count,
            timeout=args.http_timeout,
        )
        transcript_paths = assert_upstream_transcript_protected(transcript)
        if transcript is not None:
            agent_session_kinds = assert_agent_session_transcript_protected(transcript)
        else:
            # Normal local OpenAI-compatible upstreams do not expose /__dam/transcript.
            # The detector-log assertion above is mandatory for every route, so the
            # no-transcript path still proves the mixed fixture kinds were protected.
            agent_session_kinds = {
                kind: "not_checked_no_transcript_endpoint"
                for kind in agent_session_detector_kinds
            }

        return {
            "fixture": AGENT_SESSION_FIXTURE_NAME,
            "route_id": route_case.route_id,
            "route_label": route_case.label,
            "target_name": route_case.target_name,
            "target_provider": route_case.provider,
            "upstream": upstream,
            "proxy": base_url,
            "health": health,
            "vault_db": str(vault_db),
            "log_db": str(log_db),
            "log_rows": count_log_rows(log_db),
            "raw_synthetic_values_in_local_activity_log": raw_in_log,
            "raw_leak_scan": "passed",
            "detector_kind_action_counts": detector_counts,
            "agent_session_detector_kind_action_counts": agent_session_detector_counts,
            "agent_session_provider_forward_event_id": agent_session_forward_id,
            "agent_session_detector_kinds_observed": agent_session_detector_kinds,
            "agent_session_kinds_observed": agent_session_kinds,
            "provider_forward_route_messages": provider_forward_route_messages,
            "upstream_transcript_paths": transcript_paths,
            "upstream_transcript_checked": transcript is not None,
            "exact_echo_resolved": True,
            "transformed_token_reference_observed": True,
            "agent_session_response_raw_leak_scan": "passed",
            "cleanup": "kept" if args.keep_temp else "removed",
            "temp_dir": str(temp_dir),
        }
    finally:
        stop_process(process)
        if not args.keep_temp:
            shutil.rmtree(temp_dir, ignore_errors=True)


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

    route_cases = selected_route_cases(args.routes)
    route_results = [
        run_route_smoke(args, binary=binary, upstream=upstream, route_case=route_case)
        for route_case in route_cases
    ]
    return {
        "upstream": upstream,
        "routes_checked": [route.route_id for route in route_cases],
        "route_results": route_results,
        "cleanup": "kept" if args.keep_temp else "removed",
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", default=DEFAULT_UPSTREAM)
    parser.add_argument("--listen", default=DEFAULT_LISTEN)
    parser.add_argument("--binary", default="target/debug/dam-proxy")
    parser.add_argument("--build", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--keep-temp", action="store_true")
    parser.add_argument(
        "--route",
        dest="routes",
        action="append",
        choices=[route.route_id for route in DEFAULT_ROUTE_CASES],
        help="Representative MVP route ID to exercise; repeat to select a subset. Defaults to the route matrix.",
    )
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
