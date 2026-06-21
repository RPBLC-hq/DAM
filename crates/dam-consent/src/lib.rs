use dam_core::{
    PolicyAction, PolicyDecision, Reference, SensitiveType, VaultReadError, VaultReader,
    canonical_sensitive_value,
};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

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

            CREATE TABLE IF NOT EXISTS direct_access_requests (
                request_id TEXT PRIMARY KEY NOT NULL,
                grant_id TEXT,
                kind TEXT NOT NULL,
                value_fingerprint TEXT NOT NULL,
                vault_key TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                requesting_actor TEXT NOT NULL,
                purpose TEXT NOT NULL,
                reason TEXT,
                requested_duration_seconds INTEGER NOT NULL,
                pending_expires_at INTEGER NOT NULL,
                status TEXT NOT NULL,
                decision_reason TEXT,
                created_at INTEGER NOT NULL,
                decided_at INTEGER,
                grant_expires_at INTEGER,
                max_resolves INTEGER NOT NULL,
                resolve_count INTEGER NOT NULL,
                correlation_id TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_direct_access_requests_actor
                ON direct_access_requests(actor_id, status, pending_expires_at, grant_expires_at);
            CREATE INDEX IF NOT EXISTS idx_direct_access_requests_vault_key
                ON direct_access_requests(vault_key);
            ",
        )?;

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
        if request.requested_duration_seconds == 0 || request.pending_timeout_seconds == 0 {
            return Err(ConsentError::InvalidDuration);
        }

        let reference = Reference::parse_key(&request.vault_key)
            .ok_or_else(|| ConsentError::InvalidReference(request.vault_key.clone()))?;
        let Some(value) = vault.read(&reference)? else {
            return Err(ConsentError::VaultValueNotFound(request.vault_key.clone()));
        };
        let now = now_unix_secs()?;
        let entry = DirectAccessRequest {
            request_id: generate_request_id(),
            grant_id: None,
            kind: reference.kind,
            value_fingerprint: fingerprint(reference.kind, &value),
            vault_key: reference.key(),
            actor_id: request.actor_id.clone(),
            requesting_actor: request.requesting_actor.clone(),
            purpose: request.purpose.clone(),
            reason: request.reason.clone(),
            requested_duration_seconds: request.requested_duration_seconds,
            pending_expires_at: now + request.pending_timeout_seconds as i64,
            status: DirectAccessStatus::Pending,
            decision_reason: None,
            created_at: now,
            decided_at: None,
            grant_expires_at: None,
            max_resolves: 1,
            resolve_count: 0,
            correlation_id: request.correlation_id.clone(),
        };

        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        conn.execute(
            "
            INSERT INTO direct_access_requests (
                request_id, grant_id, kind, value_fingerprint, vault_key,
                actor_id, requesting_actor, purpose, reason,
                requested_duration_seconds, pending_expires_at, status,
                decision_reason, created_at, decided_at, grant_expires_at,
                max_resolves, resolve_count, correlation_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ",
            params![
                entry.request_id,
                entry.grant_id,
                entry.kind.tag(),
                entry.value_fingerprint,
                entry.vault_key,
                entry.actor_id,
                entry.requesting_actor,
                entry.purpose,
                entry.reason,
                entry.requested_duration_seconds as i64,
                entry.pending_expires_at,
                entry.status.tag(),
                entry.decision_reason,
                entry.created_at,
                entry.decided_at,
                entry.grant_expires_at,
                entry.max_resolves as i64,
                entry.resolve_count as i64,
                entry.correlation_id,
            ],
        )?;

        Ok(entry)
    }

    pub fn list_direct_access_requests(&self) -> ConsentResult<Vec<DirectAccessRequest>> {
        let now = now_unix_secs()?;
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        refresh_direct_access_timeouts(&conn, now)?;
        let mut statement = conn.prepare(
            "
            SELECT request_id, grant_id, kind, value_fingerprint, vault_key,
                   actor_id, requesting_actor, purpose, reason,
                   requested_duration_seconds, pending_expires_at, status,
                   decision_reason, created_at, decided_at, grant_expires_at,
                   max_resolves, resolve_count, correlation_id
            FROM direct_access_requests
            ORDER BY created_at DESC, request_id DESC
            ",
        )?;
        let rows = statement.query_map([], row_to_direct_access_request)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn direct_access_request(
        &self,
        request_id: &str,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        let now = now_unix_secs()?;
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        refresh_direct_access_timeouts(&conn, now)?;
        conn.query_row(
            "
            SELECT request_id, grant_id, kind, value_fingerprint, vault_key,
                   actor_id, requesting_actor, purpose, reason,
                   requested_duration_seconds, pending_expires_at, status,
                   decision_reason, created_at, decided_at, grant_expires_at,
                   max_resolves, resolve_count, correlation_id
            FROM direct_access_requests
            WHERE request_id = ?1 OR grant_id = ?1
            LIMIT 1
            ",
            params![request_id],
            row_to_direct_access_request,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn approve_direct_access_request(
        &self,
        request_id: &str,
        grant_duration_seconds: u64,
        decision_reason: Option<String>,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        if grant_duration_seconds == 0 {
            return Err(ConsentError::InvalidDuration);
        }
        let now = now_unix_secs()?;
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        refresh_direct_access_timeouts(&conn, now)?;
        let Some(current) = query_direct_access_request(&conn, request_id)? else {
            return Ok(None);
        };
        if current.status != DirectAccessStatus::Pending {
            return Ok(Some(current));
        }
        let grant_id = generate_grant_id();
        conn.execute(
            "
            UPDATE direct_access_requests
            SET grant_id = ?2,
                status = ?3,
                decision_reason = ?4,
                decided_at = ?5,
                grant_expires_at = ?6
            WHERE request_id = ?1
            ",
            params![
                current.request_id,
                grant_id,
                DirectAccessStatus::Approved.tag(),
                decision_reason,
                now,
                now + grant_duration_seconds as i64,
            ],
        )?;
        query_direct_access_request(&conn, request_id)
    }

    pub fn deny_direct_access_request(
        &self,
        request_id: &str,
        decision_reason: Option<String>,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        self.finish_direct_access_request(
            request_id,
            DirectAccessStatus::Denied,
            decision_reason.or_else(|| Some("request_denied".to_string())),
        )
    }

    pub fn revoke_direct_access_request(
        &self,
        request_id: &str,
        decision_reason: Option<String>,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        self.finish_direct_access_request(
            request_id,
            DirectAccessStatus::Revoked,
            decision_reason.or_else(|| Some("request_revoked".to_string())),
        )
    }

    pub fn resolve_direct_access_request(
        &self,
        request_id: &str,
        actor_id: &str,
        vault: &(impl VaultReader + ?Sized),
    ) -> ConsentResult<Option<DirectAccessResolveResult>> {
        let now = now_unix_secs()?;
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        refresh_direct_access_timeouts(&conn, now)?;
        let Some(current) = query_direct_access_request(&conn, request_id)? else {
            return Ok(None);
        };
        if current.actor_id != actor_id {
            return Ok(Some(DirectAccessResolveResult {
                request: current,
                value: None,
                outcome_reason: Some("actor_mismatch".to_string()),
            }));
        }
        if current.status != DirectAccessStatus::Approved {
            return Ok(Some(DirectAccessResolveResult {
                request: current.clone(),
                value: None,
                outcome_reason: current
                    .decision_reason
                    .clone()
                    .or_else(|| Some(current.status.tag().to_string())),
            }));
        }
        if current.resolve_count >= current.max_resolves {
            conn.execute(
                "
                UPDATE direct_access_requests
                SET status = ?2,
                    decision_reason = COALESCE(decision_reason, ?3)
                WHERE request_id = ?1
                ",
                params![
                    current.request_id,
                    DirectAccessStatus::Consumed.tag(),
                    "grant_consumed"
                ],
            )?;
            let request = query_direct_access_request(&conn, request_id)?
                .expect("request exists after consume update");
            return Ok(Some(DirectAccessResolveResult {
                request,
                value: None,
                outcome_reason: Some("grant_consumed".to_string()),
            }));
        }

        let reference = Reference::parse_key(&current.vault_key)
            .ok_or_else(|| ConsentError::InvalidReference(current.vault_key.clone()))?;
        let Some(value) = vault.read(&reference)? else {
            return Ok(Some(DirectAccessResolveResult {
                request: current,
                value: None,
                outcome_reason: Some("vault_value_missing".to_string()),
            }));
        };

        let next_count = current.resolve_count + 1;
        let next_status = if next_count >= current.max_resolves {
            DirectAccessStatus::Consumed
        } else {
            DirectAccessStatus::Approved
        };
        let next_reason = if next_status == DirectAccessStatus::Consumed {
            Some("grant_consumed".to_string())
        } else {
            current.decision_reason.clone()
        };
        conn.execute(
            "
            UPDATE direct_access_requests
            SET resolve_count = ?2,
                status = ?3,
                decision_reason = ?4
            WHERE request_id = ?1
            ",
            params![
                current.request_id,
                next_count as i64,
                next_status.tag(),
                next_reason
            ],
        )?;
        let request = query_direct_access_request(&conn, request_id)?
            .expect("request exists after resolve update");
        Ok(Some(DirectAccessResolveResult {
            request,
            value: Some(value),
            outcome_reason: None,
        }))
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

    fn finish_direct_access_request(
        &self,
        request_id: &str,
        status: DirectAccessStatus,
        decision_reason: Option<String>,
    ) -> ConsentResult<Option<DirectAccessRequest>> {
        let now = now_unix_secs()?;
        let conn = self.conn.lock().expect("consent sqlite mutex poisoned");
        refresh_direct_access_timeouts(&conn, now)?;
        let Some(current) = query_direct_access_request(&conn, request_id)? else {
            return Ok(None);
        };
        if matches!(
            current.status,
            DirectAccessStatus::Denied
                | DirectAccessStatus::Expired
                | DirectAccessStatus::Revoked
                | DirectAccessStatus::Consumed
        ) {
            return Ok(Some(current));
        }
        conn.execute(
            "
            UPDATE direct_access_requests
            SET status = ?2,
                decision_reason = ?3,
                decided_at = COALESCE(decided_at, ?4)
            WHERE request_id = ?1
            ",
            params![current.request_id, status.tag(), decision_reason, now],
        )?;
        query_direct_access_request(&conn, request_id)
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

fn row_to_direct_access_request(row: &rusqlite::Row<'_>) -> rusqlite::Result<DirectAccessRequest> {
    let kind_tag: String = row.get(2)?;
    let status_tag: String = row.get(11)?;
    let kind = SensitiveType::from_tag(&kind_tag).unwrap_or(SensitiveType::Email);
    let status = DirectAccessStatus::from_tag(&status_tag).unwrap_or(DirectAccessStatus::Denied);
    Ok(DirectAccessRequest {
        request_id: row.get(0)?,
        grant_id: row.get(1)?,
        kind,
        value_fingerprint: row.get(3)?,
        vault_key: row.get(4)?,
        actor_id: row.get(5)?,
        requesting_actor: row.get(6)?,
        purpose: row.get(7)?,
        reason: row.get(8)?,
        requested_duration_seconds: row.get::<_, i64>(9)? as u64,
        pending_expires_at: row.get(10)?,
        status,
        decision_reason: row.get(12)?,
        created_at: row.get(13)?,
        decided_at: row.get(14)?,
        grant_expires_at: row.get(15)?,
        max_resolves: row.get::<_, i64>(16)? as u64,
        resolve_count: row.get::<_, i64>(17)? as u64,
        correlation_id: row.get(18)?,
    })
}

fn query_direct_access_request(
    conn: &Connection,
    request_id: &str,
) -> ConsentResult<Option<DirectAccessRequest>> {
    conn.query_row(
        "
        SELECT request_id, grant_id, kind, value_fingerprint, vault_key,
               actor_id, requesting_actor, purpose, reason,
               requested_duration_seconds, pending_expires_at, status,
               decision_reason, created_at, decided_at, grant_expires_at,
               max_resolves, resolve_count, correlation_id
        FROM direct_access_requests
        WHERE request_id = ?1 OR grant_id = ?1
        LIMIT 1
        ",
        params![request_id],
        row_to_direct_access_request,
    )
    .optional()
    .map_err(Into::into)
}

fn refresh_direct_access_timeouts(conn: &Connection, now: i64) -> ConsentResult<()> {
    conn.execute(
        "
        UPDATE direct_access_requests
        SET status = ?2,
            decision_reason = COALESCE(decision_reason, ?3),
            decided_at = COALESCE(decided_at, ?1)
        WHERE status = ?4
          AND pending_expires_at <= ?1
        ",
        params![
            now,
            DirectAccessStatus::Expired.tag(),
            "pending_timeout",
            DirectAccessStatus::Pending.tag(),
        ],
    )?;
    conn.execute(
        "
        UPDATE direct_access_requests
        SET status = ?2,
            decision_reason = COALESCE(decision_reason, ?3),
            decided_at = COALESCE(decided_at, ?1)
        WHERE status = ?4
          AND grant_expires_at IS NOT NULL
          AND grant_expires_at <= ?1
        ",
        params![
            now,
            DirectAccessStatus::Expired.tag(),
            "grant_expired",
            DirectAccessStatus::Approved.tag(),
        ],
    )?;
    Ok(())
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

fn generate_request_id() -> String {
    let uuid = uuid::Uuid::new_v4();
    format!("request_{}", bs58::encode(uuid.as_bytes()).into_string())
}

fn generate_grant_id() -> String {
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

fn now_unix_secs() -> ConsentResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ConsentError::Clock)?;
    Ok(duration.as_secs() as i64)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
