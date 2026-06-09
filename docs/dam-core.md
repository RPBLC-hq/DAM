# dam-core

`dam-core` is the spine/contracts crate.

It owns shared types and coordination rules. Other modules may implement contracts, but they should not invent cross-module behavior outside `dam-core`.

## Responsibilities

- Shared detection types: `SensitiveType`, `Span`, `Detection`.
- Reference generation and parsing: base58-encoded 128-bit UUID, 22 characters.
- Vault write contract: `VaultWriter`.
- Vault read contract: `VaultReader`.
- Logging contract: `EventSink`.
- Policy action contract: `PolicyAction`.
- Replacement planning from `PolicyDecision` values.
- Kind-specific value canonicalization before replacement planning/storage.
- Operational and Activity log event creation.
- Resolve planning for `[kind:id]` references.
- Proxy log event types for forward, bypass, and failure states.

## Replacement Behavior

Policy action effects:

| Action | Vault Write | Replacement |
|---|---:|---|
| `tokenize` | yes | `[kind:id]` |
| `redact` | no | `[kind]` |
| `allow` | no | unchanged |
| `block` | no | no transformed output |

By default, replacement planning deduplicates repeated equal `(kind, action, canonical value)` matches within one plan and asks the vault writer to reuse an existing canonical reference for the same stored value. Repeated tokenized values therefore reuse the same `[kind:id]` reference when the vault writer supports value deduplication. Set `policy.deduplicate_replacements = false` to generate separate references for each occurrence when equality leakage is a concern.

Current canonicalization is intentionally narrow:

- Email values have detector-supported spaces, tabs, and newlines removed from inside the address, and the domain is lowercased before storage and deduplication.
- Replacement spans still cover the exact detected input text.
- Resolving an email token returns the stored canonical email value.
- Phone, SSN, credit-card, and API-key values are stored exactly as detected for now.

Vault write failure while tokenizing uses redact-only fallback:

```text
[email]
```

## Contracts

Implementations plug in through traits:

```rust
pub trait VaultWriter: Send + Sync {
    fn write(&self, record: &VaultRecord) -> Result<Reference, VaultWriteError>;

    fn write_with_options(
        &self,
        record: &VaultRecord,
        options: VaultWriteOptions,
    ) -> Result<Reference, VaultWriteError>;
}

pub trait VaultReader: Send + Sync {
    fn read(&self, reference: &Reference) -> Result<Option<String>, VaultReadError>;
}

pub trait EventSink: Send + Sync {
    fn record(&self, event: &LogEvent) -> Result<(), LogWriteError>;
}
```

## Resolve Behavior

`dam-core` parses valid tokenized references:

```text
[email:7B2HkqFn9xR4mWpD3nYvKt]
```

It also resolves Markdown-escaped tokenized references such as `\[email:7B2HkqFn9xR4mWpD3nYvKt\]`, because model responses may escape square brackets while terminal renderers display them as normal brackets. The escaped wrapper is removed when the reference is restored.

Redact-only placeholders such as `[email]`, unknown kinds, and malformed IDs are ignored.

Known references become replacements with the stored value. Missing references and read failures stay unchanged unless a caller chooses strict failure behavior.

## Log Value Rules

- Detection, policy-decision, and redaction events do not carry raw or canonical sensitive values in `value`; Activity must rely on kind, action, reference, counts, and non-sensitive messages unless it is reading an intentional Wallet/vault reveal surface.
- Activity values are local log facts only when explicitly supplied by a caller for non-sensitive data; they do not create Wallet rows, imply consent, or affect provider pass-through decisions.
- References may be logged after successful vault writes.
- Backend error text must not echo sensitive values.

## Log Event Types

Current event types:

- `detection`
- `policy_decision`
- `vault_write`
- `vault_write_failed`
- `vault_read`
- `vault_read_failed`
- `redaction`
- `resolve`
- `proxy_forward`
- `proxy_bypass`
- `proxy_failure`

## Tests

```bash
cargo test -p dam-core
```
