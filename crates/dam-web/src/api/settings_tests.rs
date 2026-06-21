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

    let scope = capture_scope_for_state(&dam_config::DamConfig::default(), dir.path()).unwrap();

    assert_eq!(
        scope.traffic_app_ids,
        Some(vec![
            "anthropic-api".to_string(),
            "claude-web".to_string(),
            "anthropic-console".to_string(),
            "claude-mcp-proxy".to_string(),
            "claude-platform".to_string(),
            "openai-api".to_string(),
            "openai-platform".to_string(),
            "chatgpt-web".to_string(),
            "chatgpt-legacy-web".to_string(),
        ])
    );
    assert!(scope.hosts.contains(&"api.anthropic.com".to_string()));
    assert!(scope.hosts.contains(&"claude.ai".to_string()));
    assert!(scope.hosts.contains(&"console.anthropic.com".to_string()));
    assert!(scope.hosts.contains(&"mcp-proxy.anthropic.com".to_string()));
    assert!(scope.hosts.contains(&"platform.claude.com".to_string()));
    assert!(scope.hosts.contains(&"api.openai.com".to_string()));
    assert!(scope.hosts.contains(&"platform.openai.com".to_string()));
    assert!(scope.hosts.contains(&"chatgpt.com".to_string()));
    assert!(scope.hosts.contains(&"ab.chatgpt.com".to_string()));
    assert!(scope.hosts.contains(&"chat.openai.com".to_string()));
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
            .any(|target| target.name == "claude-web")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "anthropic-console")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "claude-mcp-proxy")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "claude-platform")
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
            .any(|target| target.name == "openai-platform")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "chatgpt-web")
    );
    assert!(
        scope
            .proxy_targets
            .iter()
            .any(|target| target.name == "chatgpt-legacy-web")
    );
}

#[test]
fn capture_scope_preserves_explicit_empty_enabled_profile_state() {
    let dir = tempfile::tempdir().unwrap();
    let integration_dir = dir.path().join("integrations");
    dam_integrations::ensure_bundled_profile_files(&integration_dir).unwrap();
    dam_integrations::set_integration_enabled("claude", false, &integration_dir).unwrap();
    dam_integrations::set_integration_enabled("chatgpt", false, &integration_dir).unwrap();

    let scope = capture_scope_for_state(&dam_config::DamConfig::default(), dir.path()).unwrap();

    assert_eq!(scope.traffic_app_ids, Some(Vec::new()));
    assert!(scope.hosts.is_empty());
    assert!(scope.proxy_targets.is_empty());
}

#[test]
fn settings_apps_hide_custom_profile_files_for_mvp() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let integration_dir = state_dir.join("integrations");
    let profiles_dir = dam_integrations::profile_definitions_dir(&integration_dir);
    std::fs::create_dir_all(&profiles_dir).unwrap();
    std::fs::write(
        profiles_dir.join("example-mail.json"),
        r#"{
          "id": "example-mail",
          "name": "Example Mail",
          "summary": "Route Example Mail traffic through DAM.",
          "provider": "generic-http",
          "traffic_app_ids": ["example-mail"],
          "connect_args": ["--network-mode", "tun", "--trust-mode", "local_ca"],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();
    let state = test_state(dir.path());

    let apps = app_settings_for_state_dir(&state, &state_dir).unwrap();
    let ids: Vec<_> = apps.iter().map(|app| app.id.as_str()).collect();

    assert_eq!(ids, vec!["claude", "chatgpt"]);
    assert!(!ids.contains(&"example-mail"));
}

#[test]
fn settings_app_toggles_accept_only_mvp_visible_profiles() {
    assert!(is_mvp_settings_profile("claude"));
    assert!(is_mvp_settings_profile("chatgpt"));
    assert!(!is_mvp_settings_profile("example-mail"));
    assert!(!is_mvp_settings_profile("codex"));
}

#[test]
fn settings_app_toggle_rejects_hidden_custom_profile_ids() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let integration_dir = state_dir.join("integrations");
    let profiles_dir = dam_integrations::profile_definitions_dir(&integration_dir);
    std::fs::create_dir_all(&profiles_dir).unwrap();
    std::fs::write(
        profiles_dir.join("example-mail.json"),
        r#"{
          "id": "example-mail",
          "name": "Example Mail",
          "summary": "Route Example Mail traffic through DAM.",
          "provider": "generic-http",
          "traffic_app_ids": ["example-mail"],
          "connect_args": [],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();
    let state = test_state(dir.path());

    let error = set_app_enabled_in_state_dir(&state, "example-mail", true, &state_dir).unwrap_err();

    assert_eq!(error.code, WebErrorCode::InvalidRequest);
    assert_eq!(
        dam_integrations::runtime_enabled_profile_ids(&integration_dir).unwrap(),
        Some(vec!["claude".to_string(), "chatgpt".to_string()])
    );
}

#[test]
fn settings_apps_include_detector_toggles_with_safe_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let state = test_state(dir.path());

    let apps = app_settings_for_state_dir(&state, &state_dir).unwrap();
    let claude = apps.iter().find(|app| app.id == "claude").unwrap();

    assert_eq!(
        claude
            .detectors
            .iter()
            .map(|detector| detector.key.as_str())
            .collect::<Vec<_>>(),
        vec!["email", "phone", "ssn", "credit_card", "api_key"]
    );
    assert!(claude.detectors.iter().all(|detector| detector.enabled));
}

#[test]
fn settings_detector_toggle_rejects_hidden_profile_and_unknown_detector_key() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");

    let hidden_profile = set_profile_detector_preferences(
        &state_dir,
        "example-mail",
        &[DetectorPatch {
            key: "email".into(),
            enabled: false,
        }],
    )
    .unwrap_err();
    assert_eq!(hidden_profile.code, WebErrorCode::InvalidRequest);

    let unknown_key = set_profile_detector_preferences(
        &state_dir,
        "claude",
        &[DetectorPatch {
            key: "totally_unknown".into(),
            enabled: false,
        }],
    )
    .unwrap_err();
    assert_eq!(unknown_key.code, WebErrorCode::InvalidRequest);
}

#[test]
fn detector_preferences_persist_and_relax_only_selected_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let integration_dir = state_dir.join("integrations");
    dam_integrations::ensure_bundled_profile_files(&integration_dir).unwrap();

    set_profile_detector_preferences(
        &state_dir,
        "claude",
        &[
            DetectorPatch {
                key: "email".into(),
                enabled: false,
            },
            DetectorPatch {
                key: "phone".into(),
                enabled: true,
            },
        ],
    )
    .unwrap();

    let prefs = read_detector_preferences(&state_dir).unwrap();
    assert_eq!(
        prefs.profiles.get("claude").unwrap().disabled_kinds,
        vec!["email".to_string()]
    );

    let mut config = dam_config::DamConfig::default();
    config.traffic.profile = dam_net::llm_mvp_profile();
    let profile_ids = vec!["claude".to_string(), "chatgpt".to_string()];
    apply_detector_preferences_to_config(&mut config, &profile_ids, &prefs, &integration_dir)
        .unwrap();
    let anthropic_api = config
        .traffic
        .profile
        .apps
        .iter()
        .find(|app| app.id == "anthropic-api")
        .unwrap();

    assert_eq!(
        anthropic_api.outbound.filter.types.get("email"),
        Some(&dam_net::SensitiveDataAction::Allow)
    );
    assert_eq!(anthropic_api.outbound.filter.types.get("phone"), None);
    let openai_api = config
        .traffic
        .profile
        .apps
        .iter()
        .find(|app| app.id == "openai-api")
        .unwrap();
    assert_eq!(openai_api.outbound.filter.types.get("email"), None);
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

fn test_state(dir: &std::path::Path) -> crate::AppState {
    let vault = std::sync::Arc::new(dam_vault::Vault::open(dir.join("vault.db")).unwrap());
    let logs = std::sync::Arc::new(dam_log::LogStore::open(dir.join("log.db")).unwrap());
    crate::AppState {
        surface: crate::Surface::Web,
        tray_post_token: None,
        vault,
        consent_store: None,
        logs,
        config: std::sync::Arc::new(dam_config::DamConfig::default()),
        config_path: None,
        db_path: std::sync::Arc::new(dir.join("vault.db")),
        log_path: std::sync::Arc::new(dir.join("log.db")),
        client: reqwest::Client::new(),
        requests: std::sync::Arc::new(crate::request_store::RequestStore::default()),
        events: std::sync::Arc::new(crate::events_bus::EventBus::new()),
    }
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
