use super::*;
use dam_core::{PolicyAction, SensitiveType};
use std::fs;

fn env(entries: &[(&str, &str)]) -> Vec<(String, String)> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn defaults_are_local_development_safe() {
    let config = load_with_env(&ConfigOverrides::default(), env(&[])).unwrap();

    assert_eq!(config.vault.backend, VaultBackend::Sqlite);
    assert_eq!(config.vault.sqlite_path, PathBuf::from("vault.db"));
    assert_eq!(config.log.backend, LogBackend::Sqlite);
    assert_eq!(config.log.sqlite_path, PathBuf::from("log.db"));
    assert!(!config.log.enabled);
    assert!(config.consent.enabled);
    assert_eq!(config.consent.backend, ConsentBackend::Sqlite);
    assert_eq!(config.consent.sqlite_path, PathBuf::from("consent.db"));
    assert_eq!(config.consent.default_ttl_seconds, 86_400);
    assert!(config.consent.mcp_write_enabled);
    assert_eq!(
        config
            .traffic
            .profile
            .apps
            .iter()
            .map(|app| app.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "openai-api",
            "anthropic-api",
            "claude-web",
            "anthropic-console",
            "claude-mcp-proxy",
            "claude-platform",
            "openai-platform",
            "chatgpt-web",
            "chatgpt-legacy-web"
        ]
    );
    assert!(config.traffic.enabled_app_ids.is_none());
    assert_eq!(config.web.addr, "127.0.0.1:2896");
    assert!(!config.proxy.enabled);
    assert_eq!(config.proxy.listen, "127.0.0.1:7828");
    assert_eq!(config.proxy.mode, ProxyMode::ReverseProxy);
    assert_eq!(
        config.proxy.default_failure_mode,
        ProxyFailureMode::BypassOnError
    );
    assert!(config.policy.deduplicate_replacements);
    assert!(config.proxy.resolve_inbound);
    assert!(config.proxy.targets.is_empty());
}

#[test]
fn explicit_missing_config_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let overrides = ConfigOverrides {
        config_path: Some(dir.path().join("missing.toml")),
        ..ConfigOverrides::default()
    };

    let error = load_with_env(&overrides, env(&[])).unwrap_err();

    assert!(matches!(error, ConfigError::ConfigNotFound(_)));
}

#[test]
fn config_file_values_are_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    fs::write(
        dir.path().join("traffic-profile.json"),
        r#"
        {
          "version": 1,
          "default_action": "bypass",
          "apps": [
            {
              "id": "mail-example",
              "match": {
                "domains": ["mail.example.com"],
                "ports": [443],
                "protocols": ["https"]
              },
              "action": "inspect",
              "adapter": "email_imap",
              "provider": "imap",
              "target_name": "mail-example",
              "upstream": "https://mail.example.com",
              "steps": [
                {"id": "detect", "kind": "detect_sensitive_data", "direction": "both"}
              ]
            }
          ]
        }
        "#,
    )
    .unwrap();
    fs::write(
        &config_path,
        r#"
            [vault]
            backend = "sqlite"
            path = "file-vault.db"

            [log]
            enabled = true
            backend = "sqlite"
            path = "file-log.db"

            [consent]
            enabled = true
            backend = "sqlite"
            path = "file-consent.db"
            default_ttl_seconds = 3600
            mcp_write_enabled = false

            [policy]
            default_action = "redact"
            deduplicate_replacements = false

            [policy.kind.email]
            action = "tokenize"

            [policy.kind.cc]
            action = "block"

            [failure]
            vault_write = "redact_only"
            log_write = "warn_continue"

            [traffic]
            profile_path = "traffic-profile.json"
            enabled_apps = ["mail-example"]

            [web]
            addr = "127.0.0.1:9000"

            [proxy]
            enabled = true
            listen = "127.0.0.1:9828"
            mode = "reverse_proxy"
            default_failure_mode = "block_on_error"
            resolve_inbound = false

            [[proxy.targets]]
            name = "local-openai"
            provider = "openai-compatible"
            upstream = "http://127.0.0.1:9999"
            failure_mode = "bypass_on_error"
            api_key_env = "TEST_OPENAI_KEY"
        "#,
    )
    .unwrap();

    let config = load_with_env(
        &ConfigOverrides {
            config_path: Some(config_path),
            ..ConfigOverrides::default()
        },
        env(&[]),
    )
    .unwrap();

    assert_eq!(config.vault.sqlite_path, PathBuf::from("file-vault.db"));
    assert!(config.log.enabled);
    assert_eq!(config.log.sqlite_path, PathBuf::from("file-log.db"));
    assert_eq!(config.consent.sqlite_path, PathBuf::from("file-consent.db"));
    assert_eq!(config.consent.default_ttl_seconds, 3600);
    assert!(!config.consent.mcp_write_enabled);
    assert_eq!(config.policy.default_action, PolicyAction::Redact);
    assert!(!config.policy.deduplicate_replacements);
    assert_eq!(
        config.policy.kind_actions.get(&SensitiveType::Email),
        Some(&PolicyAction::Tokenize)
    );
    assert_eq!(
        config.policy.kind_actions.get(&SensitiveType::CreditCard),
        Some(&PolicyAction::Block)
    );
    assert_eq!(
        config.traffic.profile_path,
        Some(dir.path().join("traffic-profile.json"))
    );
    assert_eq!(
        config.traffic.enabled_app_ids,
        Some(vec!["mail-example".to_string()])
    );
    assert_eq!(
        dam_net::traffic_routes_from_profile(&config.traffic.effective_profile())[0].host,
        "mail.example.com"
    );
    assert_eq!(config.web.addr, "127.0.0.1:9000");
    assert!(config.proxy.enabled);
    assert_eq!(config.proxy.listen, "127.0.0.1:9828");
    assert_eq!(
        config.proxy.default_failure_mode,
        ProxyFailureMode::BlockOnError
    );
    assert!(!config.proxy.resolve_inbound);
    assert_eq!(config.proxy.targets.len(), 1);
    assert_eq!(config.proxy.targets[0].name, "local-openai");
    assert_eq!(config.proxy.targets[0].provider, "openai-compatible");
    assert_eq!(config.proxy.targets[0].upstream, "http://127.0.0.1:9999");
    assert_eq!(
        config.proxy.targets[0].failure_mode,
        Some(ProxyFailureMode::BypassOnError)
    );
    assert_eq!(
        config.proxy.targets[0].api_key_env,
        Some("TEST_OPENAI_KEY".to_string())
    );
}

#[test]
fn env_overrides_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    fs::write(
        &config_path,
        r#"
            [vault]
            path = "file-vault.db"

            [log]
            enabled = false
            path = "file-log.db"
        "#,
    )
    .unwrap();

    let config = load_with_env(
        &ConfigOverrides {
            config_path: Some(config_path),
            ..ConfigOverrides::default()
        },
        env(&[
            ("DAM_VAULT_PATH", "env-vault.db"),
            ("DAM_LOG_ENABLED", "true"),
            ("DAM_LOG_PATH", "env-log.db"),
            ("DAM_CONSENT_ENABLED", "true"),
            ("DAM_CONSENT_PATH", "env-consent.db"),
            ("DAM_CONSENT_DEFAULT_TTL_SECONDS", "7200"),
            ("DAM_CONSENT_MCP_WRITE_ENABLED", "false"),
            ("DAM_POLICY_DEDUPLICATE_REPLACEMENTS", "false"),
            ("DAM_TRAFFIC_ENABLED_APPS", "anthropic-api, chatgpt-web"),
            ("DAM_PROXY_ENABLED", "true"),
            ("DAM_PROXY_LISTEN", "127.0.0.1:8828"),
            ("DAM_PROXY_DEFAULT_FAILURE_MODE", "block_on_error"),
            ("DAM_PROXY_RESOLVE_INBOUND", "false"),
            ("DAM_PROXY_TARGET_UPSTREAM", "http://127.0.0.1:9999"),
            ("DAM_PROXY_TARGET_API_KEY_ENV", "TEST_KEY"),
            ("TEST_KEY", "secret-value"),
        ]),
    )
    .unwrap();

    assert_eq!(config.vault.sqlite_path, PathBuf::from("env-vault.db"));
    assert!(config.log.enabled);
    assert_eq!(config.log.sqlite_path, PathBuf::from("env-log.db"));
    assert_eq!(config.consent.sqlite_path, PathBuf::from("env-consent.db"));
    assert_eq!(config.consent.default_ttl_seconds, 7200);
    assert!(!config.consent.mcp_write_enabled);
    assert!(!config.policy.deduplicate_replacements);
    assert_eq!(
        config.traffic.enabled_app_ids,
        Some(vec!["anthropic-api".to_string(), "chatgpt-web".to_string()])
    );
    assert!(config.proxy.enabled);
    assert_eq!(config.proxy.listen, "127.0.0.1:8828");
    assert_eq!(
        config.proxy.default_failure_mode,
        ProxyFailureMode::BlockOnError
    );
    assert!(!config.proxy.resolve_inbound);
    assert_eq!(config.proxy.targets.len(), 1);
    assert_eq!(config.proxy.targets[0].name, "openai");
    assert_eq!(config.proxy.targets[0].upstream, "http://127.0.0.1:9999");
    assert_eq!(
        config.proxy.targets[0]
            .api_key
            .as_ref()
            .map(|key| key.expose()),
        Some("secret-value")
    );
}

#[test]
fn proxy_target_env_does_not_require_upstream_when_proxy_disabled() {
    let config = load_with_env(
        &ConfigOverrides::default(),
        env(&[("DAM_PROXY_TARGET_API_KEY_ENV", "TEST_KEY")]),
    )
    .unwrap();

    assert!(!config.proxy.enabled);
    assert_eq!(config.proxy.targets.len(), 1);
    assert_eq!(config.proxy.targets[0].upstream, "https://api.openai.com");
    assert_eq!(
        config.proxy.targets[0].api_key_env,
        Some("TEST_KEY".to_string())
    );
}

#[test]
fn enabled_proxy_requires_target_upstream() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    fs::write(
        &config_path,
        r#"
            [proxy]
            enabled = true
            listen = "127.0.0.1:7828"

            [[proxy.targets]]
            name = "missing-upstream"
            provider = "custom"
        "#,
    )
    .unwrap();
    let error = load_with_env(
        &ConfigOverrides {
            config_path: Some(config_path),
            ..ConfigOverrides::default()
        },
        env(&[]),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ConfigError::MissingRequired {
            field: "proxy.targets.upstream"
        }
    ));
}

#[test]
fn enabled_proxy_rejects_non_http_target_upstreams() {
    let invalid_values = [
        ("api.example.test", "absolute http(s) URL"),
        ("ftp://api.example.test", "http or https"),
        ("https:///missing-host", "host"),
        ("https://:443", "host"),
        ("https://api.example.test ", "whitespace"),
    ];

    for (upstream, expected_message) in invalid_values {
        let error = load_with_env(
            &ConfigOverrides::default(),
            env(&[
                ("DAM_PROXY_ENABLED", "true"),
                ("DAM_PROXY_TARGET_UPSTREAM", upstream),
            ]),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ConfigError::InvalidValue {
                field: "proxy.targets.upstream",
                ..
            }
        ));
        assert!(
            error.to_string().contains(expected_message),
            "error {error} did not mention {expected_message}"
        );
    }
}

#[test]
fn local_web_and_proxy_addresses_must_be_loopback() {
    let web_error = load_with_env(
        &ConfigOverrides {
            web_addr: Some("0.0.0.0:2896".to_string()),
            ..ConfigOverrides::default()
        },
        env(&[]),
    )
    .unwrap_err();
    assert!(matches!(
        web_error,
        ConfigError::InvalidValue {
            field: "web.addr",
            ..
        }
    ));

    let proxy_error = load_with_env(
        &ConfigOverrides {
            proxy_listen: Some("0.0.0.0:7828".to_string()),
            ..ConfigOverrides::default()
        },
        env(&[]),
    )
    .unwrap_err();
    assert!(matches!(
        proxy_error,
        ConfigError::InvalidValue {
            field: "proxy.listen",
            ..
        }
    ));
}

#[test]
fn enabled_proxy_accepts_multiple_targets_and_provider_labels() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    fs::write(
        &config_path,
        r#"
            [proxy]
            enabled = true
            listen = "127.0.0.1:7828"

            [[proxy.targets]]
            name = "one"
            provider = "openai-compatible"
            upstream = "https://one.example.test"

            [[proxy.targets]]
            name = "two"
            provider = "anthropic"
            upstream = "https://two.example.test"
        "#,
    )
    .unwrap();
    let config = load_with_env(
        &ConfigOverrides {
            config_path: Some(config_path),
            ..ConfigOverrides::default()
        },
        env(&[]),
    )
    .unwrap();
    assert_eq!(config.proxy.targets.len(), 2);
    assert_eq!(config.proxy.targets[0].name, "one");
    assert_eq!(config.proxy.targets[1].provider, "anthropic");

    let config = load_with_env(
        &ConfigOverrides::default(),
        env(&[
            ("DAM_PROXY_ENABLED", "true"),
            ("DAM_PROXY_TARGET_PROVIDER", "openai-compatible-typo"),
            ("DAM_PROXY_TARGET_UPSTREAM", "https://api.example.test"),
        ]),
    )
    .unwrap();
    assert_eq!(config.proxy.targets[0].provider, "openai-compatible-typo");
}

#[test]
fn generic_http_provider_is_valid_for_proxy_targets() {
    let config = load_with_env(
        &ConfigOverrides::default(),
        env(&[
            ("DAM_PROXY_ENABLED", "true"),
            ("DAM_PROXY_TARGET_NAME", "example"),
            ("DAM_PROXY_TARGET_PROVIDER", "generic-http"),
            ("DAM_PROXY_TARGET_UPSTREAM", "https://example.test"),
        ]),
    )
    .unwrap();

    assert_eq!(config.proxy.targets[0].provider, "generic-http");
    assert_eq!(config.proxy.targets[0].api_key_env, None);
}

#[test]
fn traffic_profile_json_models_private_ai_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    fs::write(
        dir.path().join("enterprise-traffic.json"),
        r#"
        {
          "version": 1,
          "default_action": "bypass",
          "apps": [
            {
              "id": "enterprise-ai",
              "match": {
                "domains": ["api.enterprise-ai.example"],
                "ports": [443],
                "protocols": ["https", "web_socket"]
              },
              "action": "inspect",
              "adapter": "http",
              "provider": "openai-compatible",
              "target_name": "enterprise-ai",
              "upstream": "https://api.enterprise-ai.example",
              "steps": [
                {"id": "detect", "kind": "detect_sensitive_data", "direction": "outbound"},
                {"id": "tokenize", "kind": "replace_sensitive_data", "direction": "outbound"},
                {"id": "resolve", "kind": "resolve_references", "direction": "inbound"}
              ]
            }
          ]
        }
        "#,
    )
    .unwrap();
    fs::write(
        &config_path,
        r#"
            [traffic]
            profile_path = "enterprise-traffic.json"
            enabled_apps = ["enterprise-ai"]
        "#,
    )
    .unwrap();

    let config = load_with_env(
        &ConfigOverrides {
            config_path: Some(config_path),
            ..ConfigOverrides::default()
        },
        env(&[]),
    )
    .unwrap();
    let routes = dam_net::traffic_routes_from_profile(&config.traffic.effective_profile());

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].host, "api.enterprise-ai.example");
    assert_eq!(routes[0].provider, "openai-compatible");
    assert_eq!(routes[0].target_name, "enterprise-ai");
    assert_eq!(routes[0].upstream, "https://api.enterprise-ai.example");
}

#[test]
fn traffic_enabled_apps_reject_unknown_profile_app_ids() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    fs::write(
        &config_path,
        r#"
            [traffic]
            enabled_apps = ["typo-id"]
        "#,
    )
    .unwrap();

    let error = load_with_env(
        &ConfigOverrides {
            config_path: Some(config_path),
            ..ConfigOverrides::default()
        },
        env(&[]),
    )
    .unwrap_err();

    assert!(error.to_string().contains("enabled app id typo-id"));
}

#[test]
fn network_ai_routes_config_is_rejected_with_profile_migration_message() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("dam.toml");
    fs::write(
        &config_path,
        r#"
            [[network.ai_routes]]
            host = "api.enterprise-ai.example"
            provider = "openai-compatible"
            target_name = "enterprise-ai"
            upstream = "https://api.enterprise-ai.example"
        "#,
    )
    .unwrap();
    let error = load_with_env(
        &ConfigOverrides {
            config_path: Some(config_path),
            ..ConfigOverrides::default()
        },
        env(&[]),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ConfigError::InvalidValue {
            field: "network.ai_routes",
            ..
        }
    ));
    assert!(error.to_string().contains("traffic profile JSON apps"));
}

#[test]
fn cli_overrides_env() {
    let config = load_with_env(
        &ConfigOverrides {
            vault_sqlite_path: Some(PathBuf::from("cli-vault.db")),
            log_sqlite_path: Some(PathBuf::from("cli-log.db")),
            log_enabled: Some(false),
            consent_sqlite_path: Some(PathBuf::from("cli-consent.db")),
            consent_enabled: Some(false),
            web_addr: Some("127.0.0.1:9999".to_string()),
            proxy_enabled: Some(true),
            proxy_listen: Some("127.0.0.1:7777".to_string()),
            proxy_resolve_inbound: Some(false),
            proxy_target_upstream: Some("http://127.0.0.1:9998".to_string()),
            proxy_target_failure_mode: Some(ProxyFailureMode::BypassOnError),
            ..ConfigOverrides::default()
        },
        env(&[
            ("DAM_VAULT_PATH", "env-vault.db"),
            ("DAM_LOG_ENABLED", "true"),
            ("DAM_LOG_PATH", "env-log.db"),
            ("DAM_WEB_ADDR", "127.0.0.1:9000"),
        ]),
    )
    .unwrap();

    assert_eq!(config.vault.sqlite_path, PathBuf::from("cli-vault.db"));
    assert_eq!(config.log.sqlite_path, PathBuf::from("cli-log.db"));
    assert!(!config.log.enabled);
    assert_eq!(config.consent.sqlite_path, PathBuf::from("cli-consent.db"));
    assert!(!config.consent.enabled);
    assert_eq!(config.web.addr, "127.0.0.1:9999");
    assert!(config.proxy.enabled);
    assert_eq!(config.proxy.listen, "127.0.0.1:7777");
    assert!(!config.proxy.resolve_inbound);
    assert_eq!(config.proxy.targets[0].upstream, "http://127.0.0.1:9998");
    assert_eq!(
        config.proxy.targets[0].failure_mode,
        Some(ProxyFailureMode::BypassOnError)
    );
}

#[test]
fn remote_vault_requires_url_and_token() {
    let error = load_with_env(
        &ConfigOverrides::default(),
        env(&[("DAM_VAULT_BACKEND", "remote")]),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ConfigError::MissingRequired { field: "vault.url" }
    ));

    let error = load_with_env(
        &ConfigOverrides::default(),
        env(&[
            ("DAM_VAULT_BACKEND", "remote"),
            ("DAM_VAULT_URL", "https://vault.example.test"),
        ]),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ConfigError::MissingRequired {
            field: "vault.token"
        }
    ));
}

#[test]
fn remote_secrets_are_resolved_from_env_and_redacted_in_debug() {
    let config = load_with_env(
        &ConfigOverrides::default(),
        env(&[
            ("DAM_VAULT_BACKEND", "remote"),
            ("DAM_VAULT_URL", "https://vault.example.test"),
            ("DAM_VAULT_TOKEN", "super-secret-token"),
        ]),
    )
    .unwrap();

    let token = config.vault.token.as_ref().unwrap();
    assert_eq!(token.env_var(), "DAM_VAULT_TOKEN");
    assert_eq!(token.expose(), "super-secret-token");

    let debug = format!("{config:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("super-secret-token"));
}

#[test]
fn env_values_are_case_insensitive() {
    let config = load_with_env(
        &ConfigOverrides::default(),
        env(&[
            ("DAM_LOG_ENABLED", "TRUE"),
            ("DAM_LOG_BACKEND", "SQLITE"),
            ("DAM_POLICY_DEFAULT_ACTION", "REDACT"),
            ("DAM_POLICY_SSN_ACTION", "BLOCK"),
            ("DAM_FAILURE_LOG_WRITE", "WARN_CONTINUE"),
        ]),
    )
    .unwrap();

    assert!(config.log.enabled);
    assert_eq!(config.log.backend, LogBackend::Sqlite);
    assert_eq!(config.policy.default_action, PolicyAction::Redact);
    assert_eq!(
        config.policy.kind_actions.get(&SensitiveType::Ssn),
        Some(&PolicyAction::Block)
    );
    assert_eq!(config.failure.log_write, LogWriteFailureMode::WarnContinue);
}
