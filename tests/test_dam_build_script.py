import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BUILD_SCRIPT = ROOT / "scripts" / "dam-build.sh"


class DamBuildScriptTests(unittest.TestCase):
    def test_help_documents_agent_protection_smoke_command(self):
        result = subprocess.run(
            [str(BUILD_SCRIPT), "--help"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        )

        self.assertIn("agent-protection-smoke", result.stdout)
        self.assertIn("DAM_AGENT_E2E_UPSTREAM", result.stdout)
        self.assertIn("DAM_AGENT_E2E_BINARY", result.stdout)
        self.assertIn("DAM_AGENT_E2E_BUILD", result.stdout)
        self.assertIn("DAM_AGENT_E2E_KEEP_TEMP", result.stdout)

    def test_agent_protection_smoke_invokes_local_smoke_script_with_safe_defaults(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "argv.txt"
            stub_path = Path(temp_dir) / "smoke_stub.py"
            stub_path.write_text(
                textwrap.dedent(
                    f"""
                    import pathlib
                    import sys
                    pathlib.Path({str(output_path)!r}).write_text("\\n".join(sys.argv[1:]), encoding="utf-8")
                    raise SystemExit(0)
                    """
                ).lstrip(),
                encoding="utf-8",
            )

            env = os.environ.copy()
            env.update(
                {
                    "DAM_AGENT_E2E_SMOKE_SCRIPT": str(stub_path),
                    "DAM_AGENT_E2E_UPSTREAM": "http://127.0.0.1:18080",
                    "DAM_AGENT_E2E_LISTEN": "127.0.0.1:17831",
                    "DAM_AGENT_E2E_STARTUP_TIMEOUT": "7",
                    "DAM_AGENT_E2E_HTTP_TIMEOUT": "11",
                }
            )
            subprocess.run(
                [str(BUILD_SCRIPT), "agent-protection-smoke"],
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=True,
            )

            argv = output_path.read_text(encoding="utf-8").splitlines()
            self.assertEqual(
                argv,
                [
                    "--upstream",
                    "http://127.0.0.1:18080",
                    "--listen",
                    "127.0.0.1:17831",
                    "--startup-timeout",
                    "7",
                    "--http-timeout",
                    "11",
                    "--binary",
                    str(ROOT / "target" / "debug" / "dam-proxy"),
                ],
            )

    def test_agent_protection_smoke_passes_debug_options_from_environment(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "argv.txt"
            stub_path = Path(temp_dir) / "smoke_stub.py"
            binary_path = Path(temp_dir) / "dam-proxy"
            stub_path.write_text(
                textwrap.dedent(
                    f"""
                    import pathlib
                    import sys
                    pathlib.Path({str(output_path)!r}).write_text("\\n".join(sys.argv[1:]), encoding="utf-8")
                    raise SystemExit(0)
                    """
                ).lstrip(),
                encoding="utf-8",
            )

            env = os.environ.copy()
            env.update(
                {
                    "DAM_AGENT_E2E_SMOKE_SCRIPT": str(stub_path),
                    "DAM_AGENT_E2E_BINARY": str(binary_path),
                    "DAM_AGENT_E2E_BUILD": "0",
                    "DAM_AGENT_E2E_KEEP_TEMP": "1",
                }
            )
            subprocess.run(
                [str(BUILD_SCRIPT), "agent-protection-smoke"],
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=True,
            )

            argv = output_path.read_text(encoding="utf-8").splitlines()
            self.assertIn("--binary", argv)
            self.assertIn(str(binary_path), argv)
            self.assertIn("--no-build", argv)
            self.assertIn("--keep-temp", argv)


if __name__ == "__main__":
    unittest.main()
