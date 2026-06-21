use super::*;

#[test]
fn active_grants_count_uses_current_unrevoked_consents() {
    let store = dam_consent::ConsentStore::open_in_memory().unwrap();
    let active = store
        .grant(&dam_consent::GrantConsent {
            kind: dam_core::SensitiveType::Email,
            value: "ada@example.test".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();
    let revoked = store
        .grant(&dam_consent::GrantConsent {
            kind: dam_core::SensitiveType::Phone,
            value: "+1 415 555 0142".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();

    assert!(store.revoke(&revoked.id).unwrap());

    assert_eq!(active_grants_count(Some(&store)), 1);
    assert!(
        store
            .active_for_value(active.kind, "ada@example.test")
            .unwrap()
            .is_some()
    );
}

#[test]
fn active_grants_count_collapses_multiple_parties_for_same_wallet_value() {
    let store = dam_consent::ConsentStore::open_in_memory().unwrap();
    for actor in ["Claude Code", "ChatGPT"] {
        store
            .grant(&dam_consent::GrantConsent {
                kind: dam_core::SensitiveType::Email,
                value: "ada@example.test".to_string(),
                vault_key: Some("email:1111111111111111111111".to_string()),
                ttl_seconds: 60,
                created_by: actor.to_string(),
                reason: None,
            })
            .unwrap();
    }

    assert_eq!(active_grants_count(Some(&store)), 1);
}

#[test]
fn redacted_today_count_uses_current_utc_day_redaction_rows() {
    let today = 2 * 86_400 + 60;
    let yesterday = today - 86_400;
    let entries = vec![
        log_entry(
            1,
            today,
            "redaction",
            Some("tokenized"),
            "email",
            "replacement applied",
        ),
        log_entry(
            2,
            today,
            "policy_decision",
            Some("tokenize"),
            "email",
            "policy decision is not the replacement row",
        ),
        log_entry(3, yesterday, "redaction", Some("tokenized"), "email", "old"),
        log_entry(
            4,
            today,
            "proxy_failure",
            Some("provider_down"),
            "provider",
            "provider down is not a redaction",
        ),
    ];

    assert_eq!(redacted_today_count(&entries, today), 1);
}

#[test]
fn blocked_today_count_uses_denied_activity_taxonomy() {
    let today = 2 * 86_400 + 60;
    let yesterday = today - 86_400;
    let entries = vec![
        log_entry(
            1,
            today,
            "policy_decision",
            Some("block"),
            "email",
            "policy denied the request",
        ),
        log_entry(
            2,
            today,
            "proxy_failure",
            Some("provider_down"),
            "provider",
            "provider down counts as denied activity",
        ),
        log_entry(
            3,
            today,
            "redaction",
            Some("redacted"),
            "email",
            "redaction is sealed, not denied",
        ),
        log_entry(
            4,
            yesterday,
            "policy_decision",
            Some("block"),
            "email",
            "yesterday should not count",
        ),
    ];

    assert_eq!(blocked_today_count(&entries, today), 2);
}

#[test]
fn apps_mediated_count_reads_enabled_integrations() {
    let dir = tempfile::tempdir().unwrap();
    let integration_state_dir = dir.path().join("integrations");

    dam_integrations::set_integration_enabled("claude", true, &integration_state_dir).unwrap();
    dam_integrations::set_integration_enabled("chatgpt", true, &integration_state_dir).unwrap();

    assert_eq!(apps_mediated_count_from(&integration_state_dir).unwrap(), 2);
}

#[test]
fn setup_plan_mapping_uses_diagnostics_next_action_for_current_step() {
    let blocked = dam_diagnostics::SetupStep {
        kind: dam_diagnostics::SetupStepKind::NetworkExtension,
        status: dam_diagnostics::SetupStepStatus::Blocked,
        detail: dam_diagnostics::SetupStepDetail::Failed,
        message: "macOS Network Extension status cannot be inspected".to_string(),
        command: None,
        requires_confirmation: false,
        changes_system: false,
    };
    let plan = dam_diagnostics::SetupPlan {
        state: dam_diagnostics::SetupPlanState::Blocked,
        message: "setup is blocked".to_string(),
        state_dir: std::path::PathBuf::from("/tmp/dam-state"),
        integration_state_dir: std::path::PathBuf::from("/tmp/dam-state/integrations"),
        proxy_url: "http://127.0.0.1:7828".to_string(),
        network_mode: dam_net::CaptureMode::Tun,
        trust_mode: dam_trust::TrustMode::LocalCa,
        active_profile: None,
        next_action: Some(blocked.clone()),
        steps: vec![
            dam_diagnostics::SetupStep {
                kind: dam_diagnostics::SetupStepKind::LaunchAtLogin,
                status: dam_diagnostics::SetupStepStatus::Needed,
                detail: dam_diagnostics::SetupStepDetail::Unconfigured,
                message: "Choose whether DAM should open at login".to_string(),
                command: None,
                requires_confirmation: false,
                changes_system: true,
            },
            blocked,
        ],
    };

    let mapped = map_setup_plan(&plan);

    assert_eq!(mapped.current_step_id.as_deref(), Some("ne_install"));
    assert_eq!(mapped.steps[0].id, "launch_at_login");
    assert_eq!(mapped.steps[0].state, SetupStepState::Todo);
    assert_eq!(mapped.steps[0].detail, "unconfigured");
    assert_eq!(mapped.steps[1].id, "ne_install");
    assert_eq!(mapped.steps[1].state, SetupStepState::Blocked);
    assert_eq!(mapped.steps[1].detail, "failed");
}

#[test]
fn connected_daemon_with_scope_mismatch_is_degraded() {
    let daemon = dam_daemon::DaemonStatus::Connected(test_daemon_state());

    assert_eq!(
        derive_connect_state(&daemon, None, false),
        ConnectState::Degraded
    );
    assert_eq!(
        derive_connect_state(&daemon, None, true),
        ConnectState::Protected
    );
}

fn log_entry(
    id: i64,
    timestamp: i64,
    event_type: &str,
    action: Option<&str>,
    kind: &str,
    message: &str,
) -> dam_log::LogEntry {
    dam_log::LogEntry {
        id,
        timestamp,
        operation_id: format!("op-{id}"),
        level: "info".to_string(),
        event_type: event_type.to_string(),
        kind: Some(kind.to_string()),
        value: None,
        reference: None,
        action: action.map(ToOwned::to_owned),
        message: message.to_string(),
    }
}

fn test_daemon_state() -> dam_daemon::DaemonState {
    dam_daemon::DaemonState {
        version: 6,
        pid: 1,
        executable_path: None,
        executable_sha256: None,
        listen: "127.0.0.1:7828".to_string(),
        proxy_url: "http://127.0.0.1:7828".to_string(),
        config_path: None,
        vault_path: std::path::PathBuf::from("vault.db"),
        log_path: Some(std::path::PathBuf::from("log.db")),
        consent_path: Some(std::path::PathBuf::from("consent.db")),
        resolve_inbound: true,
        target_name: None,
        target_provider: None,
        upstream: None,
        proxy_targets: Vec::new(),
        started_at_unix: 1,
        network_mode: dam_net::CaptureMode::Tun,
        transparent_routes: Vec::new(),
        transparent_routing_readiness: Vec::new(),
        trust: dam_trust::TrustState {
            mode: dam_trust::TrustMode::LocalCa,
            ..dam_trust::TrustState::default()
        },
        transparent_trust_readiness: Vec::new(),
        transparent_interception_readiness: Vec::new(),
        protection_enabled: true,
        protection_started_at_unix: Some(1),
    }
}
