# dam-vault

`dam-vault` is the local SQLite vault implementation.

It implements `dam-core::VaultWriter` and `dam-core::VaultReader`.

## Responsibility

Persist mappings:

```text
reference key -> original value
```

Example key:

```text
email:7B2HkqFn9xR4mWpD3nYvKt
```

When `VaultWriter` is called with deduplication enabled, the local vault reuses an existing reference for the same exact `(kind, value)` instead of storing another row. `list()` also collapses duplicate `(kind, value)` rows so the wallet presents one controllable card per value even if an older database already contains duplicates.

## SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS vault_entries (
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

## Architecture Rules

- The vault does not generate reference IDs.
- The vault may return an existing canonical reference for a duplicate value when the write contract allows deduplication.
- The vault does not redact text.
- The vault does not decide policy.
- The vault can be replaced by a remote implementation if it implements `VaultWriter` and `VaultReader`.

## Tests

```bash
cargo test -p dam-vault
```
