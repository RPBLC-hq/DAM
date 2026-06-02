# dam-proxy

Status: implemented first slice.

`dam-proxy` is the first hot-path proxy module. It is a generic mediation runtime with HTTP and WebSocket adapters for active traffic-profile routes. In daemon transparent mode, it also owns the guarded HTTP/1.1 `CONNECT` TLS interception runtime for active traffic-profile routes when routing, trust, and consent are all ready. It includes the MVP WebSocket adapter for ChatGPT-login traffic on `chatgpt.com`, `ab.chatgpt.com`, and `chat.openai.com`. It does not install local CAs, install routes, create TUN devices, or rewrite arbitrary web traffic.

## Architecture

Current first slice:

```text
client or harness
  -> dam-proxy
  -> dam-router
  -> first configured proxy target
  -> provider HTTP request body
  -> dam-pipeline
  -> expand actively allowed DAM references through dam-vault when present
  -> dam-detect
  -> dam-policy
  -> dam-consent active canonical-value overrides
  -> dam-core replacement plan
  -> dam-vault for tokenize decisions
  -> dam-redact
  -> dam-log
  -> protocol adapter
  -> upstream provider
  -> provider response body
  -> protocol adapter
  -> dam-pipeline when proxy.resolve_inbound is enabled
  -> dam-core resolve plan for existing DAM references
  -> dam-vault through VaultReader
  -> dam-pipeline redetect/redact without vault storage when no reference resolves and the route opted in
  -> dam-log
  -> client or harness
```

Profile-matched outbound requests always run through detection, policy, consent, and replacement before provider egress. The bundled LLM traffic apps default outbound detections to tokenized references such as `[email:<id>]`; those writes go to the token vault for inbound resolution and do not create Wallet rows. Active target-scoped Wallet consent lets matching canonical values pass through unredacted; without consent they remain protected as token references. ChatGPT subscription traffic is mediated by the `chatgpt-web` WebSocket adapter for `chatgpt.com` and `ab.chatgpt.com`; OpenAI API-key mode is mediated by the HTTP adapter for `api.openai.com`. Inbound HTTP response reference resolution is enabled for bundled LLM routes, and raw inbound response redetection is explicit per route through traffic profile `inbound.protect_sensitive_data`. Inbound raw detections use redact-only replacements and are not written to Wallet. Domain-only values are not detected or redacted in outbound or inbound passes. Explicit reveal/consent flows are separate from agent transcript protection.

JSON-shaped outbound request bodies and provider responses are transformed as JSON string values, so sensitive values and DAM references inside provider-escaped message fields can be protected or resolved without corrupting JSON. Request protection tries whole-body JSON first and falls back to raw text when the body is not JSON. Response resolution tries whole-body JSON first, then newline-delimited JSON, regardless of the exact response media type. Provider responses with `Content-Type: text/event-stream` are transformed when inbound reference resolution is enabled so provider-native SSE framing stays intact. The protocol adapters use bounded provider-aware SSE text-delta parsing for OpenAI-compatible and Anthropic streams, which lets known DAM references resolve even when a provider splits one reference across adjacent JSON text-delta events without buffering the whole response to EOF. The WebSocket adapter applies the same bounded idea in both directions: protected client-to-server JSON text-delta and raw text frames can hold a small incomplete sensitive suffix until the adjacent text frame completes it, then the adapter tokenizes before upstream; protected client-to-server JSON text frames are decoded and rewritten as JSON string values before forwarding; protected server-to-client text messages can hold a small incomplete DAM reference suffix until the adjacent text frame completes it, then the adapter restores the reference before the local client sees it. The SSE parser also falls back to JSON string-value event transforms for unrecognized event shapes. With `--no-resolve-inbound`, event-stream and WebSocket responses pass through without local restoration. Preserving exact token-by-token latency for every provider-specific event shape remains future work.

Routes that explicitly choose `tokenize` write references through the vault writer, and repeated equal canonical values reuse one tokenized reference by default. The bundled LLM traffic apps use this mode for normal outbound protection while keeping Wallet rows separate. Email canonicalization removes detector-supported whitespace inside the address and lowercases the domain before storage/deduplication. Disable token deduplication with `policy.deduplicate_replacements = false` or `DAM_POLICY_DEDUPLICATE_REPLACEMENTS=false` when preserving equality across repeated values is too revealing.

Active consent grants let canonical detected values pass through unredacted until expiry or revocation. Consent overrides `tokenize` and `redact`; it does not override `block`. `dam-proxy` supplies the matched route target scope to `dam-pipeline`, so a profile-level Wallet allow becomes one or more target-scoped grants and cannot be reused by another configured target. Global grants still apply to every target. If a later outbound request contains an old DAM reference for the same allowed value, `dam-proxy` passes its vault reader and target scope into `dam-pipeline` so that reference is expanded before detection and provider egress only when the grant applies. References without active consent remain protected.

The current implementation keeps HTTP serving, backend opening, and DAM-owned status responses inside `dam-proxy`. HTTP adapter storage is isolated in `src/providers.rs`, non-sensitive proxy event helpers are isolated in `src/events.rs`, and WebSocket framing lives in `src/websocket.rs`. Shared text processing orchestration lives in `dam-pipeline`, generic HTTP forwarding lives in `dam-http-adapter`, response transform utilities live in `dam-provider-common`, and first-slice route decisions live in `dam-router`.

Transparent system-proxy traffic reaches DAM as HTTP `CONNECT`. The standalone app-layer `dam-proxy` path still fails closed for `CONNECT`. When `dam-daemon` starts `dam-proxy` in transparent mode, `dam-proxy` uses a raw TCP CONNECT loop instead of the Axum app-layer server. That loop must bind to loopback and activates only when `dam-net` routing readiness, `dam-trust` local CA readiness, explicit consent, and `dam-intercept` adapter readiness are all `ready`.

The first transparent runtime slice is intentionally narrow: HTTP/1.1 requests over CONNECT, active `inspect` apps from the effective traffic profile, routes with ready HTTP/WebSocket adapters, no chunked request bodies, no HTTP/2, and request bodies capped at 32 MiB before buffering. After the TLS handshake, `dam-proxy` binds the decrypted HTTP/WebSocket request back to the active traffic route using the request authority/`Host` header. It does not fall back to provider path/header hints. This keeps ChatGPT backend HTTP endpoints such as `/backend-api/codex/responses/compact` on the `chatgpt-web` target instead of the first configured target. Outbound JSON request bodies and intercepted JSON/`text/event-stream` responses are transformed at decoded JSON string boundaries when their route policy enables protection or restoration. WebSocket upgrade traffic is supported for the ChatGPT-login path: extension negotiation is stripped, the protection enabled/disabled state is frozen for the lifetime of each WebSocket connection, unfragmented client and server text frames are protected for protected connections, JSON text frames are decoded before string replacement, adjacent outbound text frames can be coalesced only to complete a partial sensitive suffix, and fragmented, binary, or compressed frames close protected connections instead of passing through raw. Unsupported or not-ready traffic fails closed rather than becoming an opaque tunnel.

Provider labels are configuration data used to match traffic-profile routes to proxy targets. Runtime forwarding is adapter-driven; auth caller headers and optional target-key injection headers are defined by the selected profile/target, not a Rust provider enum.

## Usage

With config:

```bash
cargo run -p dam-proxy -- --config dam.example.toml
```

Without a config file, pass an upstream explicitly:

```bash
cargo run -p dam-proxy -- \
  --listen 127.0.0.1:7828 \
  --upstream https://api.openai.com \
  --api-key-env OPENAI_API_KEY
```

For local fake-upstream tests or caller-owned auth, disable proxy-managed API key injection. DAM will forward the incoming `Authorization` or provider auth headers:

```bash
cargo run -p dam-proxy -- \
  --upstream http://127.0.0.1:9999 \
  --no-api-key-env
```

Local proxy/interception flows use this pass-through auth mode by default. The old `dam claude`, `dam codex`, and `dam codex --api` one-shot launchers were removed because DAM no longer protects by rewriting provider API base URLs or Codex provider config.

To leave DAM references unresolved on the inbound response path:

```bash
cargo run -p dam-proxy -- \
  --upstream http://127.0.0.1:9999 \
  --no-resolve-inbound \
  --no-api-key-env
```

Health:

```bash
curl http://127.0.0.1:7828/health
cargo run -p damctl -- status
```

`/health` returns the standardized `dam-api` `ProxyReport` JSON shape. DAM-owned failure responses such as `config_required`, `blocked`, and `provider_down` use the same shape. Successful upstream provider responses are forwarded as provider responses, not wrapped, after DAM reference resolution when applicable.

## Failure Behavior

- Protection precondition failures fail closed before provider egress. This includes unsupported content encodings, non-UTF-8 request bodies in the current text pipeline, consent backend errors, and invariant failures where the pipeline does not produce protected output.
- Transparent `CONNECT` requests fail closed unless the daemon supplied the transparent runtime config and routing, trust, consent, and adapter readiness are all ready.
- `bypass_on_error`: retained as a visible failure-mode state for reduced-protection configurations, but it is not allowed to forward request bytes that DAM failed to inspect/protect.
- `redact_only`: supported for vault failures. If a tokenized vault write fails, the value becomes `[kind]`.
- `block_on_error`: strict proxy/protection failure behavior. The proxy returns a clear `blocked` response instead of forwarding unprotected traffic.
- `config_required`: returned when a target requires an API key env var, the env var is missing, and the incoming request has no provider-compatible auth header.
- `provider_down`: returned when DAM is reachable but the upstream provider cannot be reached.

Bypass is not silent when logging is enabled. The persisted event type is `proxy_bypass`.

When logging is enabled, the proxy also records non-sensitive diagnostic checkpoints for mediated requests:

- `route_decision`: selected target/provider, protection state, inbound-resolution state, raw inbound-protection state, and request byte count.
- `request_protection`: detection/replacement counts and whether replacements were tokenized or blocked.
- `provider_forward_start`: protocol adapter handoff and streaming-resolution intent.
- `provider_response`: provider status, content type, content encoding, and streaming classification.
- `resolve_attempt`: inbound reference count plus resolved, missing, and read-failure counts for each transformed response body segment.
- `resolve_disabled`: response body size when inbound restoration is configured off. This is recorded as proxy diagnostics, not as a `resolve` event.
- `intercepted_response_write`: transparent runtime response status/content type/streaming state immediately before writing back to the client.

These events must not include raw request bodies, raw response bodies, API keys, or resolved inbound sensitive values. Detection, policy-decision, and redaction events may include the detected Activity value so the local Activity feed can show what was detected without reading Wallet.

Provider connection errors are reported as `provider_down` without echoing upstream URLs in user-visible messages.

DAM-owned status responses include `state`, `message`, `operation_id`, `target`, `upstream`, and non-sensitive `diagnostics` through `dam-api::ProxyReport`.

## Config

```toml
[proxy]
enabled = true
listen = "127.0.0.1:7828"
mode = "reverse_proxy"
default_failure_mode = "bypass_on_error"
resolve_inbound = true

[[proxy.targets]]
name = "openai"
provider = "openai-compatible"
upstream = "https://api.openai.com"
failure_mode = "bypass_on_error"
api_key_env = "OPENAI_API_KEY"
```

Anthropic target example:

```toml
[[proxy.targets]]
name = "anthropic"
provider = "anthropic"
upstream = "https://api.anthropic.com"
failure_mode = "bypass_on_error"
api_key_env = "ANTHROPIC_API_KEY"
```

Traffic profile selection example:

```toml
[traffic]
profile_path = "traffic-profile.json"
enabled_apps = ["openai-api", "openai-platform", "anthropic-api", "claude-web", "anthropic-console", "claude-mcp-proxy", "claude-platform", "chatgpt-web", "chatgpt-legacy-web"]
```

Private OpenAI-compatible endpoint profile example:

```json
{
  "version": 1,
  "default_action": "bypass",
  "apps": [
    {
      "id": "enterprise-ai",
      "match": {"domains": ["api.enterprise-ai.example"], "ports": [443]},
      "action": "inspect",
      "adapter": "http",
      "provider": "openai-compatible",
      "target_name": "enterprise-ai",
      "upstream": "https://api.enterprise-ai.example",
      "steps": [
        {"id": "detect", "kind": "detect_sensitive_data", "direction": "outbound"},
        {"id": "redact", "kind": "replace_sensitive_data", "direction": "outbound"},
        {"id": "resolve", "kind": "resolve_references", "direction": "inbound"}
      ],
      "outbound": {"filter": {"default_action": "redact"}},
      "inbound": {"resolve_references": true, "protect_sensitive_data": true}
    }
  ]
}
```

The traffic profile controls transparent host recognition, adapter intent, per-app inbound reference restoration, and explicit raw inbound protection. Active forwarding targets are configured separately through `[[proxy.targets]]`; the daemon also adds active profile routes as non-secret proxy targets for transparent matching. The local proxy can host multiple targets in one process. Direct app-layer requests use the first target; transparent/profile-matched requests select the target from the matched route metadata. Provider API path/header guessing is intentionally out of the router.

The profile creator/import/export workflow that will produce generic website/service profiles is parked. Until that returns, `generic-http` is only a low-level target value and the visible catalog is limited to bundled JSON app profiles.

Secrets must be supplied through environment variables or deployment secret stores, not plaintext config files. For local proxy/interception flows, omit `api_key_env` so DAM forwards caller-owned auth headers instead of injecting a provider key.

## Testing

`dam-proxy` tests use fake upstream HTTP servers and do not call real OpenAI, Anthropic, or OpenRouter endpoints.

Covered cases:

- redacted request forwarding to fake upstream;
- inbound response resolution for DAM references in non-streaming responses, including JSON and JSON-lines string-value restoration;
- opt-in inbound redetection/redaction for raw sensitive response text when no DAM reference resolves, without Wallet writes;
- outbound expansion of previously tokenized references when the referenced value has active consent;
- `text/event-stream` response transformation with inbound reference resolution enabled, including references split across adjacent chunks and across Anthropic text-delta events without EOF buffering;
- WebSocket outbound tokenization when adjacent client text-delta frames split a sensitive value;
- WebSocket inbound reference resolution across adjacent text-delta frames while preserving whole WebSocket messages;
- disabled inbound response resolution leaving DAM references intact;
- vault writes and log writes during forwarding;
- bypass on invalid UTF-8 with `bypass_on_error`;
- block on invalid UTF-8 with `block_on_error`;
- policy `block` returning 403 without forwarding;
- missing API key producing `config_required`;
- configured upstream API key replacing inbound `Authorization`;
- Anthropic `x-api-key` passthrough and configured key replacement;
- transparent `CONNECT` requests failing closed without provider egress;
- transparent HTTP/1.1 CONNECT/TLS requests completing a local-CA TLS handshake and forwarding only protected request bodies to the fake upstream;
- transparent ChatGPT backend HTTP requests selecting the `chatgpt-web` route even when another provider target is first, keeping outbound bodies protected without Wallet writes, and honoring the route inbound restoration policy;
- transparent raw HTTP `text/event-stream` responses resolved before reaching the client when inbound resolution is enabled;
- non-sensitive proxy diagnostics around route selection, request protection, provider handoff/response, inbound resolution, and transparent response write boundaries;
- hop-by-hop and `Connection`-listed header stripping;
- upstream connection failure producing `provider_down`;
- `dam-api` `ProxyReport` JSON for health and DAM-owned failure responses;
- disabled proxy startup failures and provider labels treated as configuration data.

Run:

```bash
cargo test -p dam-proxy
```

## Parked

- HTTP/2 and multi-request transparent tunnel handling.
- Local CA management and OS route installation.
- VPN/TUN/network-extension routing.
- binary/non-UTF-8 upload endpoints until a profile adapter defines safe parsing behavior.
- fragmented, binary, or compressed WebSocket payload transformation beyond the MVP unfragmented text-frame adapter.
- Additional generic adapters for arbitrary web traffic beyond HTTP/WebSocket provider traffic.
- exact token-by-token provider-aware streaming/SSE response transforms and raw inbound redetection across split response chunks/events.
