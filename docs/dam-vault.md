# dam-vault

`dam-vault` is the local SQLite token-vault and Wallet reference implementation.

It implements `dam-core::VaultWriter` and `dam-core::VaultReader`.

## Responsibility

Persist reversible token mappings:

```text
reference key -> value supplied by dam-core
```

Example key:

```text
email:7B2HkqFn9xR4mWpD3nYvKt
```

When `VaultWriter` is called with deduplication enabled, the local token vault reuses an existing reference for the same exact `(kind, value)` instead of storing another row and refreshes that row's `updated_at` timestamp. `dam-core` owns any kind-specific canonicalization before the write; current email values arrive with detector-supported internal whitespace removed and the domain lowercased. `list()` collapses duplicate `(kind, value)` token rows for callers that inspect the token vault directly.

Wallet entries are stored separately in `wallet_entries`. Automatic proxy tokenization writes `vault_entries` only, so reversible `[kind:id]` tokens can resolve inbound without populating the user Wallet. Explicit user actions such as `POST /api/v1/wallet` add the chosen reference to `wallet_entries`.

## SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS vault_entries (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS wallet_entries (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

## Public Operations

- `put(key, value)`
- `get(key)`
- `delete(key)`
- `list()`
- `count()`
- `put_wallet(key, value)`
- `get_wallet(key)`
- `delete_wallet(key)`
- `list_wallet()`
- `wallet_count()`

## Architecture Rules

- The vault does not generate reference IDs.
- The vault may return an existing canonical reference for a duplicate value when the write contract allows deduplication.
- The vault does not redact text.
- The vault does not decide policy.
- Token-vault entries and Wallet entries are separate concepts even when they share the same reference key.
- The vault can be replaced by a remote implementation if it implements `VaultWriter` and `VaultReader`.

## Tests

```bash
cargo test -p dam-vault
```
