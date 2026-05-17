use super::*;
use axum::extract::{Query, State};
use dam_core::{LogEvent, LogEventType, LogLevel, Reference, SensitiveType, VaultWriter};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn activity_resolves_profile_label_and_wallet_value_from_catalog() {
    let vault = Arc::new(dam_vault::Vault::open_in_memory().unwrap());
    let logs = Arc::new(dam_log::LogStore::open_in_memory().unwrap());
    let (profile_label, target_label) = catalog_profile_fixture();
    let reference = vault
        .write(&dam_core::VaultRecord {
            reference: Reference::generate(SensitiveType::Email),
            kind: SensitiveType::Email,
            value: "ada@example.test".to_string(),
        })
        .unwrap();
    let key = reference.key();
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
        .with_reference(reference)
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
    assert_eq!(event.value.as_deref(), Some("ada@example.test"));
    assert_eq!(event.wallet_key.as_deref(), Some(key.as_str()));
    assert!(matches!(event.decision, Decision::Sealed));
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
    crate::AppState {
        surface: crate::Surface::Web,
        tray_post_token: None,
        vault,
        consent_store: None,
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
