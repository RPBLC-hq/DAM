use super::*;
use dam_core::{LogEventType, LogLevel, Reference, SensitiveType};

fn event() -> LogEvent {
    LogEvent::new(
        "op-1",
        LogLevel::Info,
        LogEventType::VaultWrite,
        "vault write succeeded",
    )
    .with_kind(SensitiveType::Email)
    .with_reference(Reference {
        kind: SensitiveType::Email,
        id: "7B2HkqFn9xR4mWpD3nYvKt".to_string(),
    })
    .with_action("vault_write_succeeded")
}

#[test]
fn record_then_list_returns_entry() {
    let store = LogStore::open_in_memory().unwrap();

    store.record(&event()).unwrap();

    let entries = store.list().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation_id, "op-1");
    assert_eq!(entries[0].level, "info");
    assert_eq!(entries[0].event_type, "vault_write");
    assert_eq!(entries[0].kind, Some("email".to_string()));
    assert_eq!(
        entries[0].reference,
        Some("email:7B2HkqFn9xR4mWpD3nYvKt".to_string())
    );
    assert_eq!(entries[0].action, Some("vault_write_succeeded".to_string()));
}

#[test]
fn entries_persist_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("log.db");

    {
        let store = LogStore::open(&db_path).unwrap();
        store.record(&event()).unwrap();
    }

    let store = LogStore::open(&db_path).unwrap();
    assert_eq!(store.count().unwrap(), 1);
}

#[test]
fn opens_legacy_log_schema_without_exposing_value_preview() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("legacy-log.db");
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE log_events (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                data_type     TEXT    NOT NULL,
                destination   TEXT    NOT NULL,
                action        TEXT    NOT NULL,
                timestamp     INTEGER NOT NULL,
                module_name   TEXT    NOT NULL,
                value_preview TEXT    NOT NULL
            );

            INSERT INTO log_events (
                data_type,
                destination,
                action,
                timestamp,
                module_name,
                value_preview
            )
            VALUES (
                'email',
                'stdout',
                'tokenize',
                1,
                'dam-filter',
                'banana@banana.com'
            );
            ",
        )
        .unwrap();
    }

    let store = LogStore::open(&db_path).unwrap();
    let entries = store.list().unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation_id, LEGACY_OPERATION_ID);
    assert_eq!(entries[0].level, "info");
    assert_eq!(entries[0].event_type, LEGACY_EVENT_TYPE);
    assert_eq!(entries[0].kind, Some("email".to_string()));
    assert!(entries[0].message.contains(LEGACY_MESSAGE));
    assert!(entries[0].message.contains("kind=email"));
    assert!(entries[0].message.contains("module=dam-filter"));
    assert!(entries[0].message.contains("destination=stdout"));
    assert!(!format!("{:?}", entries[0]).contains("banana@banana.com"));
    assert_eq!(
        Connection::open(&db_path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
    assert!(fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".pre-migration-")
    }));

    store.record(&event()).unwrap();
    let entries = store.list().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].operation_id, "op-1");
}

#[test]
fn implements_event_sink_contract() {
    let store = LogStore::open_in_memory().unwrap();
    let sink: &dyn EventSink = &store;

    sink.record(&event()).unwrap();

    assert_eq!(store.count().unwrap(), 1);
}
