import importlib.util
import sqlite3
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "rpblc_dam_local_llm_e2e_smoke.py"


def load_module():
    spec = importlib.util.spec_from_file_location("rpblc_dam_local_llm_e2e_smoke", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class LocalLlmE2eSmokeScriptTests(unittest.TestCase):
    def test_proxy_command_uses_loopback_temp_stores_and_no_api_key_env(self):
        smoke = load_module()

        command = smoke.proxy_command(
            binary=Path("target/debug/dam-proxy"),
            listen="127.0.0.1:7831",
            upstream="http://127.0.0.1:8080",
            vault_db=Path("/tmp/dam-smoke/vault.sqlite"),
            log_db=Path("/tmp/dam-smoke/log.sqlite"),
        )

        self.assertEqual(
            command,
            [
                "target/debug/dam-proxy",
                "--listen",
                "127.0.0.1:7831",
                "--upstream",
                "http://127.0.0.1:8080",
                "--target-name",
                "openai",
                "--provider",
                "openai-compatible",
                "--resolve-inbound",
                "--no-api-key-env",
                "--db",
                "/tmp/dam-smoke/vault.sqlite",
                "--log",
                "/tmp/dam-smoke/log.sqlite",
            ],
        )

    def test_proxy_command_uses_profile_target_name_and_records_route_id_separately(self):
        smoke = load_module()

        expected_target_names = {
            "openai-api": "openai",
            "anthropic-api": "anthropic",
            "claude-web": "claude-web",
            "anthropic-console": "anthropic-console",
            "claude-mcp-proxy": "claude-mcp-proxy",
            "claude-platform": "claude-platform",
            "openai-platform": "openai-platform",
        }
        for route_case in smoke.DEFAULT_ROUTE_CASES:
            with self.subTest(route_id=route_case.route_id):
                command = smoke.proxy_command(
                    binary=Path("target/debug/dam-proxy"),
                    listen="127.0.0.1:7831",
                    upstream="http://127.0.0.1:8080",
                    vault_db=Path("/tmp/dam-smoke/vault.sqlite"),
                    log_db=Path("/tmp/dam-smoke/log.sqlite"),
                    route_case=route_case,
                )
                self.assertEqual(command[command.index("--target-name") + 1], route_case.target_name)
                self.assertEqual(route_case.target_name, expected_target_names[route_case.route_id])
                self.assertEqual(command[command.index("--provider") + 1], route_case.provider)

        self.assertEqual(
            [route.route_id for route in smoke.DEFAULT_ROUTE_CASES],
            [
                "openai-api",
                "anthropic-api",
                "claude-web",
                "anthropic-console",
                "claude-mcp-proxy",
                "claude-platform",
                "openai-platform",
            ],
        )

    def test_prompts_are_deterministic_and_contain_synthetic_values_only(self):
        smoke = load_module()

        exact_prompt = smoke.exact_echo_prompt()
        transform_prompt = smoke.transform_token_prompt()
        agent_prompt = smoke.agent_session_prompt()
        request = smoke.chat_request(exact_prompt, max_tokens=48)

        self.assertEqual(request["model"], "local")
        self.assertEqual(request["temperature"], 0)
        self.assertEqual(request["max_tokens"], 48)
        self.assertEqual(request["messages"][-1]["content"], exact_prompt)
        serialized = smoke.json.dumps([exact_prompt, transform_prompt, agent_prompt, request])
        self.assertIn(smoke.SYNTHETIC_EMAIL, serialized)
        self.assertIn(smoke.SYNTHETIC_PHONE, serialized)
        self.assertIn(smoke.SYNTHETIC_SSN, serialized)
        self.assertIn(smoke.SYNTHETIC_ENV_SECRET, serialized)
        self.assertIn(smoke.synthetic_github_token(), serialized)
        self.assertIn(smoke.AGENT_SESSION_FIXTURE_NAME, agent_prompt)
        self.assertIn("OPENAI_API_KEY=", agent_prompt)
        self.assertIn("GITHUB_TOKEN=", agent_prompt)
        self.assertIn("one space after the opening bracket", transform_prompt)
        self.assertIn("[ email:abc]", transform_prompt)
        self.assertNotIn("every character separated", transform_prompt)
        self.assertNotIn("api.openai.com", serialized)

    def test_response_text_extracts_openai_compatible_content(self):
        smoke = load_module()

        data = {
            "choices": [
                {"message": {"content": "resolved text"}},
                {"message": {"content": "ignored"}},
            ]
        }

        self.assertEqual(smoke.response_text(data), "resolved text")

    def test_count_log_rows_reads_dam_log_events_table(self):
        smoke = load_module()

        with tempfile.TemporaryDirectory() as temp_dir:
            db_path = Path(temp_dir) / "activity.sqlite"
            with sqlite3.connect(db_path) as connection:
                connection.execute("create table log_events (id integer primary key)")
                connection.executemany("insert into log_events default values", [(), (), ()])

            self.assertEqual(smoke.count_log_rows(db_path), 3)

    def test_health_route_assertion_fails_when_proxy_reports_different_target(self):
        smoke = load_module()
        route_case = smoke.DEFAULT_ROUTE_CASES[1]

        smoke.assert_health_route_matches(
            {"target": "anthropic", "upstream": "http://127.0.0.1:18080/"},
            route_case=route_case,
            upstream="http://127.0.0.1:18080",
        )

        with self.assertRaisesRegex(AssertionError, "health target"):
            smoke.assert_health_route_matches(
                {"target": "openai", "upstream": "http://127.0.0.1:18080"},
                route_case=route_case,
                upstream="http://127.0.0.1:18080",
            )

    def test_provider_forward_route_assertion_uses_actual_activity_route_line(self):
        smoke = load_module()
        route_case = smoke.DEFAULT_ROUTE_CASES[2]

        with tempfile.TemporaryDirectory() as temp_dir:
            db_path = Path(temp_dir) / "activity.sqlite"
            with sqlite3.connect(db_path) as connection:
                connection.execute(
                    "create table log_events (id integer primary key, action text, message text)"
                )
                connection.executemany(
                    "insert into log_events (action, message) values (?, ?)",
                    [
                        (
                            "provider_forward_start",
                            "provider forward start target=claude-web provider=generic-http resolve_inbound=true transform_streaming=true",
                        ),
                        (
                            "provider_forward_start",
                            "provider forward start target=claude-web provider=generic-http resolve_inbound=true transform_streaming=true",
                        ),
                    ],
                )

            self.assertEqual(
                len(smoke.assert_provider_forward_route_matches(db_path, route_case, expected_count=2)),
                2,
            )

            with self.assertRaisesRegex(AssertionError, "one provider_forward_start"):
                smoke.assert_provider_forward_route_matches(db_path, route_case, expected_count=3)

            with sqlite3.connect(db_path) as connection:
                connection.execute(
                    "update log_events set message = ? where id = 2",
                    ("provider forward start target=openai provider=openai-compatible",),
                )

            with self.assertRaisesRegex(AssertionError, "provider_forward_start route line"):
                smoke.assert_provider_forward_route_matches(db_path, route_case, expected_count=2)

    def test_wait_for_proxy_reports_early_process_exit_stderr(self):
        smoke = load_module()
        process = smoke.subprocess.Popen(
            [
                smoke.sys.executable,
                "-c",
                "import sys; print('bind failed: address in use', file=sys.stderr); sys.exit(42)",
            ],
            stdout=smoke.subprocess.PIPE,
            stderr=smoke.subprocess.PIPE,
            text=True,
        )
        process.wait(timeout=5)

        with self.assertRaisesRegex(smoke.SmokeBlocked, "address in use"):
            smoke.wait_for_proxy("http://127.0.0.1:1", timeout=1, process=process)

    def test_activity_log_assertion_fails_closed_on_raw_synthetic_values(self):
        smoke = load_module()

        with tempfile.TemporaryDirectory() as temp_dir:
            db_path = Path(temp_dir) / "activity.sqlite"
            db_path.write_text(f"raw {smoke.SYNTHETIC_EMAIL}", encoding="utf-8")

            with self.assertRaisesRegex(AssertionError, "activity log leaked raw synthetic values"):
                smoke.assert_no_raw_values_in_activity_log(db_path)

    def test_assertions_distinguish_resolved_and_transformed_token_outputs(self):
        smoke = load_module()

        smoke.assert_exact_echo_resolved(
            f"client sees {smoke.SYNTHETIC_EMAIL} and {smoke.SYNTHETIC_SSN}"
        )
        smoke.assert_transformed_token_only("model saw [ email:abc123] and [ secret:def456]")

        with self.assertRaisesRegex(AssertionError, smoke.SYNTHETIC_EMAIL):
            smoke.assert_exact_echo_resolved("client sees only [email:abc123]")

        with self.assertRaisesRegex(AssertionError, "raw synthetic value") as context:
            smoke.assert_transformed_token_only(f"leaked {smoke.SYNTHETIC_EMAIL}")
        self.assertNotIn(smoke.SYNTHETIC_EMAIL, str(context.exception))

    def test_transformed_token_assertion_rejects_whitespace_obfuscated_raw_values(self):
        smoke = load_module()

        obfuscated_email = " \n ".join(smoke.SYNTHETIC_EMAIL)
        obfuscated_ssn = "\t".join(smoke.SYNTHETIC_SSN)

        with self.assertRaisesRegex(AssertionError, "raw synthetic value"):
            smoke.assert_transformed_token_only(obfuscated_email)
        with self.assertRaisesRegex(AssertionError, "raw synthetic value"):
            smoke.assert_transformed_token_only(obfuscated_ssn)

    def test_transformed_token_assertion_requires_dam_reference_kind(self):
        smoke = load_module()

        smoke.assert_transformed_token_only("model saw [ e m a i l : a b c 1 2 3 ]")

        with self.assertRaisesRegex(AssertionError, "DAM token"):
            smoke.assert_transformed_token_only("model saw [ not-a-dam-reference ]")

    def test_upstream_transcript_assertion_requires_payload_position_tokens_without_raw_values(self):
        smoke = load_module()
        paths = smoke.assert_upstream_transcript_protected(
            {
                "requests": [
                    {
                        "path": "/v1/chat/completions",
                        "body": '{"content":"alpha=[email:abc]; beta=[ssn:def]"}',
                        "user_content": "alpha=[email:abc]; beta=[ssn:def]",
                    }
                ]
            }
        )

        self.assertEqual(paths, ["/v1/chat/completions"])
        with self.assertRaisesRegex(AssertionError, "raw synthetic value"):
            smoke.assert_upstream_transcript_protected(
                {"requests": [{"body": f"leaked {smoke.SYNTHETIC_EMAIL} [ssn:def]"}]}
            )
        with self.assertRaisesRegex(AssertionError, "payload positions"):
            smoke.assert_upstream_transcript_protected({"requests": [{"body": "redacted text only"}]})
        with self.assertRaisesRegex(AssertionError, "payload positions"):
            smoke.assert_upstream_transcript_protected(
                {
                    "requests": [
                        {
                            "body": "instructions mention [email:abc] and [ssn:def]",
                            "user_content": "alpha was dropped; beta was dropped",
                        }
                    ]
                }
            )

    def test_agent_session_transcript_requires_mixed_kind_protection_without_raw_values(self):
        smoke = load_module()
        transcript = {
            "requests": [
                {
                    "path": "/v1/chat/completions",
                    "body": smoke.json.dumps(
                        {
                            "content": "\n".join(
                                [
                                    f"Fixture: {smoke.AGENT_SESSION_FIXTURE_NAME}",
                                    "email=[email:abc] phone=[phone:def] ssn=[ssn:ghi]",
                                    "OPENAI_API_KEY=[api_key:jkl] GITHUB_TOKEN=[api_key:mno]",
                                ]
                            )
                        }
                    ),
                    "user_content": "",
                }
            ]
        }

        self.assertEqual(
            smoke.assert_agent_session_transcript_protected(transcript),
            {
                "email": "reference_or_redaction_observed",
                "phone": "reference_or_redaction_observed",
                "ssn": "reference_or_redaction_observed",
                "api_key": "reference_or_redaction_observed",
            },
        )
        self.assertEqual(smoke.assert_agent_session_transcript_protected(None), {})
        with self.assertRaisesRegex(AssertionError, "raw synthetic value") as context:
            smoke.assert_agent_session_transcript_protected(
                {
                    "requests": [
                        {
                            "body": f"Fixture: {smoke.AGENT_SESSION_FIXTURE_NAME} leaked {smoke.SYNTHETIC_ENV_SECRET}",
                            "user_content": "",
                        }
                    ]
                }
            )
        self.assertNotIn(smoke.SYNTHETIC_ENV_SECRET, str(context.exception))
        with self.assertRaisesRegex(AssertionError, "expected DAM references"):
            smoke.assert_agent_session_transcript_protected(
                {
                    "requests": [
                        {
                            "body": f"Fixture: {smoke.AGENT_SESSION_FIXTURE_NAME} email=[email:abc]",
                            "user_content": "",
                        }
                    ]
                }
            )

    def test_detector_kind_counts_can_scope_to_agent_session_rows(self):
        smoke = load_module()

        with tempfile.TemporaryDirectory() as temp_dir:
            db_path = Path(temp_dir) / "activity.sqlite"
            with sqlite3.connect(db_path) as connection:
                connection.execute(
                    "create table log_events (id integer primary key, event_type text, kind text, action text)"
                )
                connection.executemany(
                    "insert into log_events (event_type, kind, action) values (?, ?, ?)",
                    [
                        ("redaction", "email", "tokenized"),
                        ("redaction", "ssn", "tokenized"),
                        ("redaction", "phone", "tokenized"),
                        ("redaction", "api_key", "tokenized"),
                        ("proxy_forward", None, "provider_forward_start"),
                        ("redaction", "email", "tokenized"),
                    ],
                )
                baseline = 2

            self.assertEqual(smoke.max_log_event_id(db_path), 6)
            self.assertEqual(smoke.first_provider_forward_id_after(db_path, baseline), 5)
            self.assertEqual(
                smoke.detector_kind_action_counts(
                    db_path,
                    after_id=baseline,
                    before_id=5,
                    event_type="redaction",
                    action="tokenized",
                ),
                {"api_key:tokenized": 1, "phone:tokenized": 1},
            )

    def test_agent_session_detector_kind_assertion_requires_all_mixed_fixture_kinds(self):
        smoke = load_module()

        self.assertEqual(
            smoke.assert_agent_session_detector_kinds_observed(
                {
                    "email:tokenize": 1,
                    "phone:tokenize": 1,
                    "ssn:tokenize": 1,
                    "api_key:tokenize": 2,
                }
            ),
            {
                "email": "detector_log_observed",
                "phone": "detector_log_observed",
                "ssn": "detector_log_observed",
                "api_key": "detector_log_observed",
            },
        )
        with self.assertRaisesRegex(AssertionError, "missing=\['phone'\]"):
            smoke.assert_agent_session_detector_kinds_observed(
                {"email:tokenize": 1, "ssn:tokenize": 1, "api_key:tokenize": 2}
            )

    def test_route_selection_defaults_to_representative_mvp_matrix(self):
        smoke = load_module()

        self.assertEqual(
            [route.route_id for route in smoke.selected_route_cases(None)],
            [
                "openai-api",
                "anthropic-api",
                "claude-web",
                "anthropic-console",
                "claude-mcp-proxy",
                "claude-platform",
                "openai-platform",
            ],
        )
        self.assertEqual(
            [route.route_id for route in smoke.selected_route_cases(["anthropic-api"])],
            ["anthropic-api"],
        )
        with self.assertRaisesRegex(smoke.SmokeBlocked, "unknown --route"):
            smoke.selected_route_cases(["not-a-route"])

    def test_route_scoped_transcript_ignores_prior_requests(self):
        smoke = load_module()
        original_upstream_transcript = smoke.__dict__["upstream_transcript"]
        try:
            smoke.__dict__["upstream_transcript"] = lambda upstream, *, timeout: {
                "requests": [
                    {"path": "/old", "body": "raw old request"},
                    {
                        "path": "/new",
                        "body": '{"content":"alpha=[email:abc]; beta=[ssn:def]"}',
                        "user_content": "alpha=[email:abc]; beta=[ssn:def]",
                    },
                ]
            }
            scoped = smoke.route_scoped_transcript("http://127.0.0.1:18080", 1, timeout=1)
            self.assertEqual(smoke.assert_upstream_transcript_protected(scoped), ["/new"])
        finally:
            smoke.__dict__["upstream_transcript"] = original_upstream_transcript

    def test_upstream_transcript_missing_endpoint_is_optional_but_malformed_endpoint_fails_closed(self):
        smoke = load_module()

        class Http404(smoke.urllib.error.HTTPError):
            def __init__(self):
                super().__init__("http://127.0.0.1/__dam/transcript", 404, "not found", {}, None)

        def missing_json(url, *, timeout):
            raise Http404()

        def malformed_json(url, *, timeout):
            raise smoke.json.JSONDecodeError("bad json", "not-json", 0)

        original_get_json = smoke.__dict__["get_json"]
        try:
            smoke.__dict__["get_json"] = missing_json
            self.assertIsNone(smoke.upstream_transcript("http://127.0.0.1", timeout=1))
            smoke.__dict__["get_json"] = malformed_json
            with self.assertRaisesRegex(AssertionError, "unreadable"):
                smoke.upstream_transcript("http://127.0.0.1", timeout=1)
        finally:
            smoke.__dict__["get_json"] = original_get_json


if __name__ == "__main__":
    unittest.main()
