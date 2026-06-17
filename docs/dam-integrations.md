# dam-integrations

`dam-integrations` loads known local harness profiles for the background DAM proxy/interception endpoint and owns the deterministic apply/rollback engine behind those profiles.

The first slice is intentionally local and reversible. It does not install system proxy settings or write secrets. It tells `dam` and future installer/tray surfaces how a harness should route normal provider traffic through the connected daemon, and it prepares safe file mutations with backup records when a profile has a known write path.

## User Commands

```bash
dam integrations list
dam integrations show <profile>
dam integrations apply <profile>
dam integrations apply <profile> --write
dam integrations rollback <profile>
dam profile status
dam profile set <profile>
dam profile clear
dam connect --profile <profile>
dam connect --apply
```

`dam integrations list` shows known profiles. `dam integrations show` renders the local settings and command snippets for one profile. `dam connect --profile` uses daemon-side defaults for profiles that need a specific provider/upstream. Add `--apply` only when you also want to write that profile's reversible local setup before connecting.

Connect app profiles are JSON files. Bundled profiles ship under `crates/dam-integrations/profiles/` and are seeded into `$DAM_STATE_DIR/integrations/profiles/<id>.json`. Runtime Settings and CLI lists are loaded from that same profile folder plus bundled JSON. A valid new profile JSON file in that folder appears as a profile without Rust code changes. Bundled profile aliases are defined in JSON and normalize old IDs such as `codex` to the current profile ID during reads and migrations. Future imported or user-authored profiles use the same state directory and one JSON file per profile, but the in-app profile creator/import/export workflow is parked. Adding a bundled profile is a data change: create a JSON profile with display metadata, connect args, optional explicit-proxy fallback settings, and `traffic_app_ids` that map to app IDs in the active `dam-net` traffic profile.

`dam profile set <id>` writes the legacy active local harness profile under DAM's integration state directory. The tray/web Settings flow writes enabled app profile state under the same integration directory. `dam profile status` reports the active profile, enabled profiles, effective proxy URL, and apply state for enabled profile JSON targets. `dam connect` uses saved enabled profiles when present, falls back to the active profile when no enabled state exists, and otherwise defaults to the bundled MVP profiles (`claude`, `chatgpt`). Imported/custom profile JSON stays loadable but opt-in until selected explicitly. During connect, enabled profile IDs become `traffic.enabled_apps` runtime overrides, so only the selected traffic profile apps are mediated by the daemon. Settings reconnects also pass the selected traffic app IDs and route-derived proxy targets explicitly, which keeps ChatGPT subscription WebSocket capture in the live daemon when the `chatgpt` profile is enabled. An explicit enabled-profile file with zero profiles is meaningful: it disables all bundled traffic-profile app mediation instead of falling back to default routes.

`dam integrations apply` previews the selected profile file operation. With the default catalog path, `--write` ensures `$DAM_STATE_DIR/integrations/profiles/<id>.json` contains the current bundled JSON for that known profile. With an explicit `--target-path`, it writes a rendered JSON export and creates a rollback record before changing that target. Legacy rollback records from the old `$DAM_STATE_DIR/integrations/profiles/<id>/` layout are migrated to `$DAM_STATE_DIR/integrations/apply-records/<id>/` before catalog files are seeded or rollback/apply status is read. Retired bundled profile JSON files are ignored until import/create/export semantics are reintroduced. This is not part of Connect onboarding; installed builds use Network Extension capture as the primary path.

- `chatgpt` uses a DAM-managed JSON profile file and does not mutate ChatGPT, OpenAI API, Codex, shell startup files, or provider credentials.
- `claude` uses a DAM-managed JSON profile file and does not mutate Claude settings.

Preview without writing:

```bash
dam integrations apply chatgpt
```

Ensure the catalog profile file:

```bash
dam integrations apply chatgpt --write
```

Override the target file for tests or non-standard rendered exports:

```bash
dam integrations apply chatgpt --write --target-path ./chatgpt-test.json
dam integrations apply claude --write --target-path ./claude-test.json
```

Rollback restores the last DAM-created backup for that profile when an apply wrote a backup-backed target:

```bash
dam integrations rollback chatgpt
```

Setup and connect:

```bash
dam trust install-local-ca --yes
dam network install-network-extension --yes
dam connect --profile claude
dam profile set claude
dam connect --network-mode tun --trust-mode local_ca
```

The tray Connect flow performs the required Network Extension routing and trust setup before starting proxy-routed app protection. Direct CLI use of these profiles also needs `local_ca` readiness because DAM must decrypt selected provider HTTPS/WSS traffic to protect request bodies.

All backup-backed apply callers refuse to overwrite a target that DAM previously applied but that no longer matches DAM's desired content. Use `damctl integrations check <id>` to inspect that state, or `dam integrations rollback <id>` to restore the last DAM-created backup.

Use `--json` on `list` or `show` for machine-readable profile data:

```bash
dam integrations list --json
dam integrations show chatgpt --json
```

Use `--proxy-url` to render snippets for a non-default daemon endpoint:

```bash
dam integrations show chatgpt --proxy-url http://127.0.0.1:7829
```

When `--proxy-url` is omitted, `dam` uses the connected daemon state if available. Otherwise it renders the default local endpoint `http://127.0.0.1:7828`.

## Current Profiles

| Profile | Purpose | Daemon target |
|---|---|---|
| `claude` | Claude, Claude Code, Anthropic API, Anthropic Console, Claude MCP proxy, and Claude platform traffic using DAM capture while keeping normal endpoints. | `traffic_app_ids = ["anthropic-api", "claude-web", "anthropic-console", "claude-mcp-proxy", "claude-platform"]`. |
| `chatgpt` | ChatGPT, OpenAI API, OpenAI platform, and Codex traffic using DAM capture while keeping normal endpoints or subscription login behavior. | `traffic_app_ids = ["openai-api", "openai-platform", "chatgpt-web", "chatgpt-legacy-web"]`; the profile aliases retired `codex` IDs for upgrade compatibility. |

## Apply Contract

`dam-integrations` owns:

- enabled app profile state and legacy active local profile state;
- bundled JSON profile loading from `crates/dam-integrations/profiles/`;
- state-backed profile definitions under `profiles/*.json`;
- default target path selection for known profiles;
- desired file content generation;
- dry-run planning;
- install-state inspection for `applied`, `needs_apply`, and `modified` profile targets;
- backup creation with unique backup directories;
- rollback record format written before target mutation so interrupted applies remain reachable;
- atomic target restore/write behavior using temporary files and rename where the filesystem supports it;
- rollback restore/delete behavior.

The `dam` binary owns the user command surface and supplies local environment context, including `DAM_STATE_DIR`, `HOME`, and the effective proxy URL.

## Privacy Rules

Profiles must not contain raw sensitive values, provider API keys, auth headers, or vault values.

Profiles may contain:

- local DAM proxy URLs;
- provider names and upstream URLs;
- environment variable names;
- command-line flags;
- notes explaining where the harness should keep its own provider credentials.

## Current Limits

- Profile files are JSON. The old direct harness-settings/env-file mutation path is retired for bundled profiles.
- No model discovery is performed.
- `dam-integrations` does not install system proxy, Network Extension, TLS trust, or protocol adapters. Profile routing requires local CA readiness when DAM decrypts selected HTTPS/WSS traffic.
- `dam connect --profile <id>` starts one explicit profile target and enables the matching traffic app IDs. `dam connect` defaults to the bundled MVP profiles (`claude`, `chatgpt`) when no saved state or explicit profile is present. Imported/custom profile JSON remains opt-in. `dam connect` with multiple enabled profiles can start one daemon with multiple provider targets and a narrowed active traffic profile. `--apply` additionally ensures selected catalog profile JSON when explicitly requested.
