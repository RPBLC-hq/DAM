//! `/api/v1/allowed` — currently-allowed grants surface (web only).
//!
//! v1 reads `dam-consent` grants if available and joins with vault
//! values when the canonical key resolves. The richer per-target /
//! per-profile scopes are parked (see `passthrough.md`); this slice
//! returns the canonical-value scope only.

use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::AppState;
use crate::activity_map::day_label;
use crate::error::{Ok, WebError, WebErrorCode, WebResult};

#[derive(Debug, Clone, Default, Serialize)]
pub struct AllowedView {
    pub active: Vec<AllowedGrant>,
    pub expired: Vec<AllowedGrant>,
    pub revoked: Vec<AllowedGrant>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowedGrant {
    pub id: String,
    pub party: String,
    pub kind: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub sort: Option<String>,
    pub dir: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> WebResult<AllowedView> {
    let Some(store) = state.consent_store.as_deref() else {
        return Ok(Ok::new(AllowedView::default()));
    };
    let entries = store
        .list()
        .map_err(|_| WebError::new(WebErrorCode::Unknown))?;
    let now = now_unix_secs()?;
    Ok(Ok::new(allowed_view_from_entries(
        state.vault.as_ref(),
        entries,
        &query,
        now,
    )?))
}

fn allowed_view_from_entries(
    vault: &dam_vault::Vault,
    entries: Vec<dam_consent::ConsentEntry>,
    query: &ListQuery,
    now: i64,
) -> Result<AllowedView, WebError> {
    let mut view = AllowedView::default();
    let q = query.q.as_deref().unwrap_or("").to_lowercase();

    for entry in entries {
        let grant = map_grant(vault, &entry)?;
        if !matches_query(&grant, &q) {
            continue;
        }
        match entry.status_at(now) {
            "active" => view.active.push(grant),
            "expired" => view.expired.push(grant),
            _ => view.revoked.push(grant),
        }
    }

    sort_grants(
        &mut view.active,
        query.sort.as_deref(),
        query.dir.as_deref(),
    );
    sort_grants(
        &mut view.expired,
        query.sort.as_deref(),
        query.dir.as_deref(),
    );
    sort_grants(
        &mut view.revoked,
        query.sort.as_deref(),
        query.dir.as_deref(),
    );

    Ok(view)
}

fn map_grant(
    vault: &dam_vault::Vault,
    entry: &dam_consent::ConsentEntry,
) -> Result<AllowedGrant, WebError> {
    Ok(AllowedGrant {
        id: entry.id.clone(),
        party: entry.created_by.clone(),
        kind: entry.kind.tag().to_string(),
        value: grant_value(vault, entry)?,
        since: Some(day_label(entry.created_at)),
        expires_at: Some(day_label(entry.expires_at)),
    })
}

fn grant_value(
    vault: &dam_vault::Vault,
    entry: &dam_consent::ConsentEntry,
) -> Result<String, WebError> {
    let Some(vault_key) = &entry.vault_key else {
        return Ok(format!("[{} grant]", entry.kind.tag()));
    };

    match vault
        .get(vault_key)
        .map_err(|_| WebError::new(WebErrorCode::WalletUnreachable))?
    {
        Some(value) => Ok(value),
        None => Ok(format!("[{vault_key}]")),
    }
}

fn matches_query(grant: &AllowedGrant, q: &str) -> bool {
    q.is_empty()
        || grant.party.to_lowercase().contains(q)
        || grant.kind.to_lowercase().contains(q)
        || grant.value.to_lowercase().contains(q)
}

fn sort_grants(grants: &mut [AllowedGrant], sort: Option<&str>, dir: Option<&str>) {
    let descending = matches!(dir, Some("desc"));
    match sort.unwrap_or("recent") {
        "kind" => grants.sort_by(|a, b| a.kind.cmp(&b.kind)),
        "party" => grants.sort_by(|a, b| a.party.cmp(&b.party)),
        "value" => grants.sort_by(|a, b| a.value.cmp(&b.value)),
        "expires" => grants.sort_by(|a, b| a.expires_at.cmp(&b.expires_at)),
        _ => {}
    }
    if descending {
        grants.reverse();
    }
}

fn now_unix_secs() -> Result<i64, WebError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .map_err(|_| WebError::new(WebErrorCode::Unknown))
}

#[cfg(test)]
#[path = "allowed_tests.rs"]
mod tests;
