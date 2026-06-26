# dam-e2e

`dam-e2e` is the process-level end-to-end test package.

It tests multiple DAM binaries and modules together using temp SQLite databases, synthetic data, fake upstreams, and no real provider calls. The repo also includes a local llama.cpp smoke script for PR verification when a loopback OpenAI-compatible endpoint is available.

## Scope

Current E2E coverage:

- `dam-filter -> dam-vault -> dam-log -> dam-resolve` roundtrip.
- Token reordering before resolve, proving resolution is keyed by `[kind:id]` and not token order.
- `dam-web` smoke test against vault/log DBs populated by `dam-filter`.
- `dam-proxy` through a fake OpenAI-like upstream, proving raw sensitive values are redacted before upstream receives the request and resolved before the local client receives the response.
- `dam-proxy` ChatGPT WebSocket route smoke through a deterministic loopback upstream, proving outbound text frames on the `chatgpt-web` profile route are tokenized before upstream egress.
- `dam-proxy` inbound resolution setting coverage in module tests, including `--no-resolve-inbound`.
- `dam-proxy -> dam-vault -> dam-log -> dam-resolve` restoration of the protected upstream payload.
- removed legacy tool launcher commands are not exposed by the `dam` CLI.
- Persisted log privacy checks for raw sensitive values.

## How It Runs

The E2E tests build the real binaries first:

```text
dam
dam-filter
dam-resolve
dam-proxy
dam-web
```

Then tests invoke the binaries from `target/debug` against temp directories. This keeps `cargo test -p dam-e2e` usable without relying on package-local `CARGO_BIN_EXE_*` variables.

## Run

```bash
cargo test -p dam-e2e
```

Full workspace verification:

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Local API-through-DAM smoke test for PR evidence:

```bash
python3 scripts/rpblc_dam_local_llm_e2e_smoke.py --upstream http://127.0.0.1:8080
```

The script builds `dam-proxy`, starts it on loopback with temporary vault/log SQLite files, and by default runs a representative MVP known-provider route matrix: `openai-api` (`openai` target / `openai-compatible` provider), `anthropic-api` (`anthropic` target / `anthropic` provider), and `claude-web` (`claude-web` target / `generic-http` provider). For each route, it sends synthetic email/SSN values through DAM to the local OpenAI-compatible upstream, records the route ID, target name, and provider in the JSON proof output, verifies exact echo resolution on the trusted client side, verifies a token-transformation prompt that asks the model to insert whitespace after reference opening brackets cannot reconstruct the raw values, fails if the local activity log database contains the synthetic values, scopes fake-upstream transcript assertions to that route when `GET /__dam/transcript` is available, and removes the temp directory unless `--keep-temp` is passed. Exit code `2` means the local upstream or binary prerequisite is unavailable; exit code `1` means DAM failed the smoke check. Use `--route openai-api`, `--route anthropic-api`, or `--route claude-web` to isolate one route while debugging.

When no local model endpoint is listening, use the deterministic loopback fake upstream instead of skipping the proxy path:

```bash
python3 scripts/dam_fake_openai_upstream.py --port 18080
DAM_AGENT_E2E_UPSTREAM=http://127.0.0.1:18080 scripts/dam-build.sh agent-protection-smoke
scripts/dam-build.sh agent-websocket-smoke
```

For low-risk VPS dogfooding proof, keep DAM in explicit-proxy mode only and verify the shared proxy + Activity + pending-consent path together:

```bash
python3 scripts/dam_fake_openai_upstream.py --port 18080
DAM_AGENT_E2E_UPSTREAM=http://127.0.0.1:18080 \
DAM_AGENT_STATE_DIR="$HOME/.dam-hermes" \
scripts/dam-build.sh agent-dogfood-verify
```

That verifier starts loopback `dam-proxy` and `dam-web` against the same state directory, proves the upstream saw DAM tokens rather than raw synthetic values, checks `/api/v1/activity?since=0&after_id=<baseline>` so the feed must include evidence from the current verification run rather than stale rows, and exercises the local pending-consent request path with `/api/v1/requests/trigger` plus `allow-once`. It does **not** enable system proxy, `tun`, or trust-store mutation. By default `scripts/dam-build.sh agent-dogfood-verify` allocates an isolated free loopback web port for the temporary proof `dam-web`; set `DAM_AGENT_E2E_WEB_ADDR` only when you need to pin a specific isolated address.

To route agent HTTP clients through this mode, print the explicit proxy exports and source them in the agent shell or service environment:

```bash
python3 scripts/dam_vps_dogfood_verify.py env
```

For a persistent VPS watchdog, supervise the loopback `dam-proxy` and `dam-web` processes separately with a user-level service manager and run `agent-dogfood-verify` from cron/timer as the daily synthetic proof. Keep the runtime loopback-only, keep state under `~/.dam-hermes`, and alert only when the verifier returns blocked or failed status.

## Rules

- Use synthetic data only.
- Use temp databases only.
- Do not call OpenAI, Anthropic, OpenRouter, or other real providers.
- Prefer fake upstreams and local processes.
- Assert that persisted logs do not contain raw sensitive values.
- For DAM PR readiness, run the local smoke script against a loopback OpenAI-compatible endpoint when available and include the command, upstream, proxy address, expected/actual results, and cleanup status in the PR/report.
