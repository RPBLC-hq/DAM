use super::*;
use axum::extract::{Query, State};
use dam_core::{
    LogEvent, LogEventType, LogLevel, Reference, SensitiveType, VaultRecord, VaultWriter,
};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn activity_resolves_profile_label_without_wallet_lookup() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let logs = Arc::new(dam_log::LogStore::open_in_memory().unwrap());
    let (profile_label, target_label) = catalog_profile_fixture();
    let reference = Reference::generate(SensitiveType::Email);
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::ProxyForward,
            format!("route target={target_label} provider={target_label}"),
        )
        .with_action("route_decision"),
    )
    .unwrap();
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::Redaction,
            "replacement applied with tokenized reference",
        )
        .with_kind(SensitiveType::Email)
        .with_value("ada@example.test")
        .with_reference(reference.clone())
        .with_action("tokenized"),
    )
    .unwrap();
    let state = test_state(vault, logs);

    let response = list(State(state), Query(ActivityQuery::default()))
        .await
        .unwrap();

    assert_eq!(response.data.events.len(), 1);
    let event = &response.data.events[0];
    assert_eq!(event.profile, profile_label);
    assert_ne!(event.profile, target_label);
    assert_eq!(event.kind, "email");
    assert_eq!(
        event.value.as_deref(),
        Some(format!("[{}]", reference.key()).as_str())
    );
    assert_eq!(event.reference.as_deref(), Some(reference.key().as_str()));
    assert!(event.can_add_to_wallet);
    assert!(matches!(event.decision, Decision::Sealed));
}

#[tokio::test]
async fn activity_defaults_to_last_hour_and_since_zero_shows_all() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let logs = Arc::new(dam_log::LogStore::open_in_memory().unwrap());
    let now = now_unix_secs();
    let mut recent = LogEvent::new(
        "op-recent",
        LogLevel::Info,
        LogEventType::Redaction,
        "recent redaction",
    )
    .with_kind(SensitiveType::Email)
    .with_action("redacted");
    recent.timestamp = now - 60;
    let mut old = LogEvent::new(
        "op-old",
        LogLevel::Info,
        LogEventType::Redaction,
        "old redaction",
    )
    .with_kind(SensitiveType::Email)
    .with_action("redacted");
    old.timestamp = now - 7_200;
    logs.record(&recent).unwrap();
    logs.record(&old).unwrap();

    let default_response = list(
        State(test_state(vault.clone(), logs.clone())),
        Query(ActivityQuery::default()),
    )
    .await
    .unwrap();
    let all_response = list(
        State(test_state(vault, logs)),
        Query(ActivityQuery {
            since: Some(0),
            ..ActivityQuery::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(default_response.data.events.len(), 1);
    assert_eq!(
        default_response.data.events[0].audit_id,
        "evt_0000000000000001"
    );
    assert_eq!(all_response.data.events.len(), 2);
}

#[tokio::test]
async fn activity_uses_redaction_event_for_sealed_activity_without_policy_duplicate() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let logs = Arc::new(dam_log::LogStore::open_in_memory().unwrap());
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::PolicyDecision,
            "policy decision applied",
        )
        .with_kind(SensitiveType::Email)
        .with_action("redact"),
    )
    .unwrap();
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::Redaction,
            "replacement applied with policy redaction",
        )
        .with_kind(SensitiveType::Email)
        .with_value("ada@example.test")
        .with_action("redacted"),
    )
    .unwrap();

    let response = list(
        State(test_state(vault, logs)),
        Query(ActivityQuery {
            since: Some(0),
            ..ActivityQuery::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(response.data.events.len(), 1);
    assert_eq!(response.data.events[0].id, 2);
    assert_eq!(response.data.events[0].kind, "email");
    assert_eq!(response.data.events[0].value.as_deref(), Some("[email]"));
    assert!(response.data.events[0].can_add_to_wallet);
    assert_eq!(response.data.summary.total, 1);
}

#[tokio::test]
async fn activity_shows_consent_outcome_without_wallet_lookup() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let logs = Arc::new(dam_log::LogStore::open_in_memory().unwrap());
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::PolicyDecision,
            "policy decision applied",
        )
        .with_kind(SensitiveType::Email)
        .with_value("ada@example.test")
        .with_action("allow"),
    )
    .unwrap();
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::Redaction,
            "replacement applied with policy redaction",
        )
        .with_kind(SensitiveType::Email)
        .with_value("ada@example.test")
        .with_action("redacted"),
    )
    .unwrap();
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::Consent,
            "active consent allowed detected value",
        )
        .with_kind(SensitiveType::Email)
        .with_action("allow:consent_example"),
    )
    .unwrap();

    let response = list(
        State(test_state(vault, logs)),
        Query(ActivityQuery {
            since: Some(0),
            ..ActivityQuery::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(response.data.events.len(), 2);
    let sealed = &response.data.events[0];
    assert_eq!(sealed.id, 2);
    assert_eq!(sealed.kind, "email");
    assert_eq!(sealed.value.as_deref(), Some("[email]"));
    assert!(sealed.can_add_to_wallet);
    assert!(matches!(sealed.decision, Decision::Sealed));

    let granted = &response.data.events[1];
    assert_eq!(granted.id, 1);
    assert_eq!(granted.kind, "email");
    assert_eq!(granted.value.as_deref(), Some("[email]"));
    assert!(granted.can_add_to_wallet);
    assert!(matches!(granted.decision, Decision::Granted));
    assert_eq!(response.data.summary.total, 2);
}

#[tokio::test]
async fn activity_detail_omits_raw_value_and_add_to_wallet_resolves_reference() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let logs = Arc::new(dam_log::LogStore::open_in_memory().unwrap());
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "ada@example.test".to_string(),
        })
        .unwrap();
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::Redaction,
            "replacement applied with tokenized reference",
        )
        .with_kind(SensitiveType::Email)
        .with_reference(reference.clone())
        .with_action("tokenized"),
    )
    .unwrap();
    let state = test_state(vault.clone(), logs);

    let list_response = list(
        State(state.clone()),
        Query(ActivityQuery {
            since: Some(0),
            ..ActivityQuery::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(list_response.data.events.len(), 1);
    assert!(list_response.data.events[0].can_add_to_wallet);

    let detail_response = detail(State(state.clone()), Path(1)).await.unwrap();
    let labels = detail_response
        .data
        .items
        .iter()
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert!(!labels.contains(&"value"));
    assert!(labels.contains(&"reference"));

    let add_response = add_to_wallet(State(state), Path(1)).await.unwrap();
    let key = add_response
        .data
        .reference
        .trim_start_matches('[')
        .trim_end_matches(']');
    assert_eq!(
        vault.get_wallet(key).unwrap().as_deref(),
        Some("ada@example.test")
    );
}

#[tokio::test]
async fn activity_rejects_add_to_wallet_for_non_allowlisted_kind() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let logs = Arc::new(dam_log::LogStore::open_in_memory().unwrap());
    let reference = Reference::generate(SensitiveType::ApiKey);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::ApiKey,
            value: "sk-synthetic-value".to_string(),
        })
        .unwrap();
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::Redaction,
            "replacement applied with tokenized reference",
        )
        .with_kind(SensitiveType::ApiKey)
        .with_reference(reference)
        .with_action("tokenized"),
    )
    .unwrap();
    let state = test_state(vault, logs);

    let list_response = list(
        State(state.clone()),
        Query(ActivityQuery {
            since: Some(0),
            ..ActivityQuery::default()
        }),
    )
    .await
    .unwrap();
    assert_eq!(list_response.data.events.len(), 1);
    assert!(!list_response.data.events[0].can_add_to_wallet);

    let error = add_to_wallet(State(state), Path(1)).await.unwrap_err();
    assert_eq!(error.code, WebErrorCode::InvalidRequest);
}

#[tokio::test]
async fn activity_uses_bounded_relevant_log_query_without_request_summaries() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let logs = Arc::new(dam_log::LogStore::open_in_memory().unwrap());
    logs.record(
        &LogEvent::new(
            "op-1",
            LogLevel::Info,
            LogEventType::ProxyForward,
            "inbound reference left unresolved",
        )
        .with_action("resolve_disabled"),
    )
    .unwrap();
    logs.record(
        &LogEvent::new(
            "op-request",
            LogLevel::Info,
            LogEventType::ProxyForward,
            "request protection detections=1 replacements=1 tokenized=1 blocked=0",
        )
        .with_action("request_protection"),
    )
    .unwrap();
    logs.record(
        &LogEvent::new(
            "op-provider",
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "upstream provider is unavailable",
        )
        .with_action("provider_down"),
    )
    .unwrap();

    let response = list(
        State(test_state(vault, logs)),
        Query(ActivityQuery {
            since: Some(0),
            limit: Some(1),
            ..ActivityQuery::default()
        }),
    )
    .await
    .unwrap();

    assert_eq!(response.data.events.len(), 1);
    assert_eq!(response.data.events[0].kind, "provider");
}

fn catalog_profile_fixture() -> (String, String) {
    let config = dam_config::DamConfig::default();
    let profiles = dam_integrations::profiles(&format!("http://{}", config.proxy.listen));
    for profile in profiles {
        for app_id in &profile.traffic_app_ids {
            if let Some(app) = config
                .traffic
                .profile
                .apps
                .iter()
                .find(|app| &app.id == app_id)
            {
                let target = app
                    .target_name
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| app.id.clone());
                return (profile.name, target);
            }
        }
    }
    panic!("bundled integration profiles must map to a traffic app");
}

fn test_state(vault: Arc<dam_vault::Vault>, logs: Arc<dam_log::LogStore>) -> crate::AppState {
    test_state_with_consent(vault, logs, None)
}

fn test_state_with_consent(
    vault: Arc<dam_vault::Vault>,
    logs: Arc<dam_log::LogStore>,
    consent_store: Option<Arc<dam_consent::ConsentStore>>,
) -> crate::AppState {
    crate::AppState {
        surface: crate::Surface::Web,
        tray_post_token: None,
        vault,
        consent_store,
        logs,
        config: Arc::new(dam_config::DamConfig::default()),
        config_path: None,
        db_path: Arc::new(PathBuf::from("vault.db")),
        log_path: Arc::new(PathBuf::from("log.db")),
        client: reqwest::Client::new(),
        requests: Arc::new(crate::request_store::RequestStore::default()),
        events: Arc::new(crate::events_bus::EventBus::new()),
    }
}
