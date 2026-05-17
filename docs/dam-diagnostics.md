# dam-diagnostics

Status: implemented first extraction.

`dam-diagnostics` owns shared local readiness checks for user-facing status surfaces. It exists so `damctl doctor` and `dam-web /doctor` do not invent separate interpretations of whether DAM is ready to protect local protected traffic.

## Responsibilities

`config_report` emits a side-effect-free `dam-api::HealthReport` for config shape and current implementation compatibility.

`doctor_report` emits a fuller `dam-api::HealthReport` for local readiness. It includes:

- config loading state;
- vault, consent, and log backend compatibility;
- failure-mode strictness and reduced-protection warnings;
- SQLite runtime open checks for local vault, consent, and log stores;
- router target/provider/auth/failure-mode decisions;
- proxy runtime `/health` reachability when proxy is enabled;
- a read-only setup plan summary for the requested local proxy/interception path;
- enabled integration profile selection for route scoping.

`dam doctor` accepts the same setup `--network-mode` and `--trust-mode` flags as `dam setup status` so installed-app automation can inspect the intended capture/trust path without getting a false mismatch from the default explicit-proxy setup plan.

`setup_plan` emits a side-effect-free setup checklist for the local "connect" UX. It evaluates:

- startup choice readiness for platform setup flows that need the app to return after reboot;
- system-proxy routing readiness when requested;
- platform `tun` routing readiness when requested:
  macOS emits System Extension approval, reboot, Network Extension manager configuration, manager enablement, and manager connection as separate steps;
  Linux emits a Linux transparent routing step and currently blocks to explicit proxy mode until that backend lands;
  Windows emits a Windows Filtering Platform step and currently blocks to explicit proxy mode until that backend lands;
- local CA trust readiness when requested;
- daemon lifecycle readiness for the requested network/trust modes.

The plan states are:

- `ready`: no setup action is needed.
- `needs_action`: DAM can continue after the listed next command or user confirmation.
- `blocked`: setup needs review before the local connect flow should continue. Network Extension inspection failures are blocked rather than guessed as install-needed so agents do not retry the wrong mutation.

The setup plan also includes `next_action`, the first `blocked` step when setup cannot continue or otherwise the first `needed` step. Each setup step reports `kind`, `status`, `detail`, `message`, optional `command`, `requires_confirmation`, and `changes_system`. Step `detail` values are stable machine-readable reasons such as `waiting_for_approval`, `waiting_for_reboot`, `needs_install`, `needs_enable`, `connected`, `disconnected`, `rolled_back`, `unsupported`, and `failed`. For macOS Network Extension setup, `rolled_back` means DAM intentionally disabled or removed its Network Extension manager after start or recovery-gate verification failed, so the next action can retry from a known state. Step messages are English diagnostic/support text; UI surfaces map stable step ids and details to localized English and French labels.

`setup_rescue` is the shared local recovery contract used by CLI/API/MCP surfaces. Preview mode reports the daemon, macOS system proxy, and macOS Network Extension recovery actions without mutating state. Apply mode stops a running or stale DAM daemon and removes DAM-managed routing while leaving local CA trust and vault data intact. It never contacts remote providers.

`setup_repair` combines `setup_rescue` preview/apply with a fresh `setup_plan` so humans and agents can run one local recovery check and immediately get the next setup action. `setup_diagnostics_export` returns an offline bundle with `doctor_report`, `setup_plan`, and a rescue preview. Neither function contacts remote providers; only confirmed repair apply mutates local daemon/routing state through the same rescue contract.

## Boundaries

The crate does not:

- start `dam-proxy`; the only lifecycle mutation is `setup_rescue` stopping a local DAM daemon during explicit recovery, including when called through confirmed `setup_repair`;
- mutate policy, vault entries, log entries, or consent grants;
- call real model providers;
- inspect request bodies;
- own CLI or HTML rendering.

Those concerns stay in `damctl`, `dam-web`, `dam-proxy`, and future daemon/integration modules.

`damctl doctor` may add CLI-local integration profile summaries after consuming `doctor_report`. Those summaries use `dam-integrations` inspection data and are not currently part of the shared web `/doctor` report. The CLI can pass a non-default state directory so tests and support sessions do not accidentally read the live user daemon/integration state.

## Current Consumers

- `damctl config check`
- `damctl doctor`
- `damctl setup plan`
- `dam doctor`
- `dam setup status`
- `dam setup plan`
- `dam setup next-action`
- `dam setup resume`
- `dam setup rescue`
- `dam setup repair`
- `dam setup export-diagnostics`
- `dam-web /doctor`
- `dam-web /api/v1/setup/plan`
- `dam-web /api/v1/setup/next-action`
- `dam-web /api/v1/setup/rescue`
- `dam-web /api/v1/setup/repair`
- `dam-web /api/v1/setup/diagnostics`
- `dam-mcp` setup tools

## Testing

Run:

```bash
cargo test -p dam-diagnostics
```
