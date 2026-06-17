use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LogEventView {
    pub(crate) id: i64,
    pub(crate) timestamp: i64,
    pub(crate) operation_id: String,
    pub(crate) level: String,
    pub(crate) event_type: String,
    pub(crate) kind: Option<String>,
    pub(crate) reference: Option<String>,
    pub(crate) action: Option<String>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LogOperationSummary {
    pub(crate) operation_id: String,
    pub(crate) first_id: i64,
    pub(crate) last_id: i64,
    pub(crate) timestamp: i64,
    pub(crate) events: usize,
    pub(crate) event_types: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) summary: String,
}

pub(crate) fn filtered_log_entries(
    entries: Vec<dam_log::LogEntry>,
    args: &crate::LogsArgs,
) -> Vec<dam_log::LogEntry> {
    entries
        .into_iter()
        .filter(|entry| args.after_id.is_none_or(|after_id| entry.id > after_id))
        .filter(|entry| {
            args.operation_id
                .as_deref()
                .is_none_or(|operation_id| entry.operation_id == operation_id)
        })
        .collect()
}

pub(crate) fn log_event_view(entry: dam_log::LogEntry) -> LogEventView {
    LogEventView {
        id: entry.id,
        timestamp: entry.timestamp,
        operation_id: entry.operation_id,
        level: entry.level,
        event_type: entry.event_type,
        kind: entry.kind,
        reference: entry.reference,
        action: entry.action,
        message: entry.message,
    }
}

pub(crate) fn limited_log_event_views(
    entries: Vec<dam_log::LogEntry>,
    limit: usize,
) -> Vec<LogEventView> {
    entries
        .into_iter()
        .take(limit)
        .map(log_event_view)
        .collect()
}

pub(crate) fn log_operation_summaries(
    entries: Vec<dam_log::LogEntry>,
    limit: usize,
) -> Vec<LogOperationSummary> {
    let mut summaries = Vec::<LogOperationSummary>::new();
    for entry in entries {
        if let Some(summary) = summaries
            .iter_mut()
            .find(|summary| summary.operation_id == entry.operation_id)
        {
            summary.first_id = summary.first_id.min(entry.id);
            summary.last_id = summary.last_id.max(entry.id);
            summary.timestamp = summary.timestamp.max(entry.timestamp);
            summary.events += 1;
            push_unique(&mut summary.event_types, &entry.event_type);
            if let Some(action) = entry.action.as_deref() {
                push_unique(&mut summary.actions, action);
            }
            summary.summary = summarize_operation_message(&summary.summary, &entry);
        } else {
            let mut event_types = Vec::new();
            push_unique(&mut event_types, &entry.event_type);
            let mut actions = Vec::new();
            if let Some(action) = entry.action.as_deref() {
                push_unique(&mut actions, action);
            }
            summaries.push(LogOperationSummary {
                operation_id: entry.operation_id.clone(),
                first_id: entry.id,
                last_id: entry.id,
                timestamp: entry.timestamp,
                events: 1,
                event_types,
                actions,
                summary: summarize_operation_message("", &entry),
            });
        }

        if summaries.len() >= limit
            && summaries
                .last()
                .is_some_and(|summary| summary.operation_id != entry.operation_id)
        {
            summaries.truncate(limit);
            break;
        }
    }
    summaries.truncate(limit);
    summaries
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn summarize_operation_message(existing: &str, entry: &dam_log::LogEntry) -> String {
    let Some(piece) = log_summary_piece(entry) else {
        return existing.to_string();
    };
    append_summary_piece(existing, &piece)
}

fn append_summary_piece(existing: &str, piece: &str) -> String {
    if existing.is_empty() {
        return piece.to_string();
    }
    if existing.split(" | ").any(|part| part == piece) {
        return existing.to_string();
    }
    format!("{existing} | {piece}")
}

fn log_summary_piece(entry: &dam_log::LogEntry) -> Option<String> {
    match entry.action.as_deref() {
        Some("route_decision") => Some(shorten_log_message(&entry.message, 90)),
        Some("request_protection") => Some(shorten_log_message(&entry.message, 90)),
        Some("provider_response") => Some(shorten_log_message(&entry.message, 100)),
        Some("resolve_attempt" | "resolve_non_utf8" | "resolve_disabled") => Some(format!(
            "{} {}",
            entry.action.as_deref().unwrap(),
            entry.message
        )),
        Some("intercepted_response_write") => Some(shorten_log_message(&entry.message, 90)),
        Some("bypassing") => Some("bypassing".to_string()),
        Some("blocked") => Some(format!("blocked {}", entry.message)),
        Some("provider_down") => Some("provider_down".to_string()),
        Some("protected") => Some("protected".to_string()),
        _ => None,
    }
    .map(|value| shorten_log_message(&value, 140))
}

pub(crate) fn render_log_summaries(summaries: &[LogOperationSummary]) -> String {
    if summaries.is_empty() {
        return "No DAM log operations matched.\n".to_string();
    }

    let mut output = String::from("LastID  Time      Operation               Events  Summary\n");
    for summary in summaries {
        output.push_str(&format!(
            "{:<6} {:<9} {:<23} {:<7} {}\n",
            summary.last_id,
            compact_time(summary.timestamp),
            summary.operation_id,
            summary.events,
            summary.summary
        ));
    }
    output
}

pub(crate) fn render_log_events(entries: &[dam_log::LogEntry], limit: usize) -> String {
    let mut selected = entries.iter().take(limit).cloned().collect::<Vec<_>>();
    selected.sort_by_key(|entry| entry.id);
    if selected.is_empty() {
        return "No DAM log events matched.\n".to_string();
    }

    let mut output = String::from(
        "ID      Time      Operation               Type            Action                  Message\n",
    );
    for entry in selected {
        output.push_str(&format!(
            "{:<7} {:<9} {:<23} {:<15} {:<23} {}\n",
            entry.id,
            compact_time(entry.timestamp),
            entry.operation_id,
            entry.event_type,
            entry.action.unwrap_or_default(),
            shorten_log_message(&entry.message, 120)
        ));
    }
    output
}

fn compact_time(timestamp: i64) -> String {
    let seconds = timestamp.rem_euclid(86_400);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn shorten_log_message(message: &str, max: usize) -> String {
    if message.chars().count() <= max {
        return message.to_string();
    }
    let mut output = message
        .chars()
        .take(max.saturating_sub(3))
        .collect::<String>();
    output.push_str("...");
    output
}
