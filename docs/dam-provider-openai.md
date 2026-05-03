# dam-provider-openai

Status: implemented first extraction.

`dam-provider-openai` owns OpenAI-compatible upstream forwarding for the app-layer proxy path. It is a provider adapter boundary, not a protection pipeline.

## Responsibilities

```text
protected HTTP request body
  -> build upstream URL from configured base and incoming URI
  -> strip hop-by-hop and Connection-listed request headers
  -> forward caller auth headers or inject configured upstream bearer auth
  -> send to OpenAI-compatible upstream with redirects disabled and timeout bounded
  -> strip hop-by-hop and Connection-listed response headers
  -> stream text/event-stream responses through, optionally transforming each chunk
  -> pass non-streaming response bytes to the caller for optional local transform
```

For local launcher flows, DAM normally uses caller-owned provider auth. When a proxy target owns an upstream API key, this crate replaces the inbound `Authorization` header with the configured upstream bearer token before forwarding.

Response bytes are handed back through a caller-provided transform hook. `dam-proxy` uses that hook for default DAM reference resolution through `dam-pipeline`. Streaming responses use the same hook chunk by chunk only when the caller enables streaming response transformation.

## Boundaries

The crate does not:

- run detection, policy, consent, vault writes, redaction, or logging;
- choose proxy targets or failure modes;
- open local vault, consent, or log backends;
- parse OpenAI JSON request/response shapes into typed DTOs;
- parse SSE events or transform references split across stream chunks;
- implement WebSocket, Anthropic, or arbitrary web adapters. Anthropic forwarding lives in `dam-provider-anthropic`.

Those responsibilities stay in `dam-proxy`, `dam-pipeline`, or future provider/router modules.

## Current Consumer

- `dam-proxy` uses `dam-provider-openai` for OpenAI-compatible request forwarding, response header filtering, configured bearer auth injection, SSE passthrough when streaming transformation is disabled, and chunk-level SSE reference resolution when streaming transformation is enabled.

## Testing

Tests use fake local upstream servers and do not call real OpenAI, Anthropic, OpenRouter, or other provider endpoints.

Covered cases:

- base-path, request-path, and query preservation;
- response body transform hook;
- configured upstream API key replacing inbound `Authorization`;
- hop-by-hop and `Connection`-listed header stripping;
- `text/event-stream` passthrough without body transformation when streaming transformation is disabled.

Run:

```bash
cargo test -p dam-provider-openai
```
