# dam-net

`dam-net` defines the first network-control contracts for DAM's local traffic mediation path.

It does not install system proxy settings, create a TUN device, intercept TLS, forward packets, or inspect traffic. It is a small control-plane crate used to keep daemon, UI, CLI, and future native network modules aligned on the same vocabulary.

DAM network control must remain portable across macOS, Linux, and Windows. Platform-specific implementations such as `dam-net-macos` live behind shared capture-mode and readiness contracts; missing or partial platform support must stay tracked in `docs/parking-lot.md` or the module parking lot until it is implemented and tested.

## Current Contracts

Capture modes:

```text
explicit_proxy  implemented routing for clients explicitly configured to use DAM as a local endpoint or HTTP(S) proxy
system_proxy    OS proxy routing mode for proxy-capable HTTP/HTTPS traffic
tun             platform capture backend mode; macOS uses Network Extension
```

`CapturePlan::for_mode` reports whether a mode is implemented, whether it requires admin/system permission, whether it installs system routes, and what TLS visibility is available.

`TransparentRouteCaptureReadiness` reports per-AI-route routing readiness for capture modes:

```text
not_transparent_mode         route capture is inactive for this mode
needs_system_proxy_install   system proxy routing is not active
needs_tun_install            TUN routing is not active
ready                        routing is active for the route
```

Current implementation status:

- `explicit_proxy`: implemented for local base-URL traffic and HTTP(S) proxy traffic from configured clients. HTTPS body protection for selected AI hosts still requires local CA trust and the CONNECT/TLS adapter.
- `system_proxy`: macOS PAC routing is implemented in `dam-net-macos` for proxy-capable HTTP/HTTPS traffic. Unknown hosts pass through DAM untouched; HTTPS body visibility for selected AI hosts still requires TLS trust and interception.
- `tun`: macOS Network Extension control-plane support is implemented in `dam-net-macos`; activation requires the signed native helper/app bundle. Windows and Linux are still behind the same shared backend contracts.

Protocol adapters are reported separately from capture. HTTP protects OpenAI/Anthropic API-key traffic, and WebSocket is the MVP adapter for Codex ChatGPT-login traffic on `chatgpt.com`.

## Full-Traffic Mediation

The macOS `system_proxy` implementation routes proxy-capable HTTP and HTTPS traffic to DAM. DAM's default host policy is conservative:

```text
unknown host                 -> pass through without TLS decrypt, body reads, or redaction
selected AI host + enabled   -> protect when routing, trust, consent, and adapter readiness pass
selected AI host + paused    -> pass through without redaction
selected AI host + not ready -> fail according to the configured failure behavior
```

PAC routing is not true packet-level full-device capture. `tun`/Network Extension is the primary full-device path: it can classify all TCP flows by destination and hand configured AI hosts to the protected proxy runtime. Unsupported protocols or encrypted bodies still require protocol-specific adapters before DAM can inspect payloads.

## Transparent AI Classification

`dam-net` classifies AI provider hosts without decrypting traffic. The repo defaults are:

```text
api.openai.com      -> openai-compatible / openai
api.anthropic.com   -> anthropic / anthropic
api.x.ai            -> openai-compatible / xai
chatgpt.com         -> openai-compatible / chatgpt-codex
```

Custom routes can be layered on top of those defaults through `[network.ai_routes]` in config. A custom route can add a private provider endpoint or replace a default host with a different target name/upstream. The merged route registry is used by daemon status, macOS PAC routing, trust readiness, and the transparent proxy runtime.

For TLS traffic, classification can identify that traffic is probably AI-related, but it cannot protect request bodies without `dam-trust` readiness and a later TLS interception implementation. The explicit decision shape is:

```text
identified AI + HTTPS/WSS -> requires TLS interception before body protection
identified AI + HTTP/WS   -> protectable without TLS
unknown host              -> non-AI traffic
```

This keeps the future transparent proxy honest: host routing alone is not data protection for encrypted provider requests.

## Current Consumers

- `dam-daemon` stores the selected `network_mode` in `daemon.json`.
- `dam-daemon` stores the merged transparent AI route registry in non-sensitive daemon state for UI/CLI/status consumers.
- `dam-daemon` stores per-route routing readiness in `daemon.json`.
- `dam status` prints `network_mode` when a daemon is connected or stale.
- `dam-net-macos` installs/removes macOS PAC routing for proxy-capable HTTP/HTTPS traffic and writes rollback state.
- `dam-net-macos` plans/records macOS Network Extension capture for `tun`; source builds require `DAM_MACOS_NE_HELPER`, while signed releases provide the helper from the app bundle.
- `dam-trust` consumes transparent route decisions when reporting future TLS interception readiness.
- `dam-intercept` consumes route readiness as the first gate before TLS interception may activate.

## Boundaries

`dam-net` owns:

- network capture-mode vocabulary;
- transparent AI route registry helpers and host classification;
- transparent route readiness reporting;
- non-TLS route-readiness decisions.

`dam-net` does not own:

- process lifecycle;
- OS proxy/TUN installation;
- TLS trust roots or certificates;
- HTTP forwarding;
- provider request/response adapters;
- detection, policy, vault, consent, logging, or redaction.

Those stay in `dam-daemon`, future platform-specific network modules, `dam-trust`, `dam-proxy`, provider adapters, and `dam-pipeline`.
