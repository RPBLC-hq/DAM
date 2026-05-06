use dam_core::{
    Reference, SensitiveType, VaultReadError, VaultReader, VaultRecord, VaultWriteError,
    VaultWriteOptions, VaultWriter,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("system clock is before unix epoch")]
    Clock,
}

pub type VaultResult<T> = Result<T, VaultError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultEntry {
    pub key: String,
    pub value: String,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct Vault {
    conn: Mutex<Connection>,
}

impl Vault {
    pub fn open(path: impl AsRef<Path>) -> VaultResult<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    pub fn open_in_memory() -> VaultResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> VaultResult<Self> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS vault_entries (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            ",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn put(&self, key: &str, value: &str) -> VaultResult<()> {
        let now = now_unix_secs()?;
        let conn = self.conn.lock().expect("vault sqlite mutex poisoned");

        conn.execute(
            "
            INSERT INTO vault_entries (key, value, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?3)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            ",
            params![key, value, now],
        )?;

        Ok(())
    }

    pub fn get(&self, key: &str) -> VaultResult<Option<String>> {
        let conn = self.conn.lock().expect("vault sqlite mutex poisoned");

        let value = conn
            .query_row(
                "SELECT value FROM vault_entries WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;

        Ok(value)
    }

    pub fn delete(&self, key: &str) -> VaultResult<bool> {
        let conn = self.conn.lock().expect("vault sqlite mutex poisoned");
        let deleted = conn.execute("DELETE FROM vault_entries WHERE key = ?1", params![key])?;
        Ok(deleted > 0)
    }

    pub fn list(&self) -> VaultResult<Vec<VaultEntry>> {
        let conn = self.conn.lock().expect("vault sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "
            SELECT key, value, created_at, updated_at
            FROM vault_entries
            ORDER BY key ASC
            ",
        )?;

        let entries = stmt
            .query_map([], |row| {
                Ok(VaultEntry {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(deduplicate_entries(entries))
    }

    pub fn count(&self) -> VaultResult<u64> {
        let conn = self.conn.lock().expect("vault sqlite mutex poisoned");
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM vault_entries", [], |row| row.get(0))?;
        Ok(count as u64)
    }
}

impl VaultWriter for Vault {
    fn write_with_options(
        &self,
        record: &VaultRecord,
        options: VaultWriteOptions,
    ) -> Result<Reference, VaultWriteError> {
        let now = now_unix_secs().map_err(|error| VaultWriteError::new(error.to_string()))?;
        let conn = self.conn.lock().expect("vault sqlite mutex poisoned");

        if options.deduplicate
            && let Some(existing) = find_existing_reference(&conn, record.kind, &record.value)
                .map_err(|error| VaultWriteError::new(error.to_string()))?
        {
            touch_reference(&conn, &existing, now)
                .map_err(|error| VaultWriteError::new(error.to_string()))?;
            return Ok(existing);
        }

        conn.execute(
            "
            INSERT INTO vault_entries (key, value, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?3)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            ",
            params![record.reference.key(), record.value, now],
        )
        .map_err(|error| VaultWriteError::new(error.to_string()))?;

        Ok(record.reference.clone())
    }
}

impl VaultReader for Vault {
    fn read(&self, reference: &Reference) -> Result<Option<String>, VaultReadError> {
        self.get(&reference.key())
            .map_err(|error| VaultReadError::new(error.to_string()))
    }
}

fn now_unix_secs() -> VaultResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| VaultError::Clock)?;
    Ok(duration.as_secs() as i64)
}

fn find_existing_reference(
    conn: &Connection,
    kind: SensitiveType,
    value: &str,
) -> VaultResult<Option<Reference>> {
    let pattern = format!("{}:%", kind.tag());
    let mut stmt = conn.prepare(
        "
        SELECT key
        FROM vault_entries
        WHERE value = ?1
          AND key LIKE ?2
        ORDER BY created_at ASC, key ASC
        ",
    )?;
    let mut rows = stmt.query(params![value, pattern])?;

    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        if let Some(reference) = Reference::parse_key(&key)
            && reference.kind == kind
        {
            return Ok(Some(reference));
        }
    }

    Ok(None)
}

fn touch_reference(conn: &Connection, reference: &Reference, updated_at: i64) -> VaultResult<()> {
    conn.execute(
        "UPDATE vault_entries SET updated_at = ?2 WHERE key = ?1",
        params![reference.key(), updated_at],
    )?;
    Ok(())
}

fn deduplicate_entries(entries: Vec<VaultEntry>) -> Vec<VaultEntry> {
    let mut seen = HashSet::<(SensitiveType, String)>::new();
    let mut deduplicated = Vec::with_capacity(entries.len());

    for entry in entries {
        let Some(reference) = Reference::parse_key(&entry.key) else {
            deduplicated.push(entry);
            continue;
        };
        if seen.insert((reference.kind, entry.value.clone())) {
            deduplicated.push(entry);
        }
    }

    deduplicated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_then_get_returns_value() {
        let vault = Vault::open_in_memory().unwrap();

        vault.put("email:alice", "alice@example.com").unwrap();

        assert_eq!(
            vault.get("email:alice").unwrap(),
            Some("alice@example.com".to_string())
        );
    }

    #[test]
    fn get_missing_key_returns_none() {
        let vault = Vault::open_in_memory().unwrap();

        assert_eq!(vault.get("missing").unwrap(), None);
    }

    #[test]
    fn put_existing_key_replaces_value_without_duplicate() {
        let vault = Vault::open_in_memory().unwrap();

        vault.put("email:alice", "old@example.com").unwrap();
        vault.put("email:alice", "new@example.com").unwrap();

        assert_eq!(vault.count().unwrap(), 1);
        assert_eq!(
            vault.get("email:alice").unwrap(),
            Some("new@example.com".to_string())
        );
    }

    #[test]
    fn delete_existing_key_returns_true() {
        let vault = Vault::open_in_memory().unwrap();

        vault.put("email:alice", "alice@example.com").unwrap();

        assert!(vault.delete("email:alice").unwrap());
        assert_eq!(vault.get("email:alice").unwrap(), None);
    }

    #[test]
    fn delete_missing_key_returns_false() {
        let vault = Vault::open_in_memory().unwrap();

        assert!(!vault.delete("missing").unwrap());
    }

    #[test]
    fn list_returns_entries_ordered_by_key() {
        let vault = Vault::open_in_memory().unwrap();

        vault.put("phone:bob", "+14155551234").unwrap();
        vault.put("email:alice", "alice@example.com").unwrap();

        let entries = vault.list().unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "email:alice");
        assert_eq!(entries[0].value, "alice@example.com");
        assert_eq!(entries[1].key, "phone:bob");
        assert_eq!(entries[1].value, "+14155551234");
    }

    #[test]
    fn list_deduplicates_equal_values_per_kind() {
        let vault = Vault::open_in_memory().unwrap();
        let first = Reference::generate(dam_core::SensitiveType::Email);
        let second = Reference::generate(dam_core::SensitiveType::Email);

        vault.put(&first.key(), "alice@example.com").unwrap();
        vault.put(&second.key(), "alice@example.com").unwrap();

        let entries = vault.list().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, "alice@example.com");
    }

    #[test]
    fn entries_persist_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("vault.db");

        {
            let vault = Vault::open(&db_path).unwrap();
            vault.put("email:alice", "alice@example.com").unwrap();
        }

        let vault = Vault::open(&db_path).unwrap();
        assert_eq!(
            vault.get("email:alice").unwrap(),
            Some("alice@example.com".to_string())
        );
    }

    #[test]
    fn implements_vault_writer_contract() {
        let vault = Vault::open_in_memory().unwrap();
        let reference = dam_core::Reference {
            kind: dam_core::SensitiveType::Email,
            id: "7B2HkqFn9xR4mWpD3nYvKt".to_string(),
        };
        let record = dam_core::VaultRecord {
            reference: reference.clone(),
            kind: dam_core::SensitiveType::Email,
            value: "alice@example.com".to_string(),
        };

        let stored_reference = vault.write(&record).unwrap();

        assert_eq!(stored_reference, reference);
        assert_eq!(
            vault.get(&reference.key()).unwrap(),
            Some("alice@example.com".to_string())
        );
    }

    #[test]
    fn vault_writer_returns_existing_reference_for_duplicate_value() {
        let vault = Vault::open_in_memory().unwrap();
        let first = dam_core::Reference::generate(dam_core::SensitiveType::Email);
        let second = dam_core::Reference::generate(dam_core::SensitiveType::Email);

        let first_stored = vault
            .write(&dam_core::VaultRecord {
                reference: first.clone(),
                kind: dam_core::SensitiveType::Email,
                value: "alice@example.com".to_string(),
            })
            .unwrap();
        let second_stored = vault
            .write(&dam_core::VaultRecord {
                reference: second.clone(),
                kind: dam_core::SensitiveType::Email,
                value: "alice@example.com".to_string(),
            })
            .unwrap();

        assert_eq!(first_stored, first);
        assert_eq!(second_stored, first);
        assert_eq!(vault.count().unwrap(), 1);
        assert_eq!(vault.get(&second.key()).unwrap(), None);
    }

    #[test]
    fn vault_writer_refreshes_existing_reference_recency_for_duplicate_value() {
        let vault = Vault::open_in_memory().unwrap();
        let first = dam_core::Reference::generate(dam_core::SensitiveType::Email);
        let second = dam_core::Reference::generate(dam_core::SensitiveType::Email);

        vault
            .write(&dam_core::VaultRecord {
                reference: first.clone(),
                kind: dam_core::SensitiveType::Email,
                value: "alice@example.com".to_string(),
            })
            .unwrap();
        let before = vault.list().unwrap()[0].updated_at;

        std::thread::sleep(std::time::Duration::from_secs(1));
        let stored = vault
            .write(&dam_core::VaultRecord {
                reference: second,
                kind: dam_core::SensitiveType::Email,
                value: "alice@example.com".to_string(),
            })
            .unwrap();
        let after = vault.list().unwrap()[0].updated_at;

        assert_eq!(stored, first);
        assert!(after > before);
    }

    #[test]
    fn vault_writer_can_store_duplicate_values_when_deduplication_is_disabled() {
        let vault = Vault::open_in_memory().unwrap();
        let first = dam_core::Reference::generate(dam_core::SensitiveType::Email);
        let second = dam_core::Reference::generate(dam_core::SensitiveType::Email);

        vault
            .write(&dam_core::VaultRecord {
                reference: first.clone(),
                kind: dam_core::SensitiveType::Email,
                value: "alice@example.com".to_string(),
            })
            .unwrap();
        let second_stored = vault
            .write_with_options(
                &dam_core::VaultRecord {
                    reference: second.clone(),
                    kind: dam_core::SensitiveType::Email,
                    value: "alice@example.com".to_string(),
                },
                dam_core::VaultWriteOptions { deduplicate: false },
            )
            .unwrap();

        assert_eq!(second_stored, second);
        assert_eq!(vault.count().unwrap(), 2);
        assert_eq!(vault.list().unwrap().len(), 1);
    }

    #[test]
    fn implements_vault_reader_contract() {
        let vault = Vault::open_in_memory().unwrap();
        let reference = dam_core::Reference {
            kind: dam_core::SensitiveType::Email,
            id: "7B2HkqFn9xR4mWpD3nYvKt".to_string(),
        };
        vault.put(&reference.key(), "alice@example.com").unwrap();

        assert_eq!(
            vault.read(&reference).unwrap(),
            Some("alice@example.com".to_string())
        );

        let missing = dam_core::Reference {
            kind: dam_core::SensitiveType::Email,
            id: "2D5hXQp8nJ9kLmN4rT6vWy".to_string(),
        };
        assert_eq!(vault.read(&missing).unwrap(), None);
    }
}
