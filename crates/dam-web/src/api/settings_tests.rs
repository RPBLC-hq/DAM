use super::*;

#[test]
fn settings_errors_map_to_stable_codes() {
    assert_eq!(
        settings_error("target changed outside DAM".into()).code,
        WebErrorCode::ApplyModifiedTarget
    );
    assert_eq!(
        settings_error("failed to write target".into()).code,
        WebErrorCode::ApplyTargetUnwritable
    );
    assert_eq!(
        settings_error("some unexpected integration error".into()).code,
        WebErrorCode::Unknown
    );
}

#[test]
fn capture_scope_expands_enabled_profiles_to_hosts_apps_and_targets() {
    let dir = tempfile::tempdir().unwrap();
    let integration_dir = dir.path().join("integrations");
    dam_integrations::ensure_bundled_profile_files(&integration_dir).unwrap();
    dam_integrations::set_integration_enabled("codex", true, &integration_dir).unwrap();

    let scope = capture_scope_for_state(&dam_config::DamConfig::default(), dir.path()).unwrap();

    assert_eq!(
        scope.traffic_app_ids,
        Some(vec![
            "anthropic-api".to_string(),
            "openai-api".to_string(),
            "chatgpt-codex".to_string(),
        ])
    );
    assert!(scope.hosts.contains(&"api.anthropic.com".to_string()));
    assert!(scope.hosts.contains(&"api.openai.com".to_string()));
    assert!(scope.hosts.contains(&"chatgpt.com".to_string()));
    assert!(scope.hosts.contains(&"ab.chatgpt.com".to_string()));
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "anthropic")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "openai")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "chatgpt-codex")
    );
}

#[test]
fn capture_scope_preserves_explicit_empty_enabled_profile_state() {
    let dir = tempfile::tempdir().unwrap();
    let integration_dir = dir.path().join("integrations");
    dam_integrations::ensure_bundled_profile_files(&integration_dir).unwrap();
    dam_integrations::set_integration_enabled("claude-code", false, &integration_dir).unwrap();

    let scope = capture_scope_for_state(&dam_config::DamConfig::default(), dir.path()).unwrap();

    assert_eq!(scope.traffic_app_ids, Some(Vec::new()));
    assert!(scope.hosts.is_empty());
    assert!(scope.proxy_targets.is_empty());
}

#[test]
fn pending_network_extension_approval_keeps_profile_toggle_state() {
    assert_eq!(
        network_extension_result_to_reconcile_outcome(
            dam_net_macos::MacosNetworkExtensionResultState::NeedsApproval,
        ),
        ReconcileOutcome::SetupPending
    );
    assert_eq!(
        network_extension_result_to_reconcile_outcome(
            dam_net_macos::MacosNetworkExtensionResultState::Installed,
        ),
        ReconcileOutcome::Reconciled
    );
    assert_eq!(
        network_extension_result_to_reconcile_outcome(
            dam_net_macos::MacosNetworkExtensionResultState::AlreadyInstalled,
        ),
        ReconcileOutcome::Reconciled
    );
}

#[test]
fn pending_setup_reconnect_output_keeps_profile_toggle_state() {
    assert!(command_output_indicates_pending_setup(
        "DAM cannot start this transparent setup yet: approve DAM Network Protection first"
    ));
    assert!(command_output_indicates_pending_setup(
        "needs_user_approval approve the DAM Network Protection configuration in System Settings"
    ));
    assert!(!command_output_indicates_pending_setup(
        "unknown daemon option: --anthropic"
    ));
}

#[test]
fn reconnect_runtime_path_uses_web_runtime_path_for_relative_daemon_state() {
    let state_dir = std::path::Path::new("/Users/example/.dam");

    assert_eq!(
        reconnect_runtime_path(
            std::path::Path::new("vault.db"),
            std::path::Path::new("/Users/example/.dam/vault.db"),
            state_dir,
        ),
        std::path::PathBuf::from("/Users/example/.dam/vault.db")
    );
    assert_eq!(
        reconnect_runtime_path(
            std::path::Path::new("vault.db"),
            std::path::Path::new("vault.db"),
            state_dir,
        ),
        std::path::PathBuf::from("/Users/example/.dam/vault.db")
    );
    assert_eq!(
        reconnect_runtime_path(
            std::path::Path::new("/tmp/custom-vault.db"),
            std::path::Path::new("/Users/example/.dam/vault.db"),
            state_dir,
        ),
        std::path::PathBuf::from("/tmp/custom-vault.db")
    );
}

#[test]
fn daemon_scope_match_detects_stale_empty_routes_for_enabled_profile() {
    let app_ids = vec!["anthropic-api".to_string()];
    let profile = dam_net::llm_mvp_profile().with_runtime_enabled_apps(&app_ids);
    let routes = dam_net::traffic_routes_from_profile(&profile);
    let scope = CaptureScope {
        hosts: routes.iter().map(|route| route.host.clone()).collect(),
        traffic_app_ids: Some(app_ids),
        proxy_targets: proxy_targets_from_traffic_routes(&routes),
        routes: routes.clone(),
    };
    let stale = daemon_state(
        Vec::new(),
        vec![("openai", "openai-compatible", "https://api.openai.com")],
    );
    let fresh = daemon_state(
        routes,
        vec![("anthropic", "anthropic", "https://api.anthropic.com")],
    );

    assert!(!daemon_matches_scope(&stale, &scope));
    assert!(daemon_matches_scope(&fresh, &scope));
}

fn daemon_state(
    transparent_routes: Vec<dam_net::TrafficRoute>,
    proxy_targets: Vec<(&str, &str, &str)>,
) -> dam_daemon::DaemonState {
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
        proxy_targets: proxy_targets
            .into_iter()
            .map(
                |(name, provider, upstream)| dam_daemon::DaemonProxyTargetState {
                    name: name.to_string(),
                    provider: provider.to_string(),
                    upstream: upstream.to_string(),
                },
            )
            .collect(),
        started_at_unix: 1,
        network_mode: dam_net::CaptureMode::Tun,
        transparent_routes,
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
