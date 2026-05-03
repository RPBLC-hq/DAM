# dam-integrations

`dam-integrations` defines known local harness profiles for the background DAM proxy/interception endpoint and owns the deterministic apply/rollback engine behind those profiles.

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

`dam profile set <id>` writes the legacy active local harness profile under DAM's integration state directory. The tray/web Settings flow writes enabled app profile state under the same integration directory. `dam profile status` reports the active profile, enabled profiles, effective proxy URL, and apply state for enabled profile targets. `dam connect` uses enabled profiles when present and falls back to the active profile when no enabled state exists.

`dam integrations apply` previews explicit-proxy fallback setup by default. Add `--write` to call the `dam-integrations` apply engine and write profile setup to a safe target with a rollback record:

- `codex-api`, `codex-chatgpt`, `openai-compatible`, `anthropic`, and `xai-compatible` write DAM-managed proxy environment files that can be sourced or inspected as an explicit-proxy fallback.
- `claude-code` can update Claude Code `settings.json` by setting `env.HTTPS_PROXY` and `env.HTTP_PROXY`, and removes the old DAM-owned `env.ANTHROPIC_BASE_URL` override.

Preview without writing:

```bash
dam integrations apply codex-api
```

Write the profile setup:

```bash
dam integrations apply codex-api --write
```

Override the target file for tests or non-standard installs:

```bash
dam integrations apply codex-api --write --target-path ./codex-test.env
dam integrations apply claude-code --write --target-path ./.claude/settings.local.json
```

Rollback restores the last DAM-created backup for that profile:

```bash
dam integrations rollback codex-api
```

Setup and connect:

```bash
dam trust install-local-ca --yes
dam network install-network-extension --yes
dam connect --profile claude-code
dam profile set claude-code
dam connect --network-mode tun --trust-mode local_ca
```

The tray Connect flow performs the required Network Extension routing and trust setup before starting proxy-routed app protection. Direct CLI use of these profiles also needs `local_ca` readiness because DAM must decrypt selected provider HTTPS/WSS traffic to protect request bodies.

All apply callers, including `dam integrations apply --write`, `dam connect --profile <id> --apply`, and `dam connect --apply` with enabled or legacy active profiles, refuse to overwrite a target that DAM previously applied but that no longer matches DAM's desired content. Use `damctl integrations check <id>` to inspect that state, or `dam integrations rollback <id>` to restore the last DAM-created backup. The tray/web Connect happy path calls apply for enabled CLI profiles because macOS PAC system proxy does not reliably capture CLI networking.

Use `--json` on `list` or `show` for machine-readable profile data:

```bash
dam integrations list --json
dam integrations show codex-api --json
```

Use `--proxy-url` to render snippets for a non-default daemon endpoint:

```bash
dam integrations show anthropic --proxy-url http://127.0.0.1:7829
```

When `--proxy-url` is omitted, `dam` uses the connected daemon state if available. Otherwise it renders the default local endpoint `http://127.0.0.1:7828`.

## Current Profiles

| Profile | Purpose | Daemon target |
|---|---|---|
| `openai-compatible` | Generic OpenAI-compatible SDK or harness using DAM as its HTTP(S) proxy while keeping the normal provider endpoint. | OpenAI-compatible default upstream. |
| `anthropic` | Generic Anthropic-compatible harness using DAM as its HTTP(S) proxy while keeping the normal Anthropic endpoint. | Anthropic default upstream. |
| `claude-code` | Claude Code using DAM as its HTTP(S) proxy while keeping the normal Anthropic endpoint. | Anthropic default upstream. |
| `codex-api` | Codex API-key mode using DAM as its HTTP(S) proxy while keeping the normal OpenAI endpoint. | OpenAI-compatible default upstream. |
| `codex-chatgpt` | Codex ChatGPT-login mode using Network Extension capture and the WebSocket adapter while keeping the normal ChatGPT login/session flow. | `https://chatgpt.com` through the OpenAI-compatible target shape. |
| `xai-compatible` | xAI traffic using DAM as its HTTP(S) proxy while keeping the normal xAI endpoint. | `https://api.x.ai` through the OpenAI-compatible provider adapter. |

## Apply Contract

`dam-integrations` owns:

- enabled app profile state and legacy active local profile state;
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

- `claude-code` edits a known harness settings file directly with a rollback record.
- Other profiles write DAM-managed proxy environment files for explicit-proxy fallback rather than mutating shell, Codex provider config, or unknown harness config.
- No model discovery is performed.
- `dam-integrations` does not install system proxy, Network Extension, TLS trust, or protocol adapters. Claude Code and Codex proxy routing require local CA readiness when DAM decrypts selected Anthropic/OpenAI/ChatGPT traffic.
- `dam connect --profile <id>` starts one explicit profile target. `dam connect` with multiple enabled profiles can start one daemon with multiple provider targets. `--apply` additionally writes reversible profile setup when explicitly requested.
