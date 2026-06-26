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

    def test_prompts_are_deterministic_and_contain_synthetic_values_only(self):
        smoke = load_module()

        exact_prompt = smoke.exact_echo_prompt()
        transform_prompt = smoke.transform_token_prompt()
        request = smoke.chat_request(exact_prompt, max_tokens=48)

        self.assertEqual(request["model"], "local")
        self.assertEqual(request["temperature"], 0)
        self.assertEqual(request["max_tokens"], 48)
        self.assertEqual(request["messages"][-1]["content"], exact_prompt)
        serialized = smoke.json.dumps([exact_prompt, transform_prompt, request])
        self.assertIn(smoke.SYNTHETIC_EMAIL, serialized)
        self.assertIn(smoke.SYNTHETIC_SSN, serialized)
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

        with self.assertRaisesRegex(AssertionError, "raw synthetic values"):
            smoke.assert_transformed_token_only(f"leaked {smoke.SYNTHETIC_EMAIL}")

    def test_transformed_token_assertion_rejects_whitespace_obfuscated_raw_values(self):
        smoke = load_module()

        obfuscated_email = " \n ".join(smoke.SYNTHETIC_EMAIL)
        obfuscated_ssn = "\t".join(smoke.SYNTHETIC_SSN)

        with self.assertRaisesRegex(AssertionError, "raw synthetic values"):
            smoke.assert_transformed_token_only(obfuscated_email)
        with self.assertRaisesRegex(AssertionError, "raw synthetic values"):
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
        with self.assertRaisesRegex(AssertionError, "raw synthetic values"):
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
