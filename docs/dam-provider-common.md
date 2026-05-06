# dam-provider-common

Status: implemented shared provider utility.

`dam-provider-common` owns provider-adapter helpers that are not specific to OpenAI-compatible or Anthropic request semantics.

## Responsibilities

- Shared streaming body transform support for provider adapters.
- Shared JSON and JSON-lines response-body string-value transform support for provider adapters.
- Small tail-buffered transformation for raw streaming bodies.
- Provider-aware `text/event-stream` text-delta transformation for OpenAI-compatible and Anthropic streaming shapes so DAM reference resolution can see a full `[kind:id]` token even when a provider splits it across SSE events.
- Shared stream type aliases used by provider crates.

The JSON path first tries to parse the whole response body as JSON. If that fails, it tries each newline-delimited line as a standalone JSON value and preserves non-JSON lines. Changed JSON values are serialized again. This lets inbound reference resolution restore tokens that providers JSON-escape inside message fields, including responses served as `application/x-ndjson` or other JSON-shaped media types.

The provider-aware SSE path buffers the upstream event-stream body, concatenates known provider text deltas, runs the caller transform once, writes the restored text into the first text-delta event, and leaves later text deltas empty. Known text paths include Anthropic `delta.text`, OpenAI-compatible `choices[].delta.content` and `delta`, top-level `completion`/`text`, and `content[].text` or `message.content[].text`. If a stream has no changed text-delta output, the SSE path falls back to transforming JSON string values in every event before using the raw body transform. This preserves provider-native SSE framing and non-text events for the MVP, but it does not preserve token-by-token response latency on parsed text-delta streams.

## Boundaries

The crate does not:

- choose providers, routes, auth mode, or failure behavior;
- parse full provider request/response DTOs beyond JSON string-value transforms and the small SSE text-delta paths needed for reference resolution;
- run detection, policy, vault writes, redaction, or logging;
- own fresh inbound detection/redaction decisions.

Provider-specific forwarding remains in `dam-provider-openai` and `dam-provider-anthropic`. Text protection and reference resolution remain in `dam-pipeline`/`dam-core`.

## Testing

Run:

```bash
cargo test -p dam-provider-common
```
