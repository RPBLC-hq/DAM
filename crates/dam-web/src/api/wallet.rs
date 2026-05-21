//! Wallet list, detail, and consent mutations.
//!
//! v1 reads user-maintained `dam-vault` wallet entries and joins simple consent state.
//! Full at-a-glance metadata and per-event last-seen derivation land
//! progressively.

use axum::Json;
use axum::extract::{Path, Query, State};
use dam_core::{Reference, SensitiveType, VaultRecord, VaultWriter};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::AppState;
use crate::activity_map::day_label;
use crate::error::{Ok, WebError, WebErrorCode, WebResult};
use crate::events_bus::EventTopic;

const DEFAULT_WALLET_GRANT_TTL_SECONDS: u64 = 365 * 24 * 60 * 60;
const GLOBAL_ALLOW_PARTY: &str = "All profiles";
#[derive(Debug, Clone, Serialize)]
pub struct WalletItem {
    pub id: String,
    pub kind: String,
    pub value: String,
    pub state: ItemState,
    pub shared_with: Vec<SharedWith>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SharedWith {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ItemState {
    Protected,
    Allowed,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletList {
    pub items: Vec<WalletItem>,
    pub total: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub state: Option<String>,
    pub sort: Option<String>,
    pub dir: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> WebResult<WalletList> {
    let entries = state
        .vault
        .list_wallet()
        .map_err(|_| WebError::new(WebErrorCode::WalletUnreachable))?;
    let consents = consent_entries(&state)?;

    let mut items = wallet_items_from_entries(entries, &consents, &query, now_unix_secs()?);
    let total = items.len() as u64;

    sort_items(&mut items, query.sort.as_deref(), query.dir.as_deref());

    Ok(Ok::new(WalletList { total, items }))
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddWalletRequest {
    pub kind: String,
    pub value: String,
}

pub async fn add(
    State(state): State<AppState>,
    Json(body): Json<AddWalletRequest>,
) -> WebResult<WalletDetail> {
    let kind = parse_kind(&body.kind)?;
    let value = body.value.trim();
    if value.is_empty() {
        return Err(WebError::new(WebErrorCode::InvalidRequest));
    }

    let record = VaultRecord {
        reference: Reference::generate(kind),
        kind,
        value: value.to_string(),
    };
    let reference = state
        .vault
        .write(&record)
        .map_err(|_| WebError::new(WebErrorCode::WalletUnreachable))?;
    state
        .vault
        .put_wallet(&reference.key(), value)
        .map_err(|_| WebError::new(WebErrorCode::WalletUnreachable))?;
    state.events.notify(EventTopic::WalletInvalidate);
    state.events.notify(EventTopic::ConnectUpdate);

    let detail = wallet_detail_for_key(&state, &reference.key())?;
    Ok(Ok::new(detail))
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletDetail {
    pub item: WalletItem,
    pub meta: Vec<MetaEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<String>,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetaEntry {
    pub key: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub emphasis: Option<bool>,
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> WebResult<WalletDetail> {
    let key = resolve_wallet_route_key(&state, &id)?;
    Ok(Ok::new(wallet_detail_for_key(&state, &key)?))
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllowRequest {
    pub party: String,
    pub ttl_seconds: Option<u64>,
    pub reason: Option<String>,
    pub scope: Option<String>,
    pub profile_id: Option<String>,
}

pub async fn allow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AllowRequest>,
) -> WebResult<WalletDetail> {
    let key = resolve_wallet_route_key(&state, &id)?;
    let party = body.party.trim();
    if party.is_empty() {
        return Err(WebError::new(WebErrorCode::InvalidRequest));
    }
    let store = state
        .consent_store
        .as_deref()
        .ok_or_else(|| WebError::new(WebErrorCode::ConsentGrantFailed))?;
    let ttl_seconds = body.ttl_seconds.unwrap_or(DEFAULT_WALLET_GRANT_TTL_SECONDS);
    let scopes = allow_scopes(&state, &body)?;
    let created_by = if explicit_global_scope(&body) {
        GLOBAL_ALLOW_PARTY
    } else {
        party
    };

    for scope in scopes {
        store
            .grant_for_reference_scoped(
                &key,
                state.vault.as_ref(),
                ttl_seconds,
                created_by.to_string(),
                body.reason.clone(),
                scope,
            )
            .map_err(|_| WebError::new(WebErrorCode::ConsentGrantFailed))?;
    }
    state.events.notify(EventTopic::WalletInvalidate);
    state.events.notify(EventTopic::ConnectUpdate);

    Ok(Ok::new(wallet_detail_for_key(&state, &key)?))
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevokeRequest {
    pub party: String,
}

pub async fn revoke(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RevokeRequest>,
) -> WebResult<WalletDetail> {
    let key = resolve_wallet_route_key(&state, &id)?;
    let party = body.party.trim();
    if party.is_empty() {
        return Err(WebError::new(WebErrorCode::InvalidRequest));
    }
    let store = state
        .consent_store
        .as_deref()
        .ok_or_else(|| WebError::new(WebErrorCode::ConsentRevokeFailed))?;
    store
        .revoke_for_vault_key_and_created_by(&key, party)
        .map_err(|_| WebError::new(WebErrorCode::ConsentRevokeFailed))?;
    state.events.notify(EventTopic::WalletInvalidate);
    state.events.notify(EventTopic::ConnectUpdate);

    Ok(Ok::new(wallet_detail_for_key(&state, &key)?))
}

pub async fn protect(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> WebResult<WalletDetail> {
    let key = resolve_wallet_route_key(&state, &id)?;
    let store = state
        .consent_store
        .as_deref()
        .ok_or_else(|| WebError::new(WebErrorCode::ConsentRevokeFailed))?;
    store
        .revoke_for_vault_key(&key)
        .map_err(|_| WebError::new(WebErrorCode::ConsentRevokeFailed))?;
    state.events.notify(EventTopic::WalletInvalidate);
    state.events.notify(EventTopic::ConnectUpdate);

    Ok(Ok::new(wallet_detail_for_key(&state, &key)?))
}

#[derive(Debug, Clone, Serialize)]
pub struct RemovedWalletValue {
    pub id: String,
}

pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> WebResult<RemovedWalletValue> {
    let key = resolve_wallet_route_key(&state, &id)?;
    if let Some(store) = state.consent_store.as_deref() {
        store
            .revoke_for_vault_key(&key)
            .map_err(|_| WebError::new(WebErrorCode::ConsentRevokeFailed))?;
    }
    let deleted = state
        .vault
        .delete_wallet(&key)
        .map_err(|_| WebError::new(WebErrorCode::WalletUnreachable))?;
    if !deleted {
        return Err(WebError::new(WebErrorCode::WalletValueMissing));
    }
    state.events.notify(EventTopic::WalletInvalidate);
    state.events.notify(EventTopic::ConnectUpdate);

    Ok(Ok::new(RemovedWalletValue {
        id: wallet_id_from_key(&key),
    }))
}

fn resolve_wallet_route_key(state: &AppState, route_id: &str) -> Result<String, WebError> {
    if Reference::parse_key(route_id).is_some() {
        return Ok(route_id.to_string());
    }

    let matches = state
        .vault
        .list_wallet()
        .map_err(|_| WebError::new(WebErrorCode::WalletUnreachable))?
        .into_iter()
        .filter_map(|entry| {
            let reference = Reference::parse_key(&entry.key)?;
            (reference.id == route_id).then_some(entry.key)
        })
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [key] => Ok(key.clone()),
        [] => Err(WebError::new(WebErrorCode::WalletValueMissing)),
        _ => Err(WebError::new(WebErrorCode::InvalidRequest)),
    }
}

fn wallet_detail_for_key(state: &AppState, key: &str) -> Result<WalletDetail, WebError> {
    let entry = state
        .vault
        .list_wallet()
        .map_err(|_| WebError::new(WebErrorCode::WalletUnreachable))?
        .into_iter()
        .find(|entry| entry.key == key)
        .ok_or_else(|| WebError::new(WebErrorCode::WalletValueMissing))?;
    let consents = consent_entries(state)?;
    Ok(wallet_detail_from_entry(entry, &consents, now_unix_secs()?))
}

fn consent_entries(state: &AppState) -> Result<Vec<dam_consent::ConsentEntry>, WebError> {
    match state.consent_store.as_deref() {
        Some(store) => store
            .list()
            .map_err(|_| WebError::new(WebErrorCode::ConsentGrantFailed)),
        None => Ok(Vec::new()),
    }
}

fn wallet_items_from_entries(
    entries: Vec<dam_vault::VaultEntry>,
    consents: &[dam_consent::ConsentEntry],
    query: &ListQuery,
    now: i64,
) -> Vec<WalletItem> {
    let q = query.q.as_deref().unwrap_or("").to_lowercase();
    let state_filter = query.state.as_deref().and_then(parse_state_filter);
    entries
        .into_iter()
        .map(|entry| wallet_item_from_entry(entry, consents, now))
        .filter(|item| {
            state_filter.is_none_or(|state| item.state == state)
                && (q.is_empty()
                    || item.kind.to_lowercase().contains(&q)
                    || item.value.to_lowercase().contains(&q)
                    || item
                        .shared_with
                        .iter()
                        .any(|party| party.name.to_lowercase().contains(&q)))
        })
        .collect()
}

fn wallet_detail_from_entry(
    entry: dam_vault::VaultEntry,
    consents: &[dam_consent::ConsentEntry],
    now: i64,
) -> WalletDetail {
    let reference = format!("[{}]", entry.key);
    let first_seen = Some(day_label(entry.created_at));
    WalletDetail {
        item: wallet_item_from_entry(entry, consents, now),
        meta: vec![MetaEntry {
            key: "stored in".into(),
            value: "local vault".into(),
            emphasis: Some(true),
        }],
        first_seen,
        reference,
    }
}

fn wallet_item_from_entry(
    entry: dam_vault::VaultEntry,
    consents: &[dam_consent::ConsentEntry],
    now: i64,
) -> WalletItem {
    let id = entry.key.clone();
    let id = wallet_id_from_key(&id);
    let kind = kind_from_key(&entry.key).to_string();
    let related = related_consents(&entry.key, consents);
    let active = related
        .iter()
        .copied()
        .filter(|entry| entry.is_active_at(now))
        .collect::<Vec<_>>();
    let shared_with =
        dedupe_shared_with(active.into_iter().map(shared_with_from_consent).collect());
    let state = wallet_item_state(&related, now);
    WalletItem {
        id,
        kind,
        value: entry.value,
        state,
        shared_with,
        last_seen: None,
    }
}

fn wallet_id_from_key(key: &str) -> String {
    Reference::parse_key(key)
        .map(|reference| reference.id)
        .unwrap_or_else(|| key.to_string())
}

fn related_consents<'a>(
    vault_key: &str,
    consents: &'a [dam_consent::ConsentEntry],
) -> Vec<&'a dam_consent::ConsentEntry> {
    let mut related = consents
        .iter()
        .filter(|entry| entry.vault_key.as_deref() == Some(vault_key))
        .collect::<Vec<_>>();
    related.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    related
}

fn wallet_item_state(consents: &[&dam_consent::ConsentEntry], now: i64) -> ItemState {
    if consents.iter().any(|entry| entry.is_active_at(now)) {
        return ItemState::Allowed;
    }
    if let Some(latest) = consents.first() {
        return match latest.status_at(now) {
            "revoked" => ItemState::Revoked,
            "expired" => ItemState::Expired,
            _ => ItemState::Protected,
        };
    }
    ItemState::Protected
}

fn shared_with_from_consent(entry: &dam_consent::ConsentEntry) -> SharedWith {
    SharedWith {
        name: entry.created_by.clone(),
        since: Some(day_label(entry.created_at)),
    }
}

fn dedupe_shared_with(shared_with: Vec<SharedWith>) -> Vec<SharedWith> {
    let mut seen = BTreeSet::new();
    shared_with
        .into_iter()
        .filter(|party| seen.insert(party.name.clone()))
        .collect()
}

fn allow_scopes(state: &AppState, body: &AllowRequest) -> Result<Vec<String>, WebError> {
    let scope = body
        .scope
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let profile_id = body
        .profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (scope, profile_id) {
        (None, None) => Ok(vec![dam_consent::DEFAULT_SCOPE.to_string()]),
        (Some(scope), None) if scope == dam_consent::DEFAULT_SCOPE => {
            Ok(vec![dam_consent::DEFAULT_SCOPE.to_string()])
        }
        (None, Some(profile_id)) => target_scopes_for_integration_profile(state, profile_id),
        _ => Err(WebError::new(WebErrorCode::InvalidRequest)),
    }
}

fn explicit_global_scope(body: &AllowRequest) -> bool {
    body.scope
        .as_deref()
        .map(str::trim)
        .is_some_and(|scope| scope == dam_consent::DEFAULT_SCOPE)
}

fn target_scopes_for_integration_profile(
    state: &AppState,
    profile_id: &str,
) -> Result<Vec<String>, WebError> {
    let integration_state_dir = integration_state_dir()?;
    dam_integrations::ensure_bundled_profile_files(&integration_state_dir)
        .map_err(|_| WebError::new(WebErrorCode::ConsentGrantFailed))?;
    let profile = dam_integrations::profiles_from_state(
        &format!("http://{}", state.config.proxy.listen),
        &integration_state_dir,
    )
    .map_err(|_| WebError::new(WebErrorCode::ConsentGrantFailed))?
    .into_iter()
    .find(|profile| profile.id == profile_id)
    .ok_or_else(|| WebError::new(WebErrorCode::InvalidRequest))?;

    let scopes =
        target_scopes_for_traffic_app_ids(&state.config.traffic.profile, &profile.traffic_app_ids);
    if scopes.is_empty() {
        Err(WebError::new(WebErrorCode::InvalidRequest))
    } else {
        Ok(scopes)
    }
}

fn target_scopes_for_traffic_app_ids(
    traffic_profile: &dam_net::TrafficProfile,
    app_ids: &[String],
) -> Vec<String> {
    let profile = traffic_profile.with_runtime_enabled_apps(app_ids);
    let mut seen = BTreeSet::new();
    dam_net::traffic_routes_from_profile(&profile)
        .into_iter()
        .map(|route| dam_consent::target_scope(&route.target_name))
        .filter(|scope| seen.insert(scope.clone()))
        .collect()
}

fn integration_state_dir() -> Result<PathBuf, WebError> {
    dam_daemon::state_paths()
        .map(|paths| paths.state_dir.join("integrations"))
        .map_err(|_| WebError::new(WebErrorCode::DaemonUnreachable))
}

fn parse_kind(value: &str) -> Result<SensitiveType, WebError> {
    SensitiveType::from_tag(value.trim()).ok_or_else(|| WebError::new(WebErrorCode::InvalidRequest))
}

fn parse_state_filter(value: &str) -> Option<ItemState> {
    match value {
        "protected" => Some(ItemState::Protected),
        "allowed" => Some(ItemState::Allowed),
        "revoked" => Some(ItemState::Revoked),
        "expired" => Some(ItemState::Expired),
        _ => None,
    }
}

fn kind_from_key(key: &str) -> &str {
    key.split_once(':').map(|(k, _)| k).unwrap_or(key)
}

fn sort_items(items: &mut [WalletItem], sort: Option<&str>, dir: Option<&str>) {
    let descending = matches!(dir, Some("desc"));
    match sort.unwrap_or("recent") {
        "kind" => items.sort_by(|a, b| a.kind.cmp(&b.kind)),
        "value" => items.sort_by(|a, b| a.value.cmp(&b.value)),
        _ => {} // recent — preserve underlying order
    }
    if descending {
        items.reverse();
    }
}

fn now_unix_secs() -> Result<i64, WebError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WebError::new(WebErrorCode::Unknown))?;
    Ok(duration.as_secs() as i64)
}

#[cfg(test)]
#[path = "wallet_tests.rs"]
mod tests;
