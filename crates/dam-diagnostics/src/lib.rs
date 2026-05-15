use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DoctorOptions {
    pub proxy_url: Option<String>,
    pub state_dir: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPlanOptions {
    pub state_dir: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub proxy_url: Option<String>,
    pub network_mode: dam_net::CaptureMode,
    pub trust_mode: dam_trust::TrustMode,
}

impl Default for SetupPlanOptions {
    fn default() -> Self {
        Self {
            state_dir: None,
            config_path: None,
            proxy_url: None,
            network_mode: dam_net::CaptureMode::ExplicitProxy,
            trust_mode: dam_trust::TrustMode::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupPlanState {
    Ready,
    NeedsAction,
    Blocked,
}

impl SetupPlanState {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NeedsAction => "needs_action",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStepKind {
    ProfileApply,
    /// Register the menu-bar app to launch at user login. Comes before
    /// any step that requires a reboot (NE install) so the user
    /// doesn't lose DAM after restart.
    LaunchAtLogin,
    SystemProxy,
    NetworkExtension,
    NetworkExtensionConfiguration,
    NetworkExtensionEnable,
    NetworkExtensionStart,
    LinuxTransparentProxy,
    WindowsFilteringPlatform,
    /// macOS Network Extension was approved by the user but the system
    /// needs a reboot to finish activating it. Surfaced as its own
    /// step so the SPA's checklist shows reboot as the next clean
    /// action, not as a hard error masquerading as the install step.
    NetworkExtensionReboot,
    LocalCa,
    Daemon,
}

impl SetupStepKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::ProfileApply => "profile_apply",
            Self::LaunchAtLogin => "launch_at_login",
            Self::SystemProxy => "system_proxy",
            Self::NetworkExtension => "network_extension",
            Self::NetworkExtensionConfiguration => "network_extension_configuration",
            Self::NetworkExtensionEnable => "network_extension_enable",
            Self::NetworkExtensionStart => "network_extension_start",
            Self::LinuxTransparentProxy => "linux_transparent_proxy",
            Self::WindowsFilteringPlatform => "windows_filtering_platform",
            Self::NetworkExtensionReboot => "network_extension_reboot",
            Self::LocalCa => "local_ca",
            Self::Daemon => "daemon",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStepStatus {
    Done,
    Needed,
    Blocked,
    Skipped,
}

impl SetupStepStatus {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Needed => "needed",
            Self::Blocked => "blocked",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStepDetail {
    Ready,
    NotRequired,
    Unconfigured,
    Requested,
    WaitingForApproval,
    WaitingForReboot,
    NeedsInstall,
    NeedsConfiguration,
    Configured,
    NeedsEnable,
    Enabled,
    NeedsStart,
    Connected,
    Disconnected,
    Stale,
    EmptyScope,
    Unsupported,
    Failed,
    Mismatch,
}

impl SetupStepDetail {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotRequired => "not_required",
            Self::Unconfigured => "unconfigured",
            Self::Requested => "requested",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::WaitingForReboot => "waiting_for_reboot",
            Self::NeedsInstall => "needs_install",
            Self::NeedsConfiguration => "needs_configuration",
            Self::Configured => "configured",
            Self::NeedsEnable => "needs_enable",
            Self::Enabled => "enabled",
            Self::NeedsStart => "needs_start",
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::Stale => "stale",
            Self::EmptyScope => "empty_scope",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
            Self::Mismatch => "mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupStep {
    pub kind: SetupStepKind,
    pub status: SetupStepStatus,
    pub detail: SetupStepDetail,
    pub message: String,
    pub command: Option<Vec<String>>,
    pub requires_confirmation: bool,
    pub changes_system: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupPlan {
    pub state: SetupPlanState,
    pub message: String,
    pub state_dir: PathBuf,
    pub integration_state_dir: PathBuf,
    pub proxy_url: String,
    pub network_mode: dam_net::CaptureMode,
    pub trust_mode: dam_trust::TrustMode,
    pub active_profile: Option<dam_integrations::ActiveProfileState>,
    pub next_action: Option<SetupStep>,
    pub steps: Vec<SetupStep>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupRescueOptions {
    pub state_dir: Option<PathBuf>,
    pub proxy_url: Option<String>,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupRescue {
    pub state: String,
    pub message: String,
    pub state_dir: PathBuf,
    pub actions: Vec<SetupRescueAction>,
}

impl SetupRescue {
    pub fn is_blocked(&self) -> bool {
        self.state == "blocked"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupRescueAction {
    pub id: String,
    pub state: String,
    pub message: String,
    pub changes_system: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupRepairOptions {
    pub setup: SetupPlanOptions,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupRepair {
    pub state: String,
    pub message: String,
    pub rescue: SetupRescue,
    pub setup_plan: SetupPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupDiagnosticsExport {
    pub generated_at_unix: u64,
    pub doctor: dam_api::HealthReport,
    pub setup_plan: SetupPlan,
    pub rescue_preview: SetupRescue,
}

pub async fn doctor_report(
    config: &dam_config::DamConfig,
    options: &DoctorOptions,
) -> dam_api::HealthReport {
    let mut report = config_report(config);

    report
        .components
        .push(router_component(config, &mut report.diagnostics));
    report
        .components
        .push(vault_runtime_component(config, &mut report.diagnostics));
    report
        .components
        .push(consent_runtime_component(config, &mut report.diagnostics));
    report
        .components
        .push(log_runtime_component(config, &mut report.diagnostics));
    report
        .components
        .push(proxy_runtime_component(config, options, &mut report.diagnostics).await);
    add_setup_plan_component(config, options, &mut report);
    report.state = aggregate_state(&report.components);

    report
}

pub fn config_report(config: &dam_config::DamConfig) -> dam_api::HealthReport {
    let mut components = Vec::new();
    let mut diagnostics = Vec::new();

    components.push(dam_api::ComponentHealth {
        component: "config".to_string(),
        state: dam_api::HealthState::Healthy,
        message: "config loaded".to_string(),
    });
    components.push(vault_component(config, &mut diagnostics));
    components.push(consent_component(config, &mut diagnostics));
    components.push(log_component(config, &mut diagnostics));
    components.push(proxy_config_component(config, &mut diagnostics));
    components.push(failure_modes_component(config, &mut diagnostics));

    dam_api::HealthReport {
        state: aggregate_state(&components),
        components,
        diagnostics,
    }
}

pub fn proxy_health_url(
    config: &dam_config::DamConfig,
    proxy_url: Option<&str>,
) -> Result<String, String> {
    if let Some(proxy_url) = proxy_url {
        return append_health(proxy_url);
    }
    append_health(&format!("http://{}", config.proxy.listen))
}

pub fn setup_plan(
    config: &dam_config::DamConfig,
    options: &SetupPlanOptions,
) -> Result<SetupPlan, String> {
    let state_dir = match &options.state_dir {
        Some(state_dir) => state_dir.clone(),
        None => {
            dam_daemon::state_paths()
                .map_err(|error| error.to_string())?
                .state_dir
        }
    };
    let integration_state_dir = state_dir.join("integrations");
    let proxy_url = options
        .proxy_url
        .clone()
        .unwrap_or_else(|| format!("http://{}", config.proxy.listen));
    let active_profile = dam_integrations::read_active_profile(&integration_state_dir)?;
    let effective_config = config_with_runtime_enabled_apps(config, &integration_state_dir)?;
    let has_active_routes =
        !dam_net::traffic_routes_from_profile(&effective_config.traffic.effective_profile())
            .is_empty();
    let mut steps = vec![
        // The startup step lands before any platform capture setup
        // deliberately: capture installation can require a system
        // reboot, and if the native shell is not registered to return
        // after restart the user loses the installer mid-flow.
        // Registering or explicitly skipping first keeps recovery
        // deterministic.
        launch_at_login_setup_step(&state_dir, options.network_mode),
    ];
    steps.extend(routing_setup_steps(
        options.network_mode,
        &state_dir,
        options.config_path.as_ref(),
        has_active_routes,
    ));
    steps.push(local_ca_setup_step(
        options.trust_mode,
        &state_dir,
        has_active_routes,
    ));
    steps.push(daemon_setup_step(
        options.network_mode,
        options.trust_mode,
        &state_dir,
    ));

    let state = if steps
        .iter()
        .any(|step| step.status == SetupStepStatus::Blocked)
    {
        SetupPlanState::Blocked
    } else if steps
        .iter()
        .any(|step| step.status == SetupStepStatus::Needed)
    {
        SetupPlanState::NeedsAction
    } else {
        SetupPlanState::Ready
    };
    let message = setup_plan_message(state, &steps);
    let next_action = setup_plan_next_action(&steps).cloned();

    Ok(SetupPlan {
        state,
        message,
        state_dir,
        integration_state_dir,
        proxy_url,
        network_mode: options.network_mode,
        trust_mode: options.trust_mode,
        active_profile,
        next_action,
        steps,
    })
}

pub fn setup_rescue(options: &SetupRescueOptions) -> Result<SetupRescue, String> {
    let state_dir = match &options.state_dir {
        Some(state_dir) => state_dir.clone(),
        None => {
            dam_daemon::state_paths()
                .map_err(|error| error.to_string())?
                .state_dir
        }
    };
    let proxy_url = options
        .proxy_url
        .clone()
        .unwrap_or_else(|| format!("http://{}", dam_daemon::DEFAULT_LISTEN));
    let actions = vec![
        setup_rescue_daemon_action(&state_dir, options.apply),
        setup_rescue_system_proxy_action(&state_dir, &proxy_url, options.apply),
        setup_rescue_network_extension_action(&state_dir, options.apply),
    ];

    let blocked = actions
        .iter()
        .any(|action| matches!(action.state.as_str(), "blocked" | "failed"));
    let state = if blocked {
        "blocked"
    } else if options.apply {
        "rescued"
    } else {
        "preview"
    };
    let message = if blocked {
        "DAM setup rescue needs manual attention before all local network protection state can be removed"
    } else if options.apply {
        "DAM setup rescue completed; run `dam setup next-action --json` to continue setup."
    } else {
        "Previewed local DAM setup rescue actions; rerun with --yes to apply."
    };

    Ok(SetupRescue {
        state: state.to_string(),
        message: message.to_string(),
        state_dir,
        actions,
    })
}

pub fn setup_repair(
    config: &dam_config::DamConfig,
    options: &SetupRepairOptions,
) -> Result<SetupRepair, String> {
    let rescue = setup_rescue(&SetupRescueOptions {
        state_dir: options.setup.state_dir.clone(),
        proxy_url: options.setup.proxy_url.clone(),
        apply: options.apply,
    })?;
    let setup_plan = setup_plan(config, &options.setup)?;
    let state = if rescue.is_blocked() {
        "blocked"
    } else if options.apply {
        "repaired"
    } else {
        "preview"
    };
    let message = if rescue.is_blocked() {
        "repair is blocked before local network setup can be reset"
    } else if options.apply {
        "repair actions applied; follow setup_plan.next_action to continue"
    } else {
        "previewed repair actions and current setup plan"
    };

    Ok(SetupRepair {
        state: state.to_string(),
        message: message.to_string(),
        rescue,
        setup_plan,
    })
}

pub async fn setup_diagnostics_export(
    config: &dam_config::DamConfig,
    doctor_options: &DoctorOptions,
    setup_options: &SetupPlanOptions,
) -> Result<SetupDiagnosticsExport, String> {
    let doctor = doctor_report(config, doctor_options).await;
    let setup_plan = setup_plan(config, setup_options)?;
    let rescue_preview = setup_rescue(&SetupRescueOptions {
        state_dir: setup_options.state_dir.clone(),
        proxy_url: setup_options.proxy_url.clone(),
        apply: false,
    })?;

    Ok(SetupDiagnosticsExport {
        generated_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0),
        doctor,
        setup_plan,
        rescue_preview,
    })
}

fn setup_rescue_daemon_action(state_dir: &Path, apply: bool) -> SetupRescueAction {
    let state_file = state_dir.join("daemon.json");
    match dam_daemon::daemon_status_from(&state_file) {
        Ok(dam_daemon::DaemonStatus::Disconnected) => {
            setup_rescue_action("daemon", "unchanged", "DAM daemon is not running", false)
        }
        Ok(dam_daemon::DaemonStatus::Stale(state)) => {
            if !apply {
                return setup_rescue_action(
                    "daemon",
                    "would_remove_stale_state",
                    format!("would remove stale DAM daemon state for pid {}", state.pid),
                    false,
                );
            }
            match dam_daemon::remove_state_file(&state_file) {
                Ok(()) => setup_rescue_action(
                    "daemon",
                    "stale_removed",
                    "removed stale DAM daemon state",
                    false,
                ),
                Err(error) => setup_rescue_action(
                    "daemon",
                    "failed",
                    format!("failed to remove stale DAM daemon state: {error}"),
                    false,
                ),
            }
        }
        Ok(dam_daemon::DaemonStatus::Connected(state)) => {
            if !apply {
                return setup_rescue_action(
                    "daemon",
                    "would_stop",
                    format!("would stop DAM daemon at {}", state.proxy_url),
                    false,
                );
            }
            match dam_daemon::terminate_process(state.pid)
                .and_then(|()| dam_daemon::remove_state_file(&state_file))
            {
                Ok(()) => setup_rescue_action("daemon", "stopped", "stopped DAM daemon", false),
                Err(error) => setup_rescue_action(
                    "daemon",
                    "failed",
                    format!("failed to stop DAM daemon: {error}"),
                    false,
                ),
            }
        }
        Err(error) => setup_rescue_action(
            "daemon",
            "failed",
            format!("failed to inspect DAM daemon state: {error}"),
            false,
        ),
    }
}

fn setup_rescue_system_proxy_action(
    state_dir: &Path,
    proxy_url: &str,
    apply: bool,
) -> SetupRescueAction {
    if !cfg!(target_os = "macos") {
        return setup_rescue_action(
            "macos_system_proxy",
            "skipped",
            "macOS system proxy routing is not available on this platform",
            false,
        );
    }

    let result = if apply {
        dam_net_macos::remove_system_proxy(state_dir, proxy_url)
    } else {
        dam_net_macos::preview_remove_system_proxy(state_dir, proxy_url)
    };
    match result {
        Ok(result) => match result.state {
            dam_net_macos::MacosSystemProxyResultState::Removed => setup_rescue_action(
                "macos_system_proxy",
                "removed",
                result.plan.message,
                result.system_routes_changed,
            ),
            dam_net_macos::MacosSystemProxyResultState::Preview => setup_rescue_action(
                "macos_system_proxy",
                "would_remove",
                result.plan.message,
                result.plan.changes_system_routes,
            ),
            dam_net_macos::MacosSystemProxyResultState::NotInstalled => setup_rescue_action(
                "macos_system_proxy",
                "unchanged",
                result.plan.message,
                false,
            ),
            _ => setup_rescue_action(
                "macos_system_proxy",
                "unchanged",
                result.plan.message,
                false,
            ),
        },
        Err(error) => setup_rescue_action(
            "macos_system_proxy",
            "failed",
            format!("failed to remove macOS system proxy routing: {error}"),
            true,
        ),
    }
}

fn setup_rescue_network_extension_action(state_dir: &Path, apply: bool) -> SetupRescueAction {
    if !cfg!(target_os = "macos") {
        return setup_rescue_action(
            "macos_network_extension",
            "skipped",
            "macOS Network Extension capture is not available on this platform",
            false,
        );
    }

    let result = if apply {
        dam_net_macos::remove_network_extension(state_dir)
    } else {
        dam_net_macos::preview_remove_network_extension(state_dir)
    };
    match result {
        Ok(result) => match result.state {
            dam_net_macos::MacosNetworkExtensionResultState::Removed => setup_rescue_action(
                "macos_network_extension",
                "removed",
                result.plan.message,
                result.system_routes_changed,
            ),
            dam_net_macos::MacosNetworkExtensionResultState::Preview => setup_rescue_action(
                "macos_network_extension",
                "would_remove",
                result.plan.message,
                result.plan.changes_system_routes,
            ),
            dam_net_macos::MacosNetworkExtensionResultState::NotInstalled => setup_rescue_action(
                "macos_network_extension",
                "unchanged",
                result.plan.message,
                false,
            ),
            _ => setup_rescue_action(
                "macos_network_extension",
                "unchanged",
                result.plan.message,
                false,
            ),
        },
        Err(error) => setup_rescue_action(
            "macos_network_extension",
            "failed",
            format!("failed to remove macOS Network Extension capture: {error}"),
            true,
        ),
    }
}

fn setup_rescue_action(
    id: &'static str,
    state: &'static str,
    message: impl Into<String>,
    changes_system: bool,
) -> SetupRescueAction {
    SetupRescueAction {
        id: id.to_string(),
        state: state.to_string(),
        message: message.into(),
        changes_system,
    }
}

fn config_with_runtime_enabled_apps(
    config: &dam_config::DamConfig,
    integration_state_dir: &std::path::Path,
) -> Result<dam_config::DamConfig, String> {
    let mut config = config.clone();
    if let Some(profile_ids) = dam_integrations::runtime_enabled_profile_ids(integration_state_dir)?
    {
        config.traffic.enabled_app_ids = Some(
            dam_integrations::traffic_app_ids_for_profile_ids_from_state(
                &profile_ids,
                integration_state_dir,
            )?,
        );
    }
    Ok(config)
}

/// Marker written after DAM registers its app bundle with macOS Login
/// Items through `SMAppService`. A legacy LaunchAgent path is still
/// accepted so upgraded installs do not regress before the user clicks
/// the new startup step again.
const LOGIN_ITEM_MARKER_RELPATH: &str = "startup/login-item.txt";
const LOGIN_ITEM_SKIP_MARKER_RELPATH: &str = "startup/login-item-skipped.txt";
const LAUNCH_AGENT_PLIST_RELPATH: &str = "Library/LaunchAgents/com.rpblc.dam-tray.plist";

fn launch_at_login_setup_step(
    state_dir: &std::path::Path,
    network_mode: dam_net::CaptureMode,
) -> SetupStep {
    if network_mode != dam_net::CaptureMode::Tun {
        return SetupStep {
            kind: SetupStepKind::LaunchAtLogin,
            status: SetupStepStatus::Skipped,
            detail: SetupStepDetail::NotRequired,
            message: "launch-at-login is only required before Network Extension setup".to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: false,
        };
    }
    if !cfg!(target_os = "macos") {
        return SetupStep {
            kind: SetupStepKind::LaunchAtLogin,
            status: SetupStepStatus::Skipped,
            detail: SetupStepDetail::NotRequired,
            message: "launch-at-login is only registered on macOS".to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: false,
        };
    }
    let marker_registered = state_dir.join(LOGIN_ITEM_MARKER_RELPATH).exists();
    let legacy_registered = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(LAUNCH_AGENT_PLIST_RELPATH).exists())
        .unwrap_or(false);
    let registered = marker_registered || legacy_registered;
    if registered {
        SetupStep {
            kind: SetupStepKind::LaunchAtLogin,
            status: SetupStepStatus::Done,
            detail: SetupStepDetail::Ready,
            message: "DAM is registered to open at login".to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: false,
        }
    } else if state_dir.join(LOGIN_ITEM_SKIP_MARKER_RELPATH).exists() {
        SetupStep {
            kind: SetupStepKind::LaunchAtLogin,
            status: SetupStepStatus::Done,
            detail: SetupStepDetail::Ready,
            message: "Open at Login was skipped for this install".to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: false,
        }
    } else {
        SetupStep {
            kind: SetupStepKind::LaunchAtLogin,
            status: SetupStepStatus::Needed,
            detail: SetupStepDetail::Unconfigured,
            message: "Choose whether DAM should open at login before setup asks macOS to restart."
                .to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: true,
        }
    }
}

fn system_proxy_setup_step(
    network_mode: dam_net::CaptureMode,
    state_dir: &std::path::Path,
    config_path: Option<&PathBuf>,
) -> SetupStep {
    match network_mode {
        dam_net::CaptureMode::ExplicitProxy => SetupStep {
            kind: SetupStepKind::SystemProxy,
            status: SetupStepStatus::Skipped,
            detail: SetupStepDetail::NotRequired,
            message: "system proxy routing is not required in explicit proxy mode".to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: false,
        },
        dam_net::CaptureMode::Tun => SetupStep {
            kind: SetupStepKind::SystemProxy,
            status: SetupStepStatus::Skipped,
            detail: SetupStepDetail::NotRequired,
            message: "system proxy routing is not used in tun mode".to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: false,
        },
        dam_net::CaptureMode::SystemProxy => {
            if dam_net_macos::system_proxy_installed(state_dir) {
                return SetupStep {
                    kind: SetupStepKind::SystemProxy,
                    status: SetupStepStatus::Done,
                    detail: SetupStepDetail::Ready,
                    message: "macOS PAC system proxy routing is installed".to_string(),
                    command: None,
                    requires_confirmation: false,
                    changes_system: false,
                };
            }
            let mut command = vec![
                "dam".to_string(),
                "network".to_string(),
                "install-system-proxy".to_string(),
            ];
            if let Some(config_path) = config_path {
                command.push("--config".to_string());
                command.push(config_path.display().to_string());
            }
            command.push("--yes".to_string());
            SetupStep {
                kind: SetupStepKind::SystemProxy,
                status: SetupStepStatus::Needed,
                detail: SetupStepDetail::NeedsInstall,
                message: "macOS PAC system proxy routing needs to be installed".to_string(),
                command: Some(command),
                requires_confirmation: true,
                changes_system: true,
            }
        }
    }
}

fn routing_setup_steps(
    network_mode: dam_net::CaptureMode,
    state_dir: &std::path::Path,
    config_path: Option<&PathBuf>,
    has_active_routes: bool,
) -> Vec<SetupStep> {
    if network_mode == dam_net::CaptureMode::Tun {
        return tun_capture_setup_steps(
            dam_net::CapturePlatform::current(),
            state_dir,
            config_path,
            has_active_routes,
        );
    }
    if !has_active_routes {
        return vec![SetupStep {
            kind: SetupStepKind::SystemProxy,
            status: SetupStepStatus::Skipped,
            detail: SetupStepDetail::EmptyScope,
            message: "platform capture is not required while no app profiles are enabled"
                .to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: false,
        }];
    }
    vec![system_proxy_setup_step(
        network_mode,
        state_dir,
        config_path,
    )]
}

fn tun_capture_setup_steps(
    platform: dam_net::CapturePlatform,
    state_dir: &std::path::Path,
    config_path: Option<&PathBuf>,
    has_active_routes: bool,
) -> Vec<SetupStep> {
    match platform {
        dam_net::CapturePlatform::Macos => {
            network_extension_setup_steps(state_dir, config_path, has_active_routes)
        }
        dam_net::CapturePlatform::Linux => vec![platform_capture_planned_step(
            SetupStepKind::LinuxTransparentProxy,
            "Linux transparent capture onboarding is planned; use explicit proxy mode on Linux for now.",
        )],
        dam_net::CapturePlatform::Windows => vec![platform_capture_planned_step(
            SetupStepKind::WindowsFilteringPlatform,
            "Windows Filtering Platform onboarding is planned; use explicit proxy mode on Windows for now.",
        )],
        dam_net::CapturePlatform::Unknown => vec![platform_capture_planned_step(
            SetupStepKind::SystemProxy,
            "transparent capture onboarding is not available on this platform; use explicit proxy mode for now.",
        )],
    }
}

fn platform_capture_planned_step(kind: SetupStepKind, message: &str) -> SetupStep {
    SetupStep {
        kind,
        status: SetupStepStatus::Blocked,
        detail: SetupStepDetail::Unsupported,
        message: message.to_string(),
        command: Some(vec![
            "dam".to_string(),
            "connect".to_string(),
            "--network-mode".to_string(),
            "explicit_proxy".to_string(),
            "--trust-mode".to_string(),
            "disabled".to_string(),
        ]),
        requires_confirmation: false,
        changes_system: true,
    }
}

fn network_extension_setup_steps(
    state_dir: &std::path::Path,
    config_path: Option<&PathBuf>,
    has_active_routes: bool,
) -> Vec<SetupStep> {
    let status = match dam_net_macos::network_extension_status(state_dir) {
        Ok(status) => Some(status),
        Err(error) => {
            return vec![SetupStep {
                kind: SetupStepKind::NetworkExtension,
                status: SetupStepStatus::Blocked,
                detail: SetupStepDetail::Failed,
                message: format!("macOS Network Extension status cannot be inspected: {error}"),
                command: Some(vec![
                    "dam".to_string(),
                    "network".to_string(),
                    "status".to_string(),
                    "--json".to_string(),
                ]),
                requires_confirmation: false,
                changes_system: false,
            }];
        }
    };
    let record = status.as_ref().and_then(|status| status.record.as_ref());
    let manager = status
        .as_ref()
        .and_then(|status| status.manager_status.as_ref());
    let activation_method = record.map(|record| record.activation_method.as_str());
    let install_command = network_extension_install_command(config_path);

    if dam_net_macos::network_extension_pending_reboot(state_dir)
        || activation_method == Some("system_extension_pending_reboot")
    {
        return vec![
            network_extension_step(
                SetupStepKind::NetworkExtension,
                SetupStepStatus::Done,
                SetupStepDetail::Ready,
                "DAM Network Protection system extension is approved",
                None,
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionReboot,
                SetupStepStatus::Needed,
                SetupStepDetail::WaitingForReboot,
                "Restart macOS to finish the Network Extension system change. DAM will re-check setup after restart.",
                None,
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionConfiguration,
                SetupStepStatus::Needed,
                SetupStepDetail::NeedsConfiguration,
                "Add the DAM Network Protection configuration in macOS",
                Some(install_command.clone()),
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionEnable,
                SetupStepStatus::Needed,
                SetupStepDetail::NeedsEnable,
                "Enable DAM Network Protection in System Settings",
                Some(install_command.clone()),
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionStart,
                SetupStepStatus::Needed,
                SetupStepDetail::NeedsStart,
                "Enable protection layer",
                Some(install_command),
            ),
        ];
    }

    if record.is_none() || activation_method == Some("system_extension_needs_user_approval") {
        return vec![
            network_extension_step(
                SetupStepKind::NetworkExtension,
                SetupStepStatus::Needed,
                if record.is_none() {
                    SetupStepDetail::NeedsInstall
                } else {
                    SetupStepDetail::WaitingForApproval
                },
                "macOS Network Extension capture needs to be installed and approved",
                Some(install_command.clone()),
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionConfiguration,
                SetupStepStatus::Needed,
                SetupStepDetail::NeedsConfiguration,
                "Add the DAM Network Protection configuration in macOS",
                Some(install_command.clone()),
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionEnable,
                SetupStepStatus::Needed,
                SetupStepDetail::NeedsEnable,
                "Enable DAM Network Protection in System Settings",
                Some(install_command.clone()),
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionStart,
                SetupStepStatus::Needed,
                SetupStepDetail::NeedsStart,
                "Enable protection layer",
                Some(install_command),
            ),
        ];
    }

    let manager_configured = manager.map(|status| status.configured).unwrap_or_else(|| {
        !matches!(
            activation_method,
            Some("system_extension_ready_needs_network_configuration")
        )
    });
    let manager_enabled = manager.map(|status| status.enabled).unwrap_or_else(|| {
        !matches!(
            activation_method,
            Some("network_extension_configured_needs_enable")
        )
    });
    let manager_connected = status
        .as_ref()
        .is_some_and(|status| status.plan.backend_status.active)
        || manager.is_some_and(|status| status.connected);
    let empty_scope_ready = !has_active_routes
        && activation_method == Some("network_extension_empty_scope_no_capture")
        && manager_configured;

    if empty_scope_ready {
        return vec![
            network_extension_step(
                SetupStepKind::NetworkExtension,
                SetupStepStatus::Done,
                SetupStepDetail::Ready,
                "DAM Network Protection system extension is approved",
                None,
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionConfiguration,
                SetupStepStatus::Done,
                SetupStepDetail::EmptyScope,
                "DAM Network Protection is configured with no protected app traffic",
                None,
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionEnable,
                SetupStepStatus::Skipped,
                SetupStepDetail::NotRequired,
                "Network Extension enablement is deferred until an app profile is enabled",
                None,
            ),
            network_extension_step(
                SetupStepKind::NetworkExtensionStart,
                SetupStepStatus::Skipped,
                SetupStepDetail::NotRequired,
                "Protection layer start is deferred until an app profile is enabled",
                None,
            ),
        ];
    }

    vec![
        network_extension_step(
            SetupStepKind::NetworkExtension,
            SetupStepStatus::Done,
            SetupStepDetail::Ready,
            "DAM Network Protection system extension is approved",
            None,
        ),
        network_extension_step(
            SetupStepKind::NetworkExtensionConfiguration,
            if manager_configured {
                SetupStepStatus::Done
            } else {
                SetupStepStatus::Needed
            },
            if manager_configured {
                SetupStepDetail::Configured
            } else {
                SetupStepDetail::NeedsConfiguration
            },
            "Add the DAM Network Protection configuration in macOS",
            Some(install_command.clone()),
        ),
        network_extension_step(
            SetupStepKind::NetworkExtensionEnable,
            if !manager_configured {
                SetupStepStatus::Needed
            } else if manager_enabled {
                SetupStepStatus::Done
            } else {
                SetupStepStatus::Needed
            },
            if !manager_configured {
                SetupStepDetail::NeedsConfiguration
            } else if manager_enabled {
                SetupStepDetail::Enabled
            } else {
                SetupStepDetail::NeedsEnable
            },
            "Enable DAM Network Protection in System Settings",
            Some(install_command.clone()),
        ),
        network_extension_step(
            SetupStepKind::NetworkExtensionStart,
            if !manager_configured || !manager_enabled {
                SetupStepStatus::Needed
            } else if manager_connected {
                SetupStepStatus::Done
            } else {
                SetupStepStatus::Needed
            },
            if !manager_configured {
                SetupStepDetail::NeedsConfiguration
            } else if !manager_enabled {
                SetupStepDetail::NeedsEnable
            } else if manager_connected {
                SetupStepDetail::Connected
            } else {
                SetupStepDetail::NeedsStart
            },
            "Enable protection layer",
            Some(install_command),
        ),
    ]
}

fn network_extension_step(
    kind: SetupStepKind,
    status: SetupStepStatus,
    detail: SetupStepDetail,
    message: &str,
    command: Option<Vec<String>>,
) -> SetupStep {
    SetupStep {
        kind,
        status,
        detail,
        message: message.to_string(),
        command,
        requires_confirmation: matches!(status, SetupStepStatus::Needed),
        changes_system: matches!(status, SetupStepStatus::Needed),
    }
}

fn network_extension_install_command(config_path: Option<&PathBuf>) -> Vec<String> {
    let mut command = vec![
        "dam".to_string(),
        "network".to_string(),
        "install-network-extension".to_string(),
    ];
    if let Some(config_path) = config_path {
        command.push("--config".to_string());
        command.push(config_path.display().to_string());
    }
    command.push("--yes".to_string());
    command
}

fn local_ca_setup_step(
    trust_mode: dam_trust::TrustMode,
    state_dir: &std::path::Path,
    _has_active_routes: bool,
) -> SetupStep {
    match trust_mode {
        dam_trust::TrustMode::Disabled => SetupStep {
            kind: SetupStepKind::LocalCa,
            status: SetupStepStatus::Skipped,
            detail: SetupStepDetail::NotRequired,
            message: "local CA trust is not required while trust mode is disabled".to_string(),
            command: None,
            requires_confirmation: false,
            changes_system: false,
        },
        dam_trust::TrustMode::LocalCa => {
            let plan = match dam_trust::local_ca_install_plan(state_dir) {
                Ok(plan) => plan,
                Err(error) => {
                    return SetupStep {
                        kind: SetupStepKind::LocalCa,
                        status: SetupStepStatus::Blocked,
                        detail: SetupStepDetail::Failed,
                        message: format!("local CA trust cannot be inspected: {error}"),
                        command: Some(vec![
                            "damctl".to_string(),
                            "trust".to_string(),
                            "inspect".to_string(),
                        ]),
                        requires_confirmation: false,
                        changes_system: true,
                    };
                }
            };
            if plan
                .artifact
                .as_ref()
                .map(dam_trust::LocalCaRecord::installed)
                .unwrap_or(false)
            {
                return SetupStep {
                    kind: SetupStepKind::LocalCa,
                    status: SetupStepStatus::Done,
                    detail: SetupStepDetail::Ready,
                    message: "DAM local CA is installed in system trust".to_string(),
                    command: None,
                    requires_confirmation: false,
                    changes_system: false,
                };
            }
            if plan.support == dam_trust::TrustSupport::Planned {
                return SetupStep {
                    kind: SetupStepKind::LocalCa,
                    status: SetupStepStatus::Blocked,
                    detail: SetupStepDetail::Unsupported,
                    message: plan.message,
                    command: None,
                    requires_confirmation: false,
                    changes_system: true,
                };
            }
            SetupStep {
                kind: SetupStepKind::LocalCa,
                status: SetupStepStatus::Needed,
                detail: SetupStepDetail::NeedsInstall,
                message: plan.message,
                command: Some(vec![
                    "dam".to_string(),
                    "trust".to_string(),
                    "install-local-ca".to_string(),
                    "--yes".to_string(),
                ]),
                requires_confirmation: true,
                changes_system: true,
            }
        }
    }
}

fn daemon_setup_step(
    network_mode: dam_net::CaptureMode,
    trust_mode: dam_trust::TrustMode,
    state_dir: &std::path::Path,
) -> SetupStep {
    let state_file = state_dir.join("daemon.json");
    let status = match dam_daemon::read_state_from(&state_file) {
        Ok(Some(state)) if dam_daemon::process_is_running(state.pid) => {
            if state.network_mode == network_mode && state.trust.mode == trust_mode {
                return SetupStep {
                    kind: SetupStepKind::Daemon,
                    status: SetupStepStatus::Done,
                    detail: SetupStepDetail::Connected,
                    message: format!("daemon is connected at {}", state.proxy_url),
                    command: None,
                    requires_confirmation: false,
                    changes_system: false,
                };
            }
            return SetupStep {
                kind: SetupStepKind::Daemon,
                status: SetupStepStatus::Blocked,
                detail: SetupStepDetail::Mismatch,
                message: format!(
                    "daemon is already running with network mode {} and trust mode {}; disconnect before changing setup",
                    state.network_mode, state.trust.mode
                ),
                command: Some(vec!["dam".to_string(), "disconnect".to_string()]),
                requires_confirmation: true,
                changes_system: false,
            };
        }
        Ok(Some(_)) => "stale",
        Ok(None) => "disconnected",
        Err(_) => {
            return SetupStep {
                kind: SetupStepKind::Daemon,
                status: SetupStepStatus::Blocked,
                detail: SetupStepDetail::Failed,
                message: "daemon state is unreadable".to_string(),
                command: Some(vec![
                    "damctl".to_string(),
                    "daemon".to_string(),
                    "inspect".to_string(),
                ]),
                requires_confirmation: false,
                changes_system: false,
            };
        }
    };

    let mut command = vec!["dam".to_string(), "connect".to_string()];
    if network_mode != dam_net::CaptureMode::ExplicitProxy {
        command.push("--network-mode".to_string());
        command.push(network_mode.tag().to_string());
    }
    if trust_mode != dam_trust::TrustMode::Disabled {
        command.push("--trust-mode".to_string());
        command.push(trust_mode.tag().to_string());
    }
    SetupStep {
        kind: SetupStepKind::Daemon,
        status: SetupStepStatus::Needed,
        detail: if status == "stale" {
            SetupStepDetail::Stale
        } else {
            SetupStepDetail::Disconnected
        },
        message: format!("DAM is {status}; start DAM"),
        command: Some(command),
        requires_confirmation: false,
        changes_system: false,
    }
}

fn setup_plan_message(state: SetupPlanState, steps: &[SetupStep]) -> String {
    match state {
        SetupPlanState::Ready => "local AI protection is ready".to_string(),
        SetupPlanState::Blocked => steps
            .iter()
            .find(|step| step.status == SetupStepStatus::Blocked)
            .map(|step| format!("setup is blocked: {}", step.message))
            .unwrap_or_else(|| "setup is blocked".to_string()),
        SetupPlanState::NeedsAction => steps
            .iter()
            .find(|step| step.status == SetupStepStatus::Needed)
            .map(|step| format!("next setup action: {}", step.message))
            .unwrap_or_else(|| "setup needs action".to_string()),
    }
}

fn setup_plan_next_action(steps: &[SetupStep]) -> Option<&SetupStep> {
    steps
        .iter()
        .find(|step| step.status == SetupStepStatus::Blocked)
        .or_else(|| {
            steps
                .iter()
                .find(|step| step.status == SetupStepStatus::Needed)
        })
}

fn add_setup_plan_component(
    config: &dam_config::DamConfig,
    options: &DoctorOptions,
    report: &mut dam_api::HealthReport,
) {
    let plan = match setup_plan(
        config,
        &SetupPlanOptions {
            state_dir: options.state_dir.clone(),
            config_path: options.config_path.clone(),
            proxy_url: options.proxy_url.clone(),
            ..SetupPlanOptions::default()
        },
    ) {
        Ok(plan) => plan,
        Err(error) => {
            report.components.push(dam_api::ComponentHealth {
                component: "setup_plan".to_string(),
                state: dam_api::HealthState::Degraded,
                message: format!("setup plan unavailable: {error}"),
            });
            report.diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Warning,
                "setup_plan_unavailable",
                error,
            ));
            return;
        }
    };
    let state = match plan.state {
        SetupPlanState::Ready => dam_api::HealthState::Healthy,
        SetupPlanState::NeedsAction => dam_api::HealthState::Degraded,
        SetupPlanState::Blocked => dam_api::HealthState::Unhealthy,
    };
    report.components.push(dam_api::ComponentHealth {
        component: "setup_plan".to_string(),
        state,
        message: plan.message.clone(),
    });
    for step in plan.steps.iter().filter(|step| {
        matches!(
            step.status,
            SetupStepStatus::Needed | SetupStepStatus::Blocked
        )
    }) {
        report.diagnostics.push(dam_api::Diagnostic::new(
            if step.status == SetupStepStatus::Blocked {
                dam_api::DiagnosticSeverity::Error
            } else {
                dam_api::DiagnosticSeverity::Warning
            },
            format!("setup_{}", step.kind.tag()),
            step.message.clone(),
        ));
    }
}

fn append_health(value: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(value)
        .map_err(|error| format!("invalid proxy url {value}: {error}"))?;
    let path = url.path().trim_end_matches('/');
    url.set_path(&format!("{path}/health"));
    Ok(url.to_string())
}

fn aggregate_state(components: &[dam_api::ComponentHealth]) -> dam_api::HealthState {
    if components
        .iter()
        .any(|component| component.state == dam_api::HealthState::Unhealthy)
    {
        dam_api::HealthState::Unhealthy
    } else if components
        .iter()
        .any(|component| component.state == dam_api::HealthState::Degraded)
    {
        dam_api::HealthState::Degraded
    } else {
        dam_api::HealthState::Healthy
    }
}

fn vault_component(
    config: &dam_config::DamConfig,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    match config.vault.backend {
        dam_config::VaultBackend::Sqlite => dam_api::ComponentHealth {
            component: "vault".to_string(),
            state: dam_api::HealthState::Healthy,
            message: format!("sqlite vault path {}", config.vault.sqlite_path.display()),
        },
        dam_config::VaultBackend::Remote
            if config.failure.vault_write == dam_config::VaultWriteFailureMode::RedactOnly =>
        {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Warning,
                "remote_vault_not_implemented",
                "remote vault backend is configured but this local build only has redact-only fallback",
            ));
            dam_api::ComponentHealth {
                component: "vault".to_string(),
                state: dam_api::HealthState::Degraded,
                message: "remote vault backend is not implemented; redact-only fallback configured"
                    .to_string(),
            }
        }
        dam_config::VaultBackend::Remote => {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Error,
                "remote_vault_not_implemented",
                "remote vault backend is configured but this local build cannot use it with fail-closed behavior",
            ));
            dam_api::ComponentHealth {
                component: "vault".to_string(),
                state: dam_api::HealthState::Unhealthy,
                message: "remote vault backend is not implemented for fail-closed behavior"
                    .to_string(),
            }
        }
    }
}

fn consent_component(
    config: &dam_config::DamConfig,
    _diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    if !config.consent.enabled {
        return dam_api::ComponentHealth {
            component: "consent".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "consent is disabled".to_string(),
        };
    }

    match config.consent.backend {
        dam_config::ConsentBackend::Sqlite => dam_api::ComponentHealth {
            component: "consent".to_string(),
            state: dam_api::HealthState::Healthy,
            message: format!(
                "sqlite consent path {}, default ttl {}s, mcp writes {}",
                config.consent.sqlite_path.display(),
                config.consent.default_ttl_seconds,
                if config.consent.mcp_write_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            ),
        },
    }
}

fn log_component(
    config: &dam_config::DamConfig,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    if !config.log.enabled || config.log.backend == dam_config::LogBackend::None {
        return dam_api::ComponentHealth {
            component: "log".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "logging is disabled".to_string(),
        };
    }

    match config.log.backend {
        dam_config::LogBackend::Sqlite => dam_api::ComponentHealth {
            component: "log".to_string(),
            state: dam_api::HealthState::Healthy,
            message: format!("sqlite log path {}", config.log.sqlite_path.display()),
        },
        dam_config::LogBackend::Remote
            if config.failure.log_write == dam_config::LogWriteFailureMode::WarnContinue =>
        {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Warning,
                "remote_log_not_implemented",
                "remote log backend is configured but this local build only supports warn-and-continue",
            ));
            dam_api::ComponentHealth {
                component: "log".to_string(),
                state: dam_api::HealthState::Degraded,
                message: "remote log backend is not implemented; warn-and-continue configured"
                    .to_string(),
            }
        }
        dam_config::LogBackend::Remote => {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Error,
                "remote_log_not_implemented",
                "remote log backend is configured but this local build cannot use it with fail-closed behavior",
            ));
            dam_api::ComponentHealth {
                component: "log".to_string(),
                state: dam_api::HealthState::Unhealthy,
                message: "remote log backend is not implemented for fail-closed behavior"
                    .to_string(),
            }
        }
        dam_config::LogBackend::None => unreachable!("none handled before backend match"),
    }
}

fn vault_runtime_component(
    config: &dam_config::DamConfig,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    match config.vault.backend {
        dam_config::VaultBackend::Sqlite => match dam_vault::Vault::open(&config.vault.sqlite_path)
        {
            Ok(_) => dam_api::ComponentHealth {
                component: "vault_runtime".to_string(),
                state: dam_api::HealthState::Healthy,
                message: format!(
                    "sqlite vault opens at {}",
                    config.vault.sqlite_path.display()
                ),
            },
            Err(error) => {
                diagnostics.push(dam_api::Diagnostic::new(
                    dam_api::DiagnosticSeverity::Error,
                    "vault_sqlite_unavailable",
                    format!("sqlite vault cannot be opened: {error}"),
                ));
                dam_api::ComponentHealth {
                    component: "vault_runtime".to_string(),
                    state: dam_api::HealthState::Unhealthy,
                    message: format!(
                        "sqlite vault unavailable at {}",
                        config.vault.sqlite_path.display()
                    ),
                }
            }
        },
        dam_config::VaultBackend::Remote => dam_api::ComponentHealth {
            component: "vault_runtime".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "remote vault runtime check is not implemented".to_string(),
        },
    }
}

fn consent_runtime_component(
    config: &dam_config::DamConfig,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    if !config.consent.enabled {
        return dam_api::ComponentHealth {
            component: "consent_runtime".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "consent is disabled".to_string(),
        };
    }

    match config.consent.backend {
        dam_config::ConsentBackend::Sqlite => {
            match dam_consent::ConsentStore::open(&config.consent.sqlite_path) {
                Ok(_) => dam_api::ComponentHealth {
                    component: "consent_runtime".to_string(),
                    state: dam_api::HealthState::Healthy,
                    message: format!(
                        "sqlite consent opens at {}",
                        config.consent.sqlite_path.display()
                    ),
                },
                Err(error) => {
                    diagnostics.push(dam_api::Diagnostic::new(
                        dam_api::DiagnosticSeverity::Error,
                        "consent_sqlite_unavailable",
                        format!("sqlite consent store cannot be opened: {error}"),
                    ));
                    dam_api::ComponentHealth {
                        component: "consent_runtime".to_string(),
                        state: dam_api::HealthState::Unhealthy,
                        message: format!(
                            "sqlite consent unavailable at {}",
                            config.consent.sqlite_path.display()
                        ),
                    }
                }
            }
        }
    }
}

fn log_runtime_component(
    config: &dam_config::DamConfig,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    if !config.log.enabled || config.log.backend == dam_config::LogBackend::None {
        return dam_api::ComponentHealth {
            component: "log_runtime".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "logging is disabled".to_string(),
        };
    }

    match config.log.backend {
        dam_config::LogBackend::Sqlite => match dam_log::LogStore::open(&config.log.sqlite_path) {
            Ok(_) => dam_api::ComponentHealth {
                component: "log_runtime".to_string(),
                state: dam_api::HealthState::Healthy,
                message: format!("sqlite log opens at {}", config.log.sqlite_path.display()),
            },
            Err(error) => {
                diagnostics.push(dam_api::Diagnostic::new(
                    dam_api::DiagnosticSeverity::Error,
                    "log_sqlite_unavailable",
                    format!("sqlite log cannot be opened: {error}"),
                ));
                dam_api::ComponentHealth {
                    component: "log_runtime".to_string(),
                    state: dam_api::HealthState::Unhealthy,
                    message: format!(
                        "sqlite log unavailable at {}",
                        config.log.sqlite_path.display()
                    ),
                }
            }
        },
        dam_config::LogBackend::Remote => dam_api::ComponentHealth {
            component: "log_runtime".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "remote log runtime check is not implemented".to_string(),
        },
        dam_config::LogBackend::None => unreachable!("none handled before backend match"),
    }
}

fn proxy_config_component(
    config: &dam_config::DamConfig,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    if !config.proxy.enabled {
        return dam_api::ComponentHealth {
            component: "proxy_config".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "proxy is disabled".to_string(),
        };
    }

    let mut errors = Vec::new();
    if config.proxy.listen.parse::<SocketAddr>().is_err() {
        errors.push(format!(
            "proxy listen address is invalid: {}",
            config.proxy.listen
        ));
    }
    for target in &config.proxy.targets {
        if reqwest::Url::parse(&target.upstream).is_err() {
            errors.push(format!(
                "proxy target {} has invalid upstream URL {}",
                target.name, target.upstream
            ));
        }
        if let Some(api_key_env) = &target.api_key_env
            && target.api_key.is_none()
        {
            errors.push(format!(
                "proxy target {} requires missing env var {}",
                target.name, api_key_env
            ));
        }
    }

    if errors.is_empty() {
        dam_api::ComponentHealth {
            component: "proxy_config".to_string(),
            state: dam_api::HealthState::Healthy,
            message: format!(
                "proxy enabled on {} with {} target(s)",
                config.proxy.listen,
                config.proxy.targets.len()
            ),
        }
    } else {
        for error in &errors {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Error,
                "proxy_config_invalid",
                error,
            ));
        }
        dam_api::ComponentHealth {
            component: "proxy_config".to_string(),
            state: dam_api::HealthState::Unhealthy,
            message: errors.join("; "),
        }
    }
}

fn failure_modes_component(
    config: &dam_config::DamConfig,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    let mut reduced_modes = Vec::new();

    match config.proxy.default_failure_mode {
        dam_config::ProxyFailureMode::BypassOnError => {
            reduced_modes.push("proxy default bypass_on_error".to_string());
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Warning,
                "proxy_bypass_on_error",
                "proxy default failure mode can forward unprotected traffic when protection fails",
            ));
        }
        dam_config::ProxyFailureMode::RedactOnly => {
            reduced_modes.push("proxy default redact_only".to_string());
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Warning,
                "proxy_redact_only",
                "proxy default failure mode can continue with irreversible placeholders when recoverability is unavailable",
            ));
        }
        dam_config::ProxyFailureMode::BlockOnError => {}
    }

    for target in &config.proxy.targets {
        match target.failure_mode {
            Some(dam_config::ProxyFailureMode::BypassOnError) => {
                reduced_modes.push(format!("proxy target {} bypass_on_error", target.name));
                diagnostics.push(dam_api::Diagnostic::new(
                    dam_api::DiagnosticSeverity::Warning,
                    "proxy_target_bypass_on_error",
                    format!(
                        "proxy target {} can forward unprotected traffic when protection fails",
                        target.name
                    ),
                ));
            }
            Some(dam_config::ProxyFailureMode::RedactOnly) => {
                reduced_modes.push(format!("proxy target {} redact_only", target.name));
                diagnostics.push(dam_api::Diagnostic::new(
                    dam_api::DiagnosticSeverity::Warning,
                    "proxy_target_redact_only",
                    format!(
                        "proxy target {} can continue with irreversible placeholders when recoverability is unavailable",
                        target.name
                    ),
                ));
            }
            Some(dam_config::ProxyFailureMode::BlockOnError) | None => {}
        }
    }

    if config.failure.vault_write == dam_config::VaultWriteFailureMode::RedactOnly {
        reduced_modes.push("vault redact_only".to_string());
        diagnostics.push(dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Warning,
            "vault_redact_only",
            "vault write failures fall back to irreversible redaction",
        ));
    }

    if config.failure.log_write == dam_config::LogWriteFailureMode::WarnContinue {
        reduced_modes.push("log warn_continue".to_string());
        diagnostics.push(dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Warning,
            "log_warn_continue",
            "log write failures do not fail the protected path",
        ));
    }

    if reduced_modes.is_empty() {
        dam_api::ComponentHealth {
            component: "failure_modes".to_string(),
            state: dam_api::HealthState::Healthy,
            message: "failure modes are strict".to_string(),
        }
    } else {
        dam_api::ComponentHealth {
            component: "failure_modes".to_string(),
            state: dam_api::HealthState::Degraded,
            message: format!(
                "{} reduced-protection mode(s): {}",
                reduced_modes.len(),
                reduced_modes.join(", ")
            ),
        }
    }
}

fn router_component(
    config: &dam_config::DamConfig,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    if !config.proxy.enabled {
        return dam_api::ComponentHealth {
            component: "router".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "proxy routing is disabled".to_string(),
        };
    }

    let route = match dam_router::RoutePlan::from_proxy_config(&config.proxy) {
        Ok(route) => route,
        Err(error) => {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Error,
                "router_invalid",
                error.to_string(),
            ));
            return dam_api::ComponentHealth {
                component: "router".to_string(),
                state: dam_api::HealthState::Unhealthy,
                message: error.to_string(),
            };
        }
    };

    let decision = route.decide(&reqwest::header::HeaderMap::new());
    let failure_mode = decision.failure_mode().tag();
    let target = decision.target();
    match decision.auth() {
        dam_router::RouteAuth::CallerPassthrough => dam_api::ComponentHealth {
            component: "router".to_string(),
            state: dam_api::HealthState::Healthy,
            message: format!(
                "target {} routes to {} with caller auth passthrough and {failure_mode}",
                target.name, target.provider
            ),
        },
        dam_router::RouteAuth::TargetApiKey => dam_api::ComponentHealth {
            component: "router".to_string(),
            state: dam_api::HealthState::Healthy,
            message: format!(
                "target {} routes to {} with configured target auth and {failure_mode}",
                target.name, target.provider
            ),
        },
        dam_router::RouteAuth::ConfigRequired => {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Warning,
                "router_config_required",
                format!(
                    "target {} requires {} or provider-compatible caller auth at request time",
                    target.name,
                    target
                        .api_key_env
                        .as_deref()
                        .unwrap_or("an API key env var")
                ),
            ));
            dam_api::ComponentHealth {
                component: "router".to_string(),
                state: dam_api::HealthState::Degraded,
                message: format!(
                    "target {} routes to {}, but auth is required before protected requests can flow",
                    target.name, target.provider
                ),
            }
        }
    }
}

async fn proxy_runtime_component(
    config: &dam_config::DamConfig,
    options: &DoctorOptions,
    diagnostics: &mut Vec<dam_api::Diagnostic>,
) -> dam_api::ComponentHealth {
    if !config.proxy.enabled {
        return dam_api::ComponentHealth {
            component: "proxy_runtime".to_string(),
            state: dam_api::HealthState::Degraded,
            message: "proxy is not configured to run".to_string(),
        };
    }

    let health_url = match proxy_health_url(config, options.proxy_url.as_deref()) {
        Ok(url) => url,
        Err(error) => {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Error,
                "proxy_url_invalid",
                &error,
            ));
            return dam_api::ComponentHealth {
                component: "proxy_runtime".to_string(),
                state: dam_api::HealthState::Unhealthy,
                message: error,
            };
        }
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(2_000))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Error,
                "http_client_unavailable",
                error.to_string(),
            ));
            return dam_api::ComponentHealth {
                component: "proxy_runtime".to_string(),
                state: dam_api::HealthState::Unhealthy,
                message: "failed to build HTTP client".to_string(),
            };
        }
    };

    let report = match client.get(&health_url).send().await {
        Ok(response) => match response.json::<dam_api::ProxyReport>().await {
            Ok(report) => report,
            Err(error) => {
                diagnostics.push(dam_api::Diagnostic::new(
                    dam_api::DiagnosticSeverity::Error,
                    "proxy_status_unreadable",
                    format!("DAM proxy returned unreadable health JSON: {error}"),
                ));
                return dam_api::ComponentHealth {
                    component: "proxy_runtime".to_string(),
                    state: dam_api::HealthState::Unhealthy,
                    message: "DAM proxy returned unreadable health JSON".to_string(),
                };
            }
        },
        Err(error) => {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Error,
                "dam_down",
                format!("DAM proxy is not reachable at {health_url}: {error}"),
            ));
            return dam_api::ComponentHealth {
                component: "proxy_runtime".to_string(),
                state: dam_api::HealthState::Unhealthy,
                message: format!("DAM proxy is not reachable at {health_url}"),
            };
        }
    };

    let state = proxy_state_to_health(report.state);
    for diagnostic in &report.diagnostics {
        diagnostics.push(diagnostic.clone());
    }
    dam_api::ComponentHealth {
        component: "proxy_runtime".to_string(),
        state,
        message: format!(
            "proxy reports {}: {}",
            proxy_state_tag(report.state),
            report.message
        ),
    }
}

fn proxy_state_to_health(state: dam_api::ProxyState) -> dam_api::HealthState {
    match state {
        dam_api::ProxyState::Protected => dam_api::HealthState::Healthy,
        dam_api::ProxyState::Bypassing | dam_api::ProxyState::ConfigRequired => {
            dam_api::HealthState::Degraded
        }
        dam_api::ProxyState::Blocked
        | dam_api::ProxyState::ProviderDown
        | dam_api::ProxyState::DamDown => dam_api::HealthState::Unhealthy,
    }
}

fn proxy_state_tag(state: dam_api::ProxyState) -> &'static str {
    match state {
        dam_api::ProxyState::Protected => "protected",
        dam_api::ProxyState::Bypassing => "bypassing",
        dam_api::ProxyState::Blocked => "blocked",
        dam_api::ProxyState::ProviderDown => "provider_down",
        dam_api::ProxyState::ConfigRequired => "config_required",
        dam_api::ProxyState::DamDown => "dam_down",
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
