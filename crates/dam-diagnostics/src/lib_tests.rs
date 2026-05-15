use super::*;
use axum::{Json, Router, routing::get};
use tokio::net::TcpListener;

fn proxy_config(upstream: &str, provider: &str) -> dam_config::DamConfig {
    let dir = tempfile::tempdir().unwrap().keep();
    let mut config = dam_config::DamConfig::default();
    config.vault.sqlite_path = dir.join("vault.db");
    config.log.sqlite_path = dir.join("log.db");
    config.consent.sqlite_path = dir.join("consent.db");
    config.log.enabled = true;
    config.proxy.enabled = true;
    config.proxy.targets.push(dam_config::ProxyTargetConfig {
        name: "test".to_string(),
        provider: provider.to_string(),
        upstream: upstream.to_string(),
        auth: match provider {
            "openai-compatible" => dam_net::UpstreamAuthConfig {
                caller_headers: vec!["authorization".to_string()],
                inject: Some(dam_net::UpstreamAuthInjection {
                    header: "authorization".to_string(),
                    scheme: Some("Bearer".to_string()),
                    strip_headers: vec!["authorization".to_string()],
                }),
            },
            "anthropic" => dam_net::UpstreamAuthConfig {
                caller_headers: vec!["x-api-key".to_string(), "authorization".to_string()],
                inject: Some(dam_net::UpstreamAuthInjection {
                    header: "x-api-key".to_string(),
                    scheme: None,
                    strip_headers: vec!["x-api-key".to_string(), "authorization".to_string()],
                }),
            },
            _ => dam_net::UpstreamAuthConfig::default(),
        },
        failure_mode: None,
        api_key_env: None,
        api_key: None,
    });
    config
}

async fn spawn_health(report: dam_api::ProxyReport) -> String {
    async fn health(
        axum::Extension(report): axum::Extension<dam_api::ProxyReport>,
    ) -> Json<dam_api::ProxyReport> {
        Json(report)
    }

    let app = Router::new().route("/health", get(health));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app.layer(axum::Extension(report)))
            .await
            .unwrap();
    });
    format!("http://{addr}")
}

#[test]
fn config_report_accepts_provider_labels() {
    let report = config_report(&proxy_config("https://api.anthropic.com", "anthropic"));

    assert_ne!(report.state, dam_api::HealthState::Unhealthy);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "proxy_config_invalid")
    );
}

#[test]
fn config_report_marks_missing_proxy_key_as_unhealthy() {
    let mut config = proxy_config("https://api.openai.com", "openai-compatible");
    config.proxy.targets[0].api_key_env = Some("MISSING_TEST_OPENAI_KEY".to_string());

    let report = config_report(&config);

    assert_eq!(report.state, dam_api::HealthState::Unhealthy);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "proxy_config_invalid"
            && diagnostic
                .message
                .contains("requires missing env var MISSING_TEST_OPENAI_KEY")
    }));
}

#[test]
fn config_report_marks_reduced_failure_modes_as_degraded() {
    let report = config_report(&proxy_config("https://api.openai.com", "openai-compatible"));

    assert!(report.components.iter().any(|component| {
        component.component == "failure_modes"
            && component.state == dam_api::HealthState::Degraded
            && component.message.contains("proxy default bypass_on_error")
            && component.message.contains("vault redact_only")
            && component.message.contains("log warn_continue")
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "proxy_bypass_on_error"
            && diagnostic.message.contains("unprotected traffic")
    }));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "vault_redact_only")
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "log_warn_continue")
    );
}

#[test]
fn config_report_marks_strict_failure_modes_as_healthy() {
    let mut config = proxy_config("https://api.openai.com", "openai-compatible");
    config.proxy.default_failure_mode = dam_config::ProxyFailureMode::BlockOnError;
    config.failure.vault_write = dam_config::VaultWriteFailureMode::FailClosed;
    config.failure.log_write = dam_config::LogWriteFailureMode::FailClosed;

    let report = config_report(&config);

    assert!(report.components.iter().any(|component| {
        component.component == "failure_modes"
            && component.state == dam_api::HealthState::Healthy
            && component.message == "failure modes are strict"
    }));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "proxy_bypass_on_error"
                || diagnostic.code == "vault_redact_only"
                || diagnostic.code == "log_warn_continue")
    );
}

#[test]
fn setup_rescue_previews_stale_daemon_state_without_mutating() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let state_file = state_dir.join("daemon.json");
    dam_daemon::write_state_to(&state_file, &test_daemon_state(999_999)).unwrap();

    let rescue = setup_rescue(&SetupRescueOptions {
        state_dir: Some(state_dir.clone()),
        apply: false,
        ..SetupRescueOptions::default()
    })
    .unwrap();

    assert_eq!(rescue.state, "preview");
    assert!(state_file.exists());
    let daemon_action = rescue
        .actions
        .iter()
        .find(|action| action.id == "daemon")
        .unwrap();
    assert_eq!(daemon_action.state, "would_remove_stale_state");
}

#[test]
fn setup_rescue_apply_removes_stale_daemon_state() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let state_file = state_dir.join("daemon.json");
    dam_daemon::write_state_to(&state_file, &test_daemon_state(999_999)).unwrap();

    let rescue = setup_rescue(&SetupRescueOptions {
        state_dir: Some(state_dir),
        apply: true,
        ..SetupRescueOptions::default()
    })
    .unwrap();

    assert!(!rescue.is_blocked());
    assert!(!state_file.exists());
    let daemon_action = rescue
        .actions
        .iter()
        .find(|action| action.id == "daemon")
        .unwrap();
    assert_eq!(daemon_action.state, "stale_removed");
}

#[test]
fn setup_plan_defaults_to_daemon_start_when_disconnected() {
    let dir = tempfile::tempdir().unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(dir.path().join("state")),
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    assert_eq!(plan.state, SetupPlanState::NeedsAction);
    assert!(plan.message.contains("DAM is disconnected"));
    assert_eq!(
        plan.next_action.as_ref().map(|step| step.kind),
        Some(SetupStepKind::Daemon)
    );
    assert!(
        !plan
            .steps
            .iter()
            .any(|step| step.kind == SetupStepKind::ProfileApply)
    );
    assert!(plan.steps.iter().any(|step| {
        step.kind == SetupStepKind::SystemProxy && step.status == SetupStepStatus::Skipped
    }));
    assert!(plan.steps.iter().any(|step| {
        step.kind == SetupStepKind::LocalCa && step.status == SetupStepStatus::Skipped
    }));
    assert!(plan.steps.iter().any(|step| {
        step.kind == SetupStepKind::Daemon
            && step.status == SetupStepStatus::Needed
            && step.command == Some(vec!["dam".to_string(), "connect".to_string()])
    }));
}

#[test]
fn setup_plan_does_not_block_on_profile_apply() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let integration_state_dir = state_dir.join("integrations");
    dam_integrations::set_active_profile("codex", &integration_state_dir).unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(state_dir),
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    assert!(
        !plan
            .steps
            .iter()
            .any(|step| step.kind == SetupStepKind::ProfileApply)
    );
    assert!(plan.steps.iter().any(|step| {
        step.kind == SetupStepKind::Daemon && step.status == SetupStepStatus::Needed
    }));
}

#[test]
fn setup_plan_reports_system_proxy_setup_when_requested() {
    let dir = tempfile::tempdir().unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(dir.path().join("state")),
            config_path: Some(PathBuf::from("dam.example.toml")),
            network_mode: dam_net::CaptureMode::SystemProxy,
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    let step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::SystemProxy)
        .unwrap();
    assert_eq!(step.status, SetupStepStatus::Needed);
    assert_eq!(
        step.command,
        Some(vec![
            "dam".to_string(),
            "network".to_string(),
            "install-system-proxy".to_string(),
            "--config".to_string(),
            "dam.example.toml".to_string(),
            "--yes".to_string()
        ])
    );
    assert!(step.requires_confirmation);
    assert!(step.changes_system);
}

#[cfg(target_os = "macos")]
#[test]
fn setup_plan_reports_network_extension_setup_when_tun_requested() {
    let dir = tempfile::tempdir().unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(dir.path().join("state")),
            config_path: Some(PathBuf::from("dam.example.toml")),
            network_mode: dam_net::CaptureMode::Tun,
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    let step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::NetworkExtension)
        .unwrap();
    assert_eq!(step.status, SetupStepStatus::Needed);
    assert_eq!(
        step.command,
        Some(vec![
            "dam".to_string(),
            "network".to_string(),
            "install-network-extension".to_string(),
            "--config".to_string(),
            "dam.example.toml".to_string(),
            "--yes".to_string()
        ])
    );
    assert!(step.requires_confirmation);
    assert!(step.changes_system);
}

#[cfg(target_os = "macos")]
#[test]
fn setup_plan_reports_network_extension_configuration_after_system_extension_ready() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let config = proxy_config("https://api.openai.com", "openai-compatible");
    dam_net_macos::record_system_extension_ready(
        &state_dir,
        "com.rpblc.dam.network-extension",
        None,
        vec!["api.openai.com".to_string()],
    )
    .unwrap();

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(state_dir),
            config_path: Some(PathBuf::from("dam.example.toml")),
            network_mode: dam_net::CaptureMode::Tun,
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    let step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::NetworkExtensionConfiguration)
        .unwrap();
    assert_eq!(step.status, SetupStepStatus::Needed);
    assert_eq!(
        step.command,
        Some(vec![
            "dam".to_string(),
            "network".to_string(),
            "install-network-extension".to_string(),
            "--config".to_string(),
            "dam.example.toml".to_string(),
            "--yes".to_string()
        ])
    );
    assert!(step.requires_confirmation);
    assert!(step.changes_system);
    assert!(step.message.contains("configuration"));
}

#[cfg(target_os = "macos")]
#[test]
fn setup_plan_blocks_when_network_extension_status_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(state_dir.join("startup")).unwrap();
    std::fs::write(state_dir.join(LOGIN_ITEM_SKIP_MARKER_RELPATH), "skipped\n").unwrap();
    let record_dir = state_dir.join("network/macos-network-extension");
    std::fs::create_dir_all(&record_dir).unwrap();
    std::fs::write(record_dir.join("latest.json"), "{not json").unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(state_dir),
            network_mode: dam_net::CaptureMode::Tun,
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    assert_eq!(plan.state, SetupPlanState::Blocked);
    let step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::NetworkExtension)
        .unwrap();
    assert_eq!(step.status, SetupStepStatus::Blocked);
    assert!(step.message.contains("status cannot be inspected"));
    assert_eq!(
        step.command,
        Some(vec![
            "dam".to_string(),
            "network".to_string(),
            "status".to_string(),
            "--json".to_string()
        ])
    );
    assert!(!step.requires_confirmation);
    assert!(!step.changes_system);
    assert_eq!(
        plan.next_action.as_ref().map(|step| step.kind),
        Some(SetupStepKind::NetworkExtension)
    );
}

#[test]
fn tun_capture_setup_steps_are_platform_specific_for_linux_and_windows() {
    let dir = tempfile::tempdir().unwrap();
    let linux_steps =
        tun_capture_setup_steps(dam_net::CapturePlatform::Linux, dir.path(), None, true);
    let windows_steps =
        tun_capture_setup_steps(dam_net::CapturePlatform::Windows, dir.path(), None, true);

    assert_eq!(linux_steps[0].kind, SetupStepKind::LinuxTransparentProxy);
    assert_eq!(linux_steps[0].status, SetupStepStatus::Blocked);
    assert!(linux_steps[0].message.contains("Linux"));
    assert_eq!(
        windows_steps[0].kind,
        SetupStepKind::WindowsFilteringPlatform
    );
    assert_eq!(windows_steps[0].status, SetupStepStatus::Blocked);
    assert!(windows_steps[0].message.contains("Windows"));
    assert_eq!(
        linux_steps[0].command,
        Some(vec![
            "dam".to_string(),
            "connect".to_string(),
            "--network-mode".to_string(),
            "explicit_proxy".to_string(),
            "--trust-mode".to_string(),
            "disabled".to_string()
        ])
    );
}

#[cfg(target_os = "macos")]
#[test]
fn setup_plan_installs_network_extension_and_trust_for_empty_app_scope() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let integration_state_dir = state_dir.join("integrations");
    dam_integrations::set_integration_enabled("claude-code", false, &integration_state_dir)
        .unwrap();
    dam_integrations::set_integration_enabled("codex", false, &integration_state_dir).unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(state_dir),
            network_mode: dam_net::CaptureMode::Tun,
            trust_mode: dam_trust::TrustMode::LocalCa,
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    let ne_step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::NetworkExtension)
        .unwrap();
    let trust_step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::LocalCa)
        .unwrap();

    assert_eq!(ne_step.status, SetupStepStatus::Needed);
    assert_eq!(trust_step.status, SetupStepStatus::Needed);
    assert!(ne_step.message.contains("Network Extension"));
    assert!(trust_step.message.contains("local CA"));
}

#[cfg(target_os = "macos")]
#[test]
fn setup_plan_treats_empty_scope_network_extension_config_as_ready() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    let integration_state_dir = state_dir.join("integrations");
    dam_integrations::set_integration_enabled("claude-code", false, &integration_state_dir)
        .unwrap();
    dam_integrations::set_integration_enabled("codex", false, &integration_state_dir).unwrap();
    let record_dir = state_dir.join("network/macos-network-extension");
    std::fs::create_dir_all(&record_dir).unwrap();
    std::fs::write(
        record_dir.join("latest.json"),
        r#"{
            "version": 1,
            "bundle_identifier": "com.rpblc.dam.network-extension",
            "team_identifier": null,
            "ai_hosts": [],
            "installed_at_unix": 1,
            "active": false,
            "activation_method": "network_extension_empty_scope_no_capture",
            "pending_reboot": false
        }"#,
    )
    .unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(state_dir),
            network_mode: dam_net::CaptureMode::Tun,
            trust_mode: dam_trust::TrustMode::Disabled,
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    let enable_step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::NetworkExtensionEnable)
        .unwrap();
    let start_step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::NetworkExtensionStart)
        .unwrap();

    assert_eq!(enable_step.status, SetupStepStatus::Skipped);
    assert_eq!(start_step.status, SetupStepStatus::Skipped);
}

#[cfg(target_os = "macos")]
#[test]
fn setup_plan_marks_launch_at_login_done_from_marker() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(state_dir.join("startup")).unwrap();
    std::fs::write(state_dir.join(LOGIN_ITEM_MARKER_RELPATH), "registered\n").unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(state_dir),
            network_mode: dam_net::CaptureMode::Tun,
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    let step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::LaunchAtLogin)
        .unwrap();
    assert_eq!(step.status, SetupStepStatus::Done);
}

#[cfg(target_os = "macos")]
#[test]
fn setup_plan_marks_launch_at_login_done_from_skip_marker() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");
    std::fs::create_dir_all(state_dir.join("startup")).unwrap();
    std::fs::write(state_dir.join(LOGIN_ITEM_SKIP_MARKER_RELPATH), "skipped\n").unwrap();
    let config = proxy_config("https://api.openai.com", "openai-compatible");

    let plan = setup_plan(
        &config,
        &SetupPlanOptions {
            state_dir: Some(state_dir),
            network_mode: dam_net::CaptureMode::Tun,
            ..SetupPlanOptions::default()
        },
    )
    .unwrap();

    let step = plan
        .steps
        .iter()
        .find(|step| step.kind == SetupStepKind::LaunchAtLogin)
        .unwrap();
    assert_eq!(step.status, SetupStepStatus::Done);
    assert!(step.message.contains("skipped"));
}

#[tokio::test]
async fn doctor_uses_router_and_proxy_runtime_status() {
    let proxy_url = spawn_health(dam_api::ProxyReport {
        operation_id: None,
        target: Some("test".to_string()),
        upstream: Some("https://api.example.test".to_string()),
        state: dam_api::ProxyState::Protected,
        message: "proxy is ready".to_string(),
        diagnostics: Vec::new(),
    })
    .await;
    let config = proxy_config("https://api.example.test", "openai-compatible");

    let report = doctor_report(
        &config,
        &DoctorOptions {
            proxy_url: Some(proxy_url),
            ..DoctorOptions::default()
        },
    )
    .await;

    assert!(report.components.iter().any(|component| {
        component.component == "router"
            && component.state == dam_api::HealthState::Healthy
            && component.message.contains("caller auth passthrough")
    }));
    assert!(report.components.iter().any(|component| {
        component.component == "proxy_runtime" && component.state == dam_api::HealthState::Healthy
    }));
}

#[tokio::test]
async fn doctor_reports_config_required_route_as_degraded() {
    let mut config = proxy_config("https://api.openai.com", "openai-compatible");
    config.proxy.targets[0].api_key_env = Some("MISSING_TEST_OPENAI_KEY".to_string());

    let report = doctor_report(
        &config,
        &DoctorOptions {
            proxy_url: Some("http://127.0.0.1:1".to_string()),
            ..DoctorOptions::default()
        },
    )
    .await;

    assert!(report.components.iter().any(|component| {
        component.component == "router" && component.state == dam_api::HealthState::Degraded
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "router_config_required"
            && diagnostic.message.contains("MISSING_TEST_OPENAI_KEY")
    }));
}

fn test_daemon_state(pid: u32) -> dam_daemon::DaemonState {
    dam_daemon::DaemonState {
        version: 6,
        pid,
        executable_path: Some(std::path::PathBuf::from("/usr/local/bin/dam")),
        executable_sha256: Some("abc123".to_string()),
        listen: "127.0.0.1:7828".to_string(),
        proxy_url: "http://127.0.0.1:7828".to_string(),
        config_path: None,
        vault_path: std::path::PathBuf::from("vault.db"),
        log_path: Some(std::path::PathBuf::from("log.db")),
        consent_path: Some(std::path::PathBuf::from("consent.db")),
        resolve_inbound: true,
        target_name: Some("openai".to_string()),
        target_provider: Some("openai-compatible".to_string()),
        upstream: Some("https://api.openai.com".to_string()),
        proxy_targets: Vec::new(),
        started_at_unix: 1_700_000_000,
        network_mode: dam_net::CaptureMode::ExplicitProxy,
        transparent_routes: Vec::new(),
        transparent_routing_readiness: Vec::new(),
        trust: dam_trust::TrustState::default(),
        transparent_trust_readiness: Vec::new(),
        transparent_interception_readiness: Vec::new(),
        protection_enabled: true,
        protection_started_at_unix: Some(1_700_000_000),
    }
}
