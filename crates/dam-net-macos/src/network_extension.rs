use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const NETWORK_EXTENSION_DIR: &str = "network/macos-network-extension";
const STATE_FILE: &str = "latest.json";
const STATE_VERSION: u32 = 1;
const HELPER_ENV: &str = "DAM_MACOS_NE_HELPER";
const BUNDLE_ID_ENV: &str = "DAM_MACOS_NE_BUNDLE_ID";
const TEAM_ID_ENV: &str = "DAM_MACOS_NE_TEAM_ID";
const PROXY_HOST_ENV: &str = "DAM_MACOS_NE_PROXY_HOST";
const PROXY_PORT_ENV: &str = "DAM_MACOS_NE_PROXY_PORT";
const EXCLUDED_SIGNING_IDS_ENV: &str = "DAM_MACOS_NE_EXCLUDED_SIGNING_IDS";
const ROUTING_FAILURE_POLICY_ENV: &str = "DAM_MACOS_NE_ROUTING_FAILURE_POLICY";
const DEFAULT_BUNDLE_ID: &str = "com.rpblc.dam.network-extension";
const DEFAULT_PROXY_HOST: &str = "127.0.0.1";
const DEFAULT_PROXY_PORT: &str = "7828";
const DEFAULT_ROUTING_FAILURE_POLICY: &str = "fail_open";
const DEFAULT_EXCLUDED_SIGNING_IDENTIFIERS: &[&str] = &[
    "com.rpblc.dam",
    "com.rpblc.dam.cli",
    "com.rpblc.dam.daemon",
    "com.rpblc.dam.helper",
    "com.rpblc.dam.mcp",
    "com.rpblc.dam.proxy",
    "com.rpblc.dam.tray",
    "com.rpblc.dam.web",
    "com.rpblc.dam.network-extension",
    "dam",
    "dam-daemon",
    "dam-macos-ne-helper",
    "dam-mcp",
    "dam-proxy",
    "dam-tray",
    "dam-web",
];

#[derive(Debug, thiserror::Error)]
pub enum MacosNetworkExtensionError {
    #[error("macOS Network Extension support is not implemented for this platform")]
    UnsupportedPlatform,

    #[error("failed to create Network Extension state directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to read Network Extension state {path}: {source}")]
    ReadState {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse Network Extension state {path}: {source}")]
    ParseState {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to serialize Network Extension state: {0}")]
    SerializeState(serde_json::Error),

    #[error("failed to write Network Extension state file {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to delete Network Extension state file {path}: {source}")]
    DeleteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to run Network Extension helper {program}: {source}")]
    RunHelper {
        program: String,
        source: std::io::Error,
    },

    #[error(
        "macOS Network Extension helper is required to configure capture for {bundle_identifier}; set DAM_MACOS_NE_HELPER in source builds or use the signed app bundle"
    )]
    MissingHelper { bundle_identifier: String },

    #[error("Network Extension helper failed ({status}): {program} {args}; {stderr}")]
    HelperFailed {
        program: String,
        args: String,
        status: String,
        stderr: String,
    },

    #[error("Network Extension needs user approval: {message}")]
    HelperNeedsApproval {
        program: String,
        args: String,
        message: String,
    },

    #[error("Network Extension recovery gate failed: {message}")]
    RecoveryGateFailed { message: String },

    #[error("system clock is before unix epoch")]
    Clock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacosNetworkExtensionPaths {
    pub directory: PathBuf,
    pub state_path: PathBuf,
}

impl MacosNetworkExtensionPaths {
    pub fn for_state_dir(state_dir: impl AsRef<Path>) -> Self {
        let directory = state_dir.as_ref().join(NETWORK_EXTENSION_DIR);
        Self {
            state_path: directory.join(STATE_FILE),
            directory,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacosNetworkExtensionStateRecord {
    pub version: u32,
    pub bundle_identifier: String,
    pub team_identifier: Option<String>,
    #[serde(default, alias = "ai_hosts")]
    pub protected_hosts: Vec<String>,
    pub installed_at_unix: u64,
    pub active: bool,
    pub activation_method: String,
    /// True when the macOS system extension was approved by the user
    /// but a system reboot is still required to finish activating it.
    /// Records persist this so the SPA's setup checklist can surface
    /// the reboot as its own step instead of presenting the install
    /// click as a hard failure that has to be re-clicked. Defaults
    /// to false for backward-compat with records written before this
    /// field existed.
    #[serde(default)]
    pub pending_reboot: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacosNetworkExtensionAction {
    Install,
    Remove,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacosNetworkExtensionSupport {
    Implemented,
    Planned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacosNetworkExtensionResultState {
    Preview,
    Installed,
    AlreadyInstalled,
    NeedsApproval,
    Removed,
    NotInstalled,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacosNetworkExtensionCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacosNetworkExtensionPlan {
    pub action: MacosNetworkExtensionAction,
    pub support: MacosNetworkExtensionSupport,
    pub paths: MacosNetworkExtensionPaths,
    pub bundle_identifier: String,
    pub team_identifier: Option<String>,
    pub protected_hosts: Vec<String>,
    pub commands: Vec<MacosNetworkExtensionCommand>,
    pub requires_admin: bool,
    pub changes_system_routes: bool,
    pub can_execute: bool,
    pub helper_required_for_release: bool,
    pub message: String,
    pub backend_status: dam_net::CaptureBackendStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacosNetworkExtensionResult {
    pub state: MacosNetworkExtensionResultState,
    pub plan: MacosNetworkExtensionPlan,
    pub record: Option<MacosNetworkExtensionStateRecord>,
    pub manager_status: Option<MacosNetworkExtensionManagerStatus>,
    pub system_routes_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacosNetworkExtensionManagerStatus {
    pub configured: bool,
    pub enabled: bool,
    pub connection_status: String,
    pub connected: bool,
    pub message: String,
}

pub fn network_extension_installed(state_dir: impl AsRef<Path>) -> bool {
    read_record(&MacosNetworkExtensionPaths::for_state_dir(state_dir))
        .ok()
        .flatten()
        .is_some()
}

pub fn network_extension_active(state_dir: impl AsRef<Path>) -> bool {
    read_record(&MacosNetworkExtensionPaths::for_state_dir(state_dir))
        .ok()
        .flatten()
        .map(|record| record.active)
        .unwrap_or(false)
}

pub fn network_extension_needs_network_configuration(state_dir: impl AsRef<Path>) -> bool {
    read_record(&MacosNetworkExtensionPaths::for_state_dir(state_dir))
        .ok()
        .flatten()
        .map(|record| {
            !record.active
                && record.activation_method == "system_extension_ready_needs_network_configuration"
        })
        .unwrap_or(false)
}

/// True when DAM has recorded that the macOS system extension was
/// approved but the user has not yet rebooted to finish activating it.
/// Used by `dam-diagnostics` to emit the reboot as its own setup step.
pub fn network_extension_pending_reboot(state_dir: impl AsRef<Path>) -> bool {
    read_record(&MacosNetworkExtensionPaths::for_state_dir(state_dir))
        .ok()
        .flatten()
        .map(|record| pending_reboot_record_is_current(&record, macos_boot_unix()))
        .unwrap_or(false)
}

/// Record that macOS finished the System Extension transition but
/// DAM still needs to configure and verify the Network Extension
/// manager before capture is considered active.
pub fn record_system_extension_ready(
    state_dir: impl AsRef<Path>,
    bundle_identifier: impl Into<String>,
    team_identifier: Option<String>,
    protected_hosts: Vec<String>,
) -> Result<(), MacosNetworkExtensionError> {
    let paths = MacosNetworkExtensionPaths::for_state_dir(state_dir);
    let bundle_identifier = bundle_identifier.into();
    if let Some(existing) = read_record(&paths)?
        && !system_extension_ready_should_replace(&existing, &bundle_identifier)
    {
        return Ok(());
    }
    let record = MacosNetworkExtensionStateRecord {
        version: STATE_VERSION,
        bundle_identifier,
        team_identifier,
        protected_hosts,
        installed_at_unix: unix_timestamp().unwrap_or(0),
        active: false,
        activation_method: "system_extension_ready_needs_network_configuration".to_string(),
        pending_reboot: false,
    };
    write_state_record(&paths, &record)
}

fn system_extension_ready_should_replace(
    record: &MacosNetworkExtensionStateRecord,
    bundle_identifier: &str,
) -> bool {
    if record.bundle_identifier != bundle_identifier {
        return true;
    }
    matches!(
        record.activation_method.as_str(),
        "system_extension_needs_user_approval" | "system_extension_pending_reboot"
    )
}

/// Record that macOS still needs explicit user approval for the
/// System Extension activation request. This clears stale reboot
/// markers so the setup checklist does not keep asking for a restart
/// after macOS has moved back to an approval state.
pub fn record_system_extension_needs_approval(
    state_dir: impl AsRef<Path>,
    bundle_identifier: impl Into<String>,
    team_identifier: Option<String>,
    protected_hosts: Vec<String>,
) -> Result<(), MacosNetworkExtensionError> {
    let paths = MacosNetworkExtensionPaths::for_state_dir(state_dir);
    let record = MacosNetworkExtensionStateRecord {
        version: STATE_VERSION,
        bundle_identifier: bundle_identifier.into(),
        team_identifier,
        protected_hosts,
        installed_at_unix: unix_timestamp().unwrap_or(0),
        active: false,
        activation_method: "system_extension_needs_user_approval".to_string(),
        pending_reboot: false,
    };
    write_state_record(&paths, &record)
}

/// Persist a "pending reboot" record after macOS reports that a System
/// Extension transition will complete after restart. This covers both
/// activation and removal transitions. Subsequent setup reads see this
/// flag and surface reboot as its own checklist step. After reboot,
/// DAM re-checks the live System Extension and Network Extension
/// manager state before marking any prior step complete.
pub fn record_pending_reboot(
    state_dir: impl AsRef<Path>,
    bundle_identifier: impl Into<String>,
    team_identifier: Option<String>,
    protected_hosts: Vec<String>,
) -> Result<(), MacosNetworkExtensionError> {
    let paths = MacosNetworkExtensionPaths::for_state_dir(state_dir);
    let installed_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let record = MacosNetworkExtensionStateRecord {
        version: 1,
        bundle_identifier: bundle_identifier.into(),
        team_identifier,
        protected_hosts,
        installed_at_unix,
        active: false,
        activation_method: "system_extension_pending_reboot".to_string(),
        pending_reboot: true,
    };
    write_state_record(&paths, &record)
}

pub fn preview_install_network_extension(
    state_dir: impl AsRef<Path>,
) -> Result<MacosNetworkExtensionResult, MacosNetworkExtensionError> {
    let hosts = default_protected_hosts();
    preview_install_network_extension_for_hosts(state_dir, &hosts)
}

pub fn preview_install_network_extension_for_hosts(
    state_dir: impl AsRef<Path>,
    protected_hosts: &[String],
) -> Result<MacosNetworkExtensionResult, MacosNetworkExtensionError> {
    ensure_macos()?;
    let plan = install_plan_for_hosts(state_dir, protected_hosts)?;
    let record = read_record(&plan.paths)?;
    Ok(MacosNetworkExtensionResult {
        state: if plan.can_execute {
            MacosNetworkExtensionResultState::Preview
        } else if plan.protected_hosts.is_empty()
            && record.as_ref().is_some_and(|record| {
                !record.active
                    && normalized_protected_hosts(&record.protected_hosts).is_empty()
                    && record.activation_method == "network_extension_empty_scope_no_capture"
            })
        {
            MacosNetworkExtensionResultState::AlreadyInstalled
        } else {
            match record.as_ref().map(|record| record.active) {
                Some(true) => MacosNetworkExtensionResultState::AlreadyInstalled,
                Some(false) => MacosNetworkExtensionResultState::NeedsApproval,
                None => MacosNetworkExtensionResultState::Preview,
            }
        },
        record,
        plan,
        manager_status: None,
        system_routes_changed: false,
    })
}

pub fn install_network_extension(
    state_dir: impl AsRef<Path>,
) -> Result<MacosNetworkExtensionResult, MacosNetworkExtensionError> {
    let hosts = default_protected_hosts();
    install_network_extension_for_hosts(state_dir, &hosts)
}

pub fn install_network_extension_for_hosts(
    state_dir: impl AsRef<Path>,
    protected_hosts: &[String],
) -> Result<MacosNetworkExtensionResult, MacosNetworkExtensionError> {
    ensure_macos()?;
    let mut plan = install_plan_for_hosts(&state_dir, protected_hosts)?;
    if !plan.can_execute {
        let record = read_record(&plan.paths)?;
        let capture_scope_matches = record
            .as_ref()
            .map(|record| {
                normalized_protected_hosts(&record.protected_hosts) == plan.protected_hosts
            })
            .unwrap_or(false);
        if (!plan.backend_status.active || !capture_scope_matches) && plan.commands.is_empty() {
            return Err(MacosNetworkExtensionError::MissingHelper {
                bundle_identifier: plan.bundle_identifier,
            });
        }
        return Ok(MacosNetworkExtensionResult {
            state: MacosNetworkExtensionResultState::AlreadyInstalled,
            record,
            plan,
            manager_status: None,
            system_routes_changed: false,
        });
    }

    for command in &plan.commands {
        if let Err(error) = run_helper_command(command) {
            match error {
                MacosNetworkExtensionError::HelperNeedsApproval { message, .. } => {
                    let record = MacosNetworkExtensionStateRecord {
                        version: STATE_VERSION,
                        bundle_identifier: plan.bundle_identifier.clone(),
                        team_identifier: plan.team_identifier.clone(),
                        protected_hosts: plan.protected_hosts.clone(),
                        installed_at_unix: unix_timestamp()?,
                        active: false,
                        activation_method:
                            "app_owned_system_extension_native_helper_needs_user_approval"
                                .to_string(),
                        pending_reboot: false,
                    };
                    write_state_record(&plan.paths, &record)?;
                    plan.message = if message.is_empty() {
                        "macOS Network Extension activation is waiting for user approval"
                            .to_string()
                    } else {
                        message
                    };
                    plan.backend_status =
                        backend_status_from_record(Some(&record), plan.message.clone());
                    return Ok(MacosNetworkExtensionResult {
                        state: MacosNetworkExtensionResultState::NeedsApproval,
                        plan,
                        record: Some(record),
                        manager_status: None,
                        system_routes_changed: true,
                    });
                }
                other => {
                    if helper_failure_disabled_manager(&other) {
                        record_start_failed_rollback(&plan)?;
                    }
                    return Err(other);
                }
            }
        }
    }

    let active = !plan.protected_hosts.is_empty();
    let verified_status = if active {
        Some(verify_network_extension_after_install(&plan)?)
    } else {
        None
    };
    let activation_method = if active {
        "app_owned_system_extension_native_helper_config"
    } else {
        "network_extension_empty_scope_no_capture"
    };
    let record = MacosNetworkExtensionStateRecord {
        version: STATE_VERSION,
        bundle_identifier: plan.bundle_identifier.clone(),
        team_identifier: plan.team_identifier.clone(),
        protected_hosts: plan.protected_hosts.clone(),
        installed_at_unix: unix_timestamp()?,
        active,
        activation_method: activation_method.to_string(),
        pending_reboot: false,
    };
    write_state_record(&plan.paths, &record)?;

    Ok(MacosNetworkExtensionResult {
        state: MacosNetworkExtensionResultState::Installed,
        plan,
        record: Some(record),
        manager_status: Some(
            verified_status.unwrap_or(MacosNetworkExtensionManagerStatus {
                configured: true,
                enabled: false,
                connection_status: "disabled".to_string(),
                connected: false,
                message: "macOS Network Extension live status: disabled for empty app scope"
                    .to_string(),
            }),
        ),
        system_routes_changed: true,
    })
}

fn verify_network_extension_after_install(
    plan: &MacosNetworkExtensionPlan,
) -> Result<MacosNetworkExtensionManagerStatus, MacosNetworkExtensionError> {
    let Some(command) = helper_command(
        "status",
        &plan.bundle_identifier,
        plan.team_identifier.as_deref(),
        &[],
    )
    .into_iter()
    .next() else {
        return fail_recovery_gate(
            plan,
            "no helper status command was available after Network Extension install".to_string(),
        );
    };

    match run_helper_status_command(&command) {
        Ok(status) if status.configured && status.enabled && status.connected => Ok(status),
        Ok(status) => fail_recovery_gate(
            plan,
            format!(
                "live status did not verify connected after install: {}",
                status.message
            ),
        ),
        Err(error) => fail_recovery_gate(
            plan,
            format!("live status check failed after install: {error}"),
        ),
    }
}

fn fail_recovery_gate<T>(
    plan: &MacosNetworkExtensionPlan,
    message: String,
) -> Result<T, MacosNetworkExtensionError> {
    let rollback = rollback_network_extension_after_failed_verification(plan);
    let activation_method = if rollback.is_ok() {
        "network_extension_recovery_gate_rolled_back"
    } else {
        "network_extension_recovery_gate_failed"
    };
    record_recovery_gate_state(plan, activation_method)?;
    let rollback_message = match rollback {
        Ok(()) => {
            "DAM removed Network Extension routing so normal networking can resume".to_string()
        }
        Err(error) => {
            format!("automatic rollback failed: {error}; run `dam setup repair --yes`")
        }
    };
    Err(MacosNetworkExtensionError::RecoveryGateFailed {
        message: format!("{message}; {rollback_message}"),
    })
}

fn rollback_network_extension_after_failed_verification(
    plan: &MacosNetworkExtensionPlan,
) -> Result<(), MacosNetworkExtensionError> {
    let Some(command) = helper_command(
        "remove",
        &plan.bundle_identifier,
        plan.team_identifier.as_deref(),
        &[],
    )
    .into_iter()
    .next() else {
        return Err(MacosNetworkExtensionError::MissingHelper {
            bundle_identifier: plan.bundle_identifier.clone(),
        });
    };
    run_helper_command(&command)
}

fn record_recovery_gate_state(
    plan: &MacosNetworkExtensionPlan,
    activation_method: &str,
) -> Result<(), MacosNetworkExtensionError> {
    let record = MacosNetworkExtensionStateRecord {
        version: STATE_VERSION,
        bundle_identifier: plan.bundle_identifier.clone(),
        team_identifier: plan.team_identifier.clone(),
        protected_hosts: plan.protected_hosts.clone(),
        installed_at_unix: unix_timestamp()?,
        active: false,
        activation_method: activation_method.to_string(),
        pending_reboot: false,
    };
    write_state_record(&plan.paths, &record)
}

fn helper_failure_disabled_manager(error: &MacosNetworkExtensionError) -> bool {
    match error {
        MacosNetworkExtensionError::HelperFailed { stderr, .. } => {
            stderr.contains("DAM Network Protection is enabled but did not connect")
                || stderr
                    .contains("DAM Network Protection is enabled but could not start automatically")
        }
        _ => false,
    }
}

fn record_start_failed_rollback(
    plan: &MacosNetworkExtensionPlan,
) -> Result<(), MacosNetworkExtensionError> {
    let record = MacosNetworkExtensionStateRecord {
        version: STATE_VERSION,
        bundle_identifier: plan.bundle_identifier.clone(),
        team_identifier: plan.team_identifier.clone(),
        protected_hosts: plan.protected_hosts.clone(),
        installed_at_unix: unix_timestamp()?,
        active: false,
        activation_method: "network_extension_start_failed_rolled_back".to_string(),
        pending_reboot: false,
    };
    write_state_record(&plan.paths, &record)
}

pub fn preview_remove_network_extension(
    state_dir: impl AsRef<Path>,
) -> Result<MacosNetworkExtensionResult, MacosNetworkExtensionError> {
    ensure_macos()?;
    let plan = remove_plan(state_dir)?;
    let record = read_record(&plan.paths)?;
    Ok(MacosNetworkExtensionResult {
        state: if record.is_some() || plan.can_execute {
            MacosNetworkExtensionResultState::Preview
        } else {
            MacosNetworkExtensionResultState::NotInstalled
        },
        record,
        plan,
        manager_status: None,
        system_routes_changed: false,
    })
}

pub fn remove_network_extension(
    state_dir: impl AsRef<Path>,
) -> Result<MacosNetworkExtensionResult, MacosNetworkExtensionError> {
    ensure_macos()?;
    let plan = remove_plan(state_dir)?;
    let record = read_record(&plan.paths)?;
    if record.is_none() && !plan.can_execute {
        return Ok(MacosNetworkExtensionResult {
            state: MacosNetworkExtensionResultState::NotInstalled,
            plan,
            record: None,
            manager_status: None,
            system_routes_changed: false,
        });
    }
    if !plan.can_execute {
        return Err(MacosNetworkExtensionError::MissingHelper {
            bundle_identifier: plan.bundle_identifier,
        });
    }

    for command in &plan.commands {
        run_helper_command(command)?;
    }
    delete_if_exists(&plan.paths.state_path)?;

    Ok(MacosNetworkExtensionResult {
        state: MacosNetworkExtensionResultState::Removed,
        plan,
        record,
        manager_status: None,
        system_routes_changed: true,
    })
}

pub fn network_extension_status(
    state_dir: impl AsRef<Path>,
) -> Result<MacosNetworkExtensionResult, MacosNetworkExtensionError> {
    let paths = MacosNetworkExtensionPaths::for_state_dir(state_dir);
    let mut record = read_record(&paths)?;
    let mut live_message = None;
    let mut manager_status = None;
    let bundle_identifier = record
        .as_ref()
        .map(|record| record.bundle_identifier.clone())
        .unwrap_or_else(bundle_identifier);
    let team_identifier = record
        .as_ref()
        .and_then(|record| record.team_identifier.clone())
        .or_else(team_identifier);
    let command = helper_command(
        "status",
        &bundle_identifier,
        team_identifier.as_deref(),
        &[],
    )
    .into_iter()
    .next();
    if let Some(command) = command {
        let status = run_helper_status_command(&command)?;
        live_message = Some(status.message.clone());
        manager_status = Some(status.clone());
        reconcile_record_with_manager_status(
            &paths,
            &mut record,
            &bundle_identifier,
            team_identifier.clone(),
            status,
        )?;
    } else if let Some(existing) = record.as_mut() {
        existing.active = false;
        live_message = Some(
            "macOS Network Extension helper is unavailable; live capture status cannot be verified"
                .to_string(),
        );
        write_state_record(&paths, existing)?;
    }
    let plan = status_plan(
        paths,
        record.as_ref(),
        manager_status.as_ref(),
        live_message,
    );
    Ok(MacosNetworkExtensionResult {
        state: MacosNetworkExtensionResultState::Status,
        plan,
        record,
        manager_status,
        system_routes_changed: false,
    })
}

fn reconcile_record_with_manager_status(
    paths: &MacosNetworkExtensionPaths,
    record: &mut Option<MacosNetworkExtensionStateRecord>,
    bundle_identifier: &str,
    team_identifier: Option<String>,
    status: MacosNetworkExtensionManagerStatus,
) -> Result<(), MacosNetworkExtensionError> {
    let method = manager_status_activation_method(record.as_ref(), bundle_identifier, &status);
    if let Some(existing) = record.as_mut() {
        existing.active = status.connected;
        existing.pending_reboot = method == "system_extension_pending_reboot";
        existing.activation_method = method.to_string();
        write_state_record(paths, existing)?;
        return Ok(());
    }

    if status.configured {
        let new_record = MacosNetworkExtensionStateRecord {
            version: STATE_VERSION,
            bundle_identifier: bundle_identifier.to_string(),
            team_identifier,
            protected_hosts: Vec::new(),
            installed_at_unix: unix_timestamp().unwrap_or(0),
            active: status.connected,
            activation_method: method.to_string(),
            pending_reboot: false,
        };
        write_state_record(paths, &new_record)?;
        *record = Some(new_record);
    }
    Ok(())
}

fn manager_status_activation_method(
    record: Option<&MacosNetworkExtensionStateRecord>,
    bundle_identifier: &str,
    status: &MacosNetworkExtensionManagerStatus,
) -> &'static str {
    if !status.connected
        && let Some(record) = record
    {
        match record.activation_method.as_str() {
            "network_extension_start_failed_rolled_back" => {
                return "network_extension_start_failed_rolled_back";
            }
            "network_extension_recovery_gate_rolled_back" => {
                return "network_extension_recovery_gate_rolled_back";
            }
            "network_extension_recovery_gate_failed" => {
                return "network_extension_recovery_gate_failed";
            }
            _ => {}
        }
    }
    if !status.configured {
        if record.is_some() {
            return system_extension_activation_method(
                system_extension_state(bundle_identifier),
                "system_extension_ready_needs_network_configuration",
            );
        }
        return "system_extension_ready_needs_network_configuration";
    }
    if status.connected {
        "app_owned_system_extension_native_helper_config"
    } else {
        match system_extension_activation_method(
            system_extension_state(bundle_identifier),
            "app_owned_system_extension_native_helper_config",
        ) {
            "app_owned_system_extension_native_helper_config" => {
                if !status.enabled {
                    "network_extension_configured_needs_enable"
                } else {
                    "network_extension_enabled_needs_start"
                }
            }
            method => method,
        }
    }
}

fn system_extension_activation_method(
    state: MacosSystemExtensionState,
    ready_method: &'static str,
) -> &'static str {
    match state {
        MacosSystemExtensionState::Enabled => ready_method,
        MacosSystemExtensionState::WaitingForReboot => "system_extension_pending_reboot",
        MacosSystemExtensionState::WaitingForUser | MacosSystemExtensionState::Unknown => {
            "system_extension_needs_user_approval"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MacosSystemExtensionState {
    Enabled,
    WaitingForUser,
    WaitingForReboot,
    Unknown,
}

fn system_extension_state(bundle_identifier: &str) -> MacosSystemExtensionState {
    let output = Command::new("/usr/bin/systemextensionsctl")
        .arg("list")
        .output();
    let Ok(output) = output else {
        return MacosSystemExtensionState::Unknown;
    };
    if !output.status.success() {
        return MacosSystemExtensionState::Unknown;
    }
    parse_system_extension_state(
        &String::from_utf8_lossy(&output.stdout),
        bundle_identifier,
        bundled_system_extension_build(bundle_identifier),
    )
}

fn parse_system_extension_state(
    output: &str,
    bundle_identifier: &str,
    bundled_build: Option<u64>,
) -> MacosSystemExtensionState {
    let Some(line) = output.lines().find(|line| {
        line.split_whitespace()
            .any(|part| part == bundle_identifier)
    }) else {
        return MacosSystemExtensionState::Unknown;
    };
    if line.contains("[activated enabled]") {
        if installed_build_is_stale(line, bundled_build) {
            return MacosSystemExtensionState::Unknown;
        }
        MacosSystemExtensionState::Enabled
    } else if line.contains("[activated waiting for user]") {
        MacosSystemExtensionState::WaitingForUser
    } else if line.contains("waiting") && line.contains("reboot") {
        MacosSystemExtensionState::WaitingForReboot
    } else {
        MacosSystemExtensionState::Unknown
    }
}

fn installed_build_is_stale(systemextensionsctl_line: &str, bundled_build: Option<u64>) -> bool {
    let Some(bundled_build) = bundled_build else {
        return false;
    };
    parse_systemextensionsctl_build(systemextensionsctl_line)
        .map(|installed_build| installed_build < bundled_build)
        .unwrap_or(false)
}

fn parse_systemextensionsctl_build(line: &str) -> Option<u64> {
    let version = line.split_once('(')?.1.split_once(')')?.0;
    let build = version.split_once('/')?.1;
    build.parse().ok()
}

fn bundled_system_extension_build(bundle_identifier: &str) -> Option<u64> {
    let exe = std::env::current_exe().ok()?;
    let contents_dir = exe.parent()?.parent()?;
    let info_plist = contents_dir
        .join("Library")
        .join("SystemExtensions")
        .join(format!("{bundle_identifier}.systemextension"))
        .join("Contents")
        .join("Info.plist");
    let xml = fs::read_to_string(info_plist).ok()?;
    parse_plist_string_value(&xml, "CFBundleVersion")?
        .parse()
        .ok()
}

fn parse_plist_string_value(xml: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let after_key = xml.split_once(&key_marker)?.1;
    let after_string = after_key.split_once("<string>")?.1;
    let value = after_string.split_once("</string>")?.0;
    Some(value.trim().to_string())
}

fn install_plan_for_hosts(
    state_dir: impl AsRef<Path>,
    protected_hosts: &[String],
) -> Result<MacosNetworkExtensionPlan, MacosNetworkExtensionError> {
    let paths = MacosNetworkExtensionPaths::for_state_dir(state_dir);
    let record = read_record(&paths)?;
    let protected_hosts = normalized_protected_hosts(protected_hosts);
    let bundle_identifier = bundle_identifier();
    let team_identifier = team_identifier();
    let installed = record.as_ref().is_some_and(|record| record.active);
    let capture_scope_matches = record
        .as_ref()
        .map(|record| normalized_protected_hosts(&record.protected_hosts) == protected_hosts)
        .unwrap_or(false);
    let empty_scope_recorded = protected_hosts.is_empty()
        && record.as_ref().is_some_and(|record| {
            !record.active
                && normalized_protected_hosts(&record.protected_hosts).is_empty()
                && record.activation_method == "network_extension_empty_scope_no_capture"
        });
    let pending_approval = record.as_ref().is_some_and(|record| !record.active);
    let commands = helper_command(
        "install",
        &bundle_identifier,
        team_identifier.as_deref(),
        &protected_hosts,
    );
    let support = support();
    let can_execute = support == MacosNetworkExtensionSupport::Implemented
        && !empty_scope_recorded
        && (!installed || !capture_scope_matches)
        && !commands.is_empty();
    let message = if protected_hosts.is_empty() {
        if empty_scope_recorded {
            "macOS Network Extension capture is disabled because no app profiles are enabled"
                .to_string()
        } else if commands.is_empty() {
            "packaged macOS Network Extension helper is required before capture can be disabled for an empty app scope"
                .to_string()
        } else {
            "will disable macOS Network Extension capture because no app profiles are enabled"
                .to_string()
        }
    } else if installed && !capture_scope_matches {
        if commands.is_empty() {
            "packaged macOS Network Extension helper is required before capture scope can be updated"
                .to_string()
        } else {
            "will update DAM macOS Network Extension capture to the current app scope".to_string()
        }
    } else if installed {
        "macOS Network Extension capture is already recorded active".to_string()
    } else if pending_approval {
        "macOS Network Extension activation is waiting for user approval".to_string()
    } else if commands.is_empty() {
        "packaged macOS Network Extension helper is required before capture can be configured"
            .to_string()
    } else {
        "will ask the packaged macOS helper to configure Network Extension capture".to_string()
    };
    let backend_status = backend_status_from_record(record.as_ref(), message.clone());

    Ok(MacosNetworkExtensionPlan {
        action: MacosNetworkExtensionAction::Install,
        support,
        paths,
        bundle_identifier,
        team_identifier,
        protected_hosts,
        commands,
        requires_admin: true,
        changes_system_routes: true,
        can_execute,
        helper_required_for_release: true,
        message,
        backend_status,
    })
}

fn remove_plan(
    state_dir: impl AsRef<Path>,
) -> Result<MacosNetworkExtensionPlan, MacosNetworkExtensionError> {
    let paths = MacosNetworkExtensionPaths::for_state_dir(state_dir);
    let record = read_record(&paths)?;
    let bundle_identifier = record
        .as_ref()
        .map(|record| record.bundle_identifier.clone())
        .unwrap_or_else(bundle_identifier);
    let team_identifier = record
        .as_ref()
        .and_then(|record| record.team_identifier.clone())
        .or_else(team_identifier);
    let commands = helper_command(
        "remove",
        &bundle_identifier,
        team_identifier.as_deref(),
        &[],
    );
    let support = support();
    let can_execute = support == MacosNetworkExtensionSupport::Implemented && !commands.is_empty();
    let message = if record.is_some() {
        if commands.is_empty() {
            "packaged macOS Network Extension helper is required before capture can be removed"
                .to_string()
        } else {
            "will ask the packaged macOS helper to deactivate Network Extension capture".to_string()
        }
    } else if !commands.is_empty() {
        "will ask the packaged macOS helper to remove any DAM Network Extension configuration"
            .to_string()
    } else {
        "no DAM macOS Network Extension capture state exists".to_string()
    };
    let backend_status = backend_status_from_record(record.as_ref(), message.clone());

    Ok(MacosNetworkExtensionPlan {
        action: MacosNetworkExtensionAction::Remove,
        support,
        paths,
        bundle_identifier,
        team_identifier,
        protected_hosts: record
            .as_ref()
            .map(|record| record.protected_hosts.clone())
            .unwrap_or_default(),
        commands,
        requires_admin: true,
        changes_system_routes: true,
        can_execute,
        helper_required_for_release: true,
        message,
        backend_status,
    })
}

fn status_plan(
    paths: MacosNetworkExtensionPaths,
    record: Option<&MacosNetworkExtensionStateRecord>,
    manager_status: Option<&MacosNetworkExtensionManagerStatus>,
    live_message: Option<String>,
) -> MacosNetworkExtensionPlan {
    let commands = record
        .map(|record| {
            helper_command(
                "status",
                &record.bundle_identifier,
                record.team_identifier.as_deref(),
                &[],
            )
        })
        .unwrap_or_default();
    let can_execute = !commands.is_empty();
    let message = live_message.unwrap_or_else(|| {
        record
            .map(|record| {
                if record.active {
                    "macOS Network Extension capture is recorded active"
                } else if manager_status.is_some_and(|status| !status.configured) {
                    "macOS Network Extension manager configuration is missing"
                } else if manager_status.is_some_and(|status| !status.enabled) {
                    "macOS Network Extension manager is configured but disabled"
                } else if manager_status.is_some_and(|status| status.enabled && !status.connected) {
                    "macOS Network Extension manager is enabled but not connected"
                } else {
                    "macOS Network Extension capture is recorded inactive"
                }
            })
            .unwrap_or("macOS Network Extension capture is not installed")
            .to_string()
    });
    MacosNetworkExtensionPlan {
        action: MacosNetworkExtensionAction::Status,
        support: support(),
        paths,
        bundle_identifier: record
            .map(|record| record.bundle_identifier.clone())
            .unwrap_or_else(bundle_identifier),
        team_identifier: record
            .and_then(|record| record.team_identifier.clone())
            .or_else(team_identifier),
        protected_hosts: record
            .map(|record| record.protected_hosts.clone())
            .unwrap_or_default(),
        commands,
        requires_admin: false,
        changes_system_routes: false,
        can_execute,
        helper_required_for_release: true,
        backend_status: backend_status_from_record(record, message.clone()),
        message,
    }
}

fn backend_status_from_record(
    record: Option<&MacosNetworkExtensionStateRecord>,
    message: String,
) -> dam_net::CaptureBackendStatus {
    match record {
        Some(record) if record.active => dam_net::CaptureBackendStatus {
            kind: dam_net::CaptureBackendKind::MacosNetworkExtension,
            platform: dam_net::CapturePlatform::Macos,
            mode: dam_net::CaptureMode::Tun,
            support: dam_net::CaptureSupport::Implemented,
            installed: true,
            active: true,
            requires_admin: true,
            changes_system_routes: true,
            rollback_available: true,
            readiness: dam_net::CaptureBackendReadiness::Ready,
            message,
        },
        Some(_) => dam_net::CaptureBackendStatus {
            kind: dam_net::CaptureBackendKind::MacosNetworkExtension,
            platform: dam_net::CapturePlatform::Macos,
            mode: dam_net::CaptureMode::Tun,
            support: dam_net::CaptureSupport::Implemented,
            installed: true,
            active: false,
            requires_admin: true,
            changes_system_routes: true,
            rollback_available: true,
            readiness: dam_net::CaptureBackendReadiness::NeedsApproval,
            message,
        },
        None => dam_net::CaptureBackendStatus {
            kind: dam_net::CaptureBackendKind::MacosNetworkExtension,
            platform: dam_net::CapturePlatform::Macos,
            mode: dam_net::CaptureMode::Tun,
            support: if cfg!(target_os = "macos") {
                dam_net::CaptureSupport::Implemented
            } else {
                dam_net::CaptureSupport::Planned
            },
            installed: false,
            active: false,
            requires_admin: true,
            changes_system_routes: true,
            rollback_available: false,
            readiness: dam_net::CaptureBackendReadiness::NeedsInstall,
            message,
        },
    }
}

fn helper_command(
    action: &str,
    bundle_identifier: &str,
    team_identifier: Option<&str>,
    protected_hosts: &[String],
) -> Vec<MacosNetworkExtensionCommand> {
    let Some(helper) = helper_path() else {
        return Vec::new();
    };
    let mut args = vec![
        action.to_string(),
        "--bundle-id".to_string(),
        bundle_identifier.to_string(),
    ];
    if let Some(team_identifier) = team_identifier {
        args.extend(["--team-id".to_string(), team_identifier.to_string()]);
    }
    if action == "install" {
        args.extend(["--proxy-host".to_string(), proxy_host()]);
        args.extend(["--proxy-port".to_string(), proxy_port()]);
        args.extend([
            "--routing-failure-policy".to_string(),
            routing_failure_policy(),
        ]);
        if protected_hosts.is_empty() {
            args.push("--no-protected-hosts".to_string());
        } else {
            for host in protected_hosts {
                args.extend(["--protect-host".to_string(), host.to_string()]);
            }
        }
        for identifier in excluded_signing_identifiers() {
            args.extend(["--exclude-signing-id".to_string(), identifier]);
        }
    }
    vec![MacosNetworkExtensionCommand {
        program: helper.display().to_string(),
        args,
    }]
}

fn helper_path() -> Option<PathBuf> {
    if let Some(helper) = env::var_os(HELPER_ENV).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(helper));
    }

    #[cfg(test)]
    {
        return None;
    }

    #[cfg(not(test))]
    {
        let exe = env::current_exe().ok()?;
        let exe_dir = exe.parent()?;
        helper_path_candidates(exe_dir)
            .into_iter()
            .find(|path| path.is_file())
    }
}

fn helper_path_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let Some(contents_dir) = exe_dir.parent() else {
        return Vec::new();
    };
    vec![
        contents_dir
            .join("Helpers")
            .join("DAMMacosNEHelper.app")
            .join("Contents")
            .join("MacOS")
            .join("dam-macos-ne-helper"),
    ]
}

fn run_helper_command(
    command: &MacosNetworkExtensionCommand,
) -> Result<(), MacosNetworkExtensionError> {
    let output = Command::new(&command.program)
        .args(&command.args)
        .output()
        .map_err(|source| MacosNetworkExtensionError::RunHelper {
            program: command.program.clone(),
            source,
        })?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Some(message) = stdout.strip_prefix("needs_user_approval ") {
            return Err(MacosNetworkExtensionError::HelperNeedsApproval {
                program: command.program.clone(),
                args: command.args.join(" "),
                message: message.trim().to_string(),
            });
        }
        return Ok(());
    }
    let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() && was_sigkill(&output.status) {
        stderr = "macOS killed the Network Extension helper before it could run; the installed app provisioning profile likely does not authorize a restricted entitlement such as com.apple.developer.networking.networkextension or com.apple.security.application-groups".to_string();
    }
    Err(MacosNetworkExtensionError::HelperFailed {
        program: command.program.clone(),
        args: command.args.join(" "),
        status: output.status.to_string(),
        stderr,
    })
}

fn run_helper_status_command(
    command: &MacosNetworkExtensionCommand,
) -> Result<MacosNetworkExtensionManagerStatus, MacosNetworkExtensionError> {
    let output = Command::new(&command.program)
        .args(&command.args)
        .output()
        .map_err(|source| MacosNetworkExtensionError::RunHelper {
            program: command.program.clone(),
            source,
        })?;
    if !output.status.success() {
        return Err(MacosNetworkExtensionError::HelperFailed {
            program: command.program.clone(),
            args: command.args.join(" "),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(parse_helper_status(&stdout))
}

fn parse_helper_status(output: &str) -> MacosNetworkExtensionManagerStatus {
    let lower = output.to_ascii_lowercase();
    let parts = lower.split_whitespace().collect::<Vec<_>>();
    let configured = parts
        .first()
        .is_some_and(|part| *part == "enabled" || *part == "disabled");
    let enabled = parts.first().is_some_and(|part| *part == "enabled");
    let connection_status = if parts.first().is_some_and(|part| *part == "not_installed") {
        "not_installed".to_string()
    } else {
        parts.get(2).copied().unwrap_or("unknown").to_string()
    };
    let connected = configured && connection_status == "connected";
    let message = if output.trim().is_empty() {
        "macOS Network Extension helper returned an empty live status".to_string()
    } else {
        format!("macOS Network Extension live status: {}", output.trim())
    };
    MacosNetworkExtensionManagerStatus {
        configured,
        enabled,
        connection_status,
        connected,
        message,
    }
}

#[cfg(unix)]
fn was_sigkill(status: &std::process::ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;

    status.signal() == Some(9)
}

#[cfg(not(unix))]
fn was_sigkill(_status: &std::process::ExitStatus) -> bool {
    false
}

fn write_state_record(
    paths: &MacosNetworkExtensionPaths,
    record: &MacosNetworkExtensionStateRecord,
) -> Result<(), MacosNetworkExtensionError> {
    fs::create_dir_all(&paths.directory).map_err(|source| {
        MacosNetworkExtensionError::CreateDir {
            path: paths.directory.clone(),
            source,
        }
    })?;
    let raw =
        serde_json::to_vec_pretty(record).map_err(MacosNetworkExtensionError::SerializeState)?;
    write_atomic(&paths.state_path, &raw, 0o600)
}

fn read_record(
    paths: &MacosNetworkExtensionPaths,
) -> Result<Option<MacosNetworkExtensionStateRecord>, MacosNetworkExtensionError> {
    if !paths.state_path.exists() {
        return Ok(None);
    }
    let raw =
        fs::read(&paths.state_path).map_err(|source| MacosNetworkExtensionError::ReadState {
            path: paths.state_path.clone(),
            source,
        })?;
    serde_json::from_slice(&raw).map(Some).map_err(|source| {
        MacosNetworkExtensionError::ParseState {
            path: paths.state_path.clone(),
            source,
        }
    })
}

fn write_atomic(
    path: &Path,
    bytes: &[u8],
    #[allow(unused_variables)] unix_mode: u32,
) -> Result<(), MacosNetworkExtensionError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| MacosNetworkExtensionError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let temp_path = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dam-net-macos-ne"),
        uuid::Uuid::new_v4().simple()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(unix_mode);
    }
    let result = (|| -> std::io::Result<()> {
        let mut file = options.open(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp_path, path)?;
        Ok(())
    })();
    if let Err(source) = result {
        let _ = fs::remove_file(&temp_path);
        return Err(MacosNetworkExtensionError::WriteFile {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

fn delete_if_exists(path: &Path) -> Result<(), MacosNetworkExtensionError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(MacosNetworkExtensionError::DeleteFile {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn default_protected_hosts() -> Vec<String> {
    dam_net::default_traffic_hosts()
}

fn normalized_protected_hosts(hosts: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for host in hosts {
        let host = dam_net::normalize_traffic_host(host);
        if !host.is_empty() && !normalized.contains(&host) {
            normalized.push(host);
        }
    }
    normalized
}

fn bundle_identifier() -> String {
    env::var(BUNDLE_ID_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BUNDLE_ID.to_string())
}

fn team_identifier() -> Option<String> {
    env::var(TEAM_ID_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn proxy_host() -> String {
    env::var(PROXY_HOST_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_PROXY_HOST.to_string())
}

fn proxy_port() -> String {
    env::var(PROXY_PORT_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| value.parse::<u16>().is_ok())
        .unwrap_or_else(|| DEFAULT_PROXY_PORT.to_string())
}

fn routing_failure_policy() -> String {
    env::var(ROUTING_FAILURE_POLICY_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| matches!(value.as_str(), "fail_open" | "fail_closed"))
        .unwrap_or_else(|| DEFAULT_ROUTING_FAILURE_POLICY.to_string())
}

fn excluded_signing_identifiers() -> Vec<String> {
    env::var(EXCLUDED_SIGNING_IDS_ENV)
        .ok()
        .map(|raw| {
            raw.split([',', ';'])
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_EXCLUDED_SIGNING_IDENTIFIERS
                .iter()
                .map(|value| value.to_string())
                .collect()
        })
}

fn support() -> MacosNetworkExtensionSupport {
    if cfg!(target_os = "macos") {
        MacosNetworkExtensionSupport::Implemented
    } else {
        MacosNetworkExtensionSupport::Planned
    }
}

fn ensure_macos() -> Result<(), MacosNetworkExtensionError> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        Err(MacosNetworkExtensionError::UnsupportedPlatform)
    }
}

fn unix_timestamp() -> Result<u64, MacosNetworkExtensionError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| MacosNetworkExtensionError::Clock)
}

fn pending_reboot_record_is_current(
    record: &MacosNetworkExtensionStateRecord,
    boot_unix: Option<u64>,
) -> bool {
    if !record.pending_reboot || record.active {
        return false;
    }
    boot_unix
        .map(|boot_unix| record.installed_at_unix >= boot_unix)
        .unwrap_or(true)
}

fn macos_boot_unix() -> Option<u64> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.boottime"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_macos_boottime_seconds(&stdout)
}

fn parse_macos_boottime_seconds(output: &str) -> Option<u64> {
    let after_sec = output.split_once("sec")?.1;
    let after_equals = after_sec.split_once('=')?.1;
    let digits: String = after_equals
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
#[path = "network_extension_tests.rs"]
mod tests;
