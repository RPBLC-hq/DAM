# dam-net

`dam-net` defines the first network-control contracts for DAM's future full-device protection path.

It does not install system proxy settings, create a TUN device, intercept TLS, forward packets, or inspect traffic. It is a small control-plane crate used to keep daemon, UI, CLI, and future native network modules aligned on the same vocabulary.

## Current Contracts

Capture modes:

```text
explicit_proxy  current implemented app-layer routing mode
system_proxy    planned OS proxy routing mode
tun             planned VPN/TUN or platform network-extension routing mode
```

`CapturePlan::for_mode` reports whether a mode is implemented, whether it requires admin/system permission, whether it installs system routes, and what TLS visibility is available.

Current implementation status:

- `explicit_proxy`: implemented.
- `system_proxy`: planned, host-only visibility before TLS trust and interception are enabled.
- `tun`: planned, host-only visibility before TLS trust and interception are enabled.

## Transparent AI Classification

`dam-net` classifies known AI provider hosts without decrypting traffic:

```text
api.openai.com      -> openai-compatible / openai
api.anthropic.com   -> anthropic / anthropic
api.x.ai            -> openai-compatible / xai
chatgpt.com         -> openai-compatible / chatgpt-codex
```

For TLS traffic, classification can identify that traffic is probably AI-related, but it cannot protect request bodies without `dam-trust` readiness and a later TLS interception implementation. The explicit decision shape is:

```text
identified AI + HTTPS/WSS -> requires TLS interception before body protection
identified AI + HTTP/WS   -> protectable without TLS
unknown host              -> non-AI traffic
```

This keeps the future transparent proxy honest: host routing alone is not data protection for encrypted provider requests.

## Current Consumers

- `dam-daemon` stores the selected `network_mode` in `daemon.json`.
- `dam-daemon` stores the known transparent AI routes in non-sensitive daemon state for UI/CLI/status consumers.
- `dam status` prints `network_mode` when a daemon is connected or stale.
- `dam-trust` consumes transparent route decisions when reporting future TLS interception readiness.

## Boundaries

`dam-net` owns:

- network capture-mode vocabulary;
- transparent AI host classification;
- non-TLS route-readiness decisions.

`dam-net` does not own:

- process lifecycle;
- OS proxy/TUN installation;
- TLS trust roots or certificates;
- HTTP forwarding;
- provider request/response adapters;
- detection, policy, vault, consent, logging, or redaction.

Those stay in `dam-daemon`, future platform-specific network modules, `dam-trust`, `dam-proxy`, provider adapters, and `dam-pipeline`.
