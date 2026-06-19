//! CTZN-facing activity feed and per-event evidence.

use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use dam_core::SensitiveType;

use crate::AppState;
use crate::activity_map::{Decision, actor_from_message, derive_event_with_actor};
use crate::error::{Ok, WebError, WebErrorCode, WebResult};

use super::wallet::{WalletDetail, add_wallet_value};

#[derive(Debug, Clone, Serialize)]
pub struct ActivityFeed {
    pub events: Vec<ActivityEvent>,
    pub summary: ActivitySummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityEvent {
    pub id: i64,
    pub ts: i64,
    pub day: String,
    pub profile: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    pub can_add_to_wallet: bool,
    pub decision: Decision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    pub audit_id: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ActivitySummary {
    pub total: u64,
    pub granted: u64,
    pub sealed: u64,
    pub denied: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActivityQuery {
    pub since: Option<i64>,
    pub after_id: Option<i64>,
    pub limit: Option<usize>,
    pub decision: Option<String>,
    pub q: Option<String>,
}

const DEFAULT_ACTIVITY_WINDOW_SECONDS: i64 = 3_600;
const DEFAULT_ACTIVITY_LIMIT: usize = 300;
const MAX_ACTIVITY_LIMIT: usize = 1_000;
const ACTIVITY_LOG_ROW_LIMIT: usize = 10_000;
const ACTIVITY_EVENT_TYPES: [&str; 4] = [
    "policy_decision",
    "redaction",
    "proxy_forward",
    "proxy_failure",
];

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ActivityQuery>,
) -> WebResult<ActivityFeed> {
    let since = query.since.unwrap_or_else(default_since_timestamp);
    let entries = state
        .logs
        .list_query(activity_log_query(&query, since))
        .map_err(|_| WebError::new(WebErrorCode::DaemonUnreachable))?;

    let q = query.q.as_deref().unwrap_or("").to_lowercase();
    let decision_filter = query.decision.as_deref();
    let limit = query
        .limit
        .unwrap_or(DEFAULT_ACTIVITY_LIMIT)
        .clamp(1, MAX_ACTIVITY_LIMIT);

    let actors = operation_actors(&entries);
    let profile_labels = profile_labels_by_target(&state);
    let mut summary = ActivitySummary::default();
    let mut events = Vec::new();
    for entry in &entries {
        let Some(ev) =
            derive_event_with_actor(entry, actors.get(&entry.operation_id).map(String::as_str))
        else {
            continue;
        };
        if entry.timestamp < since {
            continue;
        }
        match ev.decision {
            Decision::Granted => summary.granted += 1,
            Decision::Sealed => summary.sealed += 1,
            Decision::Denied => summary.denied += 1,
        }
        summary.total += 1;
        if let Some(d) = decision_filter
            && !decision_matches(d, ev.decision)
        {
            continue;
        }
        let profile = profile_labels
            .get(&ev.actor)
            .cloned()
            .unwrap_or_else(|| ev.actor.clone());
        if !q.is_empty()
            && !profile.to_lowercase().contains(&q)
            && !ev.kind.to_lowercase().contains(&q)
            && !decision_tag(ev.decision).contains(&q)
            && !entry
                .value
                .as_deref()
                .map(|value| value.to_lowercase().contains(&q))
                .unwrap_or(false)
            && !entry
                .reference
                .as_deref()
                .map(|reference| reference.to_lowercase().contains(&q))
                .unwrap_or(false)
            && !ev
                .purpose
                .as_deref()
                .map(|p| p.to_lowercase().contains(&q))
                .unwrap_or(false)
        {
            continue;
        }
        let display_value = display_value(&entry.reference, &ev.kind);
        let can_add_to_wallet = can_add_to_wallet(&ev.kind, entry.value.as_deref());
        events.push(ActivityEvent {
            id: ev.id,
            ts: ev.ts,
            day: ev.day,
            profile,
            kind: ev.kind,
            value: display_value,
            reference: entry.reference.clone(),
            can_add_to_wallet,
            decision: ev.decision,
            purpose: ev.purpose,
            audit_id: ev.audit_id,
        });
        if events.len() >= limit {
            break;
        }
    }

    Ok(Ok::new(ActivityFeed { events, summary }))
}

fn activity_log_query(query: &ActivityQuery, since: i64) -> dam_log::LogQuery {
    let mut log_query = dam_log::LogQuery::default()
        .with_min_timestamp(since)
        .with_event_types(ACTIVITY_EVENT_TYPES)
        .with_limit(ACTIVITY_LOG_ROW_LIMIT);
    if let Some(after_id) = query.after_id {
        log_query = log_query.with_after_id(after_id);
    }
    log_query
}

fn default_since_timestamp() -> i64 {
    now_unix_secs().saturating_sub(DEFAULT_ACTIVITY_WINDOW_SECONDS)
}

fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityEvidence {
    pub items: Vec<EvidenceItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceItem {
    pub label: String,
    pub value: String,
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> WebResult<ActivityEvidence> {
    let entries = state
        .logs
        .list()
        .map_err(|_| WebError::new(WebErrorCode::DaemonUnreachable))?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| WebError::new(WebErrorCode::WalletValueMissing))?;

    let mut items = vec![
        EvidenceItem {
            label: "event_type".into(),
            value: entry.event_type.clone(),
        },
        EvidenceItem {
            label: "level".into(),
            value: entry.level.clone(),
        },
    ];
    if let Some(kind) = &entry.kind {
        items.push(EvidenceItem {
            label: "kind".into(),
            value: kind.clone(),
        });
    }
    if let Some(reference) = &entry.reference {
        items.push(EvidenceItem {
            label: "reference".into(),
            value: reference.clone(),
        });
    }
    if let Some(action) = &entry.action {
        items.push(EvidenceItem {
            label: "action".into(),
            value: action.clone(),
        });
    }
    items.push(EvidenceItem {
        label: "operation".into(),
        value: entry.operation_id.clone(),
    });
    items.push(EvidenceItem {
        label: "audit_id".into(),
        value: format!("evt_{:016x}", entry.id),
    });

    Ok(Ok::new(ActivityEvidence { items }))
}

pub async fn add_to_wallet(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> WebResult<WalletDetail> {
    let entries = state
        .logs
        .list()
        .map_err(|_| WebError::new(WebErrorCode::DaemonUnreachable))?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| WebError::new(WebErrorCode::WalletValueMissing))?;
    let kind = entry
        .kind
        .as_deref()
        .and_then(SensitiveType::from_tag)
        .ok_or_else(|| WebError::new(WebErrorCode::InvalidRequest))?;
    let value = entry
        .value
        .as_deref()
        .ok_or_else(|| WebError::new(WebErrorCode::WalletValueMissing))?;
    let detail = add_wallet_value(&state, kind, value)?;
    Ok(Ok::new(detail))
}

fn display_value(reference: &Option<String>, kind: &str) -> Option<String> {
    reference
        .as_ref()
        .map(|value| format!("[{value}]"))
        .or_else(|| Some(format!("[{kind}]")))
}

fn can_add_to_wallet(kind: &str, value: Option<&str>) -> bool {
    let Some(value) = value.map(str::trim) else {
        return false;
    };
    if value.is_empty() {
        return false;
    }
    matches!(
        SensitiveType::from_tag(kind),
        Some(
            SensitiveType::Email
                | SensitiveType::Domain
                | SensitiveType::Phone
                | SensitiveType::Ssn
                | SensitiveType::CreditCard
        )
    )
}

fn decision_matches(filter: &str, decision: Decision) -> bool {
    matches!(
        (filter, decision),
        ("granted", Decision::Granted)
            | ("allowed", Decision::Granted)
            | ("sealed", Decision::Sealed)
            | ("denied", Decision::Denied)
            | ("all", _)
    )
}

fn operation_actors(entries: &[dam_log::LogEntry]) -> HashMap<String, String> {
    entries
        .iter()
        .filter_map(|entry| {
            actor_from_message(&entry.message).map(|actor| (entry.operation_id.clone(), actor))
        })
        .collect()
}

fn profile_labels_by_target(state: &AppState) -> HashMap<String, String> {
    let proxy_url = match dam_daemon::daemon_status() {
        Ok(dam_daemon::DaemonStatus::Connected(daemon))
        | Ok(dam_daemon::DaemonStatus::Stale(daemon)) => daemon.proxy_url,
        _ => format!("http://{}", state.config.proxy.listen),
    };
    let profiles = match dam_daemon::state_paths() {
        Ok(paths) => {
            let integration_state_dir = paths.state_dir.join("integrations");
            let _ = dam_integrations::ensure_bundled_profile_files(&integration_state_dir);
            dam_integrations::profiles_from_state(&proxy_url, &integration_state_dir)
                .unwrap_or_else(|_| dam_integrations::profiles(&proxy_url))
        }
        Err(_) => dam_integrations::profiles(&proxy_url),
    };

    let traffic_apps = state
        .config
        .traffic
        .profile
        .apps
        .iter()
        .map(|app| {
            let target = app
                .target_name
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(app.id.as_str());
            (app.id.as_str(), target.to_string())
        })
        .collect::<HashMap<_, _>>();

    let mut labels = HashMap::new();
    for profile in profiles {
        for app_id in &profile.traffic_app_ids {
            let target = traffic_apps
                .get(app_id.as_str())
                .cloned()
                .unwrap_or_else(|| app_id.clone());
            labels.entry(target).or_insert_with(|| profile.name.clone());
        }
    }
    labels
}

fn decision_tag(decision: Decision) -> &'static str {
    match decision {
        Decision::Granted => "granted",
        Decision::Sealed => "sealed",
        Decision::Denied => "denied",
    }
}

#[cfg(test)]
#[path = "activity_tests.rs"]
mod tests;
