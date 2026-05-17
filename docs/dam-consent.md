# dam-consent

`dam-consent` stores canonical-value passthrough grants.

A consent lets a detected value pass through unredacted until its TTL expires or it is revoked. Consent overrides `tokenize` and `redact` policy decisions. It does not override `block`. For token kinds with normalization, matching uses the same canonical value used by replacement planning; current email canonicalization removes detector-supported whitespace inside the address and lowercases the domain.

Consent records do not store raw sensitive values. Matching uses:

```text
kind + value_fingerprint + scope
```

`global` is the default scope and applies everywhere. Proxy callers may also pass route scopes such as `target:chatgpt-codex`; scoped grants apply only when the caller includes the matching route scope. Global grants are always considered alongside route scopes so older grants and explicit "all profiles" grants keep working.

When a consent is granted from the vault UI or MCP server, the caller provides the stable vault key, for example:

```text
email:ANJFsZtLfEA9WeP3bZS8Nw
```

The stable vault key is preferred over bracket display references because inbound reference resolution may turn `[email:...]` back into the local value before an agent sees it. In proxy flows, active consent also lets `dam-pipeline` expand a previously tokenized outbound DAM reference for that same canonical value before detection/redaction runs. This prevents chat history such as `[email:...]` from being re-sent to a model after the user has explicitly allowed the underlying value.

## Config

```toml
[consent]
enabled = true
backend = "sqlite"
path = "consent.db"
default_ttl_seconds = 86400
mcp_write_enabled = true
```

Supported env keys:

```text
DAM_CONSENT_ENABLED
DAM_CONSENT_BACKEND
DAM_CONSENT_PATH
DAM_CONSENT_SQLITE_PATH
DAM_CONSENT_DEFAULT_TTL_SECONDS
DAM_CONSENT_MCP_WRITE_ENABLED
```

## Behavior

- Active consent changes matching canonical detections to `allow` when the grant is global or matches the caller's route scopes.
- Active consent applies to previously tokenized outbound DAM references when the configured vault can read the stored value and the stored value still has active consent for the caller's route scopes.
- Expired or revoked consent does not affect policy.
- Revoking a consent id revokes all unrevoked grants for the same `kind + value_fingerprint + scope`, so duplicate vault rows for the same canonical value cannot keep passthrough alive.
- Wallet mutations may revoke by stable `vault_key`, either for every recorded party on that value or for one `created_by` audit label. Profile-level Wallet allows are stored as one or more target-scoped grants derived from the selected integration profile's traffic apps; `created_by` remains the UI/audit label, while `scope` is the enforcement boundary.
- Consent emits a non-sensitive `consent` log event when it allows a value.
- The SQLite store keeps `id`, `kind`, `value_fingerprint`, optional `vault_key`, TTL timestamps, source, and optional reason.

## Tests

```bash
cargo test -p dam-consent
```
