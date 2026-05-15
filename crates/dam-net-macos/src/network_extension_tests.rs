use super::*;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

static HELPER_ENV_LOCK: Mutex<()> = Mutex::new(());
const TEST_BUNDLE_ID: &str = "com.rpblc.dam.test.network-extension";

struct HelperEnvGuard {
    _lock: MutexGuard<'static, ()>,
    _temp: Option<tempfile::TempDir>,
    previous_helper: Option<std::ffi::OsString>,
    previous_bundle_id: Option<std::ffi::OsString>,
}

impl HelperEnvGuard {
    fn install() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let helper = temp.path().join("helper.sh");
        fs::write(
            &helper,
            "#!/bin/sh\nif [ \"$1\" = \"status\" ]; then echo \"enabled com.rpblc.dam.network-extension connected\"; fi\nexit 0\n",
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        let mut guard = Self::with_helper_path(&helper);
        guard._temp = Some(temp);
        guard
    }

    fn with_helper_path(path: &Path) -> Self {
        let lock = HELPER_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_helper = env::var_os(HELPER_ENV);
        let previous_bundle_id = env::var_os(BUNDLE_ID_ENV);
        unsafe {
            env::set_var(HELPER_ENV, path);
            env::set_var(BUNDLE_ID_ENV, TEST_BUNDLE_ID);
        }
        Self {
            _lock: lock,
            _temp: None,
            previous_helper,
            previous_bundle_id,
        }
    }
}

impl Drop for HelperEnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.previous_helper {
                Some(value) => env::set_var(HELPER_ENV, value),
                None => env::remove_var(HELPER_ENV),
            }
            match &self.previous_bundle_id {
                Some(value) => env::set_var(BUNDLE_ID_ENV, value),
                None => env::remove_var(BUNDLE_ID_ENV),
            }
        }
    }
}

#[test]
fn status_reports_needs_install_without_state() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("status-helper.sh");
    fs::write(
        &helper,
        "#!/bin/sh\nif [ \"$1\" = \"status\" ]; then echo \"not_installed com.rpblc.dam.network-extension\"; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let _helper = HelperEnvGuard::with_helper_path(&helper);

    let result = network_extension_status(dir.path()).unwrap();

    assert_eq!(result.state, MacosNetworkExtensionResultState::Status);
    assert_eq!(
        result.plan.backend_status.readiness,
        dam_net::CaptureBackendReadiness::NeedsInstall
    );
    assert!(!result.plan.backend_status.active);
}

#[test]
fn system_extension_ready_record_requires_network_configuration() {
    let dir = tempfile::tempdir().unwrap();

    record_system_extension_ready(
        dir.path(),
        DEFAULT_BUNDLE_ID,
        None,
        vec!["api.openai.com".to_string()],
    )
    .unwrap();

    assert!(network_extension_needs_network_configuration(dir.path()));
    assert!(!network_extension_active(dir.path()));
}

#[test]
fn install_records_active_network_extension_state() {
    let _helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();
    let result =
        install_network_extension_for_hosts(dir.path(), &["api.openai.com".to_string()]).unwrap();

    assert_eq!(result.state, MacosNetworkExtensionResultState::Installed);
    assert!(network_extension_installed(dir.path()));
    assert!(network_extension_active(dir.path()));
    let status = network_extension_status(dir.path()).unwrap();
    assert_eq!(
        status.plan.backend_status.readiness,
        dam_net::CaptureBackendReadiness::Ready
    );
    assert_eq!(
        status.record.unwrap().protected_hosts,
        vec!["api.openai.com"]
    );
}

#[test]
fn helper_needs_user_approval_records_inactive_pending_state() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper.sh");
    fs::write(
        &helper,
        "#!/bin/sh\necho 'needs_user_approval com.rpblc.dam.network-extension approve DAM Network Protection in System Settings'\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();

    let _helper = HelperEnvGuard::with_helper_path(&helper);
    let result =
        install_network_extension_for_hosts(dir.path(), &["api.openai.com".to_string()]).unwrap();

    assert_eq!(
        result.state,
        MacosNetworkExtensionResultState::NeedsApproval
    );
    assert!(
        result
            .plan
            .message
            .contains("approve DAM Network Protection")
    );
    assert!(network_extension_installed(dir.path()));
    assert!(!network_extension_active(dir.path()));
    assert_eq!(
        result.record.unwrap().activation_method,
        "app_owned_system_extension_native_helper_needs_user_approval"
    );
}

#[cfg(unix)]
#[test]
fn helper_sigkill_reports_likely_restricted_entitlement_failure() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper.sh");
    fs::write(&helper, "#!/bin/sh\nkill -9 $$\n").unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();

    let _helper = HelperEnvGuard::with_helper_path(&helper);
    let error = install_network_extension_for_hosts(dir.path(), &["api.openai.com".to_string()])
        .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("signal: 9"));
    assert!(message.contains("provisioning profile likely does not authorize"));
    assert!(message.contains("com.apple.developer.networking.networkextension"));
    assert!(!network_extension_installed(dir.path()));
}

#[test]
fn install_plan_passes_runtime_configuration_to_helper() {
    let _helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();
    let result =
        preview_install_network_extension_for_hosts(dir.path(), &["API.OpenAI.com.".to_string()])
            .unwrap();
    let command = result.plan.commands.first().unwrap();

    assert!(command.args.contains(&"--proxy-host".to_string()));
    assert!(command.args.contains(&"127.0.0.1".to_string()));
    assert!(command.args.contains(&"--proxy-port".to_string()));
    assert!(command.args.contains(&"7828".to_string()));
    assert!(
        command
            .args
            .contains(&"--routing-failure-policy".to_string())
    );
    assert!(command.args.contains(&"fail_open".to_string()));
    assert!(command.args.contains(&"--protect-host".to_string()));
    assert!(command.args.contains(&"api.openai.com".to_string()));
    assert!(command.args.contains(&"--exclude-signing-id".to_string()));
    assert!(command.args.contains(&"com.rpblc.dam.proxy".to_string()));
    assert!(command.args.contains(&"com.rpblc.dam.web".to_string()));
    assert!(command.args.contains(&"com.rpblc.dam.cli".to_string()));
    assert!(command.args.contains(&"dam-proxy".to_string()));
    assert!(command.args.contains(&"dam-web".to_string()));
    assert!(command.args.contains(&"dam-macos-ne-helper".to_string()));
}

#[test]
fn install_plan_passes_explicit_empty_protected_hosts_to_helper() {
    let _helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();
    let result = preview_install_network_extension_for_hosts(dir.path(), &[]).unwrap();
    let command = result.plan.commands.first().unwrap();

    assert!(command.args.contains(&"--no-protected-hosts".to_string()));
    assert!(!command.args.contains(&"--protect-host".to_string()));
    assert!(result.plan.protected_hosts.is_empty());
}

#[test]
fn install_reconfigures_active_capture_when_host_scope_changes() {
    let _helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();
    install_network_extension_for_hosts(dir.path(), &["api.openai.com".to_string()]).unwrap();

    let result = install_network_extension_for_hosts(dir.path(), &[]).unwrap();

    assert_eq!(result.state, MacosNetworkExtensionResultState::Installed);
    assert!(result.record.unwrap().protected_hosts.is_empty());
    assert!(!network_extension_active(dir.path()));
}

#[test]
fn helper_path_candidates_use_packaged_helper_app_wrapper() {
    let candidates = helper_path_candidates(Path::new("/Applications/DAM.app/Contents/MacOS"));

    assert_eq!(
        candidates,
        vec![PathBuf::from(
            "/Applications/DAM.app/Contents/Helpers/DAMMacosNEHelper.app/Contents/MacOS/dam-macos-ne-helper"
        )]
    );
}

#[test]
fn remove_deletes_network_extension_state() {
    let _helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();
    install_network_extension(dir.path()).unwrap();

    let removed = remove_network_extension(dir.path()).unwrap();

    assert_eq!(removed.state, MacosNetworkExtensionResultState::Removed);
    assert!(!network_extension_installed(dir.path()));
    assert!(!network_extension_active(dir.path()));
}

#[test]
fn remove_can_run_helper_when_local_state_was_deleted() {
    let _helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();

    let preview = preview_remove_network_extension(dir.path()).unwrap();
    assert_eq!(preview.state, MacosNetworkExtensionResultState::Preview);
    assert!(preview.plan.can_execute);

    let removed = remove_network_extension(dir.path()).unwrap();

    assert_eq!(removed.state, MacosNetworkExtensionResultState::Removed);
    assert!(removed.record.is_none());
    assert!(!network_extension_installed(dir.path()));
}

#[test]
fn status_reconciles_record_when_helper_reports_disconnected() {
    use std::os::unix::fs::PermissionsExt;

    let active_helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();
    install_network_extension(dir.path()).unwrap();
    drop(active_helper);

    let helper = dir.path().join("status-helper.sh");
    fs::write(
        &helper,
        "#!/bin/sh\nif [ \"$1\" = \"status\" ]; then echo \"enabled com.rpblc.dam.network-extension disconnected\"; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let _helper = HelperEnvGuard::with_helper_path(&helper);

    let status = network_extension_status(dir.path()).unwrap();

    assert!(!status.record.unwrap().active);
    assert!(!network_extension_active(dir.path()));
    assert_eq!(
        status.plan.backend_status.readiness,
        dam_net::CaptureBackendReadiness::NeedsApproval
    );
}

#[test]
fn status_rechecks_system_extension_before_disabled_manager_state() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    record_system_extension_ready(dir.path(), TEST_BUNDLE_ID, None, Vec::new()).unwrap();
    let helper = dir.path().join("status-helper.sh");
    fs::write(
        &helper,
        "#!/bin/sh\nif [ \"$1\" = \"status\" ]; then echo \"disabled com.rpblc.dam.network-extension disconnected\"; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let _helper = HelperEnvGuard::with_helper_path(&helper);

    let status = network_extension_status(dir.path()).unwrap();
    let record = status.record.unwrap();

    assert_eq!(
        record.activation_method,
        "system_extension_needs_user_approval"
    );
    assert!(!record.active);
    let manager = status.manager_status.unwrap();
    assert!(manager.configured);
    assert!(!manager.enabled);
}

#[test]
fn status_rechecks_system_extension_before_enabled_disconnected_manager_state() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    record_system_extension_ready(dir.path(), TEST_BUNDLE_ID, None, Vec::new()).unwrap();
    let helper = dir.path().join("status-helper.sh");
    fs::write(
        &helper,
        "#!/bin/sh\nif [ \"$1\" = \"status\" ]; then echo \"enabled com.rpblc.dam.network-extension disconnected\"; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let _helper = HelperEnvGuard::with_helper_path(&helper);

    let status = network_extension_status(dir.path()).unwrap();
    let record = status.record.unwrap();

    assert_eq!(
        record.activation_method,
        "system_extension_needs_user_approval"
    );
    assert!(!record.active);
    let manager = status.manager_status.unwrap();
    assert!(manager.configured);
    assert!(manager.enabled);
    assert!(!manager.connected);
}

#[test]
fn missing_live_system_extension_downgrades_manager_start_state() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    record_system_extension_ready(dir.path(), TEST_BUNDLE_ID, None, Vec::new()).unwrap();
    let helper = dir.path().join("status-helper.sh");
    fs::write(
        &helper,
        "#!/bin/sh\nif [ \"$1\" = \"status\" ]; then echo \"enabled com.rpblc.dam.network-extension disconnected\"; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let _helper = HelperEnvGuard::with_helper_path(&helper);
    let status = network_extension_status(dir.path()).unwrap();
    let record = read_record(&MacosNetworkExtensionPaths::for_state_dir(dir.path()))
        .unwrap()
        .unwrap();

    assert_eq!(
        status.record.as_ref().unwrap().activation_method,
        record.activation_method
    );
    assert_eq!(
        record.activation_method,
        "system_extension_needs_user_approval"
    );
    assert!(!record.active);
}

#[test]
fn system_extension_activation_method_maps_live_states() {
    assert_eq!(
        system_extension_activation_method(
            MacosSystemExtensionState::Enabled,
            "network_extension_enabled_needs_start"
        ),
        "network_extension_enabled_needs_start"
    );
    assert_eq!(
        system_extension_activation_method(
            MacosSystemExtensionState::WaitingForReboot,
            "network_extension_enabled_needs_start"
        ),
        "system_extension_pending_reboot"
    );
    assert_eq!(
        system_extension_activation_method(
            MacosSystemExtensionState::Unknown,
            "network_extension_enabled_needs_start"
        ),
        "system_extension_needs_user_approval"
    );
}

#[test]
fn status_reconciles_deleted_manager_to_system_extension_step_without_live_extension() {
    use std::os::unix::fs::PermissionsExt;

    let active_helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();
    install_network_extension(dir.path()).unwrap();
    drop(active_helper);

    let helper = dir.path().join("status-helper.sh");
    fs::write(
        &helper,
        "#!/bin/sh\nif [ \"$1\" = \"status\" ]; then echo \"not_installed com.rpblc.dam.network-extension\"; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
    let _helper = HelperEnvGuard::with_helper_path(&helper);

    let status = network_extension_status(dir.path()).unwrap();
    let record = status.record.unwrap();

    assert_eq!(
        record.activation_method,
        "system_extension_needs_user_approval"
    );
    assert!(!record.active);
    let manager = status.manager_status.unwrap();
    assert!(!manager.configured);
}

#[test]
fn parses_live_enabled_system_extension_when_build_is_current() {
    let output = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "*\t*\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0.2/3)\tDAM Network Protection\t[activated enabled]\n",
    );

    assert_eq!(
        parse_system_extension_state(output, DEFAULT_BUNDLE_ID, Some(3)),
        MacosSystemExtensionState::Enabled
    );
}

#[test]
fn stale_or_disabled_system_extension_requires_activation() {
    let stale = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "*\t*\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0.1/2)\tDAM Network Protection\t[activated enabled]\n",
    );
    let disabled = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "*\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0.2/3)\tDAM Network Protection\t[activated disabled]\n",
    );

    assert_eq!(
        parse_system_extension_state(stale, DEFAULT_BUNDLE_ID, Some(3)),
        MacosSystemExtensionState::Unknown
    );
    assert_eq!(
        parse_system_extension_state(disabled, DEFAULT_BUNDLE_ID, Some(3)),
        MacosSystemExtensionState::Unknown
    );
}

#[test]
fn remove_requires_helper_when_record_exists() {
    let _helper = HelperEnvGuard::install();
    let dir = tempfile::tempdir().unwrap();
    install_network_extension(dir.path()).unwrap();
    unsafe {
        env::remove_var(HELPER_ENV);
    }

    let error = remove_network_extension(dir.path()).unwrap_err();

    assert!(error.to_string().contains("helper is required"));
    assert!(network_extension_installed(dir.path()));
}

#[test]
fn pending_reboot_record_expires_after_next_boot() {
    let record = MacosNetworkExtensionStateRecord {
        version: STATE_VERSION,
        bundle_identifier: DEFAULT_BUNDLE_ID.to_string(),
        team_identifier: None,
        protected_hosts: Vec::new(),
        installed_at_unix: 2_000,
        active: false,
        activation_method: "system_extension_pending_reboot".to_string(),
        pending_reboot: true,
    };

    assert!(pending_reboot_record_is_current(&record, Some(1_000)));
    assert!(!pending_reboot_record_is_current(&record, Some(3_000)));
}

#[test]
fn parses_macos_boottime_seconds() {
    assert_eq!(
        parse_macos_boottime_seconds("{ sec = 1778370000, usec = 44877 } Mon May  9 14:20:25 2026"),
        Some(1_778_370_000)
    );
}
