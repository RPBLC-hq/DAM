use super::*;

const OPENAI_API_UPSTREAM: &str = "https://api.openai.com";
const ANTHROPIC_UPSTREAM: &str = "https://api.anthropic.com";

fn expected_profile_network_mode() -> dam_net::CaptureMode {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dam_net::CaptureMode::ExplicitProxy
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        dam_net::CaptureMode::Tun
    }
}

fn claude_traffic_app_ids() -> Vec<String> {
    [
        "anthropic-api",
        "claude-web",
        "anthropic-console",
        "claude-mcp-proxy",
        "claude-platform",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[test]
fn removed_tool_launchers_are_not_cli_commands() {
    for command in ["codex", "claude"] {
        let error = parse_cli([command.to_string()]).unwrap_err();

        assert!(error.contains(&format!("unknown command: {command}")));
        assert!(!error.contains("one-shot"));
        assert!(!error.contains("fail"));
        assert!(!error.contains("dam codex"));
        assert!(!error.contains("dam claude"));
    }
}

#[test]
fn parses_web_forwarding_command() {
    let cli = parse_cli([
        "web".to_string(),
        "--config".to_string(),
        "dam.example.toml".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Web(WebArgs {
            args: vec!["--config".to_string(), "dam.example.toml".to_string()],
        })
    );
}

#[test]
fn parses_connect_with_low_level_provider_label() {
    let cli = parse_cli([
        "connect".to_string(),
        "--json".to_string(),
        "--provider".to_string(),
        "anthropic".to_string(),
        "--upstream".to_string(),
        ANTHROPIC_UPSTREAM.to_string(),
        "--listen".to_string(),
        "127.0.0.1:9000".to_string(),
    ])
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    assert_eq!(args.apply_profile_ids, Vec::<String>::new());
    assert_eq!(args.proxy.listen, "127.0.0.1:9000");
    assert_eq!(args.proxy.target_name, "openai");
    assert_eq!(args.proxy.provider, "anthropic");
    assert_eq!(args.proxy.upstream, ANTHROPIC_UPSTREAM);
    assert_eq!(args.proxy.network_mode, dam_net::CaptureMode::ExplicitProxy);
    assert_eq!(args.proxy.trust_mode, dam_trust::TrustMode::Disabled);
    assert!(args.json);
}

#[test]
fn parses_doctor_and_setup_agent_commands() {
    let doctor = parse_cli([
        "doctor".to_string(),
        "--config".to_string(),
        "/tmp/dam.toml".to_string(),
        "--state-dir".to_string(),
        "/tmp/dam-state".to_string(),
        "--proxy-url".to_string(),
        "http://127.0.0.1:7828".to_string(),
        "--network-mode".to_string(),
        "tun".to_string(),
        "--trust-mode".to_string(),
        "local_ca".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        doctor.command,
        CommandKind::Doctor(DoctorArgs {
            json: true,
            config_path: Some(PathBuf::from("/tmp/dam.toml")),
            state_dir: Some(PathBuf::from("/tmp/dam-state")),
            proxy_url: Some("http://127.0.0.1:7828".to_string()),
            network_mode: dam_net::CaptureMode::Tun,
            trust_mode: dam_trust::TrustMode::LocalCa,
        })
    );

    let setup = parse_cli([
        "setup".to_string(),
        "next-action".to_string(),
        "--network-mode".to_string(),
        "tun".to_string(),
        "--trust-mode".to_string(),
        "local_ca".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        setup.command,
        CommandKind::Setup(SetupArgs::NextAction(SetupPlanArgs {
            json: true,
            network_mode: dam_net::CaptureMode::Tun,
            trust_mode: dam_trust::TrustMode::LocalCa,
            ..SetupPlanArgs::default()
        }))
    );

    let status = parse_cli([
        "setup".to_string(),
        "status".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        status.command,
        CommandKind::Setup(SetupArgs::Status(SetupPlanArgs {
            json: true,
            ..SetupPlanArgs::default()
        }))
    );

    let rescue = parse_cli([
        "setup".to_string(),
        "rescue".to_string(),
        "--state-dir".to_string(),
        "/tmp/dam-rescue".to_string(),
        "--yes".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        rescue.command,
        CommandKind::Setup(SetupArgs::Rescue(SetupRescueArgs {
            json: true,
            yes: true,
            state_dir: Some(PathBuf::from("/tmp/dam-rescue")),
        }))
    );

    let repair = parse_cli([
        "setup".to_string(),
        "repair".to_string(),
        "--state-dir".to_string(),
        "/tmp/dam-repair".to_string(),
        "--dry-run".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        repair.command,
        CommandKind::Setup(SetupArgs::Repair(SetupRepairArgs {
            plan: SetupPlanArgs {
                json: true,
                state_dir: Some(PathBuf::from("/tmp/dam-repair")),
                ..SetupPlanArgs::default()
            },
            yes: false,
        }))
    );

    let export = parse_cli([
        "setup".to_string(),
        "export-diagnostics".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        export.command,
        CommandKind::Setup(SetupArgs::ExportDiagnostics(SetupPlanArgs {
            json: true,
            ..SetupPlanArgs::default()
        }))
    );
}

#[test]
fn setup_plan_usage_is_specific_to_requested_command() {
    let status = usage_setup_plan("status");
    assert!(status.starts_with("Usage: dam setup status "));
    assert!(status.contains("full idempotent setup checklist"));
    assert!(!status.contains("status|plan|next-action|resume|export-diagnostics"));

    let next_action = usage_setup_plan("next-action");
    assert!(next_action.starts_with("Usage: dam setup next-action "));
    assert!(next_action.contains("only the next setup action"));
}

#[test]
fn setup_rescue_rejects_dry_run_with_yes() {
    let error = parse_cli([
        "setup".to_string(),
        "rescue".to_string(),
        "--dry-run".to_string(),
        "--yes".to_string(),
    ])
    .unwrap_err();

    assert!(error.contains("cannot combine"));
}

#[test]
fn parses_disconnect_json() {
    let cli = parse_cli([
        "disconnect".to_string(),
        "--stop".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Disconnect(DisconnectArgs {
            stop_daemon: true,
            json: true,
        })
    );
}

#[test]
fn parses_connect_with_integration_profile_defaults() {
    let cli = parse_cli([
        "connect".to_string(),
        "--profile".to_string(),
        "codex".to_string(),
        "--listen".to_string(),
        "127.0.0.1:9000".to_string(),
    ])
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    assert_eq!(args.apply_profile_ids, Vec::<String>::new());
    assert_eq!(args.proxy.listen, "127.0.0.1:9000");
    assert_eq!(args.proxy.target_name, "openai");
    assert_eq!(args.proxy.provider, "openai-compatible");
    assert_eq!(args.proxy.upstream, OPENAI_API_UPSTREAM);
    assert_eq!(
        args.proxy.traffic_app_ids,
        Some(vec![
            "openai-api".to_string(),
            "openai-platform".to_string(),
            "chatgpt-web".to_string(),
            "chatgpt-legacy-web".to_string()
        ])
    );
    assert_eq!(args.proxy.network_mode, expected_profile_network_mode());
    assert_eq!(args.proxy.trust_mode, dam_trust::TrustMode::LocalCa);
}

#[test]
fn parses_connect_profile_apply() {
    let cli = parse_cli([
        "connect".to_string(),
        "--profile".to_string(),
        "claude".to_string(),
        "--apply".to_string(),
        "--listen".to_string(),
        "127.0.0.1:9000".to_string(),
    ])
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    assert_eq!(args.apply_profile_ids, vec!["claude".to_string()]);
    assert_eq!(args.proxy.listen, "127.0.0.1:9000");
    let targets = args.proxy.targets.as_ref().unwrap();
    assert_eq!(targets[0].name, "anthropic");
    assert_eq!(targets[0].provider, "anthropic");
    assert_eq!(targets[0].upstream, ANTHROPIC_UPSTREAM);
    assert_eq!(args.proxy.traffic_app_ids, Some(claude_traffic_app_ids()));
    assert_eq!(args.proxy.network_mode, expected_profile_network_mode());
    assert_eq!(args.proxy.trust_mode, dam_trust::TrustMode::LocalCa);
}

#[test]
fn state_runtime_path_roots_relative_paths_in_state_dir() {
    let state_dir = Path::new("/Users/example/.dam");

    assert_eq!(
        state_runtime_path(state_dir, Path::new("log.db")),
        PathBuf::from("/Users/example/.dam/log.db")
    );
    assert_eq!(
        state_runtime_path(state_dir, Path::new("/tmp/log.db")),
        PathBuf::from("/tmp/log.db")
    );
}

#[test]
fn daemon_runtime_paths_detect_split_state() {
    let mut state = test_daemon_state(
        dam_net::CaptureMode::Tun,
        dam_trust::TrustMode::LocalCa,
        true,
    );
    state.vault_path = PathBuf::from("vault.db");
    state.log_path = Some(PathBuf::from("log.db"));
    state.consent_path = Some(PathBuf::from("consent.db"));
    let proxy = dam_daemon::ProxyOptions {
        vault_path: PathBuf::from("/Users/example/.dam/vault.db"),
        log_path: Some(PathBuf::from("/Users/example/.dam/log.db")),
        consent_path: Some(PathBuf::from("/Users/example/.dam/consent.db")),
        ..dam_daemon::ProxyOptions::default()
    };

    assert!(!daemon_runtime_paths_match(&state, &proxy));
}

#[test]
fn parses_connect_apply_with_enabled_profile() {
    let cli = parse_cli_with_active_profiles(
        [
            "connect".to_string(),
            "--apply".to_string(),
            "--listen".to_string(),
            "127.0.0.1:9000".to_string(),
        ],
        vec!["claude".to_string()],
    )
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    assert_eq!(args.apply_profile_ids, vec!["claude".to_string()]);
    assert_eq!(args.proxy.listen, "127.0.0.1:9000");
    let targets = args.proxy.targets.as_ref().unwrap();
    assert_eq!(targets[0].provider, "anthropic");
    assert_eq!(targets[0].upstream, ANTHROPIC_UPSTREAM);
    assert_eq!(args.proxy.traffic_app_ids, Some(claude_traffic_app_ids()));
    assert_eq!(args.proxy.network_mode, expected_profile_network_mode());
    assert_eq!(args.proxy.trust_mode, dam_trust::TrustMode::LocalCa);
}

#[test]
fn parses_connect_with_enabled_profile_selecting_targets_only() {
    let cli = parse_cli_with_active_profiles(
        [
            "connect".to_string(),
            "--listen".to_string(),
            "127.0.0.1:9000".to_string(),
        ],
        vec!["claude".to_string()],
    )
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    assert_eq!(args.apply_profile_ids, Vec::<String>::new());
    assert_eq!(args.proxy.listen, "127.0.0.1:9000");
    let targets = args.proxy.targets.as_ref().unwrap();
    assert_eq!(targets[0].provider, "anthropic");
    assert_eq!(targets[0].upstream, ANTHROPIC_UPSTREAM);
    assert_eq!(args.proxy.network_mode, expected_profile_network_mode());
    assert_eq!(args.proxy.trust_mode, dam_trust::TrustMode::LocalCa);
}

#[test]
fn connect_with_stale_stored_profile_ignores_legacy_provider_flags() {
    let dir = tempfile::tempdir().unwrap();
    let integration_state_dir = dir.path().join("integrations");
    let profiles_dir = dam_integrations::profile_definitions_dir(&integration_state_dir);
    std::fs::create_dir_all(&profiles_dir).unwrap();
    std::fs::write(
        dam_integrations::profile_definition_path(&integration_state_dir, "claude"),
        r#"{
          "id": "claude",
          "name": "Claude Code",
          "summary": "Stale profile",
          "provider": "anthropic",
          "traffic_app_ids": ["anthropic-api"],
          "connect_args": ["--anthropic", "--network-mode", "tun", "--trust-mode", "local_ca"],
          "settings": [],
          "commands": [],
          "notes": [],
          "automation": "connect_preset"
        }"#,
    )
    .unwrap();

    let cli = parse_cli_with_connect_profiles(
        [
            "connect".to_string(),
            "--listen".to_string(),
            "127.0.0.1:9000".to_string(),
        ],
        ConnectProfileSelection {
            profile_ids: vec!["claude".to_string()],
            explicit_selection: true,
            integration_state_dir: Some(integration_state_dir),
        },
    )
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    let targets = args.proxy.targets.as_ref().unwrap();
    assert_eq!(targets[0].provider, "anthropic");
    assert_eq!(args.proxy.network_mode, expected_profile_network_mode());
    assert_eq!(args.proxy.trust_mode, dam_trust::TrustMode::LocalCa);
    assert_eq!(args.proxy.traffic_app_ids, Some(claude_traffic_app_ids()));
}

#[test]
fn parses_connect_with_explicit_empty_enabled_profiles_as_empty_traffic_scope() {
    let cli = parse_cli_with_connect_profiles(
        [
            "connect".to_string(),
            "--listen".to_string(),
            "127.0.0.1:9000".to_string(),
        ],
        ConnectProfileSelection {
            profile_ids: Vec::new(),
            explicit_selection: true,
            ..ConnectProfileSelection::default()
        },
    )
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    assert_eq!(args.apply_profile_ids, Vec::<String>::new());
    assert_eq!(args.proxy.listen, "127.0.0.1:9000");
    assert_eq!(args.proxy.traffic_app_ids, Some(Vec::new()));
}

#[test]
fn connect_profile_upstream_override_rejects_multi_target_profiles() {
    let error = parse_cli_with_active_profiles(
        [
            "connect".to_string(),
            "--apply".to_string(),
            "--listen".to_string(),
            "127.0.0.1:9000".to_string(),
            "--upstream".to_string(),
            "http://127.0.0.1:9999".to_string(),
            "--network-mode".to_string(),
            "explicit_proxy".to_string(),
            "--trust-mode".to_string(),
            "disabled".to_string(),
        ],
        vec!["claude".to_string()],
    )
    .unwrap_err();

    assert!(error.contains("--upstream can override only single-target profiles"));
}

#[test]
fn parses_connect_apply_with_multiple_enabled_profiles() {
    let cli = parse_cli_with_active_profiles(
        [
            "connect".to_string(),
            "--apply".to_string(),
            "--listen".to_string(),
            "127.0.0.1:9000".to_string(),
        ],
        vec!["chatgpt".to_string(), "claude".to_string()],
    )
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    assert_eq!(
        args.apply_profile_ids,
        vec!["chatgpt".to_string(), "claude".to_string()]
    );
    assert_eq!(
        args.proxy.traffic_app_ids,
        Some(vec![
            "openai-api".to_string(),
            "openai-platform".to_string(),
            "chatgpt-web".to_string(),
            "chatgpt-legacy-web".to_string(),
            "anthropic-api".to_string(),
            "claude-web".to_string(),
            "anthropic-console".to_string(),
            "claude-mcp-proxy".to_string(),
            "claude-platform".to_string()
        ])
    );
    let targets = args.proxy.targets.unwrap();
    assert_eq!(targets.len(), 9);
    assert_eq!(args.proxy.trust_mode, dam_trust::TrustMode::LocalCa);
    assert!(
        targets
            .iter()
            .any(|target| target.provider == "openai-compatible")
    );
    assert!(targets.iter().any(|target| target.provider == "anthropic"));
    assert!(targets.iter().any(|target| target.name == "chatgpt-web"));
    assert!(
        targets
            .iter()
            .any(|target| target.name == "chatgpt-legacy-web")
    );
    assert!(
        targets
            .iter()
            .any(|target| target.name == "openai-platform")
    );
    assert!(
        targets
            .iter()
            .any(|target| target.name == "claude-platform")
    );
    assert!(
        targets
            .iter()
            .any(|target| target.name == "claude-mcp-proxy")
    );
}

#[test]
fn connect_setup_change_ignores_implicit_default_modes_for_existing_daemon() {
    let state = test_daemon_state(
        dam_net::CaptureMode::Tun,
        dam_trust::TrustMode::LocalCa,
        true,
    );
    let proxy = dam_daemon::ProxyOptions::default();

    assert!(!connect_setup_change_requested(&state, &proxy));
}

#[test]
fn connect_setup_change_honors_explicit_mode_flags() {
    let state = test_daemon_state(
        dam_net::CaptureMode::Tun,
        dam_trust::TrustMode::LocalCa,
        true,
    );
    let proxy = dam_daemon::ProxyOptions {
        network_mode: dam_net::CaptureMode::ExplicitProxy,
        network_mode_explicit: true,
        trust_mode: dam_trust::TrustMode::Disabled,
        trust_mode_explicit: true,
        ..dam_daemon::ProxyOptions::default()
    };

    assert!(connect_setup_change_requested(&state, &proxy));
}

#[test]
fn existing_daemon_restart_options_preserve_running_setup() {
    let mut state = test_daemon_state(
        dam_net::CaptureMode::Tun,
        dam_trust::TrustMode::LocalCa,
        true,
    );
    state.listen = "127.0.0.1:9001".to_string();
    state.vault_path = PathBuf::from("/tmp/dam/vault.db");
    state.log_path = Some(PathBuf::from("/tmp/dam/log.db"));
    state.consent_path = Some(PathBuf::from("/tmp/dam/consent.db"));
    state.proxy_targets = vec![
        dam_daemon::DaemonProxyTargetState {
            name: "anthropic".to_string(),
            provider: "anthropic".to_string(),
            upstream: ANTHROPIC_UPSTREAM.to_string(),
        },
        dam_daemon::DaemonProxyTargetState {
            name: "openai".to_string(),
            provider: "openai-compatible".to_string(),
            upstream: OPENAI_API_UPSTREAM.to_string(),
        },
    ];

    let requested = dam_daemon::ProxyOptions::default();
    let proxy = proxy_options_for_existing_daemon(&state, &requested);

    assert_eq!(proxy.listen, "127.0.0.1:9001");
    assert_eq!(proxy.network_mode, dam_net::CaptureMode::Tun);
    assert!(!proxy.network_mode_explicit);
    assert_eq!(proxy.trust_mode, dam_trust::TrustMode::LocalCa);
    assert!(!proxy.trust_mode_explicit);
    assert_eq!(proxy.vault_path, PathBuf::from("/tmp/dam/vault.db"));
    assert_eq!(proxy.log_path, Some(PathBuf::from("/tmp/dam/log.db")));
    assert_eq!(
        proxy.consent_path,
        Some(PathBuf::from("/tmp/dam/consent.db"))
    );
    let targets = proxy.targets.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(targets.iter().any(|target| target.name == "anthropic"));
    assert!(targets.iter().any(|target| target.name == "openai"));
}

#[test]
fn daemon_without_recorded_executable_requires_restart() {
    let mut state = test_daemon_state(
        dam_net::CaptureMode::ExplicitProxy,
        dam_trust::TrustMode::Disabled,
        true,
    );
    state.executable_path = None;

    assert!(!daemon_executable_matches_current(&state).unwrap());
}

#[test]
fn connect_apply_with_explicit_provider_requires_profile_or_enabled_profiles() {
    let error = parse_cli([
        "connect".to_string(),
        "--apply".to_string(),
        "--provider".to_string(),
        "openai-compatible".to_string(),
    ])
    .unwrap_err();

    assert!(error.contains("enabled profiles"));
}

#[test]
fn connect_apply_profile_targets_override_low_level_provider_flag() {
    let cli = parse_cli([
        "connect".to_string(),
        "--profile".to_string(),
        "claude".to_string(),
        "--apply".to_string(),
        "--provider".to_string(),
        "openai-compatible".to_string(),
    ])
    .unwrap();

    let CommandKind::Connect(args) = cli.command else {
        panic!("expected connect");
    };
    let targets = args.proxy.targets.as_ref().unwrap();
    assert_eq!(targets[0].provider, "anthropic");
    assert_eq!(targets[0].upstream, ANTHROPIC_UPSTREAM);
}

#[test]
fn connect_apply_rejects_dynamic_port() {
    let options = dam_daemon::ProxyOptions {
        listen: "127.0.0.1:0".to_string(),
        ..dam_daemon::ProxyOptions::default()
    };

    let error = proxy_url_for_connect_apply(&options).unwrap_err();

    assert!(error.contains("fixed --listen port"));
}

#[test]
fn connect_preflight_allows_default_explicit_proxy_setup() {
    let dir = tempfile::tempdir().unwrap();
    let options = dam_daemon::ProxyOptions::default();
    let config = dam_daemon::proxy_config(&options).unwrap();

    ensure_connect_transparent_prerequisites(&options, &config, Some(dir.path().join("state")))
        .unwrap();
}

#[test]
fn connect_preflight_blocks_missing_system_proxy_setup() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    std::fs::write(&config_path, "").unwrap();
    let options = dam_daemon::ProxyOptions {
        network_mode: dam_net::CaptureMode::SystemProxy,
        config_path: Some(config_path),
        ..dam_daemon::ProxyOptions::default()
    };
    let config = dam_daemon::proxy_config(&options).unwrap();

    let error =
        ensure_connect_transparent_prerequisites(&options, &config, Some(dir.path().join("state")))
            .unwrap_err();

    assert!(error.contains("system proxy routing needs to be installed"));
    assert!(error.contains("dam network install-system-proxy"));
    assert!(error.contains("--config"));
}

#[cfg(target_os = "macos")]
#[test]
fn connect_preflight_blocks_missing_network_extension_setup() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    std::fs::write(&config_path, "").unwrap();
    let options = dam_daemon::ProxyOptions {
        network_mode: dam_net::CaptureMode::Tun,
        config_path: Some(config_path),
        ..dam_daemon::ProxyOptions::default()
    };
    let config = dam_daemon::proxy_config(&options).unwrap();

    let error =
        ensure_connect_transparent_prerequisites(&options, &config, Some(dir.path().join("state")))
            .unwrap_err();

    assert!(error.contains("Network Extension capture needs to be installed"));
    assert!(error.contains("dam network install-network-extension"));
    assert!(error.contains("--config"));
}

#[cfg(target_os = "macos")]
#[test]
fn connect_preflight_blocks_missing_network_extension_for_empty_app_scope() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    dam_integrations::set_integration_enabled("claude", false, &state_dir.join("integrations"))
        .unwrap();
    dam_integrations::set_integration_enabled("chatgpt", false, &state_dir.join("integrations"))
        .unwrap();
    let options = dam_daemon::ProxyOptions {
        network_mode: dam_net::CaptureMode::Tun,
        trust_mode: dam_trust::TrustMode::LocalCa,
        ..dam_daemon::ProxyOptions::default()
    };
    let config = dam_daemon::proxy_config(&options).unwrap();

    let error =
        ensure_connect_transparent_prerequisites(&options, &config, Some(state_dir)).unwrap_err();

    assert!(error.contains("Network Extension capture needs to be installed"));
    assert!(error.contains("dam network install-network-extension"));
}

#[cfg(target_os = "macos")]
#[test]
fn connect_preflight_blocks_missing_network_extension_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    dam_net_macos::record_system_extension_ready(
        &state_dir,
        "com.rpblc.dam.network-extension",
        None,
        vec!["api.openai.com".to_string()],
    )
    .unwrap();
    let options = dam_daemon::ProxyOptions {
        network_mode: dam_net::CaptureMode::Tun,
        ..dam_daemon::ProxyOptions::default()
    };
    let config = dam_daemon::proxy_config(&options).unwrap();

    let error =
        ensure_connect_transparent_prerequisites(&options, &config, Some(state_dir)).unwrap_err();

    assert!(error.contains("configuration"));
    assert!(error.contains("dam network install-network-extension"));
}

#[test]
fn connect_preflight_blocks_missing_local_ca_setup() {
    let dir = tempfile::tempdir().unwrap();
    let options = dam_daemon::ProxyOptions {
        trust_mode: dam_trust::TrustMode::LocalCa,
        ..dam_daemon::ProxyOptions::default()
    };
    let config = dam_daemon::proxy_config(&options).unwrap();

    let error =
        ensure_connect_transparent_prerequisites(&options, &config, Some(dir.path().join("state")))
            .unwrap_err();

    assert!(error.contains("local CA"));
    assert!(error.contains("dam trust install-local-ca"));
}

#[test]
fn parses_integrations_list_json() {
    let cli = parse_cli([
        "integrations".to_string(),
        "list".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Integrations(IntegrationArgs::List {
            json: true,
            proxy_url: None,
        })
    );
}

#[test]
fn parses_integrations_show_with_proxy_url() {
    let cli = parse_cli([
        "integrations".to_string(),
        "show".to_string(),
        "chatgpt".to_string(),
        "--proxy-url".to_string(),
        "http://127.0.0.1:9000".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Integrations(IntegrationArgs::Show {
            profile_id: "chatgpt".to_string(),
            json: false,
            proxy_url: Some("http://127.0.0.1:9000".to_string()),
        })
    );
}

#[test]
fn parses_integrations_apply_with_dry_run_and_target_path() {
    let cli = parse_cli([
        "integrations".to_string(),
        "apply".to_string(),
        "chatgpt".to_string(),
        "--dry-run".to_string(),
        "--target-path".to_string(),
        "/tmp/chatgpt.json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Integrations(IntegrationArgs::Apply {
            profile_id: "chatgpt".to_string(),
            dry_run: true,
            json: false,
            proxy_url: None,
            target_path: Some(PathBuf::from("/tmp/chatgpt.json")),
        })
    );
}

#[test]
fn integrations_apply_defaults_to_dry_run_and_requires_write_for_mutation() {
    let preview = parse_cli([
        "integrations".to_string(),
        "apply".to_string(),
        "chatgpt".to_string(),
    ])
    .unwrap();

    assert_eq!(
        preview.command,
        CommandKind::Integrations(IntegrationArgs::Apply {
            profile_id: "chatgpt".to_string(),
            dry_run: true,
            json: false,
            proxy_url: None,
            target_path: None,
        })
    );

    let write = parse_cli([
        "integrations".to_string(),
        "apply".to_string(),
        "chatgpt".to_string(),
        "--write".to_string(),
    ])
    .unwrap();

    assert_eq!(
        write.command,
        CommandKind::Integrations(IntegrationArgs::Apply {
            profile_id: "chatgpt".to_string(),
            dry_run: false,
            json: false,
            proxy_url: None,
            target_path: None,
        })
    );
}

#[test]
fn integrations_apply_rejects_dry_run_with_write() {
    let error = parse_cli([
        "integrations".to_string(),
        "apply".to_string(),
        "chatgpt".to_string(),
        "--dry-run".to_string(),
        "--write".to_string(),
    ])
    .unwrap_err();

    assert!(error.contains("cannot combine"));
}

#[test]
fn parses_integrations_rollback_json() {
    let cli = parse_cli([
        "integrations".to_string(),
        "rollback".to_string(),
        "chatgpt".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Integrations(IntegrationArgs::Rollback {
            profile_id: "chatgpt".to_string(),
            json: true,
        })
    );
}

#[test]
fn integration_profile_render_quotes_spaced_command_args() {
    let profile = dam_integrations::profile("chatgpt", "http://127.0.0.1:7828").unwrap();
    let rendered = render_integration_profile(&profile, "http://127.0.0.1:7828");

    assert!(rendered.contains("HTTPS_PROXY=http://127.0.0.1:7828"));
    assert!(rendered.contains("HTTP_PROXY=http://127.0.0.1:7828"));
    assert!(!rendered.contains("dam_openai"));
}

#[test]
fn parses_status_json() {
    let cli = parse_cli(["status".to_string(), "--json".to_string()]).unwrap();

    assert_eq!(cli.command, CommandKind::Status(StatusArgs { json: true }));
}

#[test]
fn parses_logs_filters() {
    let cli = parse_cli([
        "logs".to_string(),
        "--limit".to_string(),
        "5".to_string(),
        "--after-id".to_string(),
        "42".to_string(),
        "--operation".to_string(),
        "abc123".to_string(),
        "--events".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Logs(LogsArgs {
            json: true,
            limit: 5,
            after_id: Some(42),
            operation_id: Some("abc123".to_string()),
            events: true,
        })
    );
}

#[test]
fn log_summary_collapses_proxy_diagnostics() {
    let entries = vec![
        dam_log::LogEntry {
            id: 3,
            timestamp: 3,
            operation_id: "op".to_string(),
            level: "info".to_string(),
            event_type: "proxy_forward".to_string(),
            kind: None,
            value: None,
            reference: None,
            action: Some("provider_response".to_string()),
            message: "provider response status=200 content_type=text/event-stream content_encoding=none streaming=true".to_string(),
        },
        dam_log::LogEntry {
            id: 2,
            timestamp: 2,
            operation_id: "op".to_string(),
            level: "info".to_string(),
            event_type: "proxy_forward".to_string(),
            kind: None,
            value: None,
            reference: None,
            action: Some("request_protection".to_string()),
            message: "request protection detections=1 replacements=1 tokenized=1 blocked=0".to_string(),
        },
        dam_log::LogEntry {
            id: 1,
            timestamp: 1,
            operation_id: "op".to_string(),
            level: "info".to_string(),
            event_type: "proxy_forward".to_string(),
            kind: None,
            value: None,
            reference: None,
            action: Some("route_decision".to_string()),
            message: "route target=anthropic provider=anthropic protection_enabled=true resolve_inbound=true request_bytes=100".to_string(),
        },
    ];

    let summaries = log_operation_summaries(entries, 10);

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].events, 3);
    assert!(summaries[0].summary.contains("route target=anthropic"));
    assert!(
        summaries[0]
            .summary
            .contains("request protection detections=1")
    );
    assert!(
        summaries[0]
            .summary
            .contains("provider response status=200")
    );
}

#[test]
fn parses_profile_status_json() {
    let cli = parse_cli([
        "profile".to_string(),
        "status".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Profile(ProfileArgs::Status { json: true })
    );
}

#[test]
fn parses_profile_set_json() {
    let cli = parse_cli([
        "profile".to_string(),
        "set".to_string(),
        "claude".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Profile(ProfileArgs::Set {
            profile_id: "claude".to_string(),
            json: true,
        })
    );
}

#[test]
fn parses_profile_clear_json() {
    let cli = parse_cli([
        "profile".to_string(),
        "clear".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Profile(ProfileArgs::Clear { json: true })
    );
}

#[test]
fn parses_trust_generate_local_ca_json() {
    let cli = parse_cli([
        "trust".to_string(),
        "generate-local-ca".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Trust(TrustArgs::GenerateArtifact { json: true })
    );
}

#[test]
fn parses_trust_delete_local_ca_json() {
    let cli = parse_cli([
        "trust".to_string(),
        "delete-local-ca".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Trust(TrustArgs::DeleteArtifact { json: true })
    );
}

#[test]
fn parses_trust_install_and_remove_local_ca_approval() {
    let install = parse_cli([
        "trust".to_string(),
        "install-local-ca".to_string(),
        "--yes".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    let remove = parse_cli([
        "trust".to_string(),
        "remove-local-ca".to_string(),
        "--yes".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        install.command,
        CommandKind::Trust(TrustArgs::InstallTrust {
            json: true,
            yes: true
        })
    );
    assert_eq!(
        remove.command,
        CommandKind::Trust(TrustArgs::RemoveTrust {
            json: true,
            yes: true
        })
    );
}

#[test]
fn rejects_trust_local_ca_dry_run_with_approval() {
    let install_err = parse_cli([
        "trust".to_string(),
        "install-local-ca".to_string(),
        "--dry-run".to_string(),
        "--yes".to_string(),
    ])
    .unwrap_err();
    let remove_err = parse_cli([
        "trust".to_string(),
        "remove-local-ca".to_string(),
        "--yes".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap_err();

    assert_eq!(
        install_err,
        "trust install-local-ca cannot combine --dry-run and --yes"
    );
    assert_eq!(
        remove_err,
        "trust remove-local-ca cannot combine --dry-run and --yes"
    );
}

#[test]
fn parses_network_install_and_remove_system_proxy_approval() {
    let install = parse_cli([
        "network".to_string(),
        "install-system-proxy".to_string(),
        "--yes".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    let remove = parse_cli([
        "network".to_string(),
        "remove-system-proxy".to_string(),
        "--yes".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        install.command,
        CommandKind::Network(NetworkArgs::InstallProxy {
            config_path: None,
            json: true,
            yes: true
        })
    );
    assert_eq!(
        remove.command,
        CommandKind::Network(NetworkArgs::RemoveProxy {
            json: true,
            yes: true
        })
    );
}

#[test]
fn parses_network_install_config_path() {
    let cli = parse_cli([
        "network".to_string(),
        "install-system-proxy".to_string(),
        "--config".to_string(),
        "dam.enterprise.toml".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        CommandKind::Network(NetworkArgs::InstallProxy {
            config_path: Some(PathBuf::from("dam.enterprise.toml")),
            json: true,
            yes: false
        })
    );
}

#[test]
fn parses_network_extension_commands() {
    let install = parse_cli([
        "network".to_string(),
        "install-network-extension".to_string(),
        "--config".to_string(),
        "dam.enterprise.toml".to_string(),
        "--yes".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    let remove = parse_cli([
        "network".to_string(),
        "remove-network-extension".to_string(),
        "--yes".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    let status = parse_cli([
        "network".to_string(),
        "status".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        install.command,
        CommandKind::Network(NetworkArgs::InstallNetworkExtension {
            config_path: Some(PathBuf::from("dam.enterprise.toml")),
            json: true,
            yes: true
        })
    );
    assert_eq!(
        remove.command,
        CommandKind::Network(NetworkArgs::RemoveNetworkExtension {
            json: true,
            yes: true
        })
    );
    assert_eq!(
        status.command,
        CommandKind::Network(NetworkArgs::Status { json: true })
    );
}

#[test]
fn parses_startup_commands() {
    let status = parse_cli([
        "startup".to_string(),
        "status".to_string(),
        "--json".to_string(),
    ])
    .unwrap();
    let skip = parse_cli([
        "startup".to_string(),
        "skip-open-at-login".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        status.command,
        CommandKind::Startup(StartupArgs::Status { json: true })
    );
    assert_eq!(
        skip.command,
        CommandKind::Startup(StartupArgs::SkipOpenAtLogin { json: true })
    );
}

#[test]
fn startup_skip_open_at_login_records_marker() {
    let dir = tempfile::tempdir().unwrap();
    let marker = write_startup_skip_marker(dir.path()).unwrap();

    assert!(marker.exists());
    let view = startup_status_view(dir.path());
    assert_eq!(view.state, "skipped");
    assert_eq!(view.marker, Some(marker));
}

#[test]
fn local_ca_generate_and_delete_outputs_do_not_install_local_trust() {
    let dir = tempfile::tempdir().unwrap();

    let generated = generate_local_ca_output(dir.path(), false).unwrap();
    assert!(generated.contains("state: generated"));
    assert!(generated.contains("local_trust: unchanged"));
    assert!(generated.contains("fingerprint_sha256: "));

    let deleted = delete_local_ca_output(dir.path(), false).unwrap();
    assert!(deleted.contains("state: deleted"));
    assert!(deleted.contains("local_trust: unchanged"));

    let missing = delete_local_ca_output(dir.path(), true).unwrap();
    let report: serde_json::Value = serde_json::from_str(&missing).unwrap();
    assert_eq!(report["state"], "missing");
    assert_eq!(report["deleted"], false);
}

#[test]
fn local_ca_install_and_remove_preview_require_approval() {
    let dir = tempfile::tempdir().unwrap();

    let install = install_local_ca_output(dir.path(), false, false).unwrap();
    assert!(install.contains("state: preview"));
    assert!(install.contains("will_generate_artifact: true"));
    assert!(install.contains("local_trust: unchanged"));
    assert!(install.contains("approval: rerun with --yes"));

    let generated = generate_local_ca_output(dir.path(), false).unwrap();
    assert!(generated.contains("state: generated"));

    let remove = remove_local_ca_output(dir.path(), true, false).unwrap();
    let report: serde_json::Value = serde_json::from_str(&remove).unwrap();
    assert_eq!(report["state"], "preview");
    assert_eq!(report["system_trust_changed"], false);
    assert_eq!(report["plan"]["can_execute"], false);
}

#[test]
fn parses_daemon_run_as_internal_proxy_options() {
    let cli = parse_cli([
        "daemon-run".to_string(),
        "--target-name".to_string(),
        "custom-openai".to_string(),
        "--provider".to_string(),
        "openai-compatible".to_string(),
        "--upstream".to_string(),
        "https://api.custom.example".to_string(),
    ])
    .unwrap();

    let CommandKind::DaemonRun(args) = cli.command else {
        panic!("expected daemon run");
    };
    assert_eq!(args.target_name, "custom-openai");
    assert_eq!(args.upstream, "https://api.custom.example");
}

fn test_daemon_state(
    network_mode: dam_net::CaptureMode,
    trust_mode: dam_trust::TrustMode,
    protection_enabled: bool,
) -> dam_daemon::DaemonState {
    dam_daemon::DaemonState {
        version: 4,
        pid: 1,
        executable_path: Some(PathBuf::from("/usr/local/bin/dam")),
        executable_sha256: Some("abc123".to_string()),
        listen: "127.0.0.1:7828".to_string(),
        proxy_url: "http://127.0.0.1:7828".to_string(),
        config_path: None,
        vault_path: PathBuf::from("vault.db"),
        log_path: Some(PathBuf::from("log.db")),
        consent_path: Some(PathBuf::from("consent.db")),
        resolve_inbound: true,
        target_name: Some("openai".to_string()),
        target_provider: Some("openai-compatible".to_string()),
        upstream: Some(OPENAI_API_UPSTREAM.to_string()),
        proxy_targets: Vec::new(),
        started_at_unix: 0,
        network_mode,
        transparent_routes: Vec::new(),
        transparent_routing_readiness: Vec::new(),
        trust: dam_trust::TrustState {
            mode: trust_mode,
            ..dam_trust::TrustState::default()
        },
        transparent_trust_readiness: Vec::new(),
        transparent_interception_readiness: Vec::new(),
        protection_enabled,
        protection_started_at_unix: if protection_enabled { Some(0) } else { None },
    }
}
