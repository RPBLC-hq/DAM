# dam-pipeline

Status: implemented first extraction.

`dam-pipeline` owns shared text-processing orchestration for proxy/API-style flows. It does not own HTTP serving, upstream target selection, provider auth, provider request forwarding, CLI argument parsing, or persistence backends.

## Responsibilities

Outbound protection:

```text
input text
  -> expand actively allowed DAM references through VaultReader when provided
  -> dam-detect
  -> dam-policy
  -> dam-consent active canonical-value overrides
  -> dam-core replacement plan
  -> VaultWriter for tokenize decisions
  -> dam-redact
  -> protected text or blocked result
```

Inbound reference resolution:

```text
input text with [kind:id] or \[kind:id\] references
  -> dam-core reference parser
  -> VaultReader
  -> dam-core resolve plan
  -> restored text when at least one reference resolves
```

Before detection, callers may provide both a consent store and `VaultReader`. In that mode the pipeline expands previously tokenized `[kind:id]` references only when the reference resolves and the stored canonical value has active consent for the caller's consent scopes. Missing, unreadable, expired, revoked, or wrong-scope references remain tokenized and continue through the normal protection path. Callers that do not pass scopes use the global consent scope only. Proxy callers pass the matched route target scope, for example `target:anthropic` or `target:chatgpt-web`. Domain-only values are not detected or redacted; email detection treats sentence punctuation after a normal domain as a boundary, so a prompt like `alice@example.com. What...` stores `alice@example.com` without separately storing `example.com`.

`dam-pipeline` records non-sensitive filter, consent, vault, redaction, read, and resolve events through the `EventSink` contract when a sink is provided.

## Boundaries

The crate does not:

- parse provider-specific JSON or SSE shapes;
- decide proxy target, auth mode, or failure mode;
- open SQLite databases;
- create HTTP responses;
- scan or transform non-UTF-8 bytes;
- incrementally resolve streaming/SSE responses.

Those responsibilities stay with caller, provider, or router crates.

HTTP upstream forwarding lives in `dam-http-adapter`, configured auth behavior lives on the selected target/profile route, and proxy route decisions live in `dam-router`.

## Current Consumers

- `dam-proxy` uses `dam-pipeline` for outbound request body protection and default inbound reference resolution. Streaming/SSE bodies are passed to this pipeline by protocol adapters after either raw tail-buffering or provider-aware text-delta reassembly, depending on the response shape.
- When a route explicitly enables raw inbound protection, `dam-proxy` reuses the detection/redaction pipeline on inbound HTTP response text after reference resolution has no output, so raw provider-returned sensitive values are redacted before local agent history records them. These inbound detections are not written to Wallet. Resolved DAM references are still restored for the local client when inbound reference resolution is enabled.

`dam-filter` still owns its CLI-specific pipeline wiring because it also owns report emission, exit codes, and file/stdin handling.

## Testing

Run:

```bash
cargo test -p dam-pipeline
```
