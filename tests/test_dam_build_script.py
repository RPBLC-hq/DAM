import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BUILD_SCRIPT = ROOT / "scripts" / "dam-build.sh"
VISIBLE_EVIDENCE_SMOKE_SCRIPT = ROOT / "scripts" / "rpblc_dam_visible_evidence_smoke.py"


spec = importlib.util.spec_from_file_location(
    "rpblc_dam_visible_evidence_smoke", VISIBLE_EVIDENCE_SMOKE_SCRIPT
)
assert spec is not None and spec.loader is not None
visible_evidence_smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(visible_evidence_smoke)


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
        self.assertIn("agent-mvp-readiness", result.stdout)
        self.assertIn("agent-visible-evidence-smoke", result.stdout)
        self.assertIn("agent-websocket-smoke", result.stdout)
        self.assertIn("agent-dogfood-verify", result.stdout)
        self.assertIn("agent-recovery-smoke", result.stdout)
        self.assertIn("agent-repair-smoke", result.stdout)
        self.assertIn("DAM_AGENT_E2E_UPSTREAM", result.stdout)
        self.assertIn("DAM_AGENT_E2E_BINARY", result.stdout)
        self.assertIn("DAM_AGENT_VISIBLE_EVIDENCE_SMOKE_SCRIPT", result.stdout)
        self.assertIn("DAM_AGENT_E2E_WEB_BINARY", result.stdout)
        self.assertIn("DAM_AGENT_E2E_WEB_ADDR", result.stdout)
        self.assertIn("DAM_AGENT_E2E_BUILD", result.stdout)
        self.assertIn("DAM_AGENT_E2E_KEEP_TEMP", result.stdout)
        self.assertIn("DAM_AGENT_E2E_WEB_ADDR", result.stdout)
        self.assertIn("DAM_AGENT_E2E_VERIFY_SCRIPT", result.stdout)
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
                    "DAM_AGENT_E2E_WEB_ADDR": "127.0.0.1:12896",
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
                    "--web-addr",
                    "127.0.0.1:12896",
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

    def test_agent_websocket_smoke_invokes_focused_loopback_route_test(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "argv.txt"
            cargo_stub = Path(temp_dir) / "cargo"
            cargo_stub.write_text(
                textwrap.dedent(
                    f"""
                    #!/usr/bin/env python3
                    import pathlib
                    import sys
                    pathlib.Path({str(output_path)!r}).write_text("\\n".join(sys.argv[1:]), encoding="utf-8")
                    raise SystemExit(0)
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            cargo_stub.chmod(0o755)
            env = os.environ.copy()
            env["PATH"] = f"{temp_dir}{os.pathsep}{env['PATH']}"

            subprocess.run(
                [str(BUILD_SCRIPT), "agent-websocket-smoke"],
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=True,
            )

            self.assertEqual(
                output_path.read_text(encoding="utf-8").splitlines(),
                [
                    "test",
                    "-q",
                    "-p",
                    "dam-proxy",
                    "transparent_chatgpt_websocket_route_protects_outbound_text_frames",
                    "--",
                    "--nocapture",
                ],
            )

    def test_agent_dogfood_verify_invokes_vps_verifier_with_shared_proxy_web_args(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "argv.txt"
            stub_path = Path(temp_dir) / "verify_stub.py"
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
                    "DAM_AGENT_E2E_VERIFY_SCRIPT": str(stub_path),
                    "DAM_AGENT_E2E_UPSTREAM": "http://127.0.0.1:18080",
                    "DAM_AGENT_E2E_LISTEN": "127.0.0.1:17828",
                    "DAM_AGENT_E2E_WEB_ADDR": "127.0.0.1:12896",
                    "DAM_AGENT_STATE_DIR": "/tmp/dam-hermes",
                    "DAM_AGENT_E2E_BUILD": "0",
                    "DAM_AGENT_E2E_KEEP_TEMP": "1",
                }
            )
            subprocess.run(
                [str(BUILD_SCRIPT), "agent-dogfood-verify"],
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
                    "verify",
                    "--upstream",
                    "http://127.0.0.1:18080",
                    "--listen",
                    "127.0.0.1:17828",
                    "--web-addr",
                    "127.0.0.1:12896",
                    "--proxy-binary",
                    str(ROOT / "target" / "debug" / "dam-proxy"),
                    "--web-binary",
                    str(ROOT / "target" / "debug" / "dam-web"),
                    "--startup-timeout",
                    "30",
                    "--http-timeout",
                    "60",
                    "--state-dir",
                    "/tmp/dam-hermes",
                    "--no-build",
                    "--keep-state",
                ],
            )

    def test_agent_visible_evidence_smoke_allocates_free_loopback_web_addr_by_default(self):
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
            env.pop("DAM_AGENT_E2E_WEB_ADDR", None)
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
            web_addr = argv[argv.index("--web-addr") + 1]
            host, port = web_addr.split(":", 1)
            self.assertEqual(host, "127.0.0.1")
            self.assertNotEqual(web_addr, "127.0.0.1:2896")
            self.assertGreater(int(port), 0)

    def test_agent_dogfood_verify_allocates_isolated_loopback_web_addr_when_unset(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            output_path = Path(temp_dir) / "argv.txt"
            stub_path = Path(temp_dir) / "verify_stub.py"
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
                    "DAM_AGENT_E2E_VERIFY_SCRIPT": str(stub_path),
                    "DAM_AGENT_E2E_UPSTREAM": "http://127.0.0.1:18080",
                    "DAM_AGENT_E2E_LISTEN": "127.0.0.1:17828",
                    "DAM_AGENT_STATE_DIR": "/tmp/dam-hermes",
                    "DAM_AGENT_E2E_BUILD": "0",
                    "DAM_AGENT_E2E_KEEP_TEMP": "1",
                }
            )
            env.pop("DAM_AGENT_E2E_WEB_ADDR", None)
            subprocess.run(
                [str(BUILD_SCRIPT), "agent-dogfood-verify"],
                cwd=ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=True,
            )

            argv = output_path.read_text(encoding="utf-8").splitlines()
            web_addr = argv[argv.index("--web-addr") + 1]
            host, port = web_addr.split(":", 1)
            self.assertEqual(host, "127.0.0.1")
            self.assertNotEqual(web_addr, "127.0.0.1:2896")
            self.assertGreater(int(port), 0)

    def test_visible_evidence_smoke_sanitizes_wallet_add_output(self):
        result = visible_evidence_smoke.sanitize_wallet_add_result(
            {
                "ok": True,
                "data": {
                    "item": {
                        "id": "ref-123",
                        "kind": "ssn",
                        "value": "123-45-6789",
                        "state": "protected",
                        "shared_with": [],
                    },
                    "meta": [{"key": "stored in", "value": "local vault"}],
                    "first_seen": "2026-06-19",
                    "reference": "[ssn:ref-123]",
                },
            }
        )

        self.assertEqual(
            result,
            {
                "ok": True,
                "data": {
                    "item": {
                        "id": "ref-123",
                        "kind": "ssn",
                        "state": "protected",
                    },
                    "meta": [{"key": "stored in", "value": "local vault"}],
                    "first_seen": "2026-06-19",
                    "reference": "[ssn:ref-123]",
                },
            },
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

    def test_agent_recovery_smoke_honors_environment_selected_setup_modes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            app_dir = temp_path / "DAM.app"
            dam_bin = app_dir / "Contents" / "MacOS" / "dam"
            calls_path = temp_path / "dam-calls.txt"
            state_dir = temp_path / "fixture-state"
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
                    "DAM_AGENT_NETWORK_MODE": "explicit_proxy",
                    "DAM_AGENT_TRUST_MODE": "disabled",
                    "DAM_AGENT_STATE_DIR": str(state_dir),
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                }
            )
            subprocess.run(
                [str(BUILD_SCRIPT), "agent-recovery-smoke"],
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
                    f"setup repair --dry-run --network-mode explicit_proxy --trust-mode disabled --state-dir {state_dir} --json",
                    f"setup export-diagnostics --network-mode explicit_proxy --trust-mode disabled --state-dir {state_dir} --json",
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

    def test_agent_repair_smoke_accepts_environment_confirmation_and_modes(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            app_dir = temp_path / "DAM.app"
            dam_bin = app_dir / "Contents" / "MacOS" / "dam"
            calls_path = temp_path / "dam-calls.txt"
            state_dir = temp_path / "fixture-state"
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
                    "DAM_AGENT_CONFIRM_MUTATION": "1",
                    "DAM_AGENT_NETWORK_MODE": "explicit_proxy",
                    "DAM_AGENT_TRUST_MODE": "disabled",
                    "DAM_AGENT_STATE_DIR": str(state_dir),
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                }
            )
            subprocess.run(
                [str(BUILD_SCRIPT), "agent-repair-smoke"],
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
                    f"setup repair --yes --network-mode explicit_proxy --trust-mode disabled --state-dir {state_dir} --json",
                    f"setup status --network-mode explicit_proxy --trust-mode disabled --state-dir {state_dir} --json",
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

    def test_agent_npm_readiness_reports_publish_blockers_after_local_pack_validation(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            bin_dir.mkdir()

            real_node = shutil.which("node")
            self.assertIsNotNone(real_node)

            cargo = bin_dir / "cargo"
            cargo.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
            cargo.chmod(0o755)

            node = bin_dir / "node"
            node.write_text(
                textwrap.dedent(
                    f"""
                    #!/usr/bin/env sh
                    if [ "$1" = "-p" ] && [ "$2" = "process.platform + '-' + process.arch" ]; then
                      printf 'linux-x64\\n'
                      exit 0
                    fi
                    exec {real_node} "$@"
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            node.chmod(0o755)

            npm = bin_dir / "npm"
            npm.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env sh
                    if [ "$1" = "config" ] && [ "$2" = "get" ] && [ "$3" = "registry" ]; then
                      printf 'https://registry.npmjs.org/\n'
                      exit 0
                    fi
                    if [ "$1" = "view" ] && [ "$2" = "@rpblc/dam" ] && [ "$3" = "version" ] && [ "$4" = "--json" ]; then
                      printf '"0.3.3"\n'
                      exit 0
                    fi
                    if [ "$1" = "owner" ] && [ "$2" = "ls" ] && [ "$3" = "@rpblc/dam" ]; then
                      printf 'rpblc-alexy <contact@rpblc.com>\n'
                      exit 0
                    fi
                    if [ "$1" = "whoami" ]; then
                      printf 'npm error code ENEEDAUTH\n' >&2
                      printf 'npm error need auth This command requires you to be logged in.\n' >&2
                      exit 1
                    fi
                    if [ "$1" = "pack" ]; then
                      cat <<'JSON'
[{"id":"@rpblc/dam@0.3.2","name":"@rpblc/dam","version":"0.3.2","filename":"rpblc-dam-0.3.2.tgz","files":[{"path":"README.md"},{"path":"npm/bin/dam.js"},{"path":"npm/bin/damctl.js"},{"path":"npm/bin/dam-web.js"},{"path":"npm/bin/dam-proxy.js"},{"path":"npm/bin/dam-mcp.js"},{"path":"npm/bin/dam-tray.js"},{"path":"npm/native/linux-x64/dam"},{"path":"npm/native/linux-x64/damctl"},{"path":"npm/native/linux-x64/dam-web"},{"path":"npm/native/linux-x64/dam-proxy"},{"path":"npm/native/linux-x64/dam-mcp"},{"path":"npm/native/linux-x64/dam-tray"}]}]
JSON
                      exit 0
                    fi
                    printf 'unexpected npm invocation: %s\n' "$*" >&2
                    exit 1
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            npm.chmod(0o755)

            release_dir = ROOT / "target" / "release"
            release_dir.mkdir(parents=True, exist_ok=True)
            created_release_files = []
            for name in ["dam", "damctl", "dam-web", "dam-proxy", "dam-mcp", "dam-tray"]:
                binary = release_dir / name
                if not binary.exists():
                    binary.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
                    binary.chmod(0o755)
                    created_release_files.append(binary)

            staged_dir = ROOT / "npm" / "native" / "linux-x64"
            if staged_dir.exists():
                shutil.rmtree(staged_dir)

            env = os.environ.copy()
            env["PATH"] = f"{bin_dir}{os.pathsep}{env['PATH']}"

            try:
                result = subprocess.run(
                    [str(BUILD_SCRIPT), "agent-npm-readiness"],
                    cwd=ROOT,
                    env=env,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
            finally:
                if staged_dir.exists():
                    shutil.rmtree(staged_dir)
                for binary in created_release_files:
                    if binary.exists():
                        binary.unlink()

            self.assertEqual(1, result.returncode, result.stdout + result.stderr)
            self.assertIn("DAM agent npm readiness", result.stdout)
            self.assertIn("local_version: 0.3.2", result.stdout)
            self.assertIn("registry_version: 0.3.3", result.stdout)
            self.assertIn("npm_auth: missing", result.stdout)
            self.assertIn("pack_native_files_present: yes", result.stdout)
            self.assertIn(
                "local package version 0.3.2 is not greater than published npm version 0.3.3",
                result.stdout,
            )
            self.assertIn("npm publish auth is not configured on this machine", result.stdout)

    def test_agent_npm_readiness_reports_blocker_when_pack_payload_missing_native_binaries(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            bin_dir.mkdir()

            real_node = shutil.which("node")
            self.assertIsNotNone(real_node)

            cargo = bin_dir / "cargo"
            cargo.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
            cargo.chmod(0o755)

            node = bin_dir / "node"
            node.write_text(
                textwrap.dedent(
                    f"""
                    #!/usr/bin/env sh
                    if [ "$1" = "-p" ] && [ "$2" = "process.platform + '-' + process.arch" ]; then
                      printf 'linux-x64\\n'
                      exit 0
                    fi
                    exec {real_node} "$@"
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            node.chmod(0o755)

            # Pack output omits the native binaries to exercise the missing-files blocker.
            npm = bin_dir / "npm"
            npm.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env sh
                    if [ "$1" = "config" ] && [ "$2" = "get" ] && [ "$3" = "registry" ]; then
                      printf 'https://registry.npmjs.org/\n'
                      exit 0
                    fi
                    if [ "$1" = "view" ] && [ "$2" = "@rpblc/dam" ] && [ "$3" = "version" ] && [ "$4" = "--json" ]; then
                      printf '"0.0.1"\n'
                      exit 0
                    fi
                    if [ "$1" = "owner" ] && [ "$2" = "ls" ] && [ "$3" = "@rpblc/dam" ]; then
                      printf 'rpblc-alexy <contact@rpblc.com>\n'
                      exit 0
                    fi
                    if [ "$1" = "whoami" ]; then
                      printf 'rpblc-alexy\n'
                      exit 0
                    fi
                    if [ "$1" = "pack" ]; then
                      cat <<'JSON'
[{"id":"@rpblc/dam@0.1.0","name":"@rpblc/dam","version":"0.1.0","filename":"rpblc-dam-0.1.0.tgz","files":[{"path":"README.md"},{"path":"npm/bin/dam.js"}]}]
JSON
                      exit 0
                    fi
                    printf 'unexpected npm invocation: %s\n' "$*" >&2
                    exit 1
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            npm.chmod(0o755)

            release_dir = ROOT / "target" / "release"
            release_dir.mkdir(parents=True, exist_ok=True)
            created_release_files = []
            for name in ["dam", "damctl", "dam-web", "dam-proxy", "dam-mcp", "dam-tray"]:
                binary = release_dir / name
                if not binary.exists():
                    binary.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
                    binary.chmod(0o755)
                    created_release_files.append(binary)

            staged_dir = ROOT / "npm" / "native" / "linux-x64"
            if staged_dir.exists():
                shutil.rmtree(staged_dir)

            env = os.environ.copy()
            env["PATH"] = f"{bin_dir}{os.pathsep}{env['PATH']}"

            try:
                result = subprocess.run(
                    [str(BUILD_SCRIPT), "agent-npm-readiness"],
                    cwd=ROOT,
                    env=env,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
            finally:
                if staged_dir.exists():
                    shutil.rmtree(staged_dir)
                for binary in created_release_files:
                    if binary.exists():
                        binary.unlink()

            self.assertEqual(1, result.returncode, result.stdout + result.stderr)
            self.assertIn("pack_native_files_present: no", result.stdout)
            self.assertIn(
                "npm pack payload is missing one or more staged native binaries for linux-x64",
                result.stdout,
            )

    def test_agent_mvp_readiness_composes_package_setup_and_protection_sections(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            bin_dir.mkdir()

            real_node = shutil.which("node")
            self.assertIsNotNone(real_node)

            release_dir = ROOT / "target" / "release"
            release_dir.mkdir(parents=True, exist_ok=True)
            created_release_files = []
            for name in ["dam", "damctl", "dam-web", "dam-proxy", "dam-mcp", "dam-tray"]:
                binary = release_dir / name
                if not binary.exists():
                    binary.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
                    binary.chmod(0o755)
                    created_release_files.append(binary)

            cargo = bin_dir / "cargo"
            cargo.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env sh
                    if [ "$1" = "build" ]; then
                      exit 0
                    fi
                    if [ "$1" = "run" ]; then
                      if printf '%s\n' "$@" | grep -q 'setup'; then
                        printf '{"state":"needs_action","message":"DAM is disconnected; start DAM"}\n'
                        exit 1
                      fi
                      printf '{"state":"ready"}\n'
                      exit 0
                    fi
                    printf 'unexpected cargo invocation: %s\n' "$*" >&2
                    exit 1
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            cargo.chmod(0o755)

            node = bin_dir / "node"
            node.write_text(
                textwrap.dedent(
                    f"""
                    #!/usr/bin/env sh
                    if [ "$1" = "-p" ] && [ "$2" = "process.platform + '-' + process.arch" ]; then
                      printf 'linux-x64\\n'
                      exit 0
                    fi
                    if [ "$1" = "npm/bin/dam.js" ] && [ "$2" = "package-doctor" ] && [ "$3" = "--json" ]; then
                      printf '{{"state":"ready"}}\\n'
                      exit 0
                    fi
                    exec {real_node} "$@"
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            node.chmod(0o755)

            npm = bin_dir / "npm"
            npm.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env sh
                    if [ "$1" = "config" ] && [ "$2" = "get" ] && [ "$3" = "registry" ]; then
                      printf 'https://registry.npmjs.org/\n'
                      exit 0
                    fi
                    if [ "$1" = "view" ] && [ "$2" = "@rpblc/dam" ] && [ "$3" = "version" ] && [ "$4" = "--json" ]; then
                      printf '"0.0.1"\n'
                      exit 0
                    fi
                    if [ "$1" = "owner" ] && [ "$2" = "ls" ] && [ "$3" = "@rpblc/dam" ]; then
                      printf 'rpblc-alexy <contact@rpblc.com>\n'
                      exit 0
                    fi
                    if [ "$1" = "whoami" ]; then
                      printf 'rpblc-alexy\n'
                      exit 0
                    fi
                    if [ "$1" = "pack" ]; then
                      cat <<'JSON'
[{"id":"@rpblc/dam@0.3.2","name":"@rpblc/dam","version":"0.3.2","filename":"rpblc-dam-0.3.2.tgz","files":[{"path":"npm/native/linux-x64/dam"},{"path":"npm/native/linux-x64/damctl"},{"path":"npm/native/linux-x64/dam-web"},{"path":"npm/native/linux-x64/dam-proxy"},{"path":"npm/native/linux-x64/dam-mcp"},{"path":"npm/native/linux-x64/dam-tray"}]}]
JSON
                      exit 0
                    fi
                    printf 'unexpected npm invocation: %s\n' "$*" >&2
                    exit 1
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            npm.chmod(0o755)

            smoke_stub = temp_path / "smoke_stub.py"
            smoke_stub.write_text("raise SystemExit(0)\n", encoding="utf-8")

            staged_dir = ROOT / "npm" / "native" / "linux-x64"
            if staged_dir.exists():
                shutil.rmtree(staged_dir)
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                    "DAM_AGENT_E2E_SMOKE_SCRIPT": str(smoke_stub),
                    "DAM_AGENT_NETWORK_MODE": "explicit_proxy",
                    "DAM_AGENT_TRUST_MODE": "disabled",
                }
            )

            try:
                result = subprocess.run(
                    [str(BUILD_SCRIPT), "agent-mvp-readiness"],
                    cwd=ROOT,
                    env=env,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
            finally:
                if staged_dir.exists():
                    shutil.rmtree(staged_dir)
                for binary in created_release_files:
                    if binary.exists():
                        binary.unlink()

            self.assertEqual(0, result.returncode, result.stdout + result.stderr)
            self.assertIn("DAM agent MVP release readiness", result.stdout)
            self.assertIn("package_installability_result: pass", result.stdout)
            self.assertIn("setup_doctor_readiness_result: pass", result.stdout)
            self.assertIn("setup_status_exit_status: 1", result.stdout)
            self.assertIn("setup_next_action_exit_status: 1", result.stdout)
            self.assertIn("protection_proof_result: pass", result.stdout)
            self.assertIn("readiness_result: pass", result.stdout)

    def test_agent_mvp_readiness_fails_closed_when_package_build_fails_with_stale_binaries(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            bin_dir.mkdir()

            real_node = shutil.which("node")
            self.assertIsNotNone(real_node)

            release_dir = ROOT / "target" / "release"
            release_dir.mkdir(parents=True, exist_ok=True)
            created_release_files = []
            for name in ["dam", "damctl", "dam-web", "dam-proxy", "dam-mcp", "dam-tray"]:
                binary = release_dir / name
                if not binary.exists():
                    binary.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
                    binary.chmod(0o755)
                    created_release_files.append(binary)

            cargo = bin_dir / "cargo"
            cargo.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env sh
                    if [ "$1" = "build" ]; then
                      printf 'synthetic build failure\n' >&2
                      exit 42
                    fi
                    if [ "$1" = "run" ]; then
                      printf '{"state":"ready"}\n'
                      exit 0
                    fi
                    exit 1
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            cargo.chmod(0o755)

            node = bin_dir / "node"
            node.write_text(f"#!/usr/bin/env sh\nexec {real_node} \"$@\"\n", encoding="utf-8")
            node.chmod(0o755)

            npm = bin_dir / "npm"
            npm.write_text("#!/usr/bin/env sh\nprintf 'unexpected npm invocation: %s\\n' \"$*\" >&2\nexit 1\n", encoding="utf-8")
            npm.chmod(0o755)

            smoke_stub = temp_path / "smoke_stub.py"
            smoke_stub.write_text("raise SystemExit(0)\n", encoding="utf-8")
            staged_dir = ROOT / "npm" / "native" / "linux-x64"
            if staged_dir.exists():
                shutil.rmtree(staged_dir)
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                    "DAM_AGENT_E2E_SMOKE_SCRIPT": str(smoke_stub),
                    "DAM_AGENT_NETWORK_MODE": "explicit_proxy",
                    "DAM_AGENT_TRUST_MODE": "disabled",
                }
            )

            try:
                result = subprocess.run(
                    [str(BUILD_SCRIPT), "agent-mvp-readiness"],
                    cwd=ROOT,
                    env=env,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
            finally:
                if staged_dir.exists():
                    shutil.rmtree(staged_dir)
                for binary in created_release_files:
                    if binary.exists():
                        binary.unlink()

            self.assertEqual(1, result.returncode, result.stdout + result.stderr)
            self.assertIn("package_installability_result: fail", result.stdout)
            self.assertIn("package_installability_exit_status: 42", result.stdout)
            self.assertIn("readiness_result: fail", result.stdout)

    def test_agent_mvp_readiness_rejects_blocked_setup_status(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            bin_dir = temp_path / "bin"
            bin_dir.mkdir()

            real_node = shutil.which("node")
            self.assertIsNotNone(real_node)

            release_dir = ROOT / "target" / "release"
            release_dir.mkdir(parents=True, exist_ok=True)
            created_release_files = []
            for name in ["dam", "damctl", "dam-web", "dam-proxy", "dam-mcp", "dam-tray"]:
                binary = release_dir / name
                if not binary.exists():
                    binary.write_text("#!/usr/bin/env sh\nexit 0\n", encoding="utf-8")
                    binary.chmod(0o755)
                    created_release_files.append(binary)

            cargo = bin_dir / "cargo"
            cargo.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env sh
                    if [ "$1" = "build" ]; then
                      exit 0
                    fi
                    if [ "$1" = "run" ]; then
                      if printf '%s\n' "$@" | grep -q 'setup'; then
                        printf '{"state":"blocked","message":"synthetic setup blocker"}\n'
                        exit 1
                      fi
                      printf '{"state":"ready"}\n'
                      exit 0
                    fi
                    exit 1
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            cargo.chmod(0o755)

            node = bin_dir / "node"
            node.write_text(
                textwrap.dedent(
                    f"""
                    #!/usr/bin/env sh
                    if [ "$1" = "-p" ] && [ "$2" = "process.platform + '-' + process.arch" ]; then
                      printf 'linux-x64\\n'
                      exit 0
                    fi
                    if [ "$1" = "npm/bin/dam.js" ] && [ "$2" = "package-doctor" ] && [ "$3" = "--json" ]; then
                      printf '{{"state":"ready"}}\\n'
                      exit 0
                    fi
                    exec {real_node} "$@"
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            node.chmod(0o755)

            npm = bin_dir / "npm"
            npm.write_text(
                textwrap.dedent(
                    """
                    #!/usr/bin/env sh
                    if [ "$1" = "config" ]; then printf 'https://registry.npmjs.org/\n'; exit 0; fi
                    if [ "$1" = "view" ]; then printf '"0.0.1"\n'; exit 0; fi
                    if [ "$1" = "owner" ]; then printf 'rpblc-alexy <contact@rpblc.com>\n'; exit 0; fi
                    if [ "$1" = "whoami" ]; then printf 'rpblc-alexy\n'; exit 0; fi
                    if [ "$1" = "pack" ]; then
                      cat <<'JSON'
[{"id":"@rpblc/dam@0.3.2","name":"@rpblc/dam","version":"0.3.2","filename":"rpblc-dam-0.3.2.tgz","files":[{"path":"npm/native/linux-x64/dam"},{"path":"npm/native/linux-x64/damctl"},{"path":"npm/native/linux-x64/dam-web"},{"path":"npm/native/linux-x64/dam-proxy"},{"path":"npm/native/linux-x64/dam-mcp"},{"path":"npm/native/linux-x64/dam-tray"}]}]
JSON
                      exit 0
                    fi
                    exit 1
                    """
                ).lstrip(),
                encoding="utf-8",
            )
            npm.chmod(0o755)

            smoke_stub = temp_path / "smoke_stub.py"
            smoke_stub.write_text("raise SystemExit(0)\n", encoding="utf-8")
            staged_dir = ROOT / "npm" / "native" / "linux-x64"
            if staged_dir.exists():
                shutil.rmtree(staged_dir)
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{bin_dir}{os.pathsep}{env['PATH']}",
                    "DAM_AGENT_E2E_SMOKE_SCRIPT": str(smoke_stub),
                    "DAM_AGENT_NETWORK_MODE": "explicit_proxy",
                    "DAM_AGENT_TRUST_MODE": "disabled",
                }
            )

            try:
                result = subprocess.run(
                    [str(BUILD_SCRIPT), "agent-mvp-readiness"],
                    cwd=ROOT,
                    env=env,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
            finally:
                if staged_dir.exists():
                    shutil.rmtree(staged_dir)
                for binary in created_release_files:
                    if binary.exists():
                        binary.unlink()

            self.assertEqual(1, result.returncode, result.stdout + result.stderr)
            self.assertIn("setup_status_state: blocked", result.stdout)
            self.assertIn("setup_status_needs_action: rejected", result.stdout)
            self.assertIn("setup_doctor_readiness_result: fail", result.stdout)
            self.assertIn("readiness_result: fail", result.stdout)

    def test_agent_mvp_readiness_rejects_invalid_setup_mode_before_subchecks(self):
        env = os.environ.copy()
        env["DAM_AGENT_MVP_SETUP_MODE"] = "mutating"
        result = subprocess.run(
            [str(BUILD_SCRIPT), "agent-mvp-readiness"],
            cwd=ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        self.assertEqual(2, result.returncode)
        self.assertIn("invalid DAM_AGENT_MVP_SETUP_MODE", result.stderr)


if __name__ == "__main__":
    unittest.main()
