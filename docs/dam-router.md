# dam-router

Status: implemented first extraction.

`dam-router` owns reusable proxy route decisions for the app-layer LLM proxy path. It does not serve HTTP, forward provider requests, run detection or policy, open backends, or build DAM-owned HTTP responses.

## Responsibilities

Current first slice:

```text
proxy config
  -> first configured target
  -> matched-route target choice
  -> effective failure mode

request headers
  -> auth mode decision
  -> caller passthrough, target API key injection, or config_required
```

Provider labels are target/profile metadata, not a Rust enum. The route table supports multiple configured targets. Direct app-layer requests use the first configured target. Intercepted traffic can select a specific target from the matched `TrafficRoute` metadata supplied by the active traffic profile. `dam-router` does not infer providers from API paths or provider headers. Generic website profile creation/import is parked in the current app.

Transparent host classification for system-proxy/TUN routing lives in `dam-net`, not in `dam-router`. `dam-router` still owns target/auth/failure decisions after `dam-proxy` has identified an active traffic route from the transparent request authority.

## Auth Decisions

`dam-router` returns one of three auth modes for a request:

- `CallerPassthrough`: DAM forwards caller auth from the local tool or harness.
- `TargetApiKey`: DAM injects the resolved target API key from config/env.
- `ConfigRequired`: the target names an API-key env var, no value resolved, and the request does not include any configured caller-auth header.

Caller-auth headers and target-key injection headers are configured on the selected target/profile route. If no caller-auth headers are configured, missing target API keys do not force `config_required`.

## Boundaries

The crate does not:

- parse or transform request bodies;
- classify transparent hosts or parse traffic profiles;
- forward requests to providers;
- emit log events;
- construct `dam-api` reports;
- decide provider-down behavior after forwarding starts.

Those responsibilities stay in `dam-proxy`, `dam-pipeline`, and protocol adapter crates.

## Current Consumer

- `dam-proxy` uses `dam-router` for startup provider validation, effective failure mode, health config-required checks, and per-request auth decisions.

## Testing

Run:

```bash
cargo test -p dam-router
```
