# dam-http-adapter

`dam-http-adapter` owns generic upstream HTTP forwarding for proxy flows. It is not provider-specific code: target labels, upstream URLs, caller-auth headers, and optional target-key injection rules come from config/profile data.

The adapter:

- builds upstream URLs from a configured base URL plus the incoming request path/query;
- strips hop-by-hop request/response headers and stale body integrity headers when bodies are transformed;
- sends `Accept-Encoding: identity` so response transforms can safely inspect UTF-8 JSON, JSON-lines, SSE, and raw text bodies;
- optionally injects a configured target API key into a configured header/scheme;
- runs response body transform hooks for non-streaming JSON/JSON-lines and `text/event-stream` responses.

It does not run detection, policy, consent, vault writes, redaction, route selection, TLS interception, WebSocket framing, or profile matching. Those responsibilities stay in `dam-pipeline`, `dam-router`, `dam-proxy`, `dam-net`, and `dam-trust`.

Run:

```bash
cargo test -p dam-http-adapter
```
