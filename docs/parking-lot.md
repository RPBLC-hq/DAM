# DAM Parking Lot

This file tracks important design work that is intentionally not treated as complete in the current rebuild.

Parking-lot items are not current product guarantees. Move an item out of this file only after the implementation, docs, and tests all agree on the shipped behavior.

Platform rule: DAM must be designed for macOS, Linux, and Windows. Platform-specific implementations may be staged when they require platform-local development or testing, but partial, delayed, or unavailable platform behavior must remain visible in this file or a module parking-lot doc until implementation, docs, and tests agree on the shipped behavior.

## Pre-Release Network Extension Recovery Gate

Current state: macOS Network Extension setup has a local recovery gate after helper install. DAM records active capture only after the helper reports the manager configured, enabled, and connected. If that verification fails, DAM asks the helper to remove the DAM Network Extension manager, records `rolled_back`, and returns onboarding to a repair/retry step. If removal fails, setup blocks on the local repair command instead of starting protection. This is covered by unit tests with fake helper status, but still needs installed-app validation against real macOS states before release.

Release blocker: do not ship the macOS Network Extension path as production-default until the local recovery gate is validated in a notarized installed app and the remaining restart/safe-mode cases are covered.

Parked work:

- Validate the activation watchdog after the protection layer starts in a notarized installed app. If the Network Extension does not reach connected status and pass the local readiness gate within a short timeout, DAM must automatically disable/remove the DAM Network Extension configuration and return onboarding to the correct repair step.
- Add safe-mode startup detection for interrupted activation across app/process restarts. If the previous run did not confirm healthy connectivity, DAM starts with network protection disabled and shows repair/onboarding instead of retrying silently.
- Add tray UI affordances for the CLI/API/MCP recovery contract: disable network protection, remove DAM network configuration, repair network setup, and export diagnostics.
- Add a user/admin routing failure policy setting before release. Runtime enforcement now defaults to fail-open and closes already-captured Network Extension flows when DAM is paused, unhealthy, unreachable, or otherwise not `protected`; the remaining parked work is exposing fail-open/fail-closed as an explicit setting and managed-install policy.
- Replace any long spinner in onboarding with explicit states: requested, waiting for macOS approval, configured, enabled, connected, failed, and rolled back.
- Keep a degraded fallback where DAM can run without system-wide Network Extension protection and clearly says protection is not active.
- Document the managed-install path for enterprise/MDM pre-approval, while keeping unmanaged Macs guided and recoverable.
- Add tests or deterministic fixtures that cover deleted/disabled configuration, failed local readiness verification, restart after failed activation, failed automatic removal, and successful rollback in the installed-app path.

## Onboarding UX Test Automation

Current state: macOS onboarding is covered by Rust unit tests for setup-plan state reconciliation and native helper status parsing, plus manual packaged-app validation on a signed local build. The tray/WebView flow is not yet covered by browser automation because the macOS System Settings prompts and Network Extension manager state need a controlled simulator.

Parked work:

- Add Playwright coverage for the Connect onboarding checklist using mocked `/api/v1/connect` states for each one-action step: startup choice, System Extension approval, reboot, network configuration, manager enablement, manager start, local CA, and daemon start.
- Add a deterministic macOS helper/status fixture so tests can simulate deleted, disabled, enabled-disconnected, and connected Network Extension manager states without changing the developer machine.
- Verify the tray-width layout and CTA transitions in Playwright screenshots before treating onboarding UX as seamless.

## Codebase Structure And Test Division Cleanup

Current state: many Rust unit tests have been moved out of production module bodies into sibling `*_tests.rs` files, but the codebase still needs a deliberate readability pass over crate/module boundaries, large files, and type responsibilities. The goal is not churn; it is to make each crate easier to review, test, and reuse before more platform and protocol work lands.

Parked work:

- Review each crate for oversized `lib.rs`, `main.rs`, broad manager modules, and mixed-purpose files. Split meaningful behavior into focused files with clear ownership, keeping public contracts and command wiring thin.
- Review large structs, enums, traits, and impl blocks for unrelated responsibilities. Split coordination, state, protocol handling, platform glue, and persistence into smaller types or modules where that improves readability and reuse.
- Normalize test placement after the extraction pass: private-access unit tests stay in sibling test files, integration and CLI tests stay under each crate's `tests/` directory, and production files should not regain inline test modules.
- Before removing or materially rewriting any test during cleanup, identify whether it protects edge cases, migration behavior, privacy guarantees, security boundaries, failure modes, platform behavior, or another secondary contract. Preserve the test intent when the current assertion is obsolete.
- Update crate docs and architecture docs as module boundaries change so future agents can find the implementation without rediscovering the split from source alone.

## Integration Profile Catalog And Portability

Current state: the visible bundled app profile catalog is intentionally narrow: `claude-code` and the merged `codex` profile are available, but only `claude-code` is enabled by default when no user app-selection state exists. The merged Codex profile covers both OpenAI API-key traffic and ChatGPT subscription/login traffic through separate traffic app IDs, and must be explicitly enabled for now. Generic OpenAI-compatible, generic Anthropic-compatible, xAI-compatible, and split Codex API/ChatGPT-login profiles are removed from the visible catalog for now. Existing local state that references retired profile IDs is normalized where possible so upgrades do not break the Settings or Connect views.

Parked work:

- Reintroduce generic OpenAI-compatible, generic Anthropic-compatible, xAI-compatible, and other third-party app/service profiles only after the profile model has a first-class catalog/editor story.
- Add create, import, and export profile features for JSON integration profiles and traffic-profile app entries.
- Restore the Settings profile creator only after it writes validated one-profile-per-JSON files into `$DAM_STATE_DIR/integrations/profiles/` and reconciles traffic-profile app entries through the same import/export model.
- Add an in-app profile/config builder so a user can add a normal website or service by filling in host match rules, upstream, adapter kind, auth/header behavior, timeout, and outbound/inbound policy without writing Rust.
- Define profile validation, signing/trust metadata, versioning, conflict handling, and safe rollback semantics for imported profiles.
- Decide how imported profiles surface in Settings without making onboarding depend on explicit-proxy profile setup.
- Add fixtures proving retired profile IDs migrate cleanly and imported profiles cannot introduce secrets, unsafe upstreams, or unsupported protocol claims.

## Generic Profile Builder

Current state: HTTP forwarding is generic and auth/header behavior is profile/target data. The app still needs product support for authoring, validating, importing, exporting, and enabling those JSON profiles safely.

Parked work:

- Express provider/site differences in traffic-profile or integration-profile JSON: match rules, upstream, adapter kind, timeout, auth header policy, body parser mode, mutation-safe header policy, inbound resolution, and field/path include/exclude rules.
- Keep OpenAI, Anthropic, Codex, and arbitrary websites as bundled or user-created profiles that consume generic adapters, not as a reason to add provider crates.
- Add profile-builder validation so unsupported payloads, unsafe upstreams, secrets, and body-signature requirements are surfaced before a user enables a custom config.

## Consent Scope And Policy Depth

Current state: Wallet can add/remove stored values, allow a value for all profiles, allow it for selected integration profiles, revoke one recorded party, protect every grant for a value, and remove the value. Profile-level allows expand to target-scoped `dam-consent` grants, and the proxy passes the matched target scope into the protection pipeline. `created_by` remains UI/audit metadata; `scope` is the enforcement boundary.

Parked work:

- Add richer organization/user/purpose policy dimensions on top of global and target scopes.
- Add tests for custom imported profile scopes once user-created traffic profiles are implemented.
- Decide whether non-proxy MCP/filter consent tools should expose scoped grant creation or stay global-only.

## Security And Privacy Design Work

### Full-Device Routing And TLS Trust

Current state: local protection is app-layer routing for supported AI harnesses, explicit proxy paths, the macOS `system_proxy` fallback, and macOS Network Extension control-plane support for `tun`. `dam-net` defines capture-mode/backend vocabulary, protocol adapter readiness, routing readiness, and host-only traffic-profile classification for the effective traffic profile registry. `dam-tray` owns macOS System Extension activation from `DAM.app`, and `dam-net-macos` can install/remove macOS PAC routing for proxy-capable HTTP/HTTPS traffic with rollback and configure Network Extension capture through a signed helper/app bundle. `dam-proxy` passes unknown hosts through untouched and has a daemon-gated HTTP/1.1 CONNECT/TLS runtime plus Codex ChatGPT-login WebSocket client/server text-frame protection for active traffic profile hosts when routing, trust, and consent are ready. `dam-daemon` tracks pause/resume protection state so `dam disconnect` can stop redaction without removing routing.

Parked work:

- Implement Windows/Linux system proxy routing and VPN/TUN or network-extension routing behind the shared capture backend contracts.
- Add true full-device capture for UDP and non-HTTP protocols.
- Replace the current CLI explicit-proxy fallback with process/network-level capture everywhere signed platform capture is available.
- Install and remove the local DAM CA on Windows/Linux, add CA rotation, and harden interrupted macOS trust mutation recovery.
- Implement native Linux and Windows onboarding actions behind the current platform-specific setup ids (`linux_capture`, `windows_capture`) instead of reusing macOS Network Extension steps.
- Extend transparent TLS interception beyond the current HTTP/1.1/WebSocket slice: HTTP/2, fragmented/compressed WebSocket payloads, multiple requests per tunnel, and stronger platform coverage.
- Define fail-open, fail-closed, degraded, bypass, and blocked states for transparent protection across system proxy and `tun` modes, including which states are user/admin configurable.
- Define a future short-lived app wrapper, if needed, that starts or reuses the daemon and routes traffic by proxy/system routing without provider base-url mutation.
- Add platform tests proving sensitive values do not leave before transparent protection is ready.

### Encrypted Vault And Key Management

Current state: `dam-vault` stores tokenized protected values in local SQLite as plaintext values.

Parked work:

- Define the local encryption model: AEAD choice, nonce storage, key derivation, and record format.
- Define key custody for local development and install flows: OS keychain, environment-provided key, generated file, or user-managed secret.
- Add a migration path for existing plaintext vault rows.
- Add SQLite hygiene appropriate for sensitive local state, including secure delete and journal/WAL decisions.
- Update README and module docs only after the implementation is real.

### Streaming Response Protection

Current state: outbound requests are protected; agent HTTP traffic apps keep inbound DAM references tokenized in local transcripts and can opt into redetecting/tokenizing supported raw provider-returned values through traffic profile `inbound.protect_sensitive_data`. The inbound redetection context includes email-derived domains from the protected outbound request. JSON-shaped responses are transformed string-by-string, including newline-delimited JSON, when reference restoration or explicit raw inbound protection is active. `text/event-stream` responses are transformed under the same route policy. Raw stream transformation handles references split across adjacent chunks; provider-aware SSE text-delta transformation handles references and opted-in raw values split across OpenAI-compatible and Anthropic JSON delta events with a bounded trailing event window instead of EOF buffering.

Parked work:

- Decide whether streaming responses should support reference resolution only, full inbound detection/redaction, or both.
- Preserve streaming latency while enforcing bounded buffers per event/chunk.
- Cover `text/event-stream`, NDJSON, and chunked JSON behavior with E2E tests before broadening provider-aware transformation beyond current OpenAI-compatible and Anthropic text-delta reference resolution.

### Upstream Egress And SSRF Policy

Current state: configured proxy targets define the upstream; the proxy now disables redirects, strips hop-by-hop headers, blocks encoded request bodies, and applies a request timeout. It does not yet enforce an egress allowlist.

Parked work:

- Define a target allowlist model by scheme, host, and provider profile.
- Decide default behavior for loopback, RFC1918, link-local, and metadata-service addresses.
- Revalidate DNS/redirect behavior against the allowlist before any outbound request.
- Add diagnostics that explain blocked upstreams without leaking sensitive request details.

### Tamper-Evident Audit Log

Current state: `dam-log` records non-sensitive events in SQLite. It is mutable local state and strict audit/fail-closed behavior remains parked.

Parked work:

- Add append-only or tamper-evident semantics, such as hash chaining or signed log records.
- Decide how local operators rotate, export, and verify logs.
- Enforce configured log write failure behavior consistently on hot paths.
- Add tests for log DB unavailable, write failure, tamper detection, and recovery.

### Web And MCP Auth Story

Current state: `dam-web` is a local admin UI with cleartext vault access and consent mutation. It now rejects non-local Host headers and cross-site POST origins. `dam-mcp` runs over stdio and exposes consent tools according to config.

Parked work:

- Add an explicit local admin authentication model for `dam-web`, such as a startup token or local session secret.
- Decide whether `dam-web` should allow cleartext vault views by default or require an explicit unsafe/dev flag.
- Add a capability token or equivalent authorization model for MCP consent writes.
- Define default write-tool posture for installed MCP configs and enterprise deployments.
- Add tests for unauthorized web and MCP attempts, not just malformed requests.
