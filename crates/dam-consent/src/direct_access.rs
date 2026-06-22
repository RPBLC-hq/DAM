use super::{
    ConsentError, ConsentResult, CreateDirectAccessRequest, DirectAccessRequest,
    DirectAccessResolveResult, DirectAccessStatus, fingerprint, generate_grant_id,
    generate_request_id, now_unix_secs,
};
use dam_core::{Reference, SensitiveType, VaultReader};
use rusqlite::{Connection, OptionalExtension, params};

const SELECT_DIRECT_ACCESS_REQUEST: &str = "
    SELECT request_id, grant_id, kind, value_fingerprint, vault_key,
           actor_id, requesting_actor, purpose, reason,
           requested_duration_seconds, pending_expires_at, status,
           decision_reason, created_at, decided_at, grant_expires_at,
           max_resolves, resolve_count, correlation_id
    FROM direct_access_requests
";

pub(crate) fn initialize_schema(conn: &Connection) -> ConsentResult<()> {
    conn.execute_batch(
        "
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
    Ok(())
}

pub(crate) fn create_request(
    conn: &Connection,
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

pub(crate) fn list_requests(conn: &Connection) -> ConsentResult<Vec<DirectAccessRequest>> {
    let now = now_unix_secs()?;
    refresh_timeouts(conn, now)?;
    let mut statement = conn.prepare(&format!(
        "{SELECT_DIRECT_ACCESS_REQUEST}
         ORDER BY created_at DESC, request_id DESC"
    ))?;
    let rows = statement.query_map([], row_to_direct_access_request)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub(crate) fn get_request(
    conn: &Connection,
    request_id: &str,
) -> ConsentResult<Option<DirectAccessRequest>> {
    let now = now_unix_secs()?;
    refresh_timeouts(conn, now)?;
    query_request(conn, request_id)
}

pub(crate) fn approve_request(
    conn: &Connection,
    request_id: &str,
    grant_duration_seconds: u64,
    decision_reason: Option<String>,
) -> ConsentResult<Option<DirectAccessRequest>> {
    if grant_duration_seconds == 0 {
        return Err(ConsentError::InvalidDuration);
    }
    let now = now_unix_secs()?;
    refresh_timeouts(conn, now)?;
    let Some(current) = query_request(conn, request_id)? else {
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
    query_request(conn, request_id)
}

pub(crate) fn deny_request(
    conn: &Connection,
    request_id: &str,
    decision_reason: Option<String>,
) -> ConsentResult<Option<DirectAccessRequest>> {
    finish_request(
        conn,
        request_id,
        DirectAccessStatus::Denied,
        decision_reason.or_else(|| Some("request_denied".to_string())),
    )
}

pub(crate) fn revoke_request(
    conn: &Connection,
    request_id: &str,
    decision_reason: Option<String>,
) -> ConsentResult<Option<DirectAccessRequest>> {
    finish_request(
        conn,
        request_id,
        DirectAccessStatus::Revoked,
        decision_reason.or_else(|| Some("request_revoked".to_string())),
    )
}

pub(crate) fn resolve_request(
    conn: &Connection,
    request_id: &str,
    actor_id: &str,
    vault: &(impl VaultReader + ?Sized),
) -> ConsentResult<Option<DirectAccessResolveResult>> {
    let now = now_unix_secs()?;
    refresh_timeouts(conn, now)?;
    let Some(current) = query_request(conn, request_id)? else {
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
        let request =
            query_request(conn, request_id)?.expect("request exists after consume update");
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
    if fingerprint(current.kind, &value) != current.value_fingerprint {
        conn.execute(
            "
            UPDATE direct_access_requests
            SET status = ?2,
                decision_reason = ?3
            WHERE request_id = ?1
            ",
            params![
                current.request_id,
                DirectAccessStatus::Revoked.tag(),
                "grant_value_changed"
            ],
        )?;
        let request = query_request(conn, request_id)?
            .expect("request exists after fingerprint mismatch update");
        return Ok(Some(DirectAccessResolveResult {
            request,
            value: None,
            outcome_reason: Some("grant_value_changed".to_string()),
        }));
    }

    let now = now_unix_secs()?;
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
          AND actor_id = ?5
          AND status = ?6
          AND resolve_count = ?7
          AND max_resolves = ?8
          AND value_fingerprint = ?9
          AND grant_expires_at IS NOT NULL
          AND grant_expires_at > ?10
        ",
        params![
            current.request_id,
            next_count as i64,
            next_status.tag(),
            next_reason,
            actor_id,
            DirectAccessStatus::Approved.tag(),
            current.resolve_count as i64,
            current.max_resolves as i64,
            current.value_fingerprint,
            now,
        ],
    )?;
    if conn.changes() == 0 {
        refresh_timeouts(conn, now)?;
        let request = query_request(conn, request_id)?
            .expect("request exists after failed atomic resolve update");
        let outcome_reason = request
            .decision_reason
            .clone()
            .or_else(|| Some(request.status.tag().to_string()));
        return Ok(Some(DirectAccessResolveResult {
            request,
            value: None,
            outcome_reason,
        }));
    }
    let request = query_request(conn, request_id)?.expect("request exists after resolve update");
    Ok(Some(DirectAccessResolveResult {
        request,
        value: Some(value),
        outcome_reason: None,
    }))
}

fn finish_request(
    conn: &Connection,
    request_id: &str,
    status: DirectAccessStatus,
    decision_reason: Option<String>,
) -> ConsentResult<Option<DirectAccessRequest>> {
    let now = now_unix_secs()?;
    refresh_timeouts(conn, now)?;
    let Some(current) = query_request(conn, request_id)? else {
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
    query_request(conn, request_id)
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

fn query_request(
    conn: &Connection,
    request_id: &str,
) -> ConsentResult<Option<DirectAccessRequest>> {
    conn.query_row(
        &format!(
            "{SELECT_DIRECT_ACCESS_REQUEST}
             WHERE request_id = ?1 OR grant_id = ?1
             LIMIT 1"
        ),
        params![request_id],
        row_to_direct_access_request,
    )
    .optional()
    .map_err(Into::into)
}

fn refresh_timeouts(conn: &Connection, now: i64) -> ConsentResult<()> {
    conn.execute(
        "
        UPDATE direct_access_requests
        SET status = ?2,
            decision_reason = ?3,
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
            decision_reason = ?3,
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
