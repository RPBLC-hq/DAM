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
#[path = "lib_tests.rs"]
mod tests;
