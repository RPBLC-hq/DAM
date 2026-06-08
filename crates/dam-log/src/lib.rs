use dam_core::{EventSink, LogEvent, LogWriteError};
use rusqlite::types::Value;
use rusqlite::{Connection, params, params_from_iter};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const LEGACY_OPERATION_ID: &str = "legacy";
const LEGACY_EVENT_TYPE: &str = "legacy";
const LEGACY_MESSAGE: &str = "legacy log event migrated without raw preview";
const SCHEMA_VERSION: u32 = 3;
const DEFAULT_QUERY_LIMIT: usize = 1_000;
const MAX_QUERY_LIMIT: usize = 10_000;

#[derive(Debug, thiserror::Error)]
pub enum LogStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type LogStoreResult<T> = Result<T, LogStoreError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub id: i64,
    pub timestamp: i64,
    pub operation_id: String,
    pub level: String,
    pub event_type: String,
    pub kind: Option<String>,
    pub value: Option<String>,
    pub reference: Option<String>,
    pub action: Option<String>,
    pub message: String,
}

pub struct LogStore {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogQuery {
    pub min_timestamp: Option<i64>,
    pub after_id: Option<i64>,
    pub event_types: Vec<String>,
    pub actions: Vec<String>,
    pub limit: usize,
}

impl Default for LogQuery {
    fn default() -> Self {
        Self {
            min_timestamp: None,
            after_id: None,
            event_types: Vec::new(),
            actions: Vec::new(),
            limit: DEFAULT_QUERY_LIMIT,
        }
    }
}

impl LogQuery {
    pub fn with_min_timestamp(mut self, min_timestamp: i64) -> Self {
        self.min_timestamp = Some(min_timestamp);
        self
    }

    pub fn with_after_id(mut self, after_id: i64) -> Self {
        self.after_id = Some(after_id);
        self
    }

    pub fn with_event_types<I, S>(mut self, event_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.event_types = event_types.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_actions<I, S>(mut self, actions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.actions = actions.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

impl LogStore {
    pub fn open(path: impl AsRef<Path>) -> LogStoreResult<Self> {
        let path = path.as_ref();
        let conn = Connection::open(path)?;
        Self::from_connection_with_path(conn, Some(path))
    }

    pub fn open_in_memory() -> LogStoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> LogStoreResult<Self> {
        Self::from_connection_with_path(conn, None)
    }

    fn from_connection_with_path(conn: Connection, path: Option<&Path>) -> LogStoreResult<Self> {
        conn.execute_batch(
            "
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
            ",
        )?;

        migrate_log_events_schema(&conn, path)?;

        conn.execute_batch(
            "

            CREATE INDEX IF NOT EXISTS idx_log_events_operation_id
                ON log_events(operation_id);

            CREATE INDEX IF NOT EXISTS idx_log_events_event_type
                ON log_events(event_type);

            CREATE INDEX IF NOT EXISTS idx_log_events_timestamp_id
                ON log_events(timestamp, id);

            CREATE INDEX IF NOT EXISTS idx_log_events_action
                ON log_events(action);
            ",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn record(&self, event: &LogEvent) -> LogStoreResult<()> {
        let conn = self.conn.lock().expect("log sqlite mutex poisoned");
        let kind = event.kind.map(|kind| kind.tag().to_string());
        let reference = event.reference.as_ref().map(|reference| reference.key());

        conn.execute(
            "
            INSERT INTO log_events (
                timestamp,
                operation_id,
                level,
                event_type,
                kind,
                value,
                reference,
                action,
                message
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                event.timestamp,
                event.operation_id.as_str(),
                event.level.tag(),
                event.event_type.tag(),
                kind,
                event.value.as_deref(),
                reference,
                event.action.as_deref(),
                event.message.as_str()
            ],
        )?;

        Ok(())
    }

    pub fn list(&self) -> LogStoreResult<Vec<LogEntry>> {
        let conn = self.conn.lock().expect("log sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "
            SELECT id, timestamp, operation_id, level, event_type, kind, value, reference, action, message
            FROM log_events
            ORDER BY id DESC
            ",
        )?;

        let entries = stmt
            .query_map([], |row| {
                Ok(LogEntry {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    operation_id: row.get(2)?,
                    level: row.get(3)?,
                    event_type: row.get(4)?,
                    kind: row.get(5)?,
                    value: row.get(6)?,
                    reference: row.get(7)?,
                    action: row.get(8)?,
                    message: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    pub fn list_query(&self, query: LogQuery) -> LogStoreResult<Vec<LogEntry>> {
        let conn = self.conn.lock().expect("log sqlite mutex poisoned");
        let (sql, params) = build_list_query(&query);
        let mut stmt = conn.prepare(&sql)?;
        let entries = stmt
            .query_map(params_from_iter(params.iter()), map_log_entry)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    pub fn count(&self) -> LogStoreResult<u64> {
        let conn = self.conn.lock().expect("log sqlite mutex poisoned");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM log_events", [], |row| row.get(0))?;
        Ok(count as u64)
    }
}

fn build_list_query(query: &LogQuery) -> (String, Vec<Value>) {
    let mut sql = String::from(
        "
        SELECT id, timestamp, operation_id, level, event_type, kind, value, reference, action, message
        FROM log_events
        WHERE 1 = 1
        ",
    );
    let mut params = Vec::new();

    if let Some(min_timestamp) = query.min_timestamp {
        sql.push_str(" AND timestamp >= ?");
        params.push(Value::Integer(min_timestamp));
    }
    if let Some(after_id) = query.after_id {
        sql.push_str(" AND id > ?");
        params.push(Value::Integer(after_id));
    }
    append_text_filter(&mut sql, &mut params, "event_type", &query.event_types);
    append_text_filter(&mut sql, &mut params, "action", &query.actions);
    sql.push_str(" ORDER BY id DESC LIMIT ?");
    params.push(Value::Integer(clamped_limit(query.limit) as i64));

    (sql, params)
}

fn append_text_filter(sql: &mut String, params: &mut Vec<Value>, column: &str, values: &[String]) {
    let values = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return;
    }

    sql.push_str(" AND ");
    sql.push_str(column);
    sql.push_str(" IN (");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            sql.push_str(", ");
        }
        sql.push('?');
        params.push(Value::Text((*value).to_string()));
    }
    sql.push(')');
}

fn clamped_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_QUERY_LIMIT)
}

fn map_log_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<LogEntry> {
    Ok(LogEntry {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        operation_id: row.get(2)?,
        level: row.get(3)?,
        event_type: row.get(4)?,
        kind: row.get(5)?,
        value: row.get(6)?,
        reference: row.get(7)?,
        action: row.get(8)?,
        message: row.get(9)?,
    })
}

fn migrate_log_events_schema(conn: &Connection, path: Option<&Path>) -> LogStoreResult<()> {
    let columns = table_columns(conn)?;
    if should_rebuild_legacy_schema(&columns) {
        if let Some(path) = path {
            backup_legacy_database(path)?;
        }
        rebuild_legacy_log_events_schema(conn, &columns)?;
        set_schema_version(conn)?;
        return Ok(());
    }

    ensure_column(
        conn,
        &columns,
        "operation_id",
        &format!("operation_id TEXT NOT NULL DEFAULT '{LEGACY_OPERATION_ID}'"),
    )?;
    ensure_column(
        conn,
        &columns,
        "level",
        "level TEXT NOT NULL DEFAULT 'info'",
    )?;
    ensure_column(
        conn,
        &columns,
        "event_type",
        &format!("event_type TEXT NOT NULL DEFAULT '{LEGACY_EVENT_TYPE}'"),
    )?;
    ensure_column(conn, &columns, "kind", "kind TEXT")?;
    ensure_column(conn, &columns, "value", "value TEXT")?;
    ensure_column(conn, &columns, "reference", "reference TEXT")?;
    ensure_column(conn, &columns, "action", "action TEXT")?;
    ensure_column(
        conn,
        &columns,
        "message",
        &format!("message TEXT NOT NULL DEFAULT '{LEGACY_MESSAGE}'"),
    )?;

    set_schema_version(conn)?;
    Ok(())
}

fn should_rebuild_legacy_schema(columns: &[String]) -> bool {
    ["data_type", "destination", "module_name", "value_preview"]
        .iter()
        .any(|column| has_column(columns, column))
}

fn rebuild_legacy_log_events_schema(conn: &Connection, columns: &[String]) -> rusqlite::Result<()> {
    let mut insert_columns = vec![
        "timestamp".to_string(),
        "operation_id".to_string(),
        "level".to_string(),
        "event_type".to_string(),
        "kind".to_string(),
        "value".to_string(),
        "reference".to_string(),
        "action".to_string(),
        "message".to_string(),
    ];
    let mut select_values = vec![
        if has_column(columns, "timestamp") {
            "timestamp".to_string()
        } else {
            "0".to_string()
        },
        format!("'{LEGACY_OPERATION_ID}'"),
        "'info'".to_string(),
        format!("'{LEGACY_EVENT_TYPE}'"),
        if has_column(columns, "data_type") {
            "data_type".to_string()
        } else {
            "NULL".to_string()
        },
        "NULL".to_string(),
        "NULL".to_string(),
        if has_column(columns, "action") {
            "action".to_string()
        } else {
            "NULL".to_string()
        },
        legacy_message_expr(columns),
    ];

    if has_column(columns, "id") {
        insert_columns.insert(0, "id".to_string());
        select_values.insert(0, "id".to_string());
    }

    conn.execute_batch(&format!(
        "
        BEGIN IMMEDIATE;

        DROP TABLE IF EXISTS log_events_new;

        CREATE TABLE log_events_new (
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

        INSERT INTO log_events_new ({insert_columns})
            SELECT {select_values}
            FROM log_events;

        DROP TABLE log_events;

        ALTER TABLE log_events_new RENAME TO log_events;

        COMMIT;
        ",
        insert_columns = insert_columns.join(", "),
        select_values = select_values.join(", "),
    ))
}

fn legacy_message_expr(columns: &[String]) -> String {
    let mut expr = format!("'{LEGACY_MESSAGE}'");
    if has_column(columns, "data_type") {
        expr.push_str(" || '; kind=' || COALESCE(data_type, '')");
    }
    if has_column(columns, "module_name") {
        expr.push_str(" || '; module=' || COALESCE(module_name, '')");
    }
    if has_column(columns, "destination") {
        expr.push_str(" || '; destination=' || COALESCE(destination, '')");
    }
    expr
}

fn backup_legacy_database(path: &Path) -> std::io::Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "log.db".into());
    let backup_path = path.with_file_name(format!("{file_name}.pre-migration-{timestamp}.bak"));
    fs::copy(path, backup_path)?;
    Ok(())
}

fn set_schema_version(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
}

fn table_columns(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("PRAGMA table_info(log_events)")?;
    stmt.query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()
}

fn has_column(columns: &[String], name: &str) -> bool {
    columns.iter().any(|column| column == name)
}

fn ensure_column(
    conn: &Connection,
    columns: &[String],
    name: &str,
    definition: &str,
) -> rusqlite::Result<()> {
    if has_column(columns, name) {
        return Ok(());
    }

    conn.execute_batch(&format!("ALTER TABLE log_events ADD COLUMN {definition};"))
}

impl EventSink for LogStore {
    fn record(&self, event: &LogEvent) -> Result<(), LogWriteError> {
        LogStore::record(self, event).map_err(|error| LogWriteError::new(error.to_string()))
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
