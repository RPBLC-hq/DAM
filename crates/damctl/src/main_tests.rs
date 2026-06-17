use super::*;
use axum::{Json, Router, routing::get};
use tokio::net::TcpListener;

async fn spawn_health(report: dam_api::ProxyReport) -> String {
    async fn health_from_extension(
        axum::Extension(report): axum::Extension<dam_api::ProxyReport>,
    ) -> Json<dam_api::ProxyReport> {
        Json(report)
    }

    let app = Router::new().route("/health", get(health_from_extension));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app.layer(axum::Extension(report)))
            .await
            .unwrap();
    });
    format!("http://{addr}")
}

fn write_config(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("dam.toml");
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn parse_status_accepts_proxy_url_and_json() {
    let command = parse_args([
        "status".to_string(),
        "--proxy-url".to_string(),
        "http://127.0.0.1:7828".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Status(StatusArgs {
            common: CommonArgs {
                json: true,
                ..CommonArgs::default()
            },
            proxy_url: Some("http://127.0.0.1:7828".to_string()),
        })
    );
}

#[test]
fn parse_doctor_accepts_config_proxy_url_and_json() {
    let command = parse_args([
        "doctor".to_string(),
        "--config".to_string(),
        "/tmp/dam.toml".to_string(),
        "--proxy-url".to_string(),
        "http://127.0.0.1:7828".to_string(),
        "--state-dir".to_string(),
        "/tmp/dam-state".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Doctor(DoctorArgs {
            common: CommonArgs {
                config: dam_config::ConfigOverrides {
                    config_path: Some(PathBuf::from("/tmp/dam.toml")),
                    ..dam_config::ConfigOverrides::default()
                },
                json: true,
            },
            proxy_url: Some("http://127.0.0.1:7828".to_string()),
            state_dir: Some(PathBuf::from("/tmp/dam-state")),
        })
    );
}

#[test]
fn parse_bypass_status_accepts_config_and_json() {
    let command = parse_args([
        "bypass".to_string(),
        "status".to_string(),
        "--config".to_string(),
        "/tmp/dam.toml".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Bypass(BypassArgs {
            command: BypassCommand::Status(BypassStatusArgs {
                common: CommonArgs {
                    config: dam_config::ConfigOverrides {
                        config_path: Some(PathBuf::from("/tmp/dam.toml")),
                        ..dam_config::ConfigOverrides::default()
                    },
                    json: true,
                }
            })
        })
    );
}

#[test]
fn parse_daemon_inspect_accepts_state_dir_and_json() {
    let command = parse_args([
        "daemon".to_string(),
        "inspect".to_string(),
        "--state-dir".to_string(),
        "/tmp/dam-state".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Daemon(DaemonArgs {
            command: DaemonCommand::Inspect(DaemonInspectArgs {
                json: true,
                state_dir: Some(PathBuf::from("/tmp/dam-state")),
            })
        })
    );
}

#[test]
fn parse_trust_inspect_accepts_state_dir_and_json() {
    let command = parse_args([
        "trust".to_string(),
        "inspect".to_string(),
        "--state-dir".to_string(),
        "/tmp/dam-state".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Trust(TrustArgs {
            command: TrustCommand::Inspect(TrustInspectArgs {
                json: true,
                state_dir: Some(PathBuf::from("/tmp/dam-state")),
            })
        })
    );
}

#[test]
fn parse_network_inspect_accepts_state_dir_and_json() {
    let command = parse_args([
        "network".to_string(),
        "inspect".to_string(),
        "--state-dir".to_string(),
        "/tmp/dam-state".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Network(NetworkArgs {
            command: NetworkCommand::Inspect(NetworkInspectArgs {
                json: true,
                state_dir: Some(PathBuf::from("/tmp/dam-state")),
                config_path: None,
            })
        })
    );
}

#[test]
fn parse_network_inspect_accepts_config() {
    let command = parse_args([
        "network".to_string(),
        "inspect".to_string(),
        "--config".to_string(),
        "dam.enterprise.toml".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Network(NetworkArgs {
            command: NetworkCommand::Inspect(NetworkInspectArgs {
                json: false,
                state_dir: None,
                config_path: Some(PathBuf::from("dam.enterprise.toml")),
            })
        })
    );
}

#[test]
fn parse_setup_plan_accepts_modes_state_config_proxy_and_json() {
    let command = parse_args([
        "setup".to_string(),
        "plan".to_string(),
        "--config".to_string(),
        "/tmp/dam.toml".to_string(),
        "--state-dir".to_string(),
        "/tmp/dam-state".to_string(),
        "--proxy-url".to_string(),
        "http://127.0.0.1:9000".to_string(),
        "--network-mode".to_string(),
        "system-proxy".to_string(),
        "--trust-mode".to_string(),
        "local-ca".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Setup(SetupArgs {
            command: SetupCommand::Plan(SetupPlanArgs {
                common: CommonArgs {
                    config: dam_config::ConfigOverrides {
                        config_path: Some(PathBuf::from("/tmp/dam.toml")),
                        ..dam_config::ConfigOverrides::default()
                    },
                    json: true,
                },
                state_dir: Some(PathBuf::from("/tmp/dam-state")),
                proxy_url: Some("http://127.0.0.1:9000".to_string()),
                network_mode: dam_net::CaptureMode::SystemProxy,
                trust_mode: dam_trust::TrustMode::LocalCa,
            })
        })
    );
}

#[test]
fn parse_config_check_accepts_config() {
    let command = parse_args([
        "config".to_string(),
        "check".to_string(),
        "--config".to_string(),
        "/tmp/dam.toml".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::ConfigCheck(ConfigCheckArgs {
            common: CommonArgs {
                config: dam_config::ConfigOverrides {
                    config_path: Some(PathBuf::from("/tmp/dam.toml")),
                    ..dam_config::ConfigOverrides::default()
                },
                json: false,
            }
        })
    );
}

#[test]
fn parse_integrations_check_accepts_profile_proxy_target_and_json() {
    let command = parse_args([
        "integrations".to_string(),
        "check".to_string(),
        "codex".to_string(),
        "--proxy-url".to_string(),
        "http://127.0.0.1:9000".to_string(),
        "--target-path".to_string(),
        "/tmp/codex.toml".to_string(),
        "--json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Integrations(IntegrationsArgs {
            command: IntegrationsCommand::Check(IntegrationsCheckArgs {
                profile_id: Some("codex".to_string()),
                json: true,
                proxy_url: Some("http://127.0.0.1:9000".to_string()),
                target_path: Some(PathBuf::from("/tmp/codex.toml")),
                state_dir: None,
            })
        })
    );
}

#[test]
fn parse_mcp_config_accepts_config() {
    let command = parse_args([
        "mcp".to_string(),
        "config".to_string(),
        "--config".to_string(),
        "/tmp/dam.toml".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::McpConfig(McpConfigArgs {
            config_path: Some(PathBuf::from("/tmp/dam.toml")),
        })
    );
}

#[test]
fn setup_plan_reports_next_action_without_mutating_state() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_config(
        dir.path(),
        r#"
        [proxy]
        enabled = true
        listen = "127.0.0.1:7828"

        [[proxy.targets]]
        name = "openai"
        provider = "openai-compatible"
        upstream = "https://api.openai.com"
        "#,
    );
    let state_dir = dir.path().join("state");

    let output = setup_plan(SetupPlanArgs {
        common: CommonArgs {
            config: dam_config::ConfigOverrides {
                config_path: Some(config_path),
                ..dam_config::ConfigOverrides::default()
            },
            json: true,
        },
        state_dir: Some(state_dir.clone()),
        ..SetupPlanArgs::default()
    });

    assert_eq!(output.code, 1);
    assert!(!state_dir.exists());
    let report: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report["state"], "needs_action");
    let first_needed = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["status"] == "needed")
        .unwrap();
    assert_eq!(first_needed["kind"], "daemon");
    assert_eq!(first_needed["status"], "needed");
}

#[test]
fn mcp_config_outputs_dam_mcp_server() {
    let output = mcp_config(McpConfigArgs {
        config_path: Some(PathBuf::from("dam.toml")),
    });

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("\"command\": \"dam-mcp\""));
    assert!(output.stdout.contains("\"--config\""));
}

#[test]
fn bypass_status_reports_reduced_modes() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_config(
        dir.path(),
        r#"
        [failure]
        vault_write = "redact_only"
        log_write = "warn_continue"

        [proxy]
        enabled = true
        listen = "127.0.0.1:7828"
        default_failure_mode = "bypass_on_error"

        [[proxy.targets]]
        name = "openai"
        provider = "openai-compatible"
        upstream = "https://api.openai.com"
        "#,
    );

    let output = bypass_status(BypassStatusArgs {
        common: CommonArgs {
            config: dam_config::ConfigOverrides {
                config_path: Some(path),
                ..dam_config::ConfigOverrides::default()
            },
            json: true,
        },
    });

    assert_eq!(output.code, 1);
    let report: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report["state"], "reduced");
    assert_eq!(report["reduced_guarantees"], true);
    assert_eq!(report["proxy_default_failure_mode"], "bypass_on_error");
    assert_eq!(report["proxy_targets"].as_array().unwrap().len(), 1);
    assert_eq!(report["proxy_targets"][0]["reduced_guarantee"], true);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic["code"] == "proxy_bypass_on_error"
            && diagnostic["message"]
                .as_str()
                .unwrap()
                .contains("unprotected traffic")
    }));
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "vault_redact_only")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "log_warn_continue")
    );
}

#[test]
fn bypass_status_reports_strict_modes() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_config(
        dir.path(),
        r#"
        [failure]
        vault_write = "fail_closed"
        log_write = "fail_closed"

        [proxy]
        enabled = true
        listen = "127.0.0.1:7828"
        default_failure_mode = "block_on_error"

        [[proxy.targets]]
        name = "openai"
        provider = "openai-compatible"
        upstream = "https://api.openai.com"
        "#,
    );

    let output = bypass_status(BypassStatusArgs {
        common: CommonArgs {
            config: dam_config::ConfigOverrides {
                config_path: Some(path),
                ..dam_config::ConfigOverrides::default()
            },
            json: false,
        },
    });

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("state: strict"));
    assert!(output.stdout.contains("reduced_guarantees: false"));
    assert!(
        output
            .stdout
            .contains("proxy_default_failure_mode: block_on_error")
    );
    assert!(
        output
            .stdout
            .contains("vault_write_failure_mode: fail_closed")
    );
    assert!(
        output
            .stdout
            .contains("log_write_failure_mode: fail_closed")
    );
}

#[test]
fn daemon_inspect_reports_disconnected_state() {
    let dir = tempfile::tempdir().unwrap();
    let output = daemon_inspect(DaemonInspectArgs {
        json: true,
        state_dir: Some(dir.path().join("state")),
    });

    assert_eq!(output.code, 0);
    let report: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report["state"], "disconnected");
    assert_eq!(report["process_running"], serde_json::Value::Null);
    assert!(
        report["state_file"]
            .as_str()
            .unwrap()
            .ends_with("daemon.json")
    );
}

#[test]
fn daemon_inspect_reports_stale_state() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let state = test_daemon_state(9_999_999);
    dam_daemon::write_state_to(&state_dir.join("daemon.json"), &state).unwrap();

    let output = daemon_inspect(DaemonInspectArgs {
        json: false,
        state_dir: Some(state_dir.clone()),
    });

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("state: stale"));
    assert!(output.stdout.contains("process: not_running"));
    assert!(output.stdout.contains("pid: 9999999"));
    assert!(
        output
            .stdout
            .contains(&format!("state_dir: {}", state_dir.display()))
    );
}

#[test]
fn daemon_inspect_reports_connected_state() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let state = test_daemon_state(std::process::id());
    dam_daemon::write_state_to(&state_dir.join("daemon.json"), &state).unwrap();

    let output = daemon_inspect(DaemonInspectArgs {
        json: false,
        state_dir: Some(state_dir),
    });

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("state: connected"));
    assert!(output.stdout.contains("process: running"));
    assert!(output.stdout.contains("target: openai"));
    assert!(output.stdout.contains("network_mode: explicit_proxy"));
    assert!(output.stdout.contains("transparent_routes: 10"));
    assert!(output.stdout.contains("routing_routes: 10"));
    assert!(output.stdout.contains(
        "routing_route openai: ready - explicit proxy routing is active for clients configured to use DAM"
    ));
    assert!(output.stdout.contains("trust_mode: disabled"));
    assert!(output.stdout.contains("local_ca_installed: false"));
    assert!(output.stdout.contains("trusted_hosts: 10"));
    assert!(output.stdout.contains("trust_routes: 10"));
    assert!(
        output
            .stdout
            .contains("trust_route openai: disabled - TLS interception is disabled")
    );
    assert!(output.stdout.contains("interception_routes: 10"));
    assert!(output.stdout.contains(
        "interception_route openai: needs_user_consent - TLS interception requires explicit user approval"
    ));
    assert!(output.stdout.contains("provider: openai-compatible"));
    assert!(output.stdout.contains("upstream: https://api.openai.com"));
    assert!(output.stdout.contains("log: disabled"));
    assert!(output.stdout.contains("resolve_inbound: false"));
}

#[test]
fn trust_inspect_reports_default_read_only_state() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");

    let output = trust_inspect(TrustInspectArgs {
        json: false,
        state_dir: Some(state_dir),
    });

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("state: inspectable"));
    assert!(output.stdout.contains("source: default"));
    assert!(output.stdout.contains("trust_mode: disabled"));
    assert!(output.stdout.contains("local_ca_installed: false"));
    assert!(output.stdout.contains("local_ca_artifact: missing"));
    assert!(output.stdout.contains("trust_routes: 10"));
    assert!(output.stdout.contains("action inspect: implemented"));
    let expected_install_support = match dam_trust::PlatformTrustStore::current() {
        dam_trust::PlatformTrustStore::MacosKeychain
        | dam_trust::PlatformTrustStore::LinuxNssOrSystemStore => "implemented",
        dam_trust::PlatformTrustStore::WindowsRootStore
        | dam_trust::PlatformTrustStore::Unknown => "planned",
    };
    assert!(output.stdout.contains(&format!(
        "action install_local_ca: {expected_install_support}"
    )));
}

#[test]
fn trust_inspect_reports_local_ca_artifact_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let artifact = dam_trust::generate_local_ca_artifact_at(&state_dir, 1).unwrap();

    let output = trust_inspect(TrustInspectArgs {
        json: true,
        state_dir: Some(state_dir),
    });

    assert_eq!(output.code, 0);
    let report: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report["source"], "default");
    assert_eq!(
        report["trust"]["local_ca"]["id"],
        serde_json::Value::String(artifact.record.id)
    );
    assert_eq!(
        report["local_ca_artifact"]["record"]["installed_at_unix"],
        serde_json::Value::Null
    );
    assert_eq!(report["route_readiness"].as_array().unwrap().len(), 10);
}

#[test]
fn trust_inspect_uses_daemon_trust_state_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let mut state = test_daemon_state(std::process::id());
    state.trust.mode = dam_trust::TrustMode::LocalCa;
    dam_daemon::write_state_to(&state_dir.join("daemon.json"), &state).unwrap();

    let output = trust_inspect(TrustInspectArgs {
        json: true,
        state_dir: Some(state_dir),
    });

    assert_eq!(output.code, 0);
    let report: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report["source"], "daemon");
    assert_eq!(report["trust"]["mode"], "local_ca");
    assert_eq!(report["route_readiness"].as_array().unwrap().len(), 10);
    assert_eq!(report["actions"].as_array().unwrap().len(), 3);
}

#[test]
fn network_inspect_reports_not_installed_state() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");

    let output = network_inspect(NetworkInspectArgs {
        json: false,
        state_dir: Some(state_dir.clone()),
        config_path: None,
    });

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("state: not_installed"));
    assert!(output.stdout.contains("system_proxy_installed: false"));
    assert!(output.stdout.contains("configured_hosts: 10"));
    assert!(
        output
            .stdout
            .contains("routing_route openai: needs_system_proxy_install")
    );
    assert!(
        output
            .stdout
            .contains(&format!("state_dir: {}", state_dir.display()))
    );
}

#[test]
fn network_inspect_reports_installed_state_from_rollback_record() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let network_paths = dam_net_macos::MacosNetworkPaths::for_state_dir(&state_dir);
    std::fs::create_dir_all(&network_paths.directory).unwrap();
    std::fs::write(&network_paths.rollback_path, "{}").unwrap();

    let output = network_inspect(NetworkInspectArgs {
        json: true,
        state_dir: Some(state_dir),
        config_path: None,
    });

    assert_eq!(output.code, 0);
    let report: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report["state"], "installed");
    assert_eq!(report["system_proxy_installed"], true);
    assert_eq!(report["configured_hosts"].as_array().unwrap().len(), 10);
    assert_eq!(report["route_readiness"].as_array().unwrap().len(), 10);
    assert_eq!(report["route_readiness"][0]["readiness"], "ready");
}

#[test]
fn network_inspect_uses_configured_traffic_profile_routes() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let config_path = dir.path().join("dam.toml");
    std::fs::write(
        dir.path().join("enterprise-traffic.json"),
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
    std::fs::write(
        &config_path,
        r#"
            [traffic]
            profile_path = "enterprise-traffic.json"
        "#,
    )
    .unwrap();

    let output = network_inspect(NetworkInspectArgs {
        json: true,
        state_dir: Some(state_dir),
        config_path: Some(config_path),
    });

    assert_eq!(output.code, 0);
    let report: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report["configured_hosts"].as_array().unwrap().len(), 1);
    assert!(
        report["configured_hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| host == "api.enterprise-ai.example")
    );
    assert!(
        report["route_readiness"]
            .as_array()
            .unwrap()
            .iter()
            .any(|route| route["route"]["target_name"] == "enterprise-ai")
    );
}

#[test]
fn integrations_check_reports_specific_missing_profile_as_needs_apply() {
    let dir = tempfile::tempdir().unwrap();
    let output = integrations_check(IntegrationsCheckArgs {
        profile_id: Some("claude".to_string()),
        json: true,
        proxy_url: Some("http://127.0.0.1:9000".to_string()),
        target_path: Some(dir.path().join("claude.json")),
        state_dir: Some(dir.path().join("state")),
    });

    assert_eq!(output.code, 1);
    let report: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report["proxy_url"], "http://127.0.0.1:9000");
    assert_eq!(report["profiles"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["profiles"][0]["status"],
        serde_json::Value::String("needs_apply".to_string())
    );
}

#[test]
fn integrations_check_reports_applied_profile() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state").join("integrations");
    let profile_path = dir.path().join("claude.json");
    let prepared =
        dam_integrations::prepare_apply("claude", "http://127.0.0.1:9000", profile_path.clone())
            .unwrap();
    dam_integrations::run_apply(prepared, false, &state_dir).unwrap();

    let output = integrations_check(IntegrationsCheckArgs {
        profile_id: Some("claude".to_string()),
        json: false,
        proxy_url: Some("http://127.0.0.1:9000".to_string()),
        target_path: Some(profile_path),
        state_dir: Some(dir.path().join("state")),
    });

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("profile: claude"));
    assert!(output.stdout.contains("state: applied"));
    assert!(output.stdout.contains("rollback: available"));
}

#[test]
fn integrations_check_rejects_target_path_without_profile() {
    let dir = tempfile::tempdir().unwrap();
    let output = integrations_check(IntegrationsCheckArgs {
        target_path: Some(dir.path().join("profile.env")),
        state_dir: Some(dir.path().join("state")),
        ..IntegrationsCheckArgs::default()
    });

    assert_eq!(output.code, 2);
    assert!(output.stderr.contains("--target-path can only be used"));
}

#[tokio::test]
async fn status_reports_protected_from_proxy() {
    let proxy_url = spawn_health(dam_api::ProxyReport {
        operation_id: None,
        target: Some("openai".to_string()),
        upstream: Some("http://127.0.0.1:9999".to_string()),
        state: dam_api::ProxyState::Protected,
        message: "proxy is ready".to_string(),
        diagnostics: Vec::new(),
    })
    .await;

    let output = status(StatusArgs {
        common: CommonArgs::default(),
        proxy_url: Some(proxy_url),
    })
    .await;

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("state: protected"));
    assert!(output.stdout.contains("target: openai"));
}

#[tokio::test]
async fn status_json_reports_dam_down_when_proxy_is_unreachable() {
    let output = status(StatusArgs {
        common: CommonArgs {
            json: true,
            ..CommonArgs::default()
        },
        proxy_url: Some("http://127.0.0.1:1".to_string()),
    })
    .await;

    assert_eq!(output.code, 1);
    let report: dam_api::ProxyReport = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report.state, dam_api::ProxyState::DamDown);
    assert_eq!(report.diagnostics[0].code, "dam_down");
}

#[tokio::test]
async fn doctor_json_reports_router_and_proxy_runtime() {
    let proxy_url = spawn_health(dam_api::ProxyReport {
        operation_id: None,
        target: Some("openai".to_string()),
        upstream: Some("http://127.0.0.1:9999".to_string()),
        state: dam_api::ProxyState::Protected,
        message: "proxy is ready".to_string(),
        diagnostics: Vec::new(),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let path = write_config(
        dir.path(),
        &format!(
            r#"
            [vault]
            path = "{vault}"

            [log]
            enabled = true
            path = "{log}"

            [consent]
            path = "{consent}"

            [proxy]
            enabled = true
            listen = "127.0.0.1:7828"

            [[proxy.targets]]
            name = "openai"
            provider = "openai-compatible"
            upstream = "https://api.openai.com"
            "#,
            vault = dir.path().join("vault.db").display(),
            log = dir.path().join("log.db").display(),
            consent = dir.path().join("consent.db").display(),
        ),
    );

    let output = doctor(DoctorArgs {
        common: CommonArgs {
            config: dam_config::ConfigOverrides {
                config_path: Some(path),
                ..dam_config::ConfigOverrides::default()
            },
            json: true,
        },
        proxy_url: Some(proxy_url),
        state_dir: Some(state_dir),
    })
    .await;

    assert_eq!(output.code, 0);
    let report: dam_api::HealthReport = serde_json::from_str(&output.stdout).unwrap();
    assert!(report.components.iter().any(|component| {
        component.component == "router" && component.state == dam_api::HealthState::Healthy
    }));
    assert!(report.components.iter().any(|component| {
        component.component == "proxy_runtime" && component.state == dam_api::HealthState::Healthy
    }));
    assert!(
        report
            .components
            .iter()
            .any(|component| component.component == "integrations")
    );
}

#[test]
fn config_check_reports_missing_proxy_api_key_as_unhealthy() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_config(
        dir.path(),
        r#"
        [proxy]
        enabled = true
        listen = "127.0.0.1:7828"

        [[proxy.targets]]
        name = "openai"
        provider = "openai-compatible"
        upstream = "https://api.openai.com"
        api_key_env = "MISSING_TEST_OPENAI_KEY"
        "#,
    );

    let output = config_check(ConfigCheckArgs {
        common: CommonArgs {
            config: dam_config::ConfigOverrides {
                config_path: Some(path),
                ..dam_config::ConfigOverrides::default()
            },
            json: true,
        },
    });

    assert_eq!(output.code, 1);
    let report: dam_api::HealthReport = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(report.state, dam_api::HealthState::Unhealthy);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "proxy_config_invalid"
            && diagnostic
                .message
                .contains("requires missing env var MISSING_TEST_OPENAI_KEY")
    }));
}

#[test]
fn config_check_accepts_provider_labels() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_config(
        dir.path(),
        r#"
        [proxy]
        enabled = true
        listen = "127.0.0.1:7828"

        [[proxy.targets]]
        name = "anthropic"
        provider = "anthropic"
        upstream = "https://api.anthropic.com"
        "#,
    );

    let output = config_check(ConfigCheckArgs {
        common: CommonArgs {
            config: dam_config::ConfigOverrides {
                config_path: Some(path),
                ..dam_config::ConfigOverrides::default()
            },
            json: true,
        },
    });

    assert_eq!(output.code, 0);
    let report: dam_api::HealthReport = serde_json::from_str(&output.stdout).unwrap();
    assert_ne!(report.state, dam_api::HealthState::Unhealthy);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "proxy_config_invalid")
    );
}

#[test]
fn default_config_report_is_degraded_but_not_failed() {
    let report = dam_diagnostics::config_report(&dam_config::DamConfig::default());
    let output = CommandOutput {
        code: if report.state == dam_api::HealthState::Unhealthy {
            1
        } else {
            0
        },
        stdout: render_health_report(&report),
        stderr: String::new(),
    };

    assert_eq!(output.code, 0);
    assert!(output.stdout.contains("state: degraded"));
    assert!(
        output
            .stdout
            .contains("proxy_config: degraded - proxy is disabled")
    );
}

fn test_daemon_state(pid: u32) -> dam_daemon::DaemonState {
    dam_daemon::DaemonState {
        version: 1,
        pid,
        executable_path: Some(PathBuf::from("/usr/local/bin/dam")),
        executable_sha256: Some("abc123".to_string()),
        listen: "127.0.0.1:7828".to_string(),
        proxy_url: "http://127.0.0.1:7828".to_string(),
        config_path: Some(PathBuf::from("dam.toml")),
        vault_path: PathBuf::from("vault.db"),
        log_path: None,
        consent_path: Some(PathBuf::from("consent.db")),
        resolve_inbound: false,
        target_name: Some("openai".to_string()),
        target_provider: Some("openai-compatible".to_string()),
        upstream: Some("https://api.openai.com".to_string()),
        proxy_targets: Vec::new(),
        started_at_unix: 1_700_000_000,
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
        protection_started_at_unix: Some(1_700_000_000),
    }
}
