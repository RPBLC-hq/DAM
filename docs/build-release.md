# Build And Release

`scripts/dam-build.sh` is the standard local and CI entry point for DAM builds.

## Commands

```bash
scripts/dam-build.sh check
scripts/dam-build.sh dev
scripts/dam-build.sh npm-native
scripts/dam-build.sh macos-app --mode developer-id
scripts/dam-build.sh notarize --app target/dam-build/macos/DAM.app --notary-profile DAM-notary
scripts/dam-build.sh release-macos --mode developer-id
scripts/dam-build.sh deploy-local --mode development
scripts/dam-build.sh agent-check
scripts/dam-build.sh agent-mvp-readiness
scripts/dam-build.sh agent-npm-readiness
scripts/dam-build.sh agent-protection-smoke
scripts/dam-build.sh agent-visible-evidence-smoke
scripts/dam-build.sh agent-websocket-smoke
scripts/dam-build.sh agent-dogfood-verify
scripts/dam-build.sh agent-recovery-smoke --network-mode tun --trust-mode local_ca [--state-dir PATH]
scripts/dam-build.sh agent-repair-smoke --network-mode tun --trust-mode local_ca --confirm-mutation [--state-dir PATH]
scripts/dam-build.sh agent-install --skip-checks
scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca [--state-dir PATH] [--strict-status]
```

`check` runs the repository verification suite: React/Vite UI dependency install and build for the embedded `dam-web` asset, npm package shim smoke tests, npm pack dry-run, Rust formatting, workspace clippy, workspace tests, and macOS Swift package tests when running on macOS.

`dev` builds the source-tree binaries used by local daemon/tray runs: `dam`, `damctl`, `dam-web`, and `dam-tray`.

`npm-native` builds current-platform release binaries for `dam`, `damctl`, `dam-web`, `dam-proxy`, `dam-mcp`, and `dam-tray`, then stages them under `npm/native/<platform>-<arch>/` for package smoke testing and release assembly.

`macos-app` delegates signed app assembly to `native/macos/scripts/package-dam-app.sh`, keeping entitlement and provisioning validation in the native macOS packaging script.

`notarize` zips an existing `DAM.app`, submits it with `xcrun notarytool`, staples the ticket, and validates the stapled app.

`release-macos` runs `check`, builds a signed Developer ID app by default, notarizes/staples it, and writes a release zip under `target/dam-build/macos`.

`deploy-local` builds or accepts an existing `DAM.app` and copies it to `/Applications` by default.

`agent-check` is the default verification command for local agents and maintainers. It runs `check` and adds `git diff --check` when the source tree is a git checkout.

`agent-mvp-readiness` is the one-command read-only MVP release-readiness gate for the local known-provider shield. It prints separate pass/fail sections for package/installability readiness (`agent-npm-readiness`), local setup/doctor readiness, and synthetic protection proof readiness (`agent-protection-smoke`). The default setup probe mode is `source`: it runs source-tree `dam doctor --json`, `dam setup status --json`, and `dam setup next-action --json` through `cargo run -q -p dam -- ...` with the configured setup modes, accepting setup `needs_action` exit codes as readable readiness evidence while still failing on invalid JSON, failed `doctor`, or non-`needs_action` setup failures such as `blocked`. Set `DAM_AGENT_MVP_SETUP_MODE=installed` to make the setup section use installed-app `agent-status --strict-status` instead. The command does not publish to npm, call real providers, use credentials, mutate host routing/trust/PAC/TUN/firewall state, or deploy public artifacts; the protection section remains synthetic-only and should use a loopback OpenAI-compatible upstream such as `DAM_AGENT_E2E_UPSTREAM=http://127.0.0.1:18080` with `scripts/dam_fake_openai_upstream.py --port 18080` when no local model endpoint is available.

`agent-npm-readiness` is the local/read-only npm installability probe. It stages current-platform native binaries under `npm/native/<platform>-<arch>/`, runs `dam package-doctor --json`, verifies that `npm pack --dry-run --ignore-scripts --json` includes the staged binaries, reports the current registry owner and published version for `@rpblc/dam`, and fails closed when the local package version is not publishable or this machine lacks npm publish auth. It does not publish, mutate system routing/trust, or require package credentials in repository files.

`agent-protection-smoke` runs the local API-through-DAM protection smoke test against a loopback OpenAI-compatible upstream. By default it uses local llama.cpp at `http://127.0.0.1:8080`, builds and runs `target/debug/dam-proxy` on `127.0.0.1:7831`, uses temporary vault/activity SQLite stores, and exercises the bundled MVP HTTP route matrix: `openai-api` (`openai` target / `openai-compatible` provider), `anthropic-api` (`anthropic` target / `anthropic` provider), `claude-web` (`claude-web` target / `generic-http` provider), `anthropic-console` (`anthropic-console` target / `generic-http` provider), `claude-mcp-proxy` (`claude-mcp-proxy` target / `generic-http` provider), `claude-platform` (`claude-platform` target / `generic-http` provider), and `openai-platform` (`openai-platform` target / `generic-http` provider). Each route sends synthetic exact-echo and token-transformation requests plus the `agent_session_mixed_pii_secret_v1` realistic agent-session fixture containing synthetic email, phone, SSN-like, `.env` assignment, and assembled GitHub-shaped token values. It verifies trusted-side resolution, verifies the model can transform only DAM references by inserting whitespace after reference opening brackets, checks the activity log for raw synthetic leaks, records fixture name, route ID, target name, provider, proxy `/health` target, route-wide detector kind/action counts, agent-session outbound tokenized redaction counts bounded before that request's `provider_forward_start`, raw-leak scan status, and `provider_forward_start` route lines in the JSON proof output without printing raw synthetic secret values, and fails if the health target does not match the expected route case or if the activity log lacks exactly one matching route line for each proof request, then terminates the proxy and removes the temporary stores. When the upstream exposes the fake-upstream `GET /__dam/transcript` endpoint, the smoke scopes transcript assertions to each route run and asserts the upstream request transcript contains DAM references/redactions for both the echo pair and the realistic agent-session fixture, not raw synthetic values. If `dam-proxy` exits before becoming healthy, the command fails with the captured exit code and stdout/stderr tail so port/config problems are actionable. It does not change system network settings or call paid providers. ChatGPT WebSocket routes remain covered by `agent-websocket-smoke`, not this HTTP smoke. Set `DAM_AGENT_E2E_BINARY` to test a specific proxy binary, `DAM_AGENT_E2E_BUILD=0` to reuse an existing binary, or `DAM_AGENT_E2E_KEEP_TEMP=1` to retain temporary smoke-test stores for debugging. Run `python3 scripts/rpblc_dam_local_llm_e2e_smoke.py --route <id>` directly when you need to isolate one bundled HTTP route.

`agent-websocket-smoke` runs the synthetic ChatGPT WebSocket route proof with loopback-only processes. It starts DAM's transparent proxy harness in-process, creates a deterministic plaintext loopback WebSocket upstream, drives a CONNECT/TLS-intercepted `chatgpt.com` WebSocket upgrade through the `chatgpt-web` traffic profile route, sends an outbound masked text frame containing only the synthetic detector-supported email `scrabb@jnjjj.com`, and asserts the upstream transcript saw a DAM `[email:<id>]` reference rather than the raw value. The smoke also requires `route_decision` and `provider_forward_start` activity-log evidence for target `chatgpt-web`, provider `openai-compatible`, and adapter `web_socket`, and verifies activity-log messages do not contain the raw synthetic value. Unsupported compressed, fragmented, binary, and policy-blocked WebSocket behavior remains covered by focused `dam-proxy` fail-closed tests; this smoke's process-level route proof intentionally exercises the MVP unfragmented text-frame contract only. It does not use real ChatGPT credentials, call providers, mutate host routing/proxy/CA state, or spend API credits.

`agent-dogfood-verify` runs the low-risk VPS dogfooding verifier. It builds and starts loopback `dam-proxy` plus `dam-web` against one shared state directory, sends synthetic OpenAI-compatible traffic through DAM to prove upstream tokenization and trusted-side resolve, checks the Activity API for rendered evidence from the current verification run without raw-log leakage, and exercises the local pending-consent request flow via `/api/v1/requests/trigger` and `allow-once`. The command stays in explicit-proxy mode only: no system proxy, `tun`, or trust-store mutation. By default the wrapper allocates an isolated free loopback web port for the proof run so it does not collide with a separately supervised `dam-web`; use `DAM_AGENT_E2E_UPSTREAM` for the remote or loopback OpenAI-compatible endpoint, `DAM_AGENT_E2E_WEB_ADDR` to pin a specific isolated web proof port, `DAM_AGENT_STATE_DIR` for a persistent dogfood state directory such as `~/.dam-hermes`, `DAM_AGENT_E2E_WEB_BINARY` to pin a specific `dam-web` binary, and `DAM_AGENT_E2E_KEEP_TEMP=1` to retain an otherwise-temporary verification state directory.

`agent-visible-evidence-smoke` runs a loopback-only visible-evidence verifier against temporary local DAM state. It starts a deterministic fake OpenAI-compatible upstream plus `dam-proxy` and `dam-web`, sends one synthetic request through DAM, polls `GET /api/v1/connect`, `GET /api/v1/activity`, and `GET /api/v1/activity/:id`, and fails closed if raw synthetic values appear in upstream payloads, Connect JSON, Activity JSON, Activity detail JSON, or non-vault log bytes. The build wrapper allocates a separate free loopback `dam-web` address by default so the smoke does not bind the normal local DAM port or accidentally hit an already-running tray/web instance. When Activity exposes a stored local value, the smoke also exercises `POST /api/v1/activity/:id/add-to-wallet` so Wallet add convenience still works without first rendering the raw value in Activity. Set `DAM_AGENT_VISIBLE_EVIDENCE_SMOKE_SCRIPT` to test a different verifier script, `DAM_AGENT_E2E_WEB_BINARY` to point at a specific `dam-web` build, `DAM_AGENT_E2E_WEB_ADDR` to pin the temporary web listen address, `DAM_AGENT_E2E_BUILD=0` to reuse existing binaries, or `DAM_AGENT_E2E_KEEP_TEMP=1` to retain the temporary stores.

`agent-recovery-smoke` runs the installed app's read-only recovery probes without changing routing or daemon state: `dam setup rescue --dry-run --json`, `dam setup repair --dry-run --network-mode <mode> --trust-mode <mode> --json`, and `dam setup export-diagnostics --network-mode <mode> --trust-mode <mode> --json`. It uses the same `--network-mode`, `--trust-mode`, `--state-dir`, `DAM_AGENT_NETWORK_MODE`, `DAM_AGENT_TRUST_MODE`, and `DAM_AGENT_STATE_DIR` inputs as `agent-status` for setup repair planning and diagnostics export. Use it after `agent-install` when validating that the installed release artifact can explain and preview recovery actions before any mutating rescue/repair is attempted; pass `--state-dir` when validating retained or fixture state instead of the live user state.

`agent-repair-smoke` is the opt-in mutating installed-app recovery smoke. It refuses to run unless `--confirm-mutation` or `DAM_AGENT_CONFIRM_MUTATION=1` is set, then runs `dam setup rescue --yes --json`, `dam setup repair --yes --network-mode <mode> --trust-mode <mode> --json`, and a final `dam setup status --network-mode <mode> --trust-mode <mode> --json` through the installed `dam` binary. Use it only on a disposable local installed state or with an explicit `--state-dir` fixture after the read-only `agent-recovery-smoke` output is understood; the failsafe is the same local recovery path it exercises, and the command keeps the setup mode context aligned with `agent-status`.

`agent-install` is the idempotent local release-path install command for macOS. It optionally runs `agent-check`, builds the app, notarizes Developer ID builds unless notarization is disabled, stops the installed tray/web processes before replacing the app bundle, verifies the installed app, refreshes app-owned System Extension activation, reconfigures the Network Extension manager for `tun` installs, restarts the daemon with the persisted DAM configuration, opens the tray app, and prints `agent-status`.

`agent-status` inspects the installed app without mutating setup. It reports matching DAM processes, verifies code signing, validates notarization/Gatekeeper when notarization is enabled, and runs the installed `dam doctor --json`, `dam setup status --json`, `dam setup next-action --json`, `dam setup export-diagnostics --json`, and `dam status --json` probes. Setup probes default to the release-path `tun` + `local_ca` modes and can be overridden with `--network-mode`, `--trust-mode`, and `--state-dir`/`DAM_AGENT_STATE_DIR` so support and tests can inspect a retained installed state directory without touching the live default. Pass `--strict-status` or set `DAM_AGENT_STATUS_STRICT=1` to make any failed installed-app probe fail the wrapper, which is the mode used by installed `agent-mvp-readiness` setup checks. Invalid setup probe modes are rejected during script argument validation before macOS-only installed-app checks run. The npm wrapper package doctor remains part of `check`/`agent-check` through the npm smoke test; it is not an installed native app command.

## Installed recovery scenario matrix

Use this matrix when validating release-path recovery without guessing which command to run next. The read-only path is always `agent-status` and `agent-recovery-smoke` first; only run `agent-repair-smoke` after the read-only output matches the intended scenario and the state is disposable or fixture-backed.

| Scenario | How to inspect safely | Expected signal to confirm | Safe next action |
| --- | --- | --- | --- |
| Fresh install before first connect | `scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca` | `dam setup status --json` / `next-action --json` reports install or approval-needed detail codes; no recovery mutation is needed yet. | Follow the reported setup step; do **not** run rescue/repair just to continue first-time install. |
| Reinstall after an interrupted upgrade | `scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca [--state-dir PATH]` | Status and diagnostics still resolve the retained state directory and show whether DAM needs reinstall, approval, or routing reconfiguration. | Re-run `agent-install` or the reported setup action; keep `agent-repair-smoke` for disposable retained-state fixtures only. |
| Deleted local state with packaged Network Extension still present | `scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca [--state-dir PATH]` then `agent-recovery-smoke` with the same mode/state-dir | Diagnostics export still returns an offline bundle and `repair --dry-run` explains the next recovery action instead of assuming the missing state means "all clear". | Review diagnostics first; if a disposable fixture still needs cleanup, use `agent-repair-smoke --confirm-mutation` with the same mode/state-dir. |
| Disabled Network Extension manager / capture configured but not enabled | `scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca` | Setup status or next-action reports the enable/approval step explicitly rather than a generic spinner; diagnostics preserves the disabled state. | Use the reported enable/approval action. Only run repair after the read-only plan shows repair is the correct path. |
| Stale daemon / process killed after setup | `scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca [--state-dir PATH]` | `dam status --json` or setup diagnostics shows daemon/process drift while setup probes remain readable. | Restart or reconnect through the reported next action; use repair only if rescue/repair preview says cleanup is needed first. |
| Post-reboot / waiting-for-reboot Network Extension state | `scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca` | Setup status / next-action surfaces the reboot-specific detail instead of another install prompt. | Reboot or complete the reported post-reboot step, then re-run `agent-status`. |
| Disconnect / reconnect without uninstalling DAM | `scripts/dam-build.sh agent-status --network-mode tun --trust-mode local_ca` after disconnect, then again after reconnect | Setup status remains readable in both states, and `dam status --json` distinguishes disconnected vs connected daemon state. | Reconnect using the reported next action; no repair mutation unless the read-only plan reports a broken routing state. |
| Offline recovery / no internet available | `scripts/dam-build.sh agent-recovery-smoke --network-mode tun --trust-mode local_ca [--state-dir PATH]` | `setup rescue --dry-run`, `setup repair --dry-run`, and `setup export-diagnostics --json` all succeed without contacting remote services. | Follow the offline diagnostics bundle; if mutation is required on a disposable fixture, run `agent-repair-smoke --confirm-mutation` with the same mode/state-dir. |

## Environment

- `DAM_BUILD_OUT`: artifact root, default `target/dam-build`.
- `DAM_SIGN_MODE`: `development` or `developer-id`, default `developer-id`.
- `DAM_NOTARY_PROFILE`: notarytool keychain profile, default `DAM-notary`.
- `DAM_MACOS_TEAM_ID`: optional Team ID override passed through to macOS packaging.
- `DAM_MACOS_APP_GROUP_ID`: optional App Group override passed through to macOS packaging.
- `DAM_MACOS_NE_BUNDLE_ID`: optional Network Extension bundle identifier override for local install refreshes.
- `DAM_INSTALL_DIR`: local deploy destination, default `/Applications`.
- `DAM_SKIP_NOTARIZE`: set to `1` to skip notarization in `agent-install`.
- `DAM_RESTART_AFTER_INSTALL`: set to `0` to install without restarting daemon/tray.
- `DAM_AGENT_STATUS_STRICT`: set to `1` to make `agent-status` fail when any probe fails.
- `DAM_AGENT_MVP_SETUP_MODE`: setup probe mode for `agent-mvp-readiness`: `source` (default) or `installed`.
- `DAM_AGENT_NETWORK_MODE`: setup mode for `agent-status`, default `tun`.
- `DAM_AGENT_TRUST_MODE`: trust mode for `agent-status`, default `local_ca`.
- `DAM_AGENT_STATE_DIR`: optional state directory for installed-app `agent-status`, `agent-recovery-smoke`, and `agent-repair-smoke` probes.
- `DAM_AGENT_CONFIRM_MUTATION`: set to `1` to allow the mutating `agent-repair-smoke` command.
- `DAM_AGENT_E2E_UPSTREAM`: local OpenAI-compatible upstream for `agent-protection-smoke`, default `http://127.0.0.1:8080`.
- `DAM_AGENT_E2E_LISTEN`: loopback listen address for the smoke proxy, default `127.0.0.1:7831`.
- `DAM_AGENT_E2E_WEB_ADDR`: optional loopback listen address for `agent-dogfood-verify` web proof; when unset, the wrapper allocates an isolated free `127.0.0.1:<port>` address.
- `DAM_AGENT_E2E_STARTUP_TIMEOUT`: smoke proxy startup timeout in seconds, default `30`.
- `DAM_AGENT_E2E_HTTP_TIMEOUT`: smoke request timeout in seconds, default `60`.
- `DAM_AGENT_E2E_SMOKE_SCRIPT`: verifier script path, default `scripts/rpblc_dam_local_llm_e2e_smoke.py`.
- `DAM_AGENT_VISIBLE_EVIDENCE_SMOKE_SCRIPT`: visible-evidence verifier script path, default `scripts/rpblc_dam_visible_evidence_smoke.py`.
- `DAM_AGENT_E2E_VERIFY_SCRIPT`: VPS dogfood verifier path, default `scripts/dam_vps_dogfood_verify.py`.
- `DAM_AGENT_E2E_BINARY`: `dam-proxy` binary path for smoke tests, default `target/debug/dam-proxy`.
- `DAM_AGENT_E2E_WEB_BINARY`: `dam-web` binary path for visible-evidence and dogfood verifier smokes, default `target/debug/dam-web`.
- `DAM_AGENT_E2E_BUILD`: set to `0` to skip the default smoke-test `cargo build -p dam-proxy` step.
- `DAM_AGENT_E2E_KEEP_TEMP`: set to `1` to retain the smoke-test temporary vault/log directory for debugging.

The script intentionally keeps signing, provisioning, notarization, and local smoke inputs in environment variables or keychain profiles. It must not require secrets in repository files.

For contributors without Developer ID/notary credentials, use a development build:

```bash
DAM_SIGN_MODE=development scripts/dam-build.sh agent-install --skip-notarize --no-restart
```

For maintainers validating the installed notarized path locally, use:

```bash
DAM_MACOS_APP_GROUP_ID=group.com.rpblc.dam scripts/dam-build.sh agent-install --skip-checks --require-notary
```
