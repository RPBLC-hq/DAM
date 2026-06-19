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
    def test_help_documents_agent_protection_and_visible_evidence_smoke_commands(self):
        result = subprocess.run(
            [str(BUILD_SCRIPT), "--help"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        )

        self.assertIn("agent-protection-smoke", result.stdout)
        self.assertIn("agent-visible-evidence-smoke", result.stdout)
        self.assertIn("agent-recovery-smoke", result.stdout)
        self.assertIn("agent-repair-smoke", result.stdout)
        self.assertIn("DAM_AGENT_E2E_UPSTREAM", result.stdout)
        self.assertIn("DAM_AGENT_E2E_BINARY", result.stdout)
        self.assertIn("DAM_AGENT_VISIBLE_EVIDENCE_SMOKE_SCRIPT", result.stdout)
        self.assertIn("DAM_AGENT_E2E_WEB_BINARY", result.stdout)
        self.assertIn("DAM_AGENT_E2E_BUILD", result.stdout)
        self.assertIn("DAM_AGENT_E2E_KEEP_TEMP", result.stdout)
        self.assertIn("DAM_AGENT_STATE_DIR", result.stdout)

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

    def test_agent_visible_evidence_smoke_invokes_local_script_with_safe_defaults(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "argv.txt"
            stub_path = Path(temp_dir) / "visible_smoke_stub.py"
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
                    "DAM_AGENT_VISIBLE_EVIDENCE_SMOKE_SCRIPT": str(stub_path),
                    "DAM_AGENT_E2E_LISTEN": "127.0.0.1:17831",
                    "DAM_AGENT_E2E_STARTUP_TIMEOUT": "7",
                    "DAM_AGENT_E2E_HTTP_TIMEOUT": "11",
                }
            )
            subprocess.run(
                [str(BUILD_SCRIPT), "agent-visible-evidence-smoke"],
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
                    "--listen",
                    "127.0.0.1:17831",
                    "--startup-timeout",
                    "7",
                    "--http-timeout",
                    "11",
                    "--binary",
                    str(ROOT / "target" / "debug" / "dam-proxy"),
                    "--web-binary",
                    str(ROOT / "target" / "debug" / "dam-web"),
                ],
            )

    def test_agent_status_rejects_invalid_setup_probe_modes_before_macos_checks(self):
        result = subprocess.run(
            [
                str(BUILD_SCRIPT),
                "agent-status",
                "--network-mode",
                "wireguard",
                "--trust-mode",
                "magic_ca",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        self.assertEqual(2, result.returncode, result.stdout + result.stderr)
        self.assertIn("invalid agent network mode: wireguard", result.stderr)
        self.assertIn("expected explicit_proxy, system_proxy, or tun", result.stderr)
        self.assertIn("invalid agent trust mode: magic_ca", result.stderr)
        self.assertIn("expected disabled or local_ca", result.stderr)
        self.assertNotIn("macOS packaging/notarization requires Darwin", result.stderr)

    def test_agent_status_strict_fails_when_doctor_probe_fails(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            app_dir = temp_path / "DAM.app"
            dam_bin = app_dir / "Contents" / "MacOS" / "dam"
            bin_dir.mkdir()
            dam_bin.parent.mkdir(parents=True)

            for command in ["uname", "pgrep", "codesign", "xcrun", "spctl"]:
                script = bin_dir / command
                if command == "uname":
                    script.write_text("#!/usr/bin/env sh\nprintf 'Darwin\\n'\n", encoding="utf-8")
                elif command == "pgrep":
                    script.write_text("#!/usr/bin/env sh\nexit 1\n", encoding="utf-8")
                else:
                    script.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
                script.chmod(0o755)

            dam_bin.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env sh
                    if [ "$1" = "doctor" ]; then
                      exit 7
                    fi
                    exit 0
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            dam_bin.chmod(0o755)

            env = os.environ.copy()
            env.update(
                {
                    "DAM_INSTALL_DIR": str(temp_path),
                    "DAM_SIGN_MODE": "development",
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                }
            )
            result = subprocess.run(
                [str(BUILD_SCRIPT), "agent-status", "--strict-status"],
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(1, result.returncode, result.stdout + result.stderr)
            self.assertIn("status_probe_failures: 1", result.stdout)
            self.assertIn("doctor", result.stdout)

    def test_agent_status_runs_export_diagnostics_probe(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            app_dir = temp_path / "DAM.app"
            dam_bin = app_dir / "Contents" / "MacOS" / "dam"
            calls_path = temp_path / "dam-calls.txt"
            state_dir = temp_path / "state"
            bin_dir.mkdir()
            dam_bin.parent.mkdir(parents=True)

            for command in ["uname", "pgrep", "codesign", "xcrun", "spctl"]:
                script = bin_dir / command
                if command == "uname":
                    script.write_text("#!/usr/bin/env sh\nprintf 'Darwin\\n'\n", encoding="utf-8")
                elif command == "pgrep":
                    script.write_text("#!/usr/bin/env sh\nexit 1\n", encoding="utf-8")
                else:
                    script.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
                script.chmod(0o755)

            dam_bin.write_text(
                textwrap.dedent(
                    f"""
                    #!/usr/bin/env sh
                    printf '%s\\n' "$*" >> {str(calls_path)!r}
                    exit 0
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            dam_bin.chmod(0o755)

            env = os.environ.copy()
            env.update(
                {
                    "DAM_INSTALL_DIR": str(temp_path),
                    "DAM_SIGN_MODE": "development",
                    "DAM_AGENT_STATE_DIR": str(state_dir),
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                }
            )
            subprocess.run(
                [
                    str(BUILD_SCRIPT),
                    "agent-status",
                    "--network-mode",
                    "tun",
                    "--trust-mode",
                    "local_ca",
                ],
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=True,
            )

            self.assertIn(
                f"setup export-diagnostics --network-mode tun --trust-mode local_ca --state-dir {state_dir} --json",
                calls_path.read_text(encoding="utf-8").splitlines(),
            )

    def test_agent_recovery_smoke_runs_read_only_installed_recovery_probes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            app_dir = temp_path / "DAM.app"
            dam_bin = app_dir / "Contents" / "MacOS" / "dam"
            calls_path = temp_path / "dam-calls.txt"
            state_dir = temp_path / "state"
            bin_dir.mkdir()
            dam_bin.parent.mkdir(parents=True)

            uname = bin_dir / "uname"
            uname.write_text("#!/usr/bin/env sh\nprintf 'Darwin\\n'\n", encoding="utf-8")
            uname.chmod(0o755)
            dam_bin.write_text(
                textwrap.dedent(
                    f"""
                    #!/usr/bin/env sh
                    printf '%s\\n' "$*" >> {str(calls_path)!r}
                    exit 0
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            dam_bin.chmod(0o755)

            env = os.environ.copy()
            env.update(
                {
                    "DAM_INSTALL_DIR": str(temp_path),
                    "DAM_AGENT_STATE_DIR": str(state_dir),
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                }
            )
            subprocess.run(
                [
                    str(BUILD_SCRIPT),
                    "agent-recovery-smoke",
                    "--network-mode",
                    "tun",
                    "--trust-mode",
                    "local_ca",
                ],
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=True,
            )

            self.assertEqual(
                calls_path.read_text(encoding="utf-8").splitlines(),
                [
                    f"setup rescue --dry-run --state-dir {state_dir} --json",
                    f"setup repair --dry-run --network-mode tun --trust-mode local_ca --state-dir {state_dir} --json",
                    f"setup export-diagnostics --network-mode tun --trust-mode local_ca --state-dir {state_dir} --json",
                ],
            )

    def test_agent_repair_smoke_requires_explicit_mutation_confirmation_before_macos_checks(self):
        result = subprocess.run(
            [str(BUILD_SCRIPT), "agent-repair-smoke"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        self.assertEqual(2, result.returncode, result.stdout + result.stderr)
        self.assertIn("agent-repair-smoke mutates installed DAM setup", result.stderr)
        self.assertNotIn("macOS packaging/notarization requires Darwin", result.stderr)

    def test_agent_repair_smoke_runs_mutating_installed_recovery_probes_when_confirmed(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            app_dir = temp_path / "DAM.app"
            dam_bin = app_dir / "Contents" / "MacOS" / "dam"
            calls_path = temp_path / "dam-calls.txt"
            state_dir = temp_path / "state"
            bin_dir.mkdir()
            dam_bin.parent.mkdir(parents=True)

            uname = bin_dir / "uname"
            uname.write_text("#!/usr/bin/env sh\nprintf 'Darwin\\n'\n", encoding="utf-8")
            uname.chmod(0o755)
            dam_bin.write_text(
                textwrap.dedent(
                    f"""
                    #!/usr/bin/env sh
                    printf '%s\\n' "$*" >> {str(calls_path)!r}
                    exit 0
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            dam_bin.chmod(0o755)

            env = os.environ.copy()
            env.update(
                {
                    "DAM_INSTALL_DIR": str(temp_path),
                    "DAM_AGENT_STATE_DIR": str(state_dir),
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                }
            )
            subprocess.run(
                [
                    str(BUILD_SCRIPT),
                    "agent-repair-smoke",
                    "--network-mode",
                    "tun",
                    "--trust-mode",
                    "local_ca",
                    "--confirm-mutation",
                ],
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=True,
            )

            self.assertEqual(
                calls_path.read_text(encoding="utf-8").splitlines(),
                [
                    f"setup rescue --yes --state-dir {state_dir} --json",
                    f"setup repair --yes --network-mode tun --trust-mode local_ca --state-dir {state_dir} --json",
                    f"setup status --network-mode tun --trust-mode local_ca --state-dir {state_dir} --json",
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
