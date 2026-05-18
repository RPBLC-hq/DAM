use super::*;

#[test]
fn provider_option_sets_label_without_provider_specific_defaults() {
    let options = parse_proxy_options(["--provider".to_string(), "anthropic".to_string()]).unwrap();

    assert_eq!(options.target_name, "openai");
    assert_eq!(options.provider, "anthropic");
    assert_eq!(options.upstream, OPENAI_API_UPSTREAM);
}

#[test]
fn proxy_options_round_trip_through_args() {
    let options = ProxyOptions {
        config_path: Some(PathBuf::from("dam.toml")),
        listen: "127.0.0.1:9000".to_string(),
        network_mode: dam_net::CaptureMode::SystemProxy,
        network_mode_explicit: true,
        trust_mode: dam_trust::TrustMode::LocalCa,
        trust_mode_explicit: true,
        target_name: "custom-openai".to_string(),
        provider: "openai-compatible".to_string(),
        upstream: "https://api.custom.example".to_string(),
        targets: None,
        traffic_app_ids: Some(vec!["custom-openai-api".to_string()]),
        vault_path: PathBuf::from("vault.db"),
        log_path: None,
        consent_path: Some(PathBuf::from("consent.db")),
        resolve_inbound: Some(true),
    };

    assert_eq!(
        parse_proxy_options(proxy_options_to_args(&options)).unwrap(),
        options
    );
}

#[test]
fn proxy_options_round_trip_explicit_empty_traffic_apps() {
    let options = ProxyOptions {
        traffic_app_ids: Some(Vec::new()),
        ..ProxyOptions::default()
    };
    let args = proxy_options_to_args(&options);

    assert!(args.contains(&"--no-traffic-apps".to_string()));
    assert_eq!(
        parse_proxy_options(args).unwrap().traffic_app_ids,
        Some(Vec::new())
    );
}

#[test]
fn parses_network_mode_option() {
    let options =
        parse_proxy_options(["--network-mode".to_string(), "system-proxy".to_string()]).unwrap();

    assert_eq!(options.network_mode, dam_net::CaptureMode::SystemProxy);
    assert!(options.network_mode_explicit);
}

#[test]
fn parses_trust_mode_option() {
    let options =
        parse_proxy_options(["--trust-mode".to_string(), "local-ca".to_string()]).unwrap();

    assert_eq!(options.trust_mode, dam_trust::TrustMode::LocalCa);
    assert!(options.trust_mode_explicit);
}

#[test]
fn proxy_config_uses_caller_auth_passthrough() {
    let options = ProxyOptions::default();
    let config = proxy_config(&options).unwrap();

    assert_eq!(config.proxy.targets.len(), 5);
    assert_eq!(config.proxy.targets[0].name, "openai");
    assert_eq!(config.proxy.targets[0].provider, "openai-compatible");
    assert_eq!(config.proxy.targets[0].api_key_env, None);
    assert_eq!(config.proxy.targets[0].api_key, None);
    assert!(config.proxy.targets.iter().any(|target| {
        target.name == "anthropic"
            && target.provider == "anthropic"
            && target.upstream == ANTHROPIC_UPSTREAM
            && target.api_key_env.is_none()
            && target.api_key.is_none()
    }));
    assert!(
        config
            .proxy
            .targets
            .iter()
            .any(|target| target.name == "chatgpt-codex")
    );
    assert!(
        config
            .proxy
            .targets
            .iter()
            .any(|target| target.name == "claude-web" && target.provider == "generic-http")
    );
    assert!(
        config
            .proxy
            .targets
            .iter()
            .any(|target| target.name == "anthropic-console" && target.provider == "generic-http")
    );
    assert!(config.proxy.enabled);
    assert!(config.log.enabled);
}

#[test]
fn proxy_options_round_trip_multiple_targets() {
    let options = ProxyOptions {
        network_mode_explicit: true,
        trust_mode_explicit: true,
        targets: Some(vec![
            dam_config::ProxyTargetConfig {
                name: "openai".to_string(),
                provider: "openai-compatible".to_string(),
                upstream: OPENAI_API_UPSTREAM.to_string(),
                auth: dam_net::UpstreamAuthConfig::default(),
                failure_mode: None,
                api_key_env: None,
                api_key: None,
            },
            dam_config::ProxyTargetConfig {
                name: "anthropic".to_string(),
                provider: "anthropic".to_string(),
                upstream: ANTHROPIC_UPSTREAM.to_string(),
                auth: dam_net::UpstreamAuthConfig {
                    caller_headers: vec!["x-api-key".to_string()],
                    inject: Some(dam_net::UpstreamAuthInjection {
                        header: "x-api-key".to_string(),
                        scheme: None,
                        strip_headers: vec!["x-api-key".to_string()],
                    }),
                },
                failure_mode: None,
                api_key_env: None,
                api_key: None,
            },
        ]),
        ..ProxyOptions::default()
    };

    assert_eq!(
        parse_proxy_options(proxy_options_to_args(&options)).unwrap(),
        options
    );
    let config = proxy_config(&options).unwrap();
    assert_eq!(config.proxy.targets.len(), 5);
    assert_eq!(config.proxy.targets[1].provider, "anthropic");
}

#[test]
fn proxy_config_preserves_profile_target_upstream_override() {
    let options = ProxyOptions {
        targets: Some(vec![dam_config::ProxyTargetConfig {
            name: "anthropic".to_string(),
            provider: "anthropic".to_string(),
            upstream: "http://127.0.0.1:9999".to_string(),
            auth: dam_net::UpstreamAuthConfig::default(),
            failure_mode: None,
            api_key_env: None,
            api_key: None,
        }]),
        traffic_app_ids: Some(vec!["anthropic-api".to_string()]),
        ..ProxyOptions::default()
    };

    let config = proxy_config(&options).unwrap();

    assert_eq!(config.proxy.targets.len(), 1);
    assert_eq!(config.proxy.targets[0].name, "anthropic");
    assert_eq!(config.proxy.targets[0].upstream, "http://127.0.0.1:9999");
}

#[test]
fn configured_traffic_routes_follow_custom_traffic_profile() {
    let mut config = dam_config::DamConfig::default();
    config.traffic.profile = dam_net::traffic_profile_from_json_str(
        r#"
        {
          "version": 1,
          "default_action": "bypass",
          "apps": [
            {
              "id": "enterprise-ai",
              "match": {"domains": ["api.enterprise-ai.example"], "ports": [443]},
              "action": "inspect",
              "adapter": "http",
              "provider": "openai-compatible",
              "target_name": "enterprise-ai",
              "upstream": "https://api.enterprise-ai.example"
            }
          ]
        }
        "#,
    )
    .unwrap();

    let routes = configured_traffic_routes(&config);

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].host, "api.enterprise-ai.example");
    assert_eq!(routes[0].target_name, "enterprise-ai");
}

#[test]
fn configured_traffic_routes_follow_runtime_enabled_traffic_apps() {
    let mut config = dam_config::DamConfig::default();
    config.traffic.enabled_app_ids = Some(vec!["anthropic-api".to_string()]);

    let routes = configured_traffic_routes(&config);

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].host, "api.anthropic.com");
}

#[test]
fn state_paths_prefer_explicit_state_dir() {
    let paths = state_paths_from_env(
        Some(PathBuf::from("/tmp/dam-state")),
        Some(PathBuf::from("/home/example")),
    )
    .unwrap();

    assert_eq!(paths.state_dir, PathBuf::from("/tmp/dam-state"));
    assert_eq!(
        paths.state_file,
        PathBuf::from("/tmp/dam-state").join(STATE_FILE)
    );
}

#[test]
fn state_paths_fall_back_to_home_dot_dam() {
    let paths = state_paths_from_env(None, Some(PathBuf::from("/home/example"))).unwrap();

    assert_eq!(paths.state_dir, PathBuf::from("/home/example/.dam"));
}

#[test]
fn protection_state_parses_legacy_and_timestamped_control_files() {
    assert_eq!(
        parse_protection_state("disabled\n"),
        ProtectionState {
            enabled: false,
            changed_at_unix: None,
        }
    );
    assert_eq!(
        parse_protection_state("enabled\n"),
        ProtectionState {
            enabled: true,
            changed_at_unix: None,
        }
    );
    assert_eq!(
        parse_protection_state(r#"{"enabled":true,"changed_at_unix":42}"#),
        ProtectionState {
            enabled: true,
            changed_at_unix: Some(42),
        }
    );
    assert_eq!(
        parse_protection_state(r#"{"enabled":false,"changed_at_unix":84}"#),
        ProtectionState {
            enabled: false,
            changed_at_unix: Some(84),
        }
    );
}

#[test]
fn state_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(STATE_FILE);
    let state = DaemonState {
        version: STATE_VERSION,
        pid: 123,
        executable_path: Some(PathBuf::from("/usr/local/bin/dam")),
        executable_sha256: Some("abc123".to_string()),
        listen: "127.0.0.1:7828".to_string(),
        proxy_url: "http://127.0.0.1:7828".to_string(),
        config_path: Some(PathBuf::from("dam.toml")),
        vault_path: PathBuf::from("vault.db"),
        log_path: Some(PathBuf::from("log.db")),
        consent_path: Some(PathBuf::from("consent.db")),
        resolve_inbound: false,
        target_name: Some("openai".to_string()),
        target_provider: Some("openai-compatible".to_string()),
        upstream: Some(OPENAI_API_UPSTREAM.to_string()),
        proxy_targets: vec![DaemonProxyTargetState {
            name: "openai".to_string(),
            provider: "openai-compatible".to_string(),
            upstream: OPENAI_API_UPSTREAM.to_string(),
        }],
        started_at_unix: 42,
        network_mode: dam_net::CaptureMode::ExplicitProxy,
        transparent_routes: dam_net::default_traffic_routes(),
        transparent_routing_readiness: dam_net::transparent_capture_readiness_for_default_routes(
            dam_net::CaptureMode::ExplicitProxy,
            false,
            false,
        ),
        trust: dam_trust::TrustState::default(),
        transparent_trust_readiness: dam_trust::readiness_for_default_routes(
            &dam_trust::TrustState::default(),
            false,
        ),
        transparent_interception_readiness: dam_intercept::readiness_for_default_routes(
            dam_net::CaptureMode::ExplicitProxy,
            false,
            false,
            &dam_trust::TrustState::default(),
            false,
            dam_intercept::TlsInterceptionAdapter::unavailable(),
        ),
        protection_enabled: true,
        protection_started_at_unix: Some(42),
    };

    write_state_to(&path, &state).unwrap();

    assert_eq!(read_state_from(&path).unwrap(), Some(state));
}
