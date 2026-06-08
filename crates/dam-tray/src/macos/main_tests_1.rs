use super::*;

#[test]
fn popover_origin_centers_under_tray_anchor() {
    let monitor = PhysicalFrame {
        x: 0.0,
        y: 0.0,
        width: 1440.0,
        height: 900.0,
    };
    let anchor = PhysicalFrame {
        x: 980.0,
        y: 0.0,
        width: 80.0,
        height: 24.0,
    };

    let origin = popover_origin(Some(anchor), monitor, 430.0, 720.0, 8.0);

    assert_eq!(origin.x, 805);
    assert_eq!(origin.y, 24);
}

#[test]
fn popover_origin_clamps_to_monitor_edges() {
    let monitor = PhysicalFrame {
        x: 0.0,
        y: 0.0,
        width: 1440.0,
        height: 900.0,
    };
    let anchor = PhysicalFrame {
        x: 1410.0,
        y: 0.0,
        width: 60.0,
        height: 24.0,
    };

    let origin = popover_origin(Some(anchor), monitor, 430.0, 720.0, 8.0);

    assert_eq!(origin.x, 1002);
    assert_eq!(origin.y, 24);
}

#[test]
fn webview_origin_check_allows_only_local_http_authority() {
    assert!(url_has_local_origin(
        "http://127.0.0.1:2896/connect",
        "127.0.0.1:2896"
    ));
    assert!(!url_has_local_origin(
        "http://127.0.0.1:28960/connect",
        "127.0.0.1:2896"
    ));
    assert!(!url_has_local_origin("https://rpblc.com", "127.0.0.1:2896"));
}

#[test]
fn native_connect_args_include_state_paths_and_dev_modes() {
    let data_paths = DataPaths {
        state_dir: PathBuf::from("/tmp/dam-state"),
        vault_path: PathBuf::from("/tmp/dam-state/vault.db"),
        log_path: PathBuf::from("/tmp/dam-state/log.db"),
        consent_path: PathBuf::from("/tmp/dam-state/consent.db"),
    };
    let args = connect_args(&data_paths, Some(&PathBuf::from("dam.toml")), true);

    assert!(args.contains(&"--apply".to_string()));
    assert!(arg_pair_exists(&args, "--config", "dam.toml"));
    assert!(arg_pair_exists(&args, "--db", "/tmp/dam-state/vault.db"));
    assert!(arg_pair_exists(&args, "--log", "/tmp/dam-state/log.db"));
    assert!(arg_pair_exists(
        &args,
        "--consent-db",
        "/tmp/dam-state/consent.db"
    ));
    assert!(arg_pair_exists(&args, "--network-mode", "explicit_proxy"));
    assert!(arg_pair_exists(&args, "--trust-mode", "disabled"));
}

#[test]
fn tray_connect_uses_diagnostics_next_action() {
    let needed = dam_diagnostics::SetupStep {
        kind: dam_diagnostics::SetupStepKind::LaunchAtLogin,
        status: dam_diagnostics::SetupStepStatus::Needed,
        detail: dam_diagnostics::SetupStepDetail::Unconfigured,
        message: "choose startup behavior".to_string(),
        command: None,
        requires_confirmation: false,
        changes_system: true,
    };
    let blocked = dam_diagnostics::SetupStep {
        kind: dam_diagnostics::SetupStepKind::NetworkExtension,
        status: dam_diagnostics::SetupStepStatus::Blocked,
        detail: dam_diagnostics::SetupStepDetail::Failed,
        message: "Network Extension status cannot be inspected".to_string(),
        command: None,
        requires_confirmation: false,
        changes_system: false,
    };
    let plan = dam_diagnostics::SetupPlan {
        state: dam_diagnostics::SetupPlanState::Blocked,
        message: "blocked".to_string(),
        state_dir: PathBuf::from("/tmp/dam-state"),
        integration_state_dir: PathBuf::from("/tmp/dam-state/integrations"),
        proxy_url: "http://127.0.0.1:7828".to_string(),
        network_mode: dam_net::CaptureMode::Tun,
        trust_mode: dam_trust::TrustMode::LocalCa,
        active_profile: None,
        next_action: Some(blocked.clone()),
        steps: vec![needed, blocked],
    };

    let selected = selected_setup_step(&plan).unwrap();

    assert_eq!(
        selected.kind,
        dam_diagnostics::SetupStepKind::NetworkExtension
    );
    assert_eq!(selected.detail, dam_diagnostics::SetupStepDetail::Failed);
}

#[test]
fn native_connect_notice_encoding_is_url_and_js_safe() {
    assert_eq!(
        form_url_encode_component("Connect failed: local trust"),
        "Connect+failed%3A+local+trust"
    );
    assert_eq!(js_string_literal("\"<&"), "\"\\\"\\u003c\\u0026\"");
}

#[test]
fn native_connect_failure_redirect_uses_error_banner_param() {
    assert_eq!(
        connect_result_redirect(Ok(ConnectOutcome::Connected)),
        "/connect?notice=DAM+connected"
    );
    assert_eq!(
        connect_result_redirect(Ok(ConnectOutcome::NeedsApproval)),
        "/connect"
    );
    assert_eq!(
        connect_result_redirect(Ok(ConnectOutcome::AdvancedSetup)),
        "/connect"
    );
    assert_eq!(
        connect_result_redirect(Ok(ConnectOutcome::NeedsReboot)),
        "/connect"
    );
    assert_eq!(
        connect_result_redirect(Err("local trust".to_string())),
        "/connect?error=Connect+failed%3A+local+trust"
    );
    assert_eq!(
        connect_result_redirect(Err(
            "action required: approve DAM Network Protection in System Settings, then click Connect/Resume again"
                .to_string()
        )),
        "/connect?error=Action+required%3A+approve+DAM+Network+Protection+in+System+Settings%2C+then+click+Connect%2FResume+again"
    );
    assert_eq!(
        connect_result_redirect(Err(
            "failed to install Network Extension routing: DAM Network Protection is enabled but did not connect: timeout"
                .to_string()
        )),
        "/connect?error=DAM+Network+Protection+is+enabled+but+did+not+connect%3A+timeout"
    );
}

#[test]
fn native_command_error_prefers_actionable_approval_line() {
    let stdout = concat!(
        "state: needs_approval\n",
        "message: raw helper state\n",
        "approval: approve DAM Network Protection in System Settings, then click Connect/Resume again\n",
    );

    assert_eq!(
        dam_command_failure_message(stdout, ""),
        "approve DAM Network Protection in System Settings, then click Connect/Resume again"
    );
    assert_eq!(
        dam_command_failure_message(stdout, "explicit failure"),
        "explicit failure"
    );
    assert_eq!(
        dam_command_failure_message(
            "",
            "dam-macos-ne-helper: DAM Network Protection is enabled but did not connect: timeout"
        ),
        "DAM Network Protection is enabled but did not connect: timeout"
    );
}

#[test]
fn network_extension_settings_urls_prefer_specific_extension_section() {
    assert_eq!(
        network_extension_approval_settings_urls(),
        [
            "x-apple.systempreferences:com.apple.ExtensionsPreferences?extensionPointIdentifier=com.apple.system_extension.network_extension.extension-point",
            "x-apple.systempreferences:com.apple.LoginItems-Settings.extension?ExtensionItems",
            "x-apple.systempreferences:com.apple.LoginItems-Settings.extension",
        ]
    );
}

fn arg_pair_exists(args: &[String], name: &str, value: &str) -> bool {
    args.windows(2)
        .any(|pair| pair[0] == name && pair[1] == value)
}
