//! Map raw `dam-log` events into CTZN-facing sentence-shaped Activity events.
//!
//! Lives in `dam-web` rather than `dam-log` so `dam-log`'s privacy contract
//! stays small. Surfaces derive their views.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Granted,
    Sealed,
    Denied,
}

#[derive(Debug, Clone)]
pub struct DerivedEvent {
    pub id: i64,
    pub ts: i64,
    pub day: String,
    pub actor: String,
    pub kind: String,
    pub decision: Decision,
    pub purpose: Option<String>,
    pub audit_id: String,
}

pub fn derive_event_with_actor(
    entry: &dam_log::LogEntry,
    actor_hint: Option<&str>,
) -> Option<DerivedEvent> {
    let decision = decision_for(entry)?;
    let kind = entry.kind.clone().unwrap_or_else(|| kind_for(entry));
    let actor = actor_hint
        .map(ToOwned::to_owned)
        .or_else(|| actor_from_entry(entry))
        .unwrap_or_else(|| "DAM".to_string());
    let day = day_label(entry.timestamp);
    Some(DerivedEvent {
        id: entry.id,
        ts: entry.timestamp,
        day,
        actor,
        kind,
        decision,
        purpose: None,
        audit_id: format!("evt_{:016x}", entry.id),
    })
}

fn decision_for(entry: &dam_log::LogEntry) -> Option<Decision> {
    let action = entry.action.as_deref()?;
    match (entry.event_type.as_str(), action) {
        ("policy_decision", "allow") => Some(Decision::Granted),
        ("policy_decision", "block") => Some(Decision::Denied),
        ("redaction", _) => Some(Decision::Sealed),
        ("proxy_failure", "provider_down") => Some(Decision::Denied),
        _ => None,
    }
}

fn actor_from_entry(entry: &dam_log::LogEntry) -> Option<String> {
    // Best-effort extraction from the operation_id ("profile-1234") or
    // the message. Real wiring belongs in a follow-up that adds an
    // actor field to log entries.
    if let Some((actor, _)) = entry.operation_id.split_once('-')
        && !actor.is_empty()
    {
        return Some(actor.to_string());
    }
    None
}

pub fn actor_from_message(message: &str) -> Option<String> {
    field_value(message, "target")
        .or_else(|| field_value(message, "provider"))
        .filter(|value| !value.is_empty())
}

fn kind_for(entry: &dam_log::LogEntry) -> String {
    match (entry.event_type.as_str(), entry.action.as_deref()) {
        ("proxy_failure", Some("provider_down")) => "provider".to_string(),
        _ => "unknown".to_string(),
    }
}

fn field_value(message: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    message.split_whitespace().find_map(|part| {
        part.strip_prefix(&prefix)
            .map(|value| value.trim_matches(|c| c == ',' || c == ';').to_string())
    })
}

pub fn day_label(ts: i64) -> String {
    // v1: a coarse YYYY-MM-DD label derived from epoch seconds.
    // The UI groups events by this label without further parsing.
    let secs = ts.max(0) as u64;
    let days = secs / 86_400;
    let (y, m, d) = epoch_days_to_date(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn epoch_days_to_date(days: u64) -> (i32, u32, u32) {
    // Adapted from the standard Howard Hinnant algorithm.
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m, d)
}

#[cfg(test)]
#[path = "activity_map_tests.rs"]
mod tests;
