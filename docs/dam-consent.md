# dam-consent

`dam-consent` stores both canonical-value passthrough grants and bounded direct value-access requests/grants.

A consent lets a detected value pass through unredacted until its TTL expires or it is revoked. Consent overrides `tokenize` and `redact` policy decisions. It does not override `block`. For token kinds with normalization, matching uses the same canonical value used by replacement planning; current email canonicalization removes detector-supported whitespace inside the address and lowercases the domain.

Direct value-access requests add a second path for local control surfaces and MCP clients: a caller can request one bounded raw-value reveal for a specific vault key, actor, purpose, and duration. The request stays metadata-only until a local approver resolves it to `approved` or `denied`.

Consent records do not store raw sensitive values. Matching uses:

```text
kind + value_fingerprint + scope
```

`global` is the default scope and applies everywhere. Proxy callers may also pass route scopes such as `target:<traffic-route>`; scoped grants apply only when the caller includes the matching route scope. Global grants are always considered alongside route scopes so older grants and explicit "all profiles" grants keep working.

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
pending_timeout_seconds = 60
max_request_duration_seconds = 86400
```

Supported env keys:

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

## Behavior

- Active consent changes matching canonical detections to `allow` when the grant is global or matches the caller's route scopes.
- Active consent applies to previously tokenized outbound DAM references when the configured vault can read the stored value and the stored value still has active consent for the caller's route scopes.
- Expired or revoked consent does not affect policy.
- Revoking a consent id revokes all unrevoked grants for the same `kind + value_fingerprint + scope`, so duplicate vault rows for the same canonical value cannot keep passthrough alive.
- Wallet mutations may revoke by stable `vault_key`, either for every recorded party on that value or for one `created_by` audit label. Profile-level Wallet allows are stored as one or more target-scoped grants derived from the selected integration profile's traffic apps; `created_by` remains the UI/audit label, while `scope` is the enforcement boundary.
- Direct value-access requests store only non-sensitive metadata plus the canonical fingerprint, vault key, actor binding, requested purpose, requested duration, and state timestamps.
- Direct value-access states are `pending`, `approved`, `denied`, `expired`, `revoked`, and `consumed`.
- Pending requests expire after `pending_timeout_seconds` if no approver acts.
- Approved direct-access grants are single-use and expire at the earlier of their approved grant deadline or an explicit revoke.
- `resolve_direct_access_request` fails closed: it returns no raw value for pending, denied, expired, revoked, consumed, actor-mismatch, vault-read-failure, or vault-value-changed cases.
- Consent emits non-sensitive `consent` log events when passthrough matching allows a value; the direct value-access first slice does not add raw-value logging surfaces.
- The SQLite store keeps `id`, `kind`, `value_fingerprint`, optional `vault_key`, TTL timestamps, source, and optional reason for passthrough grants, plus a separate direct-access request/grant table with non-sensitive request metadata.

## Tests

```bash
cargo test -p dam-consent
cargo test -p dam-mcp --bin dam-mcp
```
