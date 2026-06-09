# DAM Module Docs

This folder documents the current DAM rebuild modules.

The architecture rule is: modules stay replaceable, and cross-module coordination goes through spine-owned contracts in `dam-core`.

Deferred security and product-design work is tracked in [parking-lot.md](parking-lot.md). Parking-lot items are not current product guarantees.

DAM is designed for macOS, Linux, and Windows. Platform-specific routing, trust, tray, and packaging implementations may land in staged slices, but partial or delayed platform behavior must be tracked in [parking-lot.md](parking-lot.md) or the relevant module parking-lot doc.

## Modules

- [dam-core](dam-core.md): shared contracts, reference generation, replacement planning, policy actions, log event shape.
- [dam](dam.md): local UX entry point for `connect/status/logs/disconnect`, integration profiles, and the npm wrapper.
- [dam-api](dam-api.md): shared JSON/report/status DTOs for CLIs, proxy status, health, and future automation.
- [dam-config](dam-config.md): layered runtime config for defaults, TOML, env, and CLI overrides.
- [dam-consent](dam-consent.md): canonical-value passthrough grants with TTL and revocation.
- [dam-daemon](dam-daemon.md): background local proxy lifecycle, pause/resume protection state, state file, and `dam connect/status/disconnect` support.
- [dam-diagnostics](dam-diagnostics.md): shared local readiness checks, setup-plan/next-action, repair, rescue, and diagnostics-export contract for CLI, web/API, and MCP.
- [dam-intercept](dam-intercept.md): guarded TLS interception activation contract for transparent routes.
- [dam-integrations](dam-integrations.md): JSON local harness profiles, enabled app state, and legacy active profile state for `dam integrations`, `dam profile`, and `dam connect --profile`.
- [damctl](damctl.md): local status and config diagnostics CLI.
- [dam-detect](dam-detect.md): pure rule-based sensitive value detection.
- [dam-e2e](dam-e2e.md): process-level end-to-end tests across the local binaries.
- [dam-policy](dam-policy.md): maps detections to `tokenize`, `redact`, `allow`, or `block`.
- [dam-pipeline](dam-pipeline.md): shared text processing orchestration for detect, policy, consent, vault/log events, redaction, and inbound reference resolution.
- [dam-http-adapter](dam-http-adapter.md): generic HTTP upstream forwarding, configured auth/header injection, JSON/JSON-lines response transforms, and SSE text-delta response transforms for proxy flows.
- [dam-provider-common](dam-provider-common.md): shared response transform utilities for JSON/JSON-lines string-value, raw stream, and SSE text-delta transforms.
- [dam-router](dam-router.md): proxy target selection, matched-route target choice, auth mode, and failure-mode decisions.
- [dam-vault](dam-vault.md): local SQLite `VaultWriter` and `VaultReader` implementation.
- [dam-log](dam-log.md): local SQLite `EventSink` implementation, Activity values, and bounded indexed log queries for UI surfaces.
- [dam-net](dam-net.md): network capture-mode vocabulary, generic traffic profile contracts, routing readiness, capture backend status, protocol adapter status, and profile-derived host classification.
- [dam-net-macos](dam-net-macos.md): macOS PAC system-proxy install/remove plus Network Extension capture planning/status for `tun`.
- [dam-trust](dam-trust.md): TLS trust-mode vocabulary, local CA artifacts, leaf issuance, macOS trust install/remove, readiness contracts, and trusted host scope for transparent protection.
- [dam-redact](dam-redact.md): pure replacement application.
- [dam-filter](dam-filter.md): CLI pipeline wiring detection, policy, vault, logs, and redaction.
- [dam-resolve](dam-resolve.md): CLI pipeline for resolving `[kind:id]` references through `VaultReader`.
- [dam-proxy](dam-proxy.md): generic mediation runtime with MVP LLM HTTP/WebSocket adapters plus daemon-gated HTTP/1.1 CONNECT/TLS for ready profile routes.
- [dam-web](dam-web.md): local web UI for setup-plan-driven Connect/app controls, Settings, Wallet value/allow management, bounded Activity, and diagnostics.
- [dam-tray](dam-tray.md): native desktop shell that hosts the Connect surface from the local web UI.
- [dam-mcp](dam-mcp.md): MCP tools for agent status/setup inspection and consent operations.

## Current Pipeline

```text
input text
  -> dam-pipeline expands actively allowed DAM references when VaultReader is available
  -> dam-detect
  -> dam-policy
  -> dam-consent active canonical-value overrides
  -> dam-core replacement plan
  -> dam-vault only for tokenize decisions
  -> dam-redact
  -> stdout

optional dam-api JSON report
  -> stderr

dam-core also builds non-sensitive log events
  -> dam-log when enabled
```

Replacement planning deduplicates repeated equal canonical values by default, and compatible vault writers reuse an existing canonical reference for the same stored value. Current email canonicalization removes detector-supported whitespace inside the address and lowercases the domain before storage/deduplication. Domain-only values are not detected globally; proxy callers can pass email-derived domains back as related context for inbound response redaction. API-key detections currently cover common assignment labels plus direct OpenAI, Anthropic, GitHub, Stripe API-key/webhook-signing-secret, Google, AWS access-key-ID, PEM private key block, database connection URL, and `Bearer`-labeled JWT families, and they preserve the detected secret exactly. Set `policy.deduplicate_replacements = false` to issue a distinct reference per occurrence when repeated-reference equality is too revealing.

## Resolve Pipeline

```text
input text with [kind:id] references
  -> dam-core reference parser
  -> dam-vault through VaultReader
  -> dam-core resolve plan
  -> stdout

optional dam-api JSON report
  -> stderr

dam-core also builds non-sensitive resolve log events
  -> dam-log when enabled
```

## Proxy Pipeline

```text
LLM request
  -> dam-proxy
  -> dam-router
  -> dam-pipeline
  -> dam-vault through VaultReader for actively allowed references
  -> dam-detect
  -> dam-policy
  -> dam-consent active canonical-value overrides
  -> dam-core replacement plan
  -> dam-vault only for tokenize decisions
  -> dam-redact
  -> dam-log
  -> dam-http-adapter
  -> upstream provider

provider response
  -> dam-http-adapter
  -> dam-pipeline
  -> dam-core reference parser
  -> dam-vault through VaultReader
  -> dam-core resolve plan
  -> dam-log
  -> LLM client
```

Proxy defaults are directional: profile-matched outbound requests are tokenized before the provider sees them, and those automatic detections create Activity log values and token-vault mappings but do not write to Wallet. Active consent applies to canonical detected values and, in proxy flows with a vault reader, previously tokenized outbound DAM references for that same allowed value. JSON-shaped outbound request bodies are protected string-by-string after JSON decoding so escaped newlines and similar JSON escapes cannot be rewritten into invalid JSON, and changed object keys keep their input order. Agent traffic apps resolve known DAM references in inbound local transcripts when global inbound resolution is enabled, while raw inbound HTTP response redetection/redaction is explicit per route through traffic profile `inbound.protect_sensitive_data` and does not write to Wallet. Domains learned from outbound email detections are retained as request or WebSocket context so opted-in inbound protection can redact the same standalone company domain if a provider repeats it. JSON-shaped responses are transformed string-by-string, including newline-delimited JSON, when reference restoration or explicit raw inbound protection is active. `text/event-stream` responses are transformed under the same route policy; provider-aware SSE text-delta parsing handles references and opted-in raw values split across adjacent OpenAI-compatible or Anthropic JSON delta events with a bounded event window, while raw streams still use tail-buffered transformation. The ChatGPT-login WebSocket MVP freezes protection state at connection start, strips WebSocket extension negotiation, protects unfragmented client and server text frames on protected connections, decodes JSON text frames before rewriting string values, and buffers adjacent outbound text-delta/raw text frames only when needed to complete a partial sensitive value; fragmented, binary, or compressed frames close protected connections instead of passing through raw.

`dam-pipeline`, `dam-provider-common`, `dam-http-adapter`, and `dam-router` have been extracted from the first compact proxy implementation.

## Control And Diagnostics

```text
dam connect
  -> background daemon process
  -> dam-proxy
  -> HTTP(S) proxy / transparent route for active traffic profile apps
  -> pass-through provider auth

dam status / dam disconnect
  -> daemon state file
  -> dam-proxy /health when connected

dam logs
  -> local dam-log SQLite store
  -> concise non-sensitive operation summaries or event timelines

dam doctor / dam setup status / dam setup plan / dam setup next-action / dam setup resume / dam setup rescue / dam setup repair / dam setup export-diagnostics
  -> dam-diagnostics
  -> machine-readable install/resume/recovery contract

/api/v1/setup/rescue / dam_setup_rescue
  -> dam-diagnostics setup_rescue
  -> confirmed local recovery for API/MCP agents

/api/v1/setup/repair / dam_setup_repair
  -> dam-diagnostics setup_repair
  -> confirmed local recovery plus a fresh setup plan

/api/v1/setup/diagnostics / dam_setup_export_diagnostics
  -> dam-diagnostics setup_diagnostics_export
  -> offline doctor/setup/rescue-preview bundle for support and agents

dam profile
  -> enabled JSON app profile state and legacy active harness profile state

dam integrations list/show/apply/rollback
  -> dam-integrations JSON profile catalog
  -> local proxy URL and harness setup snippets

damctl status
  -> dam-proxy /health
  -> dam-api ProxyReport

damctl doctor
  -> dam-diagnostics
  -> dam-integrations apply-state summary
  -> dam-api HealthReport

damctl bypass status
  -> dam-config
  -> proxy/vault/log failure-mode report

damctl daemon inspect
  -> dam-daemon state file
  -> dam-net routing readiness
  -> dam-intercept guarded interception readiness

damctl network inspect
  -> dam-net-macos routing state
  -> dam-net route readiness

dam network install-system-proxy / remove-system-proxy
  -> dam-net-macos macOS all-proxyable HTTP/HTTPS PAC routing with rollback

dam network install-network-extension / remove-network-extension / status
  -> dam-net-macos macOS Network Extension capture state for tun mode

dam startup status / skip-open-at-login
  -> local startup setup choice for tray and scripted installs

damctl trust inspect
  -> dam-trust readiness and action plans

dam trust generate-local-ca / delete-local-ca / install-local-ca / remove-local-ca
  -> dam-trust local CA artifacts and explicit macOS system trust changes

damctl integrations check
  -> dam-integrations apply-state inspection

damctl config check
  -> dam-diagnostics
  -> dam-api HealthReport

dam-web /connect
  -> dam-integrations enabled profiles and apply-state inspection
  -> in-memory protected-state/request trigger for local Connect QA until dam-notify owns delivery
  -> dam connect/disconnect pause-resume control

dam-tray
  -> native desktop shell
  -> hosted dam-web /connect

dam-web /health
  -> dam-config
  -> dam-diagnostics
  -> dam-proxy /health when enabled
  -> dam-api HealthReport + ProxyReport

dam-web /api/v1/setup/plan and /api/v1/setup/next-action
  -> dam-diagnostics setup plan
  -> agent-readable setup state without UI copy dependencies

dam-mcp
  -> read-only status/setup tools
  -> gated consent tools
```

## Config Precedence

From lowest to highest priority:

1. Built-in defaults.
2. `dam.toml`, `--config <path>`, or `DAM_CONFIG`.
3. Environment variables.
4. CLI overrides.

Use [../dam.example.toml](../dam.example.toml) as the local starting point.

## Verification

```bash
scripts/dam-build.sh agent-check
scripts/dam-build.sh agent-protection-smoke
```

The build/release entrypoint in [build-release.md](build-release.md) wraps local verification, source builds, local API-through-DAM protection smoke testing, signed macOS app packaging, notarization, local deploy, installed-app verification, restart, and status steps so local, CI, and agent workflows use the same command surface.

Run only the E2E suite with:

```bash
cargo test -p dam-e2e
```
