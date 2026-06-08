# dam-log

`dam-log` is the local SQLite operational and Activity log implementation.

It implements `dam-core::EventSink`.

## Responsibility

Persist operational events and the local Activity feed facts needed to show what DAM detected and what happened to it. Activity values are separate from Wallet values: they are not token vault entries, do not imply consent, and are not used for provider pass-through decisions.

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

## SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS log_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    operation_id TEXT NOT NULL,
    level TEXT NOT NULL,
    event_type TEXT NOT NULL,
    kind TEXT,
    value TEXT,
    reference TEXT,
    action TEXT,
    message TEXT NOT NULL
);
```

Existing local databases with older `log_events` schemas are migrated in place. Additive legacy schemas receive missing columns. Legacy schemas that contain `value_preview` are backed up to `log.db.pre-migration-<timestamp>.bak`, rebuilt without legacy preview columns, and marked with SQLite `PRAGMA user_version = 3`.

Legacy `data_type`, `module_name`, `destination`, and `action` context is preserved where available. Legacy `value_preview` is never copied into the current schema or `LogEntry`; new `value` entries are written only by current detection/redaction/decision events.

The SQLite reference store keeps indexes on `operation_id`, `event_type`, `timestamp/id`, and `action`. `LogStore::list_query` supports bounded, indexed reads by minimum timestamp, id cursor, event type, action, and limit so UI surfaces such as Activity do not scan the full local log on every refresh.

## Value Rules

Allowed:

- Sensitive kind.
- Detected value for Activity rows.
- Operation ID.
- Generated reference after a successful vault write.
- Policy action.
- Non-sensitive message.

Forbidden:

- Backend error text that echoes sensitive values.

The Activity value is local log data, not Wallet data. Removing a Wallet row does not remove historical Activity events, and adding a Wallet row is never inferred from an Activity value.

## Failure Behavior

Current `dam-filter` and `dam-proxy` behavior: log write failure warns/continues or disables logging when configured for non-strict behavior.

Strict audit/fail-closed remains parked.

## Tests

```bash
cargo test -p dam-log
```
