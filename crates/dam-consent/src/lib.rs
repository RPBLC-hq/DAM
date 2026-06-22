use dam_core::{
    PolicyAction, PolicyDecision, Reference, SensitiveType, VaultReadError, VaultReader,
    canonical_sensitive_value,
};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

mod direct_access;

pub const DEFAULT_SCOPE: &str = "global";

#[derive(Debug, thiserror::Error)]
pub enum ConsentError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("system clock is before unix epoch")]
    Clock,

    #[error("invalid vault reference: {0}")]
    InvalidReference(String),

    #[error("vault value not found for {0}")]
    VaultValueNotFound(String),

    #[error("vault read failed")]
    VaultRead,

    #[error("request duration must be positive")]
    InvalidDuration,
}

impl From<VaultReadError> for ConsentError {
    fn from(_: VaultReadError) -> Self {
        Self::VaultRead
    }
}

pub type ConsentResult<T> = Result<T, ConsentError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentEntry {
    pub id: String,
    pub kind: SensitiveType,
    pub value_fingerprint: String,
    pub vault_key: Option<String>,
    pub scope: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub created_by: String,
    pub reason: Option<String>,
}

impl ConsentEntry {
    pub fn is_active_at(&self, now: i64) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }

    pub fn status_at(&self, now: i64) -> &'static str {
        if self.revoked_at.is_some() {
            "revoked"
        } else if self.expires_at <= now {
            "expired"
        } else {
            "active"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantConsent {
    pub kind: SensitiveType,
    pub value: String,
    pub vault_key: Option<String>,
    pub ttl_seconds: u64,
    pub created_by: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentMatch {
    pub consent_id: String,
    pub kind: SensitiveType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectAccessStatus {
    Pending,
    Approved,
    Denied,
    Expired,
    Revoked,
    Consumed,
}

impl DirectAccessStatus {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
            Self::Consumed => "consumed",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "denied" => Some(Self::Denied),
            "expired" => Some(Self::Expired),
            "revoked" => Some(Self::Revoked),
            "consumed" => Some(Self::Consumed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectAccessRequest {
    pub request_id: String,
    pub grant_id: Option<String>,
    pub kind: SensitiveType,
    pub value_fingerprint: String,
    pub vault_key: String,
    pub actor_id: String,
    pub requesting_actor: String,
    pub purpose: String,
    pub reason: Option<String>,
    pub requested_duration_seconds: u64,
    pub pending_expires_at: i64,
    pub status: DirectAccessStatus,
    pub decision_reason: Option<String>,
    pub created_at: i64,
    pub decided_at: Option<i64>,
    pub grant_expires_at: Option<i64>,
    pub max_resolves: u64,
    pub resolve_count: u64,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDirectAccessRequest {
    pub vault_key: String,
    pub actor_id: String,
    pub requesting_actor: String,
    pub purpose: String,
    pub reason: Option<String>,
    pub requested_duration_seconds: u64,
    pub pending_timeout_seconds: u64,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectAccessResolveResult {
    pub request: DirectAccessRequest,
    pub value: Option<String>,
    pub outcome_reason: Option<String>,
}

pub struct ConsentStore {
    conn: Mutex<Connection>,
}

impl ConsentStore {
    pub fn open(path: impl AsRef<Path>) -> ConsentResult<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    pub fn open_in_memory() -> ConsentResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> ConsentResult<Self> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS consents (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                value_fingerprint TEXT NOT NULL,
                vault_key TEXT,
                scope TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                revoked_at INTEGER,
                created_by TEXT NOT NULL,
                reason TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_consents_lookup
                ON consents(kind, value_fingerprint, scope, expires_at, revoked_at);
            CREATE INDEX IF NOT EXISTS idx_consents_vault_key
                ON consents(vault_key);
            ",
        )?;
        direct_access::initialize_schema(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn grant(&self, grant: &GrantConsent) -> ConsentResult<ConsentEntry> {
        self.grant_scoped(grant, DEFAULT_SCOPE)
    }

    pub fn grant_scoped(
        &self,
        grant: &GrantConsent,
        scope: impl AsRef<str>,
    ) -> ConsentResult<ConsentEntry> {
        let now = now_unix_secs()?;
        let scope = normalize_scope(scope.as_ref());
        let entry = ConsentEntry {
            id: generate_consent_id(),
            kind: grant.kind,
            value_fingerprint: fingerprint(grant.kind, &grant.value),
            vault_key: grant.vault_key.clone(),
            scope,
            created_at: now,
            expires_at: now + grant.ttl_seconds as i64,
            revoked_at: None,
            created_by: grant.created_by.clone(),
            reason: grant.reason.clone(),
        };

        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        conn.execute(
            "
            INSERT INTO consents (
                id, kind, value_fingerprint, vault_key, scope,
                created_at, expires_at, revoked_at, created_by, reason
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                entry.id,
                entry.kind.tag(),
                entry.value_fingerprint,
                entry.vault_key,
                entry.scope,
                entry.created_at,
                entry.expires_at,
                entry.revoked_at,
                entry.created_by,
                entry.reason,
            ],
        )?;

        Ok(entry)
    }

    pub fn grant_for_reference(
        &self,
        vault_key: &str,
        vault: &(impl VaultReader + ?Sized),
        ttl_seconds: u64,
        created_by: impl Into<String>,
        reason: Option<String>,
    ) -> ConsentResult<ConsentEntry> {
        self.grant_for_reference_scoped(
            vault_key,
            vault,
            ttl_seconds,
            created_by,
            reason,
            DEFAULT_SCOPE,
        )
    }

    pub fn grant_for_reference_scoped(
        &self,
        vault_key: &str,
        vault: &(impl VaultReader + ?Sized),
        ttl_seconds: u64,
        created_by: impl Into<String>,
        reason: Option<String>,
        scope: impl AsRef<str>,
    ) -> ConsentResult<ConsentEntry> {
        let reference = Reference::parse_key(vault_key)
            .ok_or_else(|| ConsentError::InvalidReference(vault_key.to_string()))?;
        let Some(value) = vault.read(&reference)? else {
            return Err(ConsentError::VaultValueNotFound(vault_key.to_string()));
        };

        self.grant_scoped(
            &GrantConsent {
                kind: reference.kind,
                value,
                vault_key: Some(reference.key()),
                ttl_seconds,
                created_by: created_by.into(),
                reason,
            },
            scope,
        )
    }

    pub fn create_direct_access_request(
        &self,
        request: &CreateDirectAccessRequest,
        vault: &(impl VaultReader + ?Sized),
    ) -> ConsentResult<DirectAccessRequest> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        direct_access::create_request(&conn, request, vault)
    }

    pub fn list_direct_access_requests(&self) -> ConsentResult<Vec<DirectAccessRequest>> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        direct_access::list_requests(&conn)
    }

    pub fn direct_access_request(
        &self,
        request_id: &str,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        direct_access::get_request(&conn, request_id)
    }

    pub fn approve_direct_access_request(
        &self,
        request_id: &str,
        grant_duration_seconds: u64,
        decision_reason: Option<String>,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        direct_access::approve_request(&conn, request_id, grant_duration_seconds, decision_reason)
    }

    pub fn deny_direct_access_request(
        &self,
        request_id: &str,
        decision_reason: Option<String>,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        direct_access::deny_request(&conn, request_id, decision_reason)
    }

    pub fn revoke_direct_access_request(
        &self,
        request_id: &str,
        decision_reason: Option<String>,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        direct_access::revoke_request(&conn, request_id, decision_reason)
    }

    pub fn resolve_direct_access_request(
        &self,
        request_id: &str,
        actor_id: &str,
        vault: &(impl VaultReader + ?Sized),
    ) -> ConsentResult<Option<DirectAccessResolveResult>> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        direct_access::resolve_request(&conn, request_id, actor_id, vault)
    }

    pub fn active_for_value(
        &self,
        kind: SensitiveType,
        value: &str,
    ) -> ConsentResult<Option<ConsentEntry>> {
        self.active_for_value_in_scopes(kind, value, &[])
    }

    pub fn active_for_value_in_scopes(
        &self,
        kind: SensitiveType,
        value: &str,
        scopes: &[String],
    ) -> ConsentResult<Option<ConsentEntry>> {
        let lookup_scopes = lookup_scopes(scopes);
        for scope in lookup_scopes {
            if let Some(entry) = self.active_for_value_in_scope(kind, value, &scope)? {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    fn active_for_value_in_scope(
        &self,
        kind: SensitiveType,
        value: &str,
        scope: &str,
    ) -> ConsentResult<Option<ConsentEntry>> {
        let now = now_unix_secs()?;
        let value_fingerprint = fingerprint(kind, value);
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        let entry = conn
            .query_row(
                "
                SELECT id, kind, value_fingerprint, vault_key, scope,
                       created_at, expires_at, revoked_at, created_by, reason
                FROM consents
                WHERE kind = ?1
                  AND value_fingerprint = ?2
                  AND scope = ?3
                  AND revoked_at IS NULL
                  AND expires_at > ?4
                ORDER BY expires_at DESC
                LIMIT 1
                ",
                params![kind.tag(), value_fingerprint, scope, now],
                row_to_entry,
            )
            .optional()?;

        Ok(entry)
    }

    pub fn revoke(&self, id: &str) -> ConsentResult<bool> {
        let now = now_unix_secs()?;
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        let target = conn
            .query_row(
                "
                SELECT kind, value_fingerprint, scope
                FROM consents
                WHERE id = ?1
                ",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((kind, value_fingerprint, scope)) = target else {
            return Ok(false);
        };

        let changed = conn.execute(
            "
            UPDATE consents
            SET revoked_at = ?1
            WHERE kind = ?2
              AND value_fingerprint = ?3
              AND scope = ?4
              AND revoked_at IS NULL
            ",
            params![now, kind, value_fingerprint, scope],
        )?;
        Ok(changed > 0)
    }

    pub fn revoke_for_vault_key(&self, vault_key: &str) -> ConsentResult<u64> {
        self.revoke_for_vault_key_where(vault_key, None)
    }

    pub fn revoke_for_vault_key_and_created_by(
        &self,
        vault_key: &str,
        created_by: &str,
    ) -> ConsentResult<u64> {
        self.revoke_for_vault_key_where(vault_key, Some(created_by))
    }

    fn revoke_for_vault_key_where(
        &self,
        vault_key: &str,
        created_by: Option<&str>,
    ) -> ConsentResult<u64> {
        let now = now_unix_secs()?;
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        let changed = if let Some(created_by) = created_by {
            conn.execute(
                "
                UPDATE consents
                SET revoked_at = ?1
                WHERE vault_key = ?2
                  AND created_by = ?3
                  AND revoked_at IS NULL
                ",
                params![now, vault_key, created_by],
            )?
        } else {
            conn.execute(
                "
                UPDATE consents
                SET revoked_at = ?1
                WHERE vault_key = ?2
                  AND revoked_at IS NULL
                ",
                params![now, vault_key],
            )?
        };
        Ok(changed as u64)
    }

    pub fn list(&self) -> ConsentResult<Vec<ConsentEntry>> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        let mut stmt = conn.prepare(
            "
            SELECT id, kind, value_fingerprint, vault_key, scope,
                   created_at, expires_at, revoked_at, created_by, reason
            FROM consents
            ORDER BY created_at DESC, id ASC
            ",
        )?;

        let entries = stmt
            .query_map([], row_to_entry)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    pub fn count(&self) -> ConsentResult<u64> {
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM consents", [], |row| row.get(0))?;
        Ok(count as u64)
    }
}

pub fn apply_consents_to_decisions(
    decisions: &[PolicyDecision],
    store: Option<&ConsentStore>,
) -> ConsentResult<(Vec<PolicyDecision>, Vec<ConsentMatch>)> {
    apply_consents_to_decisions_in_scopes(decisions, store, &[])
}

pub fn apply_consents_to_decisions_in_scopes(
    decisions: &[PolicyDecision],
    store: Option<&ConsentStore>,
    scopes: &[String],
) -> ConsentResult<(Vec<PolicyDecision>, Vec<ConsentMatch>)> {
    let Some(store) = store else {
        return Ok((decisions.to_vec(), Vec::new()));
    };

    let mut matches = Vec::new();
    let mut applied = Vec::with_capacity(decisions.len());
    for decision in decisions {
        if decision.action == PolicyAction::Block {
            applied.push(decision.clone());
            continue;
        }

        if let Some(consent) = store.active_for_value_in_scopes(
            decision.detection.kind,
            &decision.detection.value,
            scopes,
        )? {
            matches.push(ConsentMatch {
                consent_id: consent.id,
                kind: decision.detection.kind,
            });
            applied.push(PolicyDecision::new(
                decision.detection.clone(),
                PolicyAction::Allow,
            ));
        } else {
            applied.push(decision.clone());
        }
    }

    Ok((applied, matches))
}

pub fn target_scope(target_name: &str) -> String {
    format!("target:{}", target_name.trim())
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConsentEntry> {
    let kind_tag: String = row.get(1)?;
    let kind = SensitiveType::from_tag(&kind_tag).unwrap_or(SensitiveType::Email);
    Ok(ConsentEntry {
        id: row.get(0)?,
        kind,
        value_fingerprint: row.get(2)?,
        vault_key: row.get(3)?,
        scope: row.get(4)?,
        created_at: row.get(5)?,
        expires_at: row.get(6)?,
        revoked_at: row.get(7)?,
        created_by: row.get(8)?,
        reason: row.get(9)?,
    })
}

pub fn fingerprint(kind: SensitiveType, value: &str) -> String {
    let canonical_value = canonical_sensitive_value(kind, value);
    let mut hasher = Sha256::new();
    hasher.update(b"dam-consent-v1\0");
    hasher.update(kind.tag().as_bytes());
    hasher.update(b"\0");
    hasher.update(canonical_value.as_bytes());
    bs58::encode(hasher.finalize()).into_string()
}

fn generate_consent_id() -> String {
    let uuid = uuid::Uuid::new_v4();
    format!("consent_{}", bs58::encode(uuid.as_bytes()).into_string())
}

pub(crate) fn generate_request_id() -> String {
    let uuid = uuid::Uuid::new_v4();
    format!("request_{}", bs58::encode(uuid.as_bytes()).into_string())
}

pub(crate) fn generate_grant_id() -> String {
    let uuid = uuid::Uuid::new_v4();
    format!("grant_{}", bs58::encode(uuid.as_bytes()).into_string())
}

fn normalize_scope(scope: &str) -> String {
    let trimmed = scope.trim();
    if trimmed.is_empty() {
        DEFAULT_SCOPE.to_string()
    } else {
        trimmed.to_string()
    }
}

fn lookup_scopes(scopes: &[String]) -> Vec<String> {
    let mut normalized = vec![DEFAULT_SCOPE.to_string()];
    for scope in scopes {
        let scope = normalize_scope(scope);
        if scope != DEFAULT_SCOPE && !normalized.contains(&scope) {
            normalized.push(scope);
        }
    }
    normalized
}

pub(crate) fn now_unix_secs() -> ConsentResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ConsentError::Clock)?;
    Ok(duration.as_secs() as i64)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "direct_access_tests.rs"]
mod direct_access_tests;
