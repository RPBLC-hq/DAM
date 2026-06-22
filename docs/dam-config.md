# dam-config

`dam-config` loads typed runtime configuration.

It describes topology and module selection. It does not create vaults, logs, policies, or proxies directly.

## Source Precedence

From lowest to highest:

1. Built-in defaults.
2. `dam.toml`, explicit `--config`, or `DAM_CONFIG`.
3. Environment variables.
4. CLI overrides.

Missing default `dam.toml` is allowed. Missing explicit config file is an error.

## Current Config Shape

```toml
[vault]
backend = "sqlite"
path = "vault.db"
token_env = "DAM_VAULT_TOKEN"
timeout_ms = 2000

[log]
enabled = true
backend = "sqlite"
path = "log.db"
token_env = "DAM_LOG_TOKEN"
timeout_ms = 2000

[consent]
enabled = true
backend = "sqlite"
path = "consent.db"
default_ttl_seconds = 86400
mcp_write_enabled = true
pending_timeout_seconds = 60
max_request_duration_seconds = 86400

[policy]
default_action = "tokenize"
deduplicate_replacements = true

[policy.kind.ssn]
action = "tokenize"

[failure]
vault_write = "redact_only"
log_write = "warn_continue"

[traffic]
profile_path = "traffic-profile.json"
enabled_apps = ["openai-api", "openai-platform", "anthropic-api", "claude-web", "anthropic-console", "claude-mcp-proxy", "claude-platform", "chatgpt-web", "chatgpt-legacy-web"]

[web]
addr = "127.0.0.1:2896"

[proxy]
enabled = false
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

[proxy.targets.auth]
caller_headers = ["authorization"]

[proxy.targets.auth.inject]
header = "authorization"
scheme = "Bearer"
strip_headers = ["authorization"]
```

`consent.pending_timeout_seconds` controls how long a direct MCP value-access request can remain pending before DAM expires it. It must be a positive integer. `consent.max_request_duration_seconds` caps approved direct value-access grants and must be at least 30 seconds so MCP request validation and config load-time validation fail coherently.

Provider values are labels used to match traffic-profile routes to proxy targets. Each `proxy.targets.upstream` must be an absolute `http://` or `https://` origin URL with a host, no embedded userinfo credentials, and no path, query string, or fragment; optional ports must be valid numeric TCP ports, and IPv6 literals must use bracket syntax such as `https://[::1]:8443`. DAM rejects relative values, unsupported schemes, hostless URLs, userinfo credentials, path/query/fragment components, malformed IPv6 authorities, and invalid ports before the proxy starts so configured forwarding remains pinned to an explicit upstream origin. The local proxy can accept multiple configured targets; `dam-router` selects the default target for direct app-layer requests and selects a specific target only when `dam-proxy` passes in a matched traffic-profile route. It no longer infers providers from request paths or provider headers. Caller-auth headers and optional target-key injection are target/profile data. Settings-created generic website profiles are not wired in the current app.

`traffic.profile_path` is optional. Without it, DAM loads the bundled JSON traffic profile at `crates/dam-net/profiles/llm-mvp.json`. A traffic profile contains app entries: each entry names match rules such as domains, IPs, URLs, ports, protocols, and process names; an action such as `inspect` or `bypass`; the protocol adapter; and the generic pipeline steps to run. LLM providers are only the bundled MVP entries, not the shape of the system.

`traffic.enabled_apps` is optional. When present, only those app IDs remain active in the loaded profile. Runtime Connect app selection uses the same mechanism through CLI overrides, so toggling Connect apps changes the active profile subset instead of changing proxy code.

Private enterprise gateways and provider-compatible endpoints are traffic profile apps. Example JSON:

```json
{
  "version": 1,
  "default_action": "bypass",
  "apps": [
    {
      "id": "enterprise-ai",
      "match": {
        "domains": ["api.enterprise-ai.example"],
        "ports": [443],
        "protocols": ["https", "web_socket"]
      },
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
      "outbound": {
        "filter": {"default_action": "redact"}
      },
      "inbound": {
        "resolve_references": true,
        "protect_sensitive_data": true
      }
    }
  ]
}
```

`inbound.resolve_references` controls whether existing DAM references are restored in HTTP responses. `inbound.protect_sensitive_data` is separate and defaults to `false`; it must be set for routes that should redetect/redact raw inbound HTTP response text without writing those inbound detections to Wallet. Stateful or browser-like routes should leave HTTP inbound protection off unless their response envelopes are known safe to rewrite.

`network.ai_routes` has been removed. Config files that still contain `[[network.ai_routes]]` fail validation with a migration message instead of silently dropping private endpoint protection.

`web.addr` and `proxy.listen` must be loopback socket addresses in this local build, for example `127.0.0.1:2896` and `127.0.0.1:7828`.

## Environment Overrides

Common deployment overrides:

```bash
export DAM_CONFIG=/etc/dam/dam.toml
export DAM_VAULT_BACKEND=sqlite
export DAM_VAULT_PATH=/var/lib/dam/vault.db
export DAM_VAULT_TOKEN_ENV=DAM_VAULT_TOKEN
export DAM_LOG_ENABLED=true
export DAM_LOG_BACKEND=sqlite
export DAM_LOG_PATH=/var/lib/dam/log.db
export DAM_LOG_TOKEN_ENV=DAM_LOG_TOKEN
export DAM_CONSENT_ENABLED=true
export DAM_CONSENT_PATH=/var/lib/dam/consent.db
export DAM_CONSENT_DEFAULT_TTL_SECONDS=86400
export DAM_CONSENT_MCP_WRITE_ENABLED=true
export DAM_CONSENT_PENDING_TIMEOUT_SECONDS=60
export DAM_CONSENT_MAX_REQUEST_DURATION_SECONDS=86400
export DAM_POLICY_DEFAULT_ACTION=tokenize
export DAM_POLICY_DEDUPLICATE_REPLACEMENTS=true
export DAM_POLICY_SSN_ACTION=redact
export DAM_FAILURE_VAULT_WRITE=redact_only
export DAM_FAILURE_LOG_WRITE=warn_continue
export DAM_TRAFFIC_PROFILE=/etc/dam/traffic-profile.json
export DAM_TRAFFIC_ENABLED_APPS=openai-api,openai-platform,anthropic-api,claude-web,anthropic-console,claude-mcp-proxy,claude-platform,chatgpt-web,chatgpt-legacy-web
export DAM_WEB_ADDR=127.0.0.1:2896
export DAM_PROXY_ENABLED=true
export DAM_PROXY_LISTEN=127.0.0.1:7828
export DAM_PROXY_DEFAULT_FAILURE_MODE=bypass_on_error
export DAM_PROXY_RESOLVE_INBOUND=false
export DAM_PROXY_TARGET_UPSTREAM=https://api.openai.com
export DAM_PROXY_TARGET_API_KEY_ENV=OPENAI_API_KEY
```

Supported policy env keys:

```text
DAM_POLICY_DEFAULT_ACTION
DAM_POLICY_DEDUPLICATE_REPLACEMENTS
DAM_POLICY_EMAIL_ACTION
DAM_POLICY_PHONE_ACTION
DAM_POLICY_SSN_ACTION
DAM_POLICY_CC_ACTION
DAM_POLICY_CREDIT_CARD_ACTION
```

Supported consent env keys:

```text
DAM_CONSENT_ENABLED
DAM_CONSENT_BACKEND
DAM_CONSENT_PATH
DAM_CONSENT_SQLITE_PATH
DAM_CONSENT_DEFAULT_TTL_SECONDS
DAM_CONSENT_MCP_WRITE_ENABLED
DAM_CONSENT_PENDING_TIMEOUT_SECONDS
DAM_CONSENT_MAX_REQUEST_DURATION_SECONDS
```

Supported vault env keys:

```text
DAM_VAULT_BACKEND
DAM_VAULT_PATH
DAM_VAULT_SQLITE_PATH
DAM_VAULT_URL
DAM_VAULT_TOKEN_ENV
DAM_VAULT_TIMEOUT_MS
DAM_VAULT_TOKEN
```

Supported log env keys:

```text
DAM_LOG_ENABLED
DAM_LOG_BACKEND
DAM_LOG_PATH
DAM_LOG_SQLITE_PATH
DAM_LOG_URL
DAM_LOG_TOKEN_ENV
DAM_LOG_TIMEOUT_MS
DAM_LOG_TOKEN
```

Supported failure env keys:

```text
DAM_FAILURE_VAULT_WRITE
DAM_FAILURE_LOG_WRITE
```

Supported traffic env keys:

```text
DAM_TRAFFIC_PROFILE
DAM_TRAFFIC_ENABLED_APPS
```

Supported proxy env keys:

```text
DAM_PROXY_ENABLED
DAM_PROXY_LISTEN
DAM_PROXY_MODE
DAM_PROXY_DEFAULT_FAILURE_MODE
DAM_PROXY_RESOLVE_INBOUND
DAM_PROXY_TARGET_NAME
DAM_PROXY_TARGET_PROVIDER
DAM_PROXY_TARGET_UPSTREAM
DAM_PROXY_TARGET_FAILURE_MODE
DAM_PROXY_TARGET_API_KEY_ENV
```

## Secrets

Config files may name secret env vars but should not contain secret values.

```toml
[vault]
backend = "remote"
url = "https://vault.internal"
token_env = "DAM_VAULT_TOKEN"
```

Current binaries validate remote settings but do not implement remote backends yet.

`policy.deduplicate_replacements` defaults to `true`. Repeated equal values reuse one token reference within a replacement plan, and vault writers that support value deduplication may return an existing canonical reference across plans. Set it to `false` when each occurrence should receive an independent token reference to reduce equality leakage.

Proxy target API keys are also secret refs:

```toml
[[proxy.targets]]
name = "openai"
provider = "openai-compatible"
upstream = "https://api.openai.com"
api_key_env = "OPENAI_API_KEY"
```

`dam-proxy` uses the resolved key if present and the target defines `auth.inject`. If `api_key_env` is configured but missing and the request does not provide one of the target's configured `auth.caller_headers`, the proxy returns `config_required`. Bundled profiles define the OpenAI-compatible and Anthropic header behavior in JSON rather than Rust provider code.

## Tests

```bash
cargo test -p dam-config
```
