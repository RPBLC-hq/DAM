import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "dam_vps_dogfood_verify.py"


def load_module():
    spec = importlib.util.spec_from_file_location("dam_vps_dogfood_verify", SCRIPT)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class DamVpsDogfoodVerifyScriptTests(unittest.TestCase):
    def test_runtime_paths_use_canonical_dam_hermes_filenames(self):
        verify = load_module()

        with tempfile.TemporaryDirectory() as temp_dir:
            paths = verify.runtime_paths(Path(temp_dir))

        self.assertEqual(paths.state_dir, Path(temp_dir))
        self.assertEqual(paths.vault_db, Path(temp_dir) / "vault.db")
        self.assertEqual(paths.log_db, Path(temp_dir) / "log.db")
        self.assertEqual(paths.consent_db, Path(temp_dir) / "consent.db")

    def test_proxy_command_uses_loopback_state_and_no_api_key_env(self):
        verify = load_module()
        paths = verify.runtime_paths(Path("/tmp/dam-hermes"))

        command = verify.proxy_command(
            binary=Path("target/debug/dam-proxy"),
            listen="127.0.0.1:7828",
            upstream="http://127.0.0.1:18080",
            paths=paths,
        )

        self.assertEqual(
            command,
            [
                "target/debug/dam-proxy",
                "--listen",
                "127.0.0.1:7828",
                "--upstream",
                "http://127.0.0.1:18080",
                "--provider",
                "openai-compatible",
                "--resolve-inbound",
                "--no-api-key-env",
                "--db",
                str(paths.vault_db),
                "--log",
                str(paths.log_db),
            ],
        )

    def test_web_command_reuses_same_state_dbs_on_isolated_loopback_port(self):
        verify = load_module()
        paths = verify.runtime_paths(Path("/tmp/dam-hermes"))

        command = verify.web_command(
            binary=Path("target/debug/dam-web"),
            addr="127.0.0.1:2896",
            paths=paths,
        )

        self.assertEqual(
            command,
            [
                "target/debug/dam-web",
                "--addr",
                "127.0.0.1:2896",
                "--db",
                "/tmp/dam-hermes/vault.db",
                "--log",
                "/tmp/dam-hermes/log.db",
                "--consent-db",
                "/tmp/dam-hermes/consent.db",
            ],
        )

    def test_proxy_env_routes_http_clients_through_loopback_dam(self):
        verify = load_module()

        env = verify.proxy_env("127.0.0.1:7828")

        self.assertEqual(env["HTTP_PROXY"], "http://127.0.0.1:7828")
        self.assertEqual(env["HTTPS_PROXY"], "http://127.0.0.1:7828")
        self.assertEqual(env["ALL_PROXY"], "http://127.0.0.1:7828")
        self.assertIn("127.0.0.1", env["NO_PROXY"])
        self.assertIn("localhost", env["NO_PROXY"])

    def test_pending_request_payload_is_synthetic_and_deterministic(self):
        verify = load_module()

        payload = verify.pending_request_payload()

        self.assertEqual(payload["actor"], "codex")
        self.assertEqual(payload["value_label"], "synthetic email")
        self.assertIn("example.test", payload["value_preview"])
        self.assertIn("purpose", payload)
        self.assertEqual(payload["expires_in_sec"], 600)


if __name__ == "__main__":
    unittest.main()
