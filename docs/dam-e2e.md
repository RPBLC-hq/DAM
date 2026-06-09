# dam-e2e

`dam-e2e` is the process-level end-to-end test package.

It tests multiple DAM binaries and modules together using temp SQLite databases, synthetic data, fake upstreams, and no real provider calls. The repo also includes a local llama.cpp smoke script for PR verification when a loopback OpenAI-compatible endpoint is available.

## Scope

Current E2E coverage:

- `dam-filter -> dam-vault -> dam-log -> dam-resolve` roundtrip.
- Token reordering before resolve, proving resolution is keyed by `[kind:id]` and not token order.
- `dam-web` smoke test against vault/log DBs populated by `dam-filter`.
- `dam-proxy` through a fake OpenAI-like upstream, proving raw sensitive values are redacted before upstream receives the request and resolved before the local client receives the response.
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

The script builds `dam-proxy`, starts it on loopback with temporary vault/log SQLite files, sends synthetic email/SSN values through DAM to the local OpenAI-compatible upstream, verifies exact echo resolution on the trusted client side, verifies a token-transformation prompt that asks the model to insert whitespace after reference opening brackets cannot reconstruct the raw values, fails if the local activity log database contains the synthetic values, and removes the temp directory unless `--keep-temp` is passed. Exit code `2` means the local upstream or binary prerequisite is unavailable; exit code `1` means DAM failed the smoke check.

## Rules

- Use synthetic data only.
- Use temp databases only.
- Do not call OpenAI, Anthropic, OpenRouter, or other real providers.
- Prefer fake upstreams and local processes.
- Assert that persisted logs do not contain raw sensitive values.
- For DAM PR readiness, run the local smoke script against a loopback OpenAI-compatible endpoint when available and include the command, upstream, proxy address, expected/actual results, and cleanup status in the PR/report.
