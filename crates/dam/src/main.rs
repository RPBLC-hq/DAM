use std::{
    collections::BTreeSet,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use serde::Serialize;

mod log_rendering;

use log_rendering::{
    filtered_log_entries, limited_log_event_views, log_operation_summaries, render_log_events,
    render_log_summaries,
};

const DEFAULT_LISTEN: &str = "127.0.0.1:7828";
const DEFAULT_LOG_PATH: &str = "log.db";
const DAM_WEB_BIN_ENV: &str = "DAM_WEB_BIN";
const LOGIN_ITEM_MARKER_RELPATH: &str = "startup/login-item.txt";
const LOGIN_ITEM_SKIP_MARKER_RELPATH: &str = "startup/login-item-skipped.txt";
const LAUNCH_AGENT_PLIST_RELPATH: &str = "Library/LaunchAgents/com.rpblc.dam-tray.plist";
const DEFAULT_CONSENT_PATH: &str = "consent.db";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cli {
    command: CommandKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandKind {
    Connect(ConnectArgs),
    Disconnect(DisconnectArgs),
    Doctor(DoctorArgs),
    Status(StatusArgs),
    Logs(LogsArgs),
    Profile(ProfileArgs),
    Trust(TrustArgs),
    Network(NetworkArgs),
    Setup(SetupArgs),
    Startup(StartupArgs),
    Integrations(IntegrationArgs),
    Web(WebArgs),
    DaemonRun(dam_daemon::ProxyOptions),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebArgs {
    args: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StatusArgs {
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorArgs {
    json: bool,
    config_path: Option<PathBuf>,
    state_dir: Option<PathBuf>,
    proxy_url: Option<String>,
    network_mode: dam_net::CaptureMode,
    trust_mode: dam_trust::TrustMode,
}

impl Default for DoctorArgs {
    fn default() -> Self {
        Self {
            json: false,
            config_path: None,
            state_dir: None,
            proxy_url: None,
            network_mode: dam_net::CaptureMode::ExplicitProxy,
            trust_mode: dam_trust::TrustMode::Disabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SetupArgs {
    Status(SetupPlanArgs),
    Plan(SetupPlanArgs),
    NextAction(SetupPlanArgs),
    Resume(SetupPlanArgs),
    Rescue(SetupRescueArgs),
    Repair(SetupRepairArgs),
    ExportDiagnostics(SetupPlanArgs),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetupPlanArgs {
    json: bool,
    config_path: Option<PathBuf>,
    state_dir: Option<PathBuf>,
    proxy_url: Option<String>,
    network_mode: dam_net::CaptureMode,
    trust_mode: dam_trust::TrustMode,
}

impl Default for SetupPlanArgs {
    fn default() -> Self {
        Self {
            json: false,
            config_path: None,
            state_dir: None,
            proxy_url: None,
            network_mode: dam_net::CaptureMode::ExplicitProxy,
            trust_mode: dam_trust::TrustMode::Disabled,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SetupRescueArgs {
    json: bool,
    yes: bool,
    state_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetupRepairArgs {
    plan: SetupPlanArgs,
    yes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogsArgs {
    json: bool,
    limit: usize,
    after_id: Option<i64>,
    operation_id: Option<String>,
    events: bool,
}

impl Default for LogsArgs {
    fn default() -> Self {
        Self {
            json: false,
            limit: 20,
            after_id: None,
            operation_id: None,
            events: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectArgs {
    proxy: dam_daemon::ProxyOptions,
    apply_profile_ids: Vec<String>,
    json: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DisconnectArgs {
    stop_daemon: bool,
    json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProfileArgs {
    Status { json: bool },
    Set { profile_id: String, json: bool },
    Clear { json: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TrustArgs {
    GenerateArtifact { json: bool },
    DeleteArtifact { json: bool },
    InstallTrust { json: bool, yes: bool },
    RemoveTrust { json: bool, yes: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NetworkArgs {
    InstallProxy {
        config_path: Option<PathBuf>,
        json: bool,
        yes: bool,
    },
    RemoveProxy {
        json: bool,
        yes: bool,
    },
    InstallNetworkExtension {
        config_path: Option<PathBuf>,
        json: bool,
        yes: bool,
    },
    RemoveNetworkExtension {
        json: bool,
        yes: bool,
    },
    Status {
        json: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupArgs {
    Status { json: bool },
    SkipOpenAtLogin { json: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IntegrationArgs {
    List {
        json: bool,
        proxy_url: Option<String>,
    },
    Show {
        profile_id: String,
        json: bool,
        proxy_url: Option<String>,
    },
    Apply {
        profile_id: String,
        dry_run: bool,
        json: bool,
        proxy_url: Option<String>,
        target_path: Option<PathBuf>,
    },
    Rollback {
        profile_id: String,
        json: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
struct StatusView {
    state: &'static str,
    message: String,
    daemon: Option<dam_daemon::DaemonState>,
    proxy: Option<dam_api::ProxyReport>,
    active_profile: Option<dam_integrations::ActiveProfileState>,
    active_profile_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileStatusView {
    active_profile: Option<dam_integrations::ActiveProfileState>,
    enabled_profiles: Vec<dam_integrations::EnabledIntegrationState>,
    proxy_url: String,
    applies: Vec<dam_integrations::IntegrationApplyInspection>,
    inspection_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct StartupStatusView {
    state: &'static str,
    message: String,
    platform: &'static str,
    state_dir: PathBuf,
    marker: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
struct SetupNextActionView {
    state: dam_diagnostics::SetupPlanState,
    message: String,
    state_dir: PathBuf,
    proxy_url: String,
    network_mode: dam_net::CaptureMode,
    trust_mode: dam_trust::TrustMode,
    next_action: Option<dam_diagnostics::SetupStep>,
}

#[derive(Debug, Clone, Serialize)]
struct ConnectResultView {
    state: &'static str,
    message: String,
    proxy_url: Option<String>,
    network_mode: dam_net::CaptureMode,
    trust_mode: dam_trust::TrustMode,
    protection_enabled: bool,
    restarted: bool,
    target: Option<String>,
    upstream: Option<String>,
    applied_profiles: Vec<ConnectApplyView>,
}

#[derive(Debug, Clone, Serialize)]
struct ConnectApplyView {
    profile_id: String,
    message: String,
    rollback_available: bool,
    changes: Vec<dam_integrations::IntegrationFileChange>,
}

#[derive(Debug, Clone, Serialize)]
struct DisconnectResultView {
    state: &'static str,
    message: String,
    stopped: bool,
    protection_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
struct UnsupportedPlatformView {
    state: &'static str,
    support: &'static str,
    platform: &'static str,
    backend: &'static str,
    message: String,
    fallback_command: Vec<String>,
    system_routes_changed: bool,
}

#[derive(Debug, Clone, Serialize)]
struct LocalCaGenerateView {
    state: &'static str,
    artifact: dam_trust::LocalCaArtifact,
}

#[derive(Debug, Clone, Serialize)]
struct LocalCaDeleteView {
    state: &'static str,
    deleted: bool,
    state_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectProfileExpansion {
    args: Vec<String>,
    selected_profile_ids: Vec<String>,
    traffic_app_ids: Option<Vec<String>>,
    apply_profile_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ConnectProfileSelection {
    profile_ids: Vec<String>,
    explicit_selection: bool,
    integration_state_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectApplyOutcome {
    result: dam_integrations::IntegrationApplyResult,
    rollback_available: bool,
}

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    };

    std::process::exit(code);
}

async fn run() -> Result<i32, String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let enabled_connect_profiles = enabled_profiles_for_connect_parse(&args)?;
    match parse_cli_with_connect_profiles(args, enabled_connect_profiles)? {
        Cli {
            command: CommandKind::Help,
        } => {
            println!("{}", usage());
            Ok(0)
        }
        Cli {
            command: CommandKind::Connect(args),
        } => connect(args).await,
        Cli {
            command: CommandKind::Disconnect(args),
        } => disconnect(args).await,
        Cli {
            command: CommandKind::Doctor(args),
        } => doctor(args).await,
        Cli {
            command: CommandKind::Status(args),
        } => status(args).await,
        Cli {
            command: CommandKind::Logs(args),
        } => logs_command(args),
        Cli {
            command: CommandKind::Profile(args),
        } => profile_command(args),
        Cli {
            command: CommandKind::Trust(args),
        } => trust_command(args),
        Cli {
            command: CommandKind::Network(args),
        } => network_command(args),
        Cli {
            command: CommandKind::Setup(args),
        } => setup_command(args).await,
        Cli {
            command: CommandKind::Startup(args),
        } => startup_command(args),
        Cli {
            command: CommandKind::Integrations(args),
        } => integrations(args).await,
        Cli {
            command: CommandKind::Web(args),
        } => web_command(args),
        Cli {
            command: CommandKind::DaemonRun(args),
        } => daemon_run(args).await,
    }
}

async fn connect(mut args: ConnectArgs) -> Result<i32, String> {
    normalize_connect_state_paths(&mut args.proxy)?;
    let mut config = dam_daemon::proxy_config(&args.proxy)?;
    let mut applied_profiles = Vec::new();
    let mut restarted = false;

    match dam_daemon::daemon_status().map_err(|error| error.to_string())? {
        dam_daemon::DaemonStatus::Connected(state) => {
            if !daemon_proxy_targets_match(&state, &config.proxy.targets)
                || !daemon_transparent_routes_match(&state, &config)
                || !daemon_runtime_paths_match(&state, &args.proxy)
            {
                ensure_connect_transparent_prerequisites(&args.proxy, &config, None)?;
                if !args.json {
                    println!("DAM profile traffic scope changed; restarting daemon");
                }
                restarted = true;
                stop_connected_daemon(&state).await?;
            } else if !daemon_executable_matches_current(&state)? {
                if connect_setup_change_requested(&state, &args.proxy) && state.protection_enabled {
                    return Err(format!(
                        "DAM is already connected with network mode {} and trust mode {}; run `dam disconnect --stop` before changing setup",
                        state.network_mode, state.trust.mode
                    ));
                }
                args.proxy = proxy_options_for_existing_daemon(&state, &args.proxy);
                config = dam_daemon::proxy_config(&args.proxy)?;
                ensure_connect_transparent_prerequisites(&args.proxy, &config, None)?;
                if !args.json {
                    println!("DAM daemon executable changed; restarting daemon");
                }
                restarted = true;
                stop_connected_daemon(&state).await?;
            } else {
                if connect_setup_change_requested(&state, &args.proxy) {
                    if !state.protection_enabled {
                        dam_daemon::set_protection_enabled(true)
                            .map_err(|error| error.to_string())?;
                        for profile_id in &args.apply_profile_ids {
                            let outcome = apply_connect_profile(profile_id, &state.proxy_url)?;
                            if !args.json {
                                print!("{}", render_connect_apply_outcome(&outcome));
                            }
                            applied_profiles.push(connect_apply_view(&outcome));
                        }
                        return finish_connect(
                            &args,
                            ConnectResultView {
                                state: "connected",
                                message: format!(
                                    "DAM protection enabled at {} using existing network mode {} and trust mode {}",
                                    state.proxy_url, state.network_mode, state.trust.mode
                                ),
                                proxy_url: Some(state.proxy_url),
                                network_mode: state.network_mode,
                                trust_mode: state.trust.mode,
                                protection_enabled: true,
                                restarted: false,
                                target: state.target_name,
                                upstream: state.upstream,
                                applied_profiles,
                            },
                        );
                    }
                    return Err(format!(
                        "DAM is already connected with network mode {} and trust mode {}; run `dam disconnect --stop` before changing setup",
                        state.network_mode, state.trust.mode
                    ));
                }
                dam_daemon::set_protection_enabled(true).map_err(|error| error.to_string())?;
                for profile_id in &args.apply_profile_ids {
                    let outcome = apply_connect_profile(profile_id, &state.proxy_url)?;
                    if !args.json {
                        print!("{}", render_connect_apply_outcome(&outcome));
                    }
                    applied_profiles.push(connect_apply_view(&outcome));
                }
                return finish_connect(
                    &args,
                    ConnectResultView {
                        state: "connected",
                        message: format!("DAM protection enabled at {}", state.proxy_url),
                        proxy_url: Some(state.proxy_url),
                        network_mode: state.network_mode,
                        trust_mode: state.trust.mode,
                        protection_enabled: true,
                        restarted: false,
                        target: state.target_name,
                        upstream: state.upstream,
                        applied_profiles,
                    },
                );
            }
        }
        dam_daemon::DaemonStatus::Stale(state) => {
            dam_daemon::remove_state_if_pid(state.pid).map_err(|error| error.to_string())?;
        }
        dam_daemon::DaemonStatus::Disconnected => {}
    }

    ensure_connect_transparent_prerequisites(&args.proxy, &config, None)?;

    for profile_id in &args.apply_profile_ids {
        let proxy_url = proxy_url_for_connect_apply(&args.proxy)?;
        let outcome = apply_connect_profile(profile_id, &proxy_url)?;
        if !args.json {
            print!("{}", render_connect_apply_outcome(&outcome));
        }
        applied_profiles.push(connect_apply_view(&outcome));
    }

    let exe = std::env::current_exe()
        .map_err(|error| format!("failed to locate current dam executable: {error}"))?;
    let mut child = StdCommand::new(exe);
    child
        .arg("daemon-run")
        .args(dam_daemon::proxy_options_to_args(&args.proxy))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    child.process_group(0);
    child
        .spawn()
        .map_err(|error| format!("failed to start DAM daemon: {error}"))?;

    let state = wait_for_daemon_ready(Duration::from_secs(30)).await?;
    finish_connect(
        &args,
        ConnectResultView {
            state: "connected",
            message: format!("DAM connected at {}", state.proxy_url),
            proxy_url: Some(state.proxy_url),
            network_mode: state.network_mode,
            trust_mode: state.trust.mode,
            protection_enabled: state.protection_enabled,
            restarted,
            target: state.target_name,
            upstream: state.upstream,
            applied_profiles,
        },
    )
}

fn finish_connect(args: &ConnectArgs, view: ConnectResultView) -> Result<i32, String> {
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view)
                .map_err(|error| format!("failed to serialize connect result: {error}"))?
        );
        return Ok(0);
    }

    println!("{}", view.message);
    if let Some(target) = view.target.as_deref() {
        println!("target: {target}");
    }
    if let Some(upstream) = view.upstream.as_deref() {
        println!("upstream: {upstream}");
    }
    Ok(0)
}

fn connect_apply_view(outcome: &ConnectApplyOutcome) -> ConnectApplyView {
    ConnectApplyView {
        profile_id: outcome.result.profile_id.clone(),
        message: outcome.result.message.clone(),
        rollback_available: outcome.rollback_available,
        changes: outcome.result.changes.clone(),
    }
}

fn normalize_connect_state_paths(proxy: &mut dam_daemon::ProxyOptions) -> Result<(), String> {
    let paths = dam_daemon::state_paths().map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&paths.state_dir).map_err(|error| {
        format!(
            "failed to create DAM state directory {}: {error}",
            paths.state_dir.display()
        )
    })?;
    proxy.vault_path = state_runtime_path(&paths.state_dir, &proxy.vault_path);
    proxy.log_path = proxy
        .log_path
        .as_ref()
        .map(|path| state_runtime_path(&paths.state_dir, path));
    proxy.consent_path = Some(state_runtime_path(
        &paths.state_dir,
        proxy
            .consent_path
            .as_deref()
            .unwrap_or_else(|| Path::new(DEFAULT_CONSENT_PATH)),
    ));
    Ok(())
}

fn state_runtime_path(state_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        state_dir.join(path)
    }
}

fn proxy_options_for_existing_daemon(
    state: &dam_daemon::DaemonState,
    requested: &dam_daemon::ProxyOptions,
) -> dam_daemon::ProxyOptions {
    let mut proxy = requested.clone();
    proxy.config_path = state
        .config_path
        .clone()
        .or_else(|| requested.config_path.clone());
    proxy.listen = state.listen.clone();
    proxy.network_mode = state.network_mode;
    proxy.network_mode_explicit = false;
    proxy.trust_mode = state.trust.mode;
    proxy.trust_mode_explicit = false;
    proxy.targets = proxy_targets_for_existing_daemon(state);
    if proxy.targets.is_none() {
        if let Some(target_name) = &state.target_name {
            proxy.target_name = target_name.clone();
        }
        if let Some(provider) = &state.target_provider {
            proxy.provider = provider.clone();
        }
        if let Some(upstream) = &state.upstream {
            proxy.upstream = upstream.clone();
        }
    }
    proxy.vault_path = existing_daemon_path_or_requested(&state.vault_path, &requested.vault_path);
    proxy.log_path = existing_daemon_optional_path_or_requested(
        state.log_path.as_deref(),
        requested.log_path.as_deref(),
    );
    proxy.consent_path = existing_daemon_optional_path_or_requested(
        state.consent_path.as_deref(),
        requested.consent_path.as_deref(),
    );
    proxy.resolve_inbound = Some(state.resolve_inbound);
    proxy
}

fn existing_daemon_optional_path_or_requested(
    current: Option<&Path>,
    requested: Option<&Path>,
) -> Option<PathBuf> {
    match (current, requested) {
        (Some(current), Some(requested)) => {
            Some(existing_daemon_path_or_requested(current, requested))
        }
        (Some(current), None) if current.is_absolute() => Some(current.to_path_buf()),
        (None, Some(requested)) => Some(requested.to_path_buf()),
        (Some(current), None) => Some(current.to_path_buf()),
        (None, None) => None,
    }
}

fn existing_daemon_path_or_requested(current: &Path, requested: &Path) -> PathBuf {
    if current.is_absolute() {
        current.to_path_buf()
    } else {
        requested.to_path_buf()
    }
}

fn proxy_targets_for_existing_daemon(
    state: &dam_daemon::DaemonState,
) -> Option<Vec<dam_config::ProxyTargetConfig>> {
    if !state.proxy_targets.is_empty() {
        return Some(
            state
                .proxy_targets
                .iter()
                .map(|target| dam_config::ProxyTargetConfig {
                    name: target.name.clone(),
                    provider: target.provider.clone(),
                    upstream: target.upstream.clone(),
                    auth: dam_net::UpstreamAuthConfig::default(),
                    failure_mode: None,
                    api_key_env: None,
                    api_key: None,
                })
                .collect(),
        );
    }

    let (Some(name), Some(provider), Some(upstream)) = (
        state.target_name.as_ref(),
        state.target_provider.as_ref(),
        state.upstream.as_ref(),
    ) else {
        return None;
    };

    Some(vec![dam_config::ProxyTargetConfig {
        name: name.clone(),
        provider: provider.clone(),
        upstream: upstream.clone(),
        auth: dam_net::UpstreamAuthConfig::default(),
        failure_mode: None,
        api_key_env: None,
        api_key: None,
    }])
}

fn daemon_executable_matches_current(state: &dam_daemon::DaemonState) -> Result<bool, String> {
    let Some(executable_path) = state.executable_path.as_deref() else {
        return Ok(false);
    };
    let Some(executable_sha256) = state.executable_sha256.as_deref() else {
        return Ok(false);
    };
    let current = std::env::current_exe()
        .map_err(|error| format!("failed to locate current dam executable: {error}"))?;
    let current_sha256 = dam_daemon::executable_sha256(&current)
        .map_err(|error| format!("failed to fingerprint current dam executable: {error}"))?;

    Ok(paths_match(executable_path, &current) && executable_sha256 == current_sha256)
}

fn paths_match(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn daemon_runtime_paths_match(
    state: &dam_daemon::DaemonState,
    proxy: &dam_daemon::ProxyOptions,
) -> bool {
    paths_match(&state.vault_path, &proxy.vault_path)
        && optional_paths_match(state.log_path.as_deref(), proxy.log_path.as_deref())
        && optional_paths_match(state.consent_path.as_deref(), proxy.consent_path.as_deref())
}

fn optional_paths_match(left: Option<&Path>, right: Option<&Path>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => paths_match(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn daemon_proxy_targets_match(
    state: &dam_daemon::DaemonState,
    requested_targets: &[dam_config::ProxyTargetConfig],
) -> bool {
    let current_targets: BTreeSet<_> = if state.proxy_targets.is_empty() {
        legacy_daemon_proxy_target_set(state)
    } else {
        state
            .proxy_targets
            .iter()
            .map(|target| {
                (
                    target.name.clone(),
                    target.provider.clone(),
                    target.upstream.clone(),
                )
            })
            .collect()
    };
    let requested_targets = requested_targets
        .iter()
        .map(|target| {
            (
                target.name.clone(),
                target.provider.clone(),
                target.upstream.clone(),
            )
        })
        .collect::<BTreeSet<_>>();

    current_targets == requested_targets
}

fn daemon_transparent_routes_match(
    state: &dam_daemon::DaemonState,
    config: &dam_config::DamConfig,
) -> bool {
    let current_routes = state
        .transparent_routes
        .iter()
        .map(route_identity)
        .collect::<BTreeSet<_>>();
    let requested_routes =
        dam_net::traffic_routes_from_profile(&config.traffic.effective_profile())
            .iter()
            .map(route_identity)
            .collect::<BTreeSet<_>>();

    current_routes == requested_routes
}

fn route_identity(route: &dam_net::TrafficRoute) -> (String, String, String, String, &'static str) {
    (
        route.host.clone(),
        route.provider.clone(),
        route.target_name.clone(),
        route.upstream.clone(),
        route.adapter.tag(),
    )
}

fn legacy_daemon_proxy_target_set(
    state: &dam_daemon::DaemonState,
) -> BTreeSet<(String, String, String)> {
    match (
        state.target_name.as_ref(),
        state.target_provider.as_ref(),
        state.upstream.as_ref(),
    ) {
        (Some(name), Some(provider), Some(upstream)) => {
            BTreeSet::from([(name.clone(), provider.clone(), upstream.clone())])
        }
        _ => BTreeSet::new(),
    }
}

fn connect_setup_change_requested(
    state: &dam_daemon::DaemonState,
    proxy: &dam_daemon::ProxyOptions,
) -> bool {
    (proxy.network_mode_explicit && state.network_mode != proxy.network_mode)
        || (proxy.trust_mode_explicit && state.trust.mode != proxy.trust_mode)
}

async fn stop_connected_daemon(state: &dam_daemon::DaemonState) -> Result<(), String> {
    dam_daemon::terminate_process(state.pid).map_err(|error| error.to_string())?;
    wait_for_daemon_stop(state.pid, Duration::from_secs(5)).await;
    dam_daemon::remove_state_if_pid(state.pid).map_err(|error| error.to_string())?;
    Ok(())
}

async fn disconnect(args: DisconnectArgs) -> Result<i32, String> {
    match dam_daemon::daemon_status().map_err(|error| error.to_string())? {
        dam_daemon::DaemonStatus::Disconnected => {
            print_disconnect_result(
                &args,
                DisconnectResultView {
                    state: "disconnected",
                    message: "DAM is not connected".to_string(),
                    stopped: false,
                    protection_enabled: false,
                },
            )?;
            Ok(0)
        }
        dam_daemon::DaemonStatus::Stale(state) => {
            dam_daemon::remove_state_if_pid(state.pid).map_err(|error| error.to_string())?;
            print_disconnect_result(
                &args,
                DisconnectResultView {
                    state: "stale_removed",
                    message: "Removed stale DAM daemon state".to_string(),
                    stopped: true,
                    protection_enabled: false,
                },
            )?;
            Ok(0)
        }
        dam_daemon::DaemonStatus::Connected(state) => {
            if !args.stop_daemon {
                dam_daemon::set_protection_enabled(false).map_err(|error| error.to_string())?;
                print_disconnect_result(
                    &args,
                    DisconnectResultView {
                        state: "paused",
                        message: "DAM protection paused; daemon remains active".to_string(),
                        stopped: false,
                        protection_enabled: false,
                    },
                )?;
                return Ok(0);
            }
            stop_connected_daemon(&state).await?;
            print_disconnect_result(
                &args,
                DisconnectResultView {
                    state: "disconnected",
                    message: "DAM disconnected".to_string(),
                    stopped: true,
                    protection_enabled: false,
                },
            )?;
            Ok(0)
        }
    }
}

fn print_disconnect_result(
    args: &DisconnectArgs,
    view: DisconnectResultView,
) -> Result<(), String> {
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view)
                .map_err(|error| format!("failed to serialize disconnect result: {error}"))?
        );
    } else {
        println!("{}", view.message);
    }
    Ok(())
}

async fn status(args: StatusArgs) -> Result<i32, String> {
    let (active_profile, active_profile_error) = active_profile_for_status();
    let view = match dam_daemon::daemon_status().map_err(|error| error.to_string())? {
        dam_daemon::DaemonStatus::Disconnected => StatusView {
            state: "disconnected",
            message: "DAM is not connected".to_string(),
            daemon: None,
            proxy: None,
            active_profile,
            active_profile_error,
        },
        dam_daemon::DaemonStatus::Stale(state) => StatusView {
            state: "stale",
            message: format!("daemon state points at stopped pid {}", state.pid),
            daemon: Some(state),
            proxy: None,
            active_profile,
            active_profile_error,
        },
        dam_daemon::DaemonStatus::Connected(state) => {
            let proxy = fetch_proxy_report(&state.proxy_url).await;
            match proxy {
                Ok(report) => StatusView {
                    state: match report.state {
                        dam_api::ProxyState::Protected => "connected",
                        dam_api::ProxyState::Bypassing => "bypassing",
                        _ => "degraded",
                    },
                    message: report.message.clone(),
                    daemon: Some(state),
                    proxy: Some(report),
                    active_profile,
                    active_profile_error,
                },
                Err(error) => StatusView {
                    state: "degraded",
                    message: error,
                    daemon: Some(state),
                    proxy: None,
                    active_profile,
                    active_profile_error,
                },
            }
        }
    };
    let code = if matches!(view.state, "connected" | "bypassing") {
        0
    } else {
        1
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view)
                .map_err(|error| format!("failed to serialize status: {error}"))?
        );
    } else {
        print!("{}", render_status_view(&view));
    }

    Ok(code)
}

async fn doctor(args: DoctorArgs) -> Result<i32, String> {
    let overrides = dam_config::ConfigOverrides {
        config_path: args.config_path.clone(),
        ..dam_config::ConfigOverrides::default()
    };
    let config = dam_config::load(&overrides).map_err(|error| error.to_string())?;
    let report = dam_diagnostics::doctor_report(
        &config,
        &dam_diagnostics::DoctorOptions {
            proxy_url: args.proxy_url,
            state_dir: args.state_dir,
            config_path: args.config_path,
            network_mode: args.network_mode,
            trust_mode: args.trust_mode,
        },
    )
    .await;
    let code = if report.state == dam_api::HealthState::Unhealthy {
        1
    } else {
        0
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to serialize doctor report: {error}"))?
        );
    } else {
        print!("{}", render_health_report(&report));
    }

    Ok(code)
}

async fn setup_command(args: SetupArgs) -> Result<i32, String> {
    match args {
        SetupArgs::Status(args) | SetupArgs::Plan(args) => setup_plan_command(args, false),
        SetupArgs::NextAction(args) | SetupArgs::Resume(args) => setup_plan_command(args, true),
        SetupArgs::Rescue(args) => setup_rescue_command(args).await,
        SetupArgs::Repair(args) => setup_repair_command(args),
        SetupArgs::ExportDiagnostics(args) => setup_export_diagnostics_command(args).await,
    }
}

fn setup_plan_command(args: SetupPlanArgs, next_action_only: bool) -> Result<i32, String> {
    let config = dam_config::load(&dam_config::ConfigOverrides {
        config_path: args.config_path.clone(),
        ..dam_config::ConfigOverrides::default()
    })
    .map_err(|error| error.to_string())?;
    let plan = dam_diagnostics::setup_plan(
        &config,
        &dam_diagnostics::SetupPlanOptions {
            state_dir: args.state_dir,
            config_path: args.config_path,
            proxy_url: args.proxy_url,
            network_mode: args.network_mode,
            trust_mode: args.trust_mode,
        },
    )?;
    let code = if plan.state == dam_diagnostics::SetupPlanState::Ready {
        0
    } else {
        1
    };

    if next_action_only {
        let view = SetupNextActionView {
            state: plan.state,
            message: plan.message,
            state_dir: plan.state_dir,
            proxy_url: plan.proxy_url,
            network_mode: plan.network_mode,
            trust_mode: plan.trust_mode,
            next_action: plan.next_action,
        };
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&view)
                    .map_err(|error| format!("failed to serialize setup next action: {error}"))?
            );
        } else {
            print!("{}", render_setup_next_action(&view));
        }
        return Ok(code);
    }

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&plan)
                .map_err(|error| format!("failed to serialize setup plan: {error}"))?
        );
    } else {
        print!("{}", render_setup_plan(&plan));
    }

    Ok(code)
}

async fn setup_rescue_command(args: SetupRescueArgs) -> Result<i32, String> {
    let view = dam_diagnostics::setup_rescue(&dam_diagnostics::SetupRescueOptions {
        state_dir: args.state_dir,
        proxy_url: Some(format!("http://{DEFAULT_LISTEN}")),
        apply: args.yes,
    })?;
    print_setup_rescue_view(&view, args.json)?;

    Ok(if view.is_blocked() { 1 } else { 0 })
}

fn setup_repair_command(args: SetupRepairArgs) -> Result<i32, String> {
    let config = dam_config::load(&dam_config::ConfigOverrides {
        config_path: args.plan.config_path.clone(),
        ..dam_config::ConfigOverrides::default()
    })
    .map_err(|error| error.to_string())?;
    let view = dam_diagnostics::setup_repair(
        &config,
        &dam_diagnostics::SetupRepairOptions {
            setup: dam_diagnostics::SetupPlanOptions {
                state_dir: args.plan.state_dir,
                config_path: args.plan.config_path,
                proxy_url: args.plan.proxy_url,
                network_mode: args.plan.network_mode,
                trust_mode: args.plan.trust_mode,
            },
            apply: args.yes,
        },
    )?;
    let code = if view.rescue.is_blocked()
        || view.setup_plan.state != dam_diagnostics::SetupPlanState::Ready
    {
        1
    } else {
        0
    };

    if args.plan.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view)
                .map_err(|error| format!("failed to serialize setup repair result: {error}"))?
        );
    } else {
        print!("{}", render_setup_repair_view(&view));
    }
    Ok(code)
}

async fn setup_export_diagnostics_command(args: SetupPlanArgs) -> Result<i32, String> {
    let config = dam_config::load(&dam_config::ConfigOverrides {
        config_path: args.config_path.clone(),
        ..dam_config::ConfigOverrides::default()
    })
    .map_err(|error| error.to_string())?;
    let view = dam_diagnostics::setup_diagnostics_export(
        &config,
        &dam_diagnostics::DoctorOptions {
            proxy_url: args.proxy_url.clone(),
            state_dir: args.state_dir.clone(),
            config_path: args.config_path.clone(),
            network_mode: args.network_mode,
            trust_mode: args.trust_mode,
        },
        &dam_diagnostics::SetupPlanOptions {
            state_dir: args.state_dir,
            config_path: args.config_path,
            proxy_url: args.proxy_url,
            network_mode: args.network_mode,
            trust_mode: args.trust_mode,
        },
    )
    .await?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view).map_err(|error| {
                format!("failed to serialize setup diagnostics export: {error}")
            })?
        );
    } else {
        print!("{}", render_setup_diagnostics_export(&view));
    }
    Ok(0)
}

fn logs_command(args: LogsArgs) -> Result<i32, String> {
    let log_path = current_log_path()?
        .ok_or_else(|| "DAM logging is disabled for the current daemon/config".to_string())?;
    let store = dam_log::LogStore::open(&log_path)
        .map_err(|error| format!("failed to open DAM log at {}: {error}", log_path.display()))?;
    let entries = filtered_log_entries(store.list().map_err(|error| error.to_string())?, &args);

    if args.json {
        if args.events || args.operation_id.is_some() {
            let events = limited_log_event_views(entries, args.limit);
            println!(
                "{}",
                serde_json::to_string_pretty(&events)
                    .map_err(|error| format!("failed to serialize logs: {error}"))?
            );
        } else {
            let summaries = log_operation_summaries(entries, args.limit);
            println!(
                "{}",
                serde_json::to_string_pretty(&summaries)
                    .map_err(|error| format!("failed to serialize log summaries: {error}"))?
            );
        }
        return Ok(0);
    }

    if args.events || args.operation_id.is_some() {
        print!("{}", render_log_events(&entries, args.limit));
    } else {
        let summaries = log_operation_summaries(entries, args.limit);
        print!("{}", render_log_summaries(&summaries));
    }

    Ok(0)
}

fn current_log_path() -> Result<Option<PathBuf>, String> {
    match dam_daemon::daemon_status().map_err(|error| error.to_string())? {
        dam_daemon::DaemonStatus::Connected(state) | dam_daemon::DaemonStatus::Stale(state) => {
            Ok(state.log_path)
        }
        dam_daemon::DaemonStatus::Disconnected => {
            let paths = dam_daemon::state_paths().map_err(|error| error.to_string())?;
            Ok(Some(paths.state_dir.join(DEFAULT_LOG_PATH)))
        }
    }
}

fn profile_command(args: ProfileArgs) -> Result<i32, String> {
    let state_dir = integration_state_dir()?;
    match args {
        ProfileArgs::Status { json } => {
            let view = profile_status_view(&state_dir)?;
            print_profile_status_view(&view, json)?;
        }
        ProfileArgs::Set { profile_id, json } => {
            dam_integrations::set_active_profile(&profile_id, &state_dir)?;
            let view = profile_status_view(&state_dir)?;
            print_profile_status_view(&view, json)?;
        }
        ProfileArgs::Clear { json } => {
            dam_integrations::clear_active_profile(&state_dir)?;
            let view = profile_status_view(&state_dir)?;
            print_profile_status_view(&view, json)?;
        }
    }
    Ok(0)
}

fn trust_command(args: TrustArgs) -> Result<i32, String> {
    let state_dir = dam_daemon::state_paths()
        .map(|paths| paths.state_dir)
        .map_err(|error| error.to_string())?;
    match args {
        TrustArgs::GenerateArtifact { json } => {
            let output = generate_local_ca_output(&state_dir, json)?;
            print!("{output}");
        }
        TrustArgs::DeleteArtifact { json } => {
            let output = delete_local_ca_output(&state_dir, json)?;
            print!("{output}");
        }
        TrustArgs::InstallTrust { json, yes } => {
            let output = install_local_ca_output(&state_dir, json, yes)?;
            print!("{output}");
        }
        TrustArgs::RemoveTrust { json, yes } => {
            let output = remove_local_ca_output(&state_dir, json, yes)?;
            print!("{output}");
        }
    }
    Ok(0)
}

fn network_command(args: NetworkArgs) -> Result<i32, String> {
    let state_dir = dam_daemon::state_paths()
        .map(|paths| paths.state_dir)
        .map_err(|error| error.to_string())?;
    let proxy_url = format!("http://{DEFAULT_LISTEN}");
    match args {
        NetworkArgs::InstallProxy {
            config_path,
            json,
            yes,
        } => {
            if !cfg!(target_os = "macos") {
                print_unsupported_platform(
                    "macos_system_proxy",
                    "system proxy routing is not implemented on this platform; use explicit proxy mode",
                    json,
                )?;
                return Ok(1);
            }
            let config = dam_config::load(&dam_config::ConfigOverrides {
                config_path,
                ..dam_config::ConfigOverrides::default()
            })
            .map_err(|error| error.to_string())?;
            let hosts = configured_hosts_for_state(&config, &state_dir)?;
            let result = if yes {
                dam_net_macos::install_system_proxy_for_hosts(&state_dir, &proxy_url, &hosts)
            } else {
                dam_net_macos::preview_install_system_proxy_for_hosts(
                    &state_dir, &proxy_url, &hosts,
                )
            }
            .map_err(|error| error.to_string())?;
            print_network_result(&result, json, yes)?;
        }
        NetworkArgs::RemoveProxy { json, yes } => {
            if !cfg!(target_os = "macos") {
                print_unsupported_platform(
                    "macos_system_proxy",
                    "system proxy routing is not implemented on this platform; no system route changes were made",
                    json,
                )?;
                return Ok(1);
            }
            let result = if yes {
                dam_net_macos::remove_system_proxy(&state_dir, &proxy_url)
            } else {
                dam_net_macos::preview_remove_system_proxy(&state_dir, &proxy_url)
            }
            .map_err(|error| error.to_string())?;
            print_network_result(&result, json, yes)?;
        }
        NetworkArgs::InstallNetworkExtension {
            config_path,
            json,
            yes,
        } => {
            if !cfg!(target_os = "macos") {
                print_unsupported_platform(
                    "macos_network_extension",
                    "Network Extension capture is not implemented on this platform; use explicit proxy mode",
                    json,
                )?;
                return Ok(1);
            }
            let config = dam_config::load(&dam_config::ConfigOverrides {
                config_path,
                ..dam_config::ConfigOverrides::default()
            })
            .map_err(|error| error.to_string())?;
            let hosts = configured_hosts_for_state(&config, &state_dir)?;
            let result = if yes {
                dam_net_macos::install_network_extension_for_hosts(&state_dir, &hosts)
            } else {
                dam_net_macos::preview_install_network_extension_for_hosts(&state_dir, &hosts)
            }
            .map_err(|error| error.to_string())?;
            print_network_extension_result(&result, json, yes)?;
            if yes && result.state == dam_net_macos::MacosNetworkExtensionResultState::NeedsApproval
            {
                return Ok(75);
            }
        }
        NetworkArgs::RemoveNetworkExtension { json, yes } => {
            if !cfg!(target_os = "macos") {
                print_unsupported_platform(
                    "macos_network_extension",
                    "Network Extension capture is not implemented on this platform; no system route changes were made",
                    json,
                )?;
                return Ok(1);
            }
            let result = if yes {
                dam_net_macos::remove_network_extension(&state_dir)
            } else {
                dam_net_macos::preview_remove_network_extension(&state_dir)
            }
            .map_err(|error| error.to_string())?;
            print_network_extension_result(&result, json, yes)?;
        }
        NetworkArgs::Status { json } => {
            let result =
                dam_net_macos::network_extension_status(&state_dir).map_err(|e| e.to_string())?;
            print_network_extension_result(&result, json, false)?;
        }
    }
    Ok(0)
}

fn startup_command(args: StartupArgs) -> Result<i32, String> {
    let state_dir = dam_daemon::state_paths()
        .map(|paths| paths.state_dir)
        .map_err(|error| error.to_string())?;
    match args {
        StartupArgs::Status { json } => {
            let view = startup_status_view(&state_dir);
            print_startup_status_view(&view, json)?;
        }
        StartupArgs::SkipOpenAtLogin { json } => {
            write_startup_skip_marker(&state_dir)?;
            let view = startup_status_view(&state_dir);
            print_startup_status_view(&view, json)?;
        }
    }
    Ok(0)
}

fn startup_status_view(state_dir: &Path) -> StartupStatusView {
    let registered_marker = state_dir.join(LOGIN_ITEM_MARKER_RELPATH);
    let skip_marker = state_dir.join(LOGIN_ITEM_SKIP_MARKER_RELPATH);
    let legacy_marker = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(LAUNCH_AGENT_PLIST_RELPATH));

    if registered_marker.exists() {
        return StartupStatusView {
            state: "registered",
            message: "DAM is marked to open at login".to_string(),
            platform: std::env::consts::OS,
            state_dir: state_dir.to_path_buf(),
            marker: Some(registered_marker),
        };
    }

    if let Some(legacy_marker) = legacy_marker.filter(|path| path.exists()) {
        return StartupStatusView {
            state: "registered",
            message: "DAM has a legacy launch agent registration".to_string(),
            platform: std::env::consts::OS,
            state_dir: state_dir.to_path_buf(),
            marker: Some(legacy_marker),
        };
    }

    if skip_marker.exists() {
        return StartupStatusView {
            state: "skipped",
            message: "Open at Login was skipped for this install".to_string(),
            platform: std::env::consts::OS,
            state_dir: state_dir.to_path_buf(),
            marker: Some(skip_marker),
        };
    }

    StartupStatusView {
        state: "unconfigured",
        message: if cfg!(target_os = "macos") {
            "Choose whether DAM should open at login".to_string()
        } else {
            "This platform does not currently require a DAM startup setup step".to_string()
        },
        platform: std::env::consts::OS,
        state_dir: state_dir.to_path_buf(),
        marker: None,
    }
}

fn write_startup_skip_marker(state_dir: &Path) -> Result<PathBuf, String> {
    let marker_path = state_dir.join(LOGIN_ITEM_SKIP_MARKER_RELPATH);
    if let Some(parent) = marker_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create startup marker dir: {error}"))?;
    }
    std::fs::write(&marker_path, "skipped\n")
        .map_err(|error| format!("write {}: {error}", marker_path.display()))?;
    Ok(marker_path)
}

fn print_startup_status_view(view: &StartupStatusView, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(view)
                .map_err(|error| format!("failed to serialize startup status: {error}"))?
        );
    } else {
        print!("{}", render_startup_status_view(view));
    }
    Ok(())
}

fn render_startup_status_view(view: &StartupStatusView) -> String {
    let marker = view
        .marker
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "state: {}\nmessage: {}\nplatform: {}\nstate_dir: {}\nmarker: {}\n",
        view.state,
        view.message,
        view.platform,
        view.state_dir.display(),
        marker
    )
}

fn configured_hosts(config: &dam_config::DamConfig) -> Vec<String> {
    dam_net::traffic_routes_from_profile(&config.traffic.effective_profile())
        .into_iter()
        .map(|route| route.host)
        .collect()
}

fn configured_hosts_for_state(
    config: &dam_config::DamConfig,
    state_dir: &Path,
) -> Result<Vec<String>, String> {
    let mut config = config.clone();
    let integration_state_dir = state_dir.join("integrations");
    if let Some(profile_ids) =
        dam_integrations::runtime_enabled_profile_ids(&integration_state_dir)?
    {
        config.traffic.enabled_app_ids = Some(
            dam_integrations::traffic_app_ids_for_profile_ids_from_state(
                &profile_ids,
                &integration_state_dir,
            )?,
        );
    }
    Ok(configured_hosts(&config))
}

async fn integrations(args: IntegrationArgs) -> Result<i32, String> {
    match args {
        IntegrationArgs::List { json, proxy_url } => {
            let proxy_url = integration_proxy_url(proxy_url);
            let state_dir = integration_state_dir()?;
            let profiles = dam_integrations::profiles_from_state(&proxy_url, &state_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&profiles)
                        .map_err(|error| format!("failed to serialize integrations: {error}"))?
                );
            } else {
                print!("{}", render_integration_list(&profiles, &proxy_url));
            }
            Ok(0)
        }
        IntegrationArgs::Show {
            profile_id,
            json,
            proxy_url,
        } => {
            let proxy_url = integration_proxy_url(proxy_url);
            let state_dir = integration_state_dir()?;
            let profile =
                dam_integrations::profile_from_state(&profile_id, &proxy_url, &state_dir)?
                    .ok_or_else(|| {
                        format!(
                            "unknown integration profile: {profile_id}\nknown profiles: {}",
                            dam_integrations::profile_ids().join(", ")
                        )
                    })?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&profile)
                        .map_err(|error| format!("failed to serialize integration: {error}"))?
                );
            } else {
                print!("{}", render_integration_profile(&profile, &proxy_url));
            }
            Ok(0)
        }
        IntegrationArgs::Apply {
            profile_id,
            dry_run,
            json,
            proxy_url,
            target_path,
        } => {
            let proxy_url = integration_proxy_url(proxy_url);
            let state_dir = integration_state_dir()?;
            let target_path = match target_path {
                Some(path) => path,
                None => default_integration_target_path(&profile_id, &state_dir)?,
            };
            let prepared = dam_integrations::prepare_apply_in_state(
                &profile_id,
                &proxy_url,
                target_path,
                &state_dir,
            )?;
            let result = dam_integrations::run_apply(prepared, dry_run, &state_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result)
                        .map_err(|error| format!("failed to serialize apply result: {error}"))?
                );
            } else {
                print!("{}", render_integration_apply_result(&result));
            }
            Ok(0)
        }
        IntegrationArgs::Rollback { profile_id, json } => {
            let state_dir = integration_state_dir()?;
            let result = dam_integrations::rollback_profile(&profile_id, &state_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).map_err(|error| {
                        format!("failed to serialize rollback result: {error}")
                    })?
                );
            } else {
                print!("{}", render_integration_rollback_result(&result));
            }
            Ok(0)
        }
    }
}

async fn daemon_run(args: dam_daemon::ProxyOptions) -> Result<i32, String> {
    let config = dam_daemon::proxy_config(&args)?;
    dam_daemon::serve_with_modes(config, args.config_path, args.network_mode, args.trust_mode)
        .await
        .map_err(|error| error.to_string())?;
    Ok(0)
}

fn web_command(args: WebArgs) -> Result<i32, String> {
    let binary = dam_web_binary();
    let status = StdCommand::new(&binary)
        .args(args.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| format!("failed to start dam-web from {}: {error}", binary.display()))?;

    Ok(status.code().unwrap_or(1))
}

fn dam_web_binary() -> PathBuf {
    if let Some(path) = std::env::var_os(DAM_WEB_BIN_ENV)
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    if let Ok(current) = std::env::current_exe()
        && let Some(parent) = current.parent()
    {
        let sibling = parent.join(native_binary_name("dam-web"));
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from(native_binary_name("dam-web"))
}

fn native_binary_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

#[cfg(test)]
fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<Cli, String> {
    parse_cli_with_connect_profiles(args, ConnectProfileSelection::default())
}

#[cfg(test)]
fn parse_cli_with_active_profiles(
    args: impl IntoIterator<Item = String>,
    active_profile_ids: Vec<String>,
) -> Result<Cli, String> {
    parse_cli_with_connect_profiles(
        args,
        ConnectProfileSelection {
            explicit_selection: !active_profile_ids.is_empty(),
            profile_ids: active_profile_ids,
            ..ConnectProfileSelection::default()
        },
    )
}

fn parse_cli_with_connect_profiles(
    args: impl IntoIterator<Item = String>,
    connect_profiles: ConnectProfileSelection,
) -> Result<Cli, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    let Some(command) = args.first() else {
        return Ok(Cli {
            command: CommandKind::Help,
        });
    };

    match command.as_str() {
        "-h" | "--help" | "help" => Ok(Cli {
            command: CommandKind::Help,
        }),
        "connect" => parse_connect_command(&args[1..], connect_profiles),
        "disconnect" => parse_disconnect_command(&args[1..]),
        "doctor" => parse_doctor_command(&args[1..]),
        "status" => parse_status_command(&args[1..]),
        "logs" => parse_logs_command(&args[1..]),
        "profile" => parse_profile_command(&args[1..]),
        "trust" => parse_trust_command(&args[1..]),
        "network" => parse_network_command(&args[1..]),
        "setup" => parse_setup_command(&args[1..]),
        "startup" => parse_startup_command(&args[1..]),
        "integrations" => parse_integrations_command(&args[1..]),
        "web" => Ok(Cli {
            command: CommandKind::Web(WebArgs {
                args: args[1..].to_vec(),
            }),
        }),
        "daemon-run" => parse_daemon_run_command(&args[1..]),
        other => Err(format!("unknown command: {other}\n{}", usage())),
    }
}

fn parse_connect_command(
    args: &[String],
    connect_profiles: ConnectProfileSelection,
) -> Result<Cli, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
    {
        println!("{}", usage_connect());
        std::process::exit(0);
    }

    let (args, json) = split_json_flag(args, "connect")?;
    let expanded = expand_connect_profile_args(&args, &connect_profiles)?;
    let explicit_upstream = last_option_value(&expanded.args, "--upstream");
    let mut proxy = dam_daemon::parse_proxy_options(expanded.args)?;
    if !expanded.selected_profile_ids.is_empty() && proxy.targets.is_none() {
        let mut targets = proxy_targets_for_profiles(
            &expanded.selected_profile_ids,
            connect_profiles.integration_state_dir.as_deref(),
        )?;
        if let Some(upstream) = explicit_upstream {
            override_single_profile_target_upstream(&mut targets, &upstream)?;
        }
        proxy.targets = Some(targets);
    }
    if expanded.traffic_app_ids.is_some() {
        proxy.traffic_app_ids = expanded.traffic_app_ids;
    }
    for profile_id in &expanded.selected_profile_ids {
        validate_connect_apply_profile_matches_proxy(
            profile_id,
            &proxy,
            connect_profiles.integration_state_dir.as_deref(),
        )?;
    }
    Ok(Cli {
        command: CommandKind::Connect(ConnectArgs {
            proxy,
            apply_profile_ids: expanded.apply_profile_ids,
            json,
        }),
    })
}

fn parse_daemon_run_command(args: &[String]) -> Result<Cli, String> {
    Ok(Cli {
        command: CommandKind::DaemonRun(dam_daemon::parse_proxy_options(args.iter().cloned())?),
    })
}

fn parse_disconnect_command(args: &[String]) -> Result<Cli, String> {
    if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        println!("{}", usage_disconnect());
        std::process::exit(0);
    }
    let mut parsed = DisconnectArgs::default();
    for arg in args {
        match arg.as_str() {
            "--stop" => parsed.stop_daemon = true,
            "--json" => parsed.json = true,
            _ => return Err(format!("unknown disconnect argument: {arg}")),
        }
    }

    Ok(Cli {
        command: CommandKind::Disconnect(parsed),
    })
}

fn split_json_flag(args: &[String], command: &str) -> Result<(Vec<String>, bool), String> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if arg == "-h" || arg == "--help" {
            return Err(format!(
                "unexpected help flag after {command} parser preflight"
            ));
        } else {
            filtered.push(arg.clone());
        }
    }
    Ok((filtered, json))
}

fn parse_status_command(args: &[String]) -> Result<Cli, String> {
    let mut parsed = StatusArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => parsed.json = true,
            "-h" | "--help" => {
                println!("{}", usage_status());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown status argument: {arg}")),
        }
        i += 1;
    }

    Ok(Cli {
        command: CommandKind::Status(parsed),
    })
}

fn parse_doctor_command(args: &[String]) -> Result<Cli, String> {
    let mut parsed = DoctorArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => parsed.json = true,
            "--config" => {
                i += 1;
                parsed.config_path = Some(PathBuf::from(required_value(args, i, "--config")?));
            }
            "--state-dir" => {
                i += 1;
                parsed.state_dir = Some(PathBuf::from(required_value(args, i, "--state-dir")?));
            }
            "--proxy-url" => {
                i += 1;
                parsed.proxy_url = Some(required_value(args, i, "--proxy-url")?.to_string());
            }
            "--network-mode" => {
                i += 1;
                parsed.network_mode = required_value(args, i, "--network-mode")?.parse()?;
            }
            "--trust-mode" => {
                i += 1;
                parsed.trust_mode = required_value(args, i, "--trust-mode")?.parse()?;
            }
            "-h" | "--help" => {
                println!("{}", usage_doctor());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown doctor argument: {arg}")),
        }
        i += 1;
    }

    Ok(Cli {
        command: CommandKind::Doctor(parsed),
    })
}

fn parse_setup_command(args: &[String]) -> Result<Cli, String> {
    if args.is_empty() || matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        println!("{}", usage_setup());
        std::process::exit(0);
    }

    match args[0].as_str() {
        "status" => Ok(Cli {
            command: CommandKind::Setup(SetupArgs::Status(parse_setup_plan_args(
                "status",
                &args[1..],
            )?)),
        }),
        "plan" => Ok(Cli {
            command: CommandKind::Setup(SetupArgs::Plan(parse_setup_plan_args(
                "plan",
                &args[1..],
            )?)),
        }),
        "next-action" => Ok(Cli {
            command: CommandKind::Setup(SetupArgs::NextAction(parse_setup_plan_args(
                "next-action",
                &args[1..],
            )?)),
        }),
        "resume" => Ok(Cli {
            command: CommandKind::Setup(SetupArgs::Resume(parse_setup_plan_args(
                "resume",
                &args[1..],
            )?)),
        }),
        "rescue" => Ok(Cli {
            command: CommandKind::Setup(SetupArgs::Rescue(parse_setup_rescue_args(&args[1..])?)),
        }),
        "repair" => Ok(Cli {
            command: CommandKind::Setup(SetupArgs::Repair(parse_setup_repair_args(&args[1..])?)),
        }),
        "export-diagnostics" => Ok(Cli {
            command: CommandKind::Setup(SetupArgs::ExportDiagnostics(parse_setup_plan_args(
                "export-diagnostics",
                &args[1..],
            )?)),
        }),
        command => Err(format!("unknown setup command: {command}")),
    }
}

fn parse_setup_plan_args(command: &str, args: &[String]) -> Result<SetupPlanArgs, String> {
    let mut parsed = SetupPlanArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => parsed.json = true,
            "--config" => {
                i += 1;
                parsed.config_path = Some(PathBuf::from(required_value(args, i, "--config")?));
            }
            "--state-dir" => {
                i += 1;
                parsed.state_dir = Some(PathBuf::from(required_value(args, i, "--state-dir")?));
            }
            "--proxy-url" => {
                i += 1;
                parsed.proxy_url = Some(required_value(args, i, "--proxy-url")?.to_string());
            }
            "--network-mode" => {
                i += 1;
                parsed.network_mode = required_value(args, i, "--network-mode")?.parse()?;
            }
            "--trust-mode" => {
                i += 1;
                parsed.trust_mode = required_value(args, i, "--trust-mode")?.parse()?;
            }
            "-h" | "--help" => {
                println!("{}", usage_setup_plan(command));
                std::process::exit(0);
            }
            arg => return Err(format!("unknown setup argument: {arg}")),
        }
        i += 1;
    }
    Ok(parsed)
}

fn parse_setup_rescue_args(args: &[String]) -> Result<SetupRescueArgs, String> {
    let mut parsed = SetupRescueArgs::default();
    let mut dry_run_explicit = false;
    let mut yes_explicit = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => parsed.json = true,
            "--dry-run" => {
                dry_run_explicit = true;
                parsed.yes = false;
            }
            "--yes" => {
                yes_explicit = true;
                parsed.yes = true;
            }
            "--state-dir" => {
                i += 1;
                parsed.state_dir = Some(PathBuf::from(required_value(args, i, "--state-dir")?));
            }
            "-h" | "--help" => {
                println!("{}", usage_setup_rescue());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown setup rescue argument: {arg}")),
        }
        i += 1;
    }
    if dry_run_explicit && yes_explicit {
        return Err("setup rescue cannot combine --dry-run and --yes".to_string());
    }
    Ok(parsed)
}

fn parse_setup_repair_args(args: &[String]) -> Result<SetupRepairArgs, String> {
    let mut plan_args = Vec::new();
    let mut dry_run_explicit = false;
    let mut yes_explicit = false;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--dry-run" => {
                dry_run_explicit = true;
                yes = false;
            }
            "--yes" => {
                yes_explicit = true;
                yes = true;
            }
            "-h" | "--help" => {
                println!("{}", usage_setup_repair());
                std::process::exit(0);
            }
            _ => plan_args.push(arg.clone()),
        }
    }
    if dry_run_explicit && yes_explicit {
        return Err("setup repair cannot combine --dry-run and --yes".to_string());
    }
    Ok(SetupRepairArgs {
        plan: parse_setup_plan_args("repair", &plan_args)?,
        yes,
    })
}

fn parse_startup_command(args: &[String]) -> Result<Cli, String> {
    if args.is_empty() || matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        println!("{}", usage_startup());
        std::process::exit(0);
    }

    match args[0].as_str() {
        "status" => parse_startup_status(&args[1..]),
        "skip-open-at-login" => parse_startup_skip_open_at_login(&args[1..]),
        command => Err(format!("unknown startup command: {command}")),
    }
}

fn parse_startup_status(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_startup_status());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown startup status argument: {arg}")),
        }
    }
    Ok(Cli {
        command: CommandKind::Startup(StartupArgs::Status { json }),
    })
}

fn parse_startup_skip_open_at_login(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_startup_skip_open_at_login());
                std::process::exit(0);
            }
            arg => {
                return Err(format!(
                    "unknown startup skip-open-at-login argument: {arg}"
                ));
            }
        }
    }
    Ok(Cli {
        command: CommandKind::Startup(StartupArgs::SkipOpenAtLogin { json }),
    })
}

fn parse_logs_command(args: &[String]) -> Result<Cli, String> {
    let mut parsed = LogsArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => parsed.json = true,
            "--events" => parsed.events = true,
            "--limit" => {
                i += 1;
                parsed.limit = args
                    .get(i)
                    .ok_or_else(|| "--limit requires a number".to_string())?
                    .parse::<usize>()
                    .map_err(|_| "--limit requires a positive number".to_string())?;
                if parsed.limit == 0 {
                    return Err("--limit must be greater than zero".to_string());
                }
            }
            "--after-id" => {
                i += 1;
                parsed.after_id = Some(
                    args.get(i)
                        .ok_or_else(|| "--after-id requires an id".to_string())?
                        .parse::<i64>()
                        .map_err(|_| "--after-id requires an integer id".to_string())?,
                );
            }
            "--operation" => {
                i += 1;
                parsed.operation_id = Some(
                    args.get(i)
                        .ok_or_else(|| "--operation requires an operation id".to_string())?
                        .to_string(),
                );
            }
            "-h" | "--help" => {
                println!("{}", usage_logs());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown logs argument: {arg}")),
        }
        i += 1;
    }

    Ok(Cli {
        command: CommandKind::Logs(parsed),
    })
}

fn parse_profile_command(args: &[String]) -> Result<Cli, String> {
    if args.is_empty() || matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        println!("{}", usage_profile());
        std::process::exit(0);
    }

    match args[0].as_str() {
        "status" => parse_profile_status(&args[1..]),
        "set" => parse_profile_set(&args[1..]),
        "clear" => parse_profile_clear(&args[1..]),
        command => Err(format!("unknown profile command: {command}")),
    }
}

fn parse_profile_status(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_profile_status());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown profile status argument: {arg}")),
        }
    }

    Ok(Cli {
        command: CommandKind::Profile(ProfileArgs::Status { json }),
    })
}

fn parse_profile_set(args: &[String]) -> Result<Cli, String> {
    let mut profile_id = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_profile_set());
                std::process::exit(0);
            }
            arg if profile_id.is_none() => profile_id = Some(arg.to_string()),
            arg => return Err(format!("unexpected profile set argument: {arg}")),
        }
        i += 1;
    }

    let profile_id = profile_id.ok_or_else(|| "profile set requires a profile id".to_string())?;
    Ok(Cli {
        command: CommandKind::Profile(ProfileArgs::Set { profile_id, json }),
    })
}

fn parse_profile_clear(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_profile_clear());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown profile clear argument: {arg}")),
        }
    }

    Ok(Cli {
        command: CommandKind::Profile(ProfileArgs::Clear { json }),
    })
}

fn parse_trust_command(args: &[String]) -> Result<Cli, String> {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        println!("{}", usage_trust());
        std::process::exit(0);
    }
    match args[0].as_str() {
        "generate-local-ca" => parse_trust_generate_local_ca(&args[1..]),
        "delete-local-ca" => parse_trust_delete_local_ca(&args[1..]),
        "install-local-ca" => parse_trust_install_local_ca(&args[1..]),
        "remove-local-ca" => parse_trust_remove_local_ca(&args[1..]),
        command => Err(format!("unknown trust command: {command}")),
    }
}

fn parse_trust_generate_local_ca(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_trust_generate_local_ca());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown trust generate-local-ca argument: {arg}")),
        }
    }
    Ok(Cli {
        command: CommandKind::Trust(TrustArgs::GenerateArtifact { json }),
    })
}

fn parse_trust_delete_local_ca(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_trust_delete_local_ca());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown trust delete-local-ca argument: {arg}")),
        }
    }
    Ok(Cli {
        command: CommandKind::Trust(TrustArgs::DeleteArtifact { json }),
    })
}

fn parse_trust_install_local_ca(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--yes" => yes = true,
            "--dry-run" => yes = false,
            "-h" | "--help" => {
                println!("{}", usage_trust_install_local_ca());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown trust install-local-ca argument: {arg}")),
        }
    }
    Ok(Cli {
        command: CommandKind::Trust(TrustArgs::InstallTrust { json, yes }),
    })
}

fn parse_trust_remove_local_ca(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--yes" => yes = true,
            "--dry-run" => yes = false,
            "-h" | "--help" => {
                println!("{}", usage_trust_remove_local_ca());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown trust remove-local-ca argument: {arg}")),
        }
    }
    Ok(Cli {
        command: CommandKind::Trust(TrustArgs::RemoveTrust { json, yes }),
    })
}

fn parse_network_command(args: &[String]) -> Result<Cli, String> {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        println!("{}", usage_network());
        std::process::exit(0);
    }
    match args[0].as_str() {
        "install-system-proxy" => parse_network_install_system_proxy(&args[1..]),
        "remove-system-proxy" => parse_network_remove_system_proxy(&args[1..]),
        "install-network-extension" => parse_network_install_network_extension(&args[1..]),
        "remove-network-extension" => parse_network_remove_network_extension(&args[1..]),
        "status" => parse_network_status(&args[1..]),
        command => Err(format!("unknown network command: {command}")),
    }
}

fn parse_network_install_system_proxy(args: &[String]) -> Result<Cli, String> {
    let mut config_path = None;
    let mut json = false;
    let mut yes = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--json" => json = true,
            "--yes" => yes = true,
            "--dry-run" => yes = false,
            "-h" | "--help" => {
                println!("{}", usage_network_install_system_proxy());
                std::process::exit(0);
            }
            arg => {
                return Err(format!(
                    "unknown network install-system-proxy argument: {arg}"
                ));
            }
        }
        i += 1;
    }
    Ok(Cli {
        command: CommandKind::Network(NetworkArgs::InstallProxy {
            config_path,
            json,
            yes,
        }),
    })
}

fn parse_network_remove_system_proxy(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--yes" => yes = true,
            "--dry-run" => yes = false,
            "-h" | "--help" => {
                println!("{}", usage_network_remove_system_proxy());
                std::process::exit(0);
            }
            arg => {
                return Err(format!(
                    "unknown network remove-system-proxy argument: {arg}"
                ));
            }
        }
    }
    Ok(Cli {
        command: CommandKind::Network(NetworkArgs::RemoveProxy { json, yes }),
    })
}

fn parse_network_install_network_extension(args: &[String]) -> Result<Cli, String> {
    let mut config_path = None;
    let mut json = false;
    let mut yes = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--json" => json = true,
            "--yes" => yes = true,
            "--dry-run" => yes = false,
            "-h" | "--help" => {
                println!("{}", usage_network_install_network_extension());
                std::process::exit(0);
            }
            arg => {
                return Err(format!(
                    "unknown network install-network-extension argument: {arg}"
                ));
            }
        }
        i += 1;
    }
    Ok(Cli {
        command: CommandKind::Network(NetworkArgs::InstallNetworkExtension {
            config_path,
            json,
            yes,
        }),
    })
}

fn parse_network_remove_network_extension(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    let mut yes = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--yes" => yes = true,
            "--dry-run" => yes = false,
            "-h" | "--help" => {
                println!("{}", usage_network_remove_network_extension());
                std::process::exit(0);
            }
            arg => {
                return Err(format!(
                    "unknown network remove-network-extension argument: {arg}"
                ));
            }
        }
    }
    Ok(Cli {
        command: CommandKind::Network(NetworkArgs::RemoveNetworkExtension { json, yes }),
    })
}

fn parse_network_status(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_network_status());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown network status argument: {arg}")),
        }
    }
    Ok(Cli {
        command: CommandKind::Network(NetworkArgs::Status { json }),
    })
}

fn parse_integrations_command(args: &[String]) -> Result<Cli, String> {
    if args.is_empty() || matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        println!("{}", usage_integrations());
        std::process::exit(0);
    }

    match args[0].as_str() {
        "list" => parse_integrations_list(&args[1..]),
        "show" => parse_integrations_show(&args[1..]),
        "apply" => parse_integrations_apply(&args[1..]),
        "rollback" => parse_integrations_rollback(&args[1..]),
        command => Err(format!("unknown integrations command: {command}")),
    }
}

fn parse_integrations_list(args: &[String]) -> Result<Cli, String> {
    let mut json = false;
    let mut proxy_url = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--proxy-url" => {
                i += 1;
                proxy_url = Some(required_value(args, i, "--proxy-url")?.to_string());
            }
            "-h" | "--help" => {
                println!("{}", usage_integrations_list());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown integrations list argument: {arg}")),
        }
        i += 1;
    }

    Ok(Cli {
        command: CommandKind::Integrations(IntegrationArgs::List { json, proxy_url }),
    })
}

fn parse_integrations_show(args: &[String]) -> Result<Cli, String> {
    let mut profile_id = None;
    let mut json = false;
    let mut proxy_url = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--proxy-url" => {
                i += 1;
                proxy_url = Some(required_value(args, i, "--proxy-url")?.to_string());
            }
            "-h" | "--help" => {
                println!("{}", usage_integrations_show());
                std::process::exit(0);
            }
            arg if profile_id.is_none() => profile_id = Some(arg.to_string()),
            arg => return Err(format!("unexpected integrations show argument: {arg}")),
        }
        i += 1;
    }

    let profile_id =
        profile_id.ok_or_else(|| "integrations show requires a profile id".to_string())?;
    Ok(Cli {
        command: CommandKind::Integrations(IntegrationArgs::Show {
            profile_id,
            json,
            proxy_url,
        }),
    })
}

fn parse_integrations_apply(args: &[String]) -> Result<Cli, String> {
    let mut profile_id = None;
    let mut dry_run = true;
    let mut dry_run_explicit = false;
    let mut write = false;
    let mut json = false;
    let mut proxy_url = None;
    let mut target_path = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" => {
                dry_run = true;
                dry_run_explicit = true;
            }
            "--write" => {
                write = true;
                dry_run = false;
            }
            "--json" => json = true,
            "--proxy-url" => {
                i += 1;
                proxy_url = Some(required_value(args, i, "--proxy-url")?.to_string());
            }
            "--target-path" => {
                i += 1;
                target_path = Some(PathBuf::from(required_value(args, i, "--target-path")?));
            }
            "-h" | "--help" => {
                println!("{}", usage_integrations_apply());
                std::process::exit(0);
            }
            arg if profile_id.is_none() => profile_id = Some(arg.to_string()),
            arg => return Err(format!("unexpected integrations apply argument: {arg}")),
        }
        i += 1;
    }

    if dry_run_explicit && write {
        return Err("integrations apply cannot combine --dry-run and --write".to_string());
    }
    let profile_id =
        profile_id.ok_or_else(|| "integrations apply requires a profile id".to_string())?;
    Ok(Cli {
        command: CommandKind::Integrations(IntegrationArgs::Apply {
            profile_id,
            dry_run,
            json,
            proxy_url,
            target_path,
        }),
    })
}

fn parse_integrations_rollback(args: &[String]) -> Result<Cli, String> {
    let mut profile_id = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{}", usage_integrations_rollback());
                std::process::exit(0);
            }
            arg if profile_id.is_none() => profile_id = Some(arg.to_string()),
            arg => return Err(format!("unexpected integrations rollback argument: {arg}")),
        }
        i += 1;
    }

    let profile_id =
        profile_id.ok_or_else(|| "integrations rollback requires a profile id".to_string())?;
    Ok(Cli {
        command: CommandKind::Integrations(IntegrationArgs::Rollback { profile_id, json }),
    })
}

fn expand_connect_profile_args(
    args: &[String],
    connect_profiles: &ConnectProfileSelection,
) -> Result<ConnectProfileExpansion, String> {
    let integration_state_dir = connect_profiles.integration_state_dir.as_deref();
    let mut expanded = Vec::new();
    let mut remaining = Vec::new();
    let mut selected_profile_ids = Vec::new();
    let mut traffic_selection_explicit = connect_profiles.explicit_selection;
    let mut apply = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--profile" => {
                i += 1;
                let id = required_value(args, i, "--profile")?;
                if !selected_profile_ids.is_empty() {
                    return Err("--profile can only be supplied once".to_string());
                }
                let profile = integration_profile_for_state(
                    id,
                    dam_integrations::DEFAULT_PROXY_URL,
                    integration_state_dir,
                )?
                .ok_or_else(|| {
                    format!(
                        "unknown integration profile: {id}\nknown profiles: {}",
                        dam_integrations::profile_ids().join(", ")
                    )
                })?;
                let profile_id = profile.id.clone();
                expanded.extend(profile.connect_args);
                selected_profile_ids.push(profile_id);
                traffic_selection_explicit = true;
            }
            "--apply" => apply = true,
            arg => remaining.push(arg.to_string()),
        }
        i += 1;
    }

    if selected_profile_ids.is_empty() && !connect_profiles.profile_ids.is_empty() {
        selected_profile_ids = connect_profiles.profile_ids.clone();
        if selected_profile_ids.len() == 1 {
            let id = &selected_profile_ids[0];
            let profile = integration_profile_for_state(
                id,
                dam_integrations::DEFAULT_PROXY_URL,
                integration_state_dir,
            )?
            .ok_or_else(|| {
                format!(
                    "unknown enabled integration profile: {id}\nknown profiles: {}",
                    dam_integrations::profile_ids().join(", ")
                )
            })?;
            expanded.extend(profile.connect_args);
        }
    }

    if apply && selected_profile_ids.is_empty() {
        return Err(
            "--apply requires --profile <id> or enabled profiles in `dam profile status`"
                .to_string(),
        );
    }
    if selected_profile_ids.len() > 1 {
        expanded.extend([
            "--network-mode".to_string(),
            "tun".to_string(),
            "--trust-mode".to_string(),
            "local_ca".to_string(),
        ]);
    } else if profiles_require_local_ca(&selected_profile_ids, integration_state_dir)? {
        expanded.extend(["--trust-mode".to_string(), "local_ca".to_string()]);
    }

    expanded.extend(remaining);
    let apply_profile_ids = if apply {
        selected_profile_ids.clone()
    } else {
        Vec::new()
    };
    let traffic_app_ids = if traffic_selection_explicit {
        Some(traffic_app_ids_for_profiles(
            &selected_profile_ids,
            integration_state_dir,
        )?)
    } else {
        None
    };
    Ok(ConnectProfileExpansion {
        args: expanded,
        selected_profile_ids,
        traffic_app_ids,
        apply_profile_ids,
    })
}

fn integration_profile_for_state(
    profile_id: &str,
    proxy_url: &str,
    integration_state_dir: Option<&Path>,
) -> Result<Option<dam_integrations::IntegrationProfile>, String> {
    match integration_state_dir {
        Some(state_dir) => dam_integrations::profile_from_state(profile_id, proxy_url, state_dir),
        None => Ok(dam_integrations::profile(profile_id, proxy_url)),
    }
}

fn profiles_require_local_ca(
    profile_ids: &[String],
    integration_state_dir: Option<&Path>,
) -> Result<bool, String> {
    for profile_id in profile_ids {
        let profile = integration_profile_for_state(
            profile_id,
            dam_integrations::DEFAULT_PROXY_URL,
            integration_state_dir,
        )?
        .ok_or_else(|| {
            format!(
                "unknown enabled integration profile: {profile_id}\nknown profiles: {}",
                dam_integrations::profile_ids().join(", ")
            )
        })?;
        if profile
            .connect_args
            .windows(2)
            .any(|pair| pair[0] == "--trust-mode" && pair[1] == "local_ca")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn required_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

async fn wait_for_daemon_ready(timeout: Duration) -> Result<dam_daemon::DaemonState, String> {
    let started = std::time::Instant::now();
    let mut last_error = None;
    loop {
        match dam_daemon::daemon_status().map_err(|error| error.to_string())? {
            dam_daemon::DaemonStatus::Connected(state) => {
                match fetch_proxy_report(&state.proxy_url).await {
                    Ok(report) if report.state == dam_api::ProxyState::Protected => {
                        return Ok(state);
                    }
                    Ok(report) => {
                        last_error = Some(format!(
                            "proxy reported {}: {}",
                            proxy_state_tag(report.state),
                            report.message
                        ));
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            dam_daemon::DaemonStatus::Stale(state) => {
                last_error = Some(format!("daemon exited early with pid {}", state.pid));
            }
            dam_daemon::DaemonStatus::Disconnected => {}
        }

        if started.elapsed() >= timeout {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Err(match last_error {
        Some(error) => format!("DAM daemon did not become ready: {error}"),
        None => "DAM daemon did not become ready".to_string(),
    })
}

async fn wait_for_daemon_stop(pid: u32, timeout: Duration) {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if !dam_daemon::process_is_running(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn fetch_proxy_report(proxy_url: &str) -> Result<dam_api::ProxyReport, String> {
    let url = format!("{}/health", proxy_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(2_000))
        .build()
        .map_err(|error| format!("failed to build status client: {error}"))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("DAM proxy is not reachable at {url}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("DAM proxy status returned {}", response.status()));
    }

    response
        .json::<dam_api::ProxyReport>()
        .await
        .map_err(|error| format!("DAM proxy returned an unreadable status response: {error}"))
}

fn render_status_view(view: &StatusView) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", view.state));
    output.push_str(&format!("message: {}\n", view.message));
    match &view.active_profile {
        Some(profile) => output.push_str(&format!("active_profile: {}\n", profile.profile_id)),
        None => output.push_str("active_profile: none\n"),
    }
    if let Some(error) = &view.active_profile_error {
        output.push_str(&format!("warning active_profile: {error}\n"));
    }
    if let Some(state) = &view.daemon {
        output.push_str(&format!("pid: {}\n", state.pid));
        output.push_str(&format!("proxy: {}\n", state.proxy_url));
        output.push_str(&format!("network_mode: {}\n", state.network_mode));
        output.push_str(&format!(
            "protection_enabled: {}\n",
            state.protection_enabled
        ));
        output.push_str(&format!(
            "routing_routes: {}\n",
            state.transparent_routing_readiness.len()
        ));
        for route in &state.transparent_routing_readiness {
            output.push_str(&format!(
                "routing_route {}: {} - {}\n",
                route.route.target_name, route.readiness, route.message
            ));
        }
        output.push_str(&format!("trust_mode: {}\n", state.trust.mode));
        output.push_str(&format!(
            "trust_routes: {}\n",
            state.transparent_trust_readiness.len()
        ));
        for route in &state.transparent_trust_readiness {
            output.push_str(&format!(
                "trust_route {}: {} - {}\n",
                route.route.target_name, route.readiness, route.message
            ));
        }
        output.push_str(&format!(
            "interception_routes: {}\n",
            state.transparent_interception_readiness.len()
        ));
        for route in &state.transparent_interception_readiness {
            output.push_str(&format!(
                "interception_route {}: {} - {}\n",
                route.route.target_name, route.readiness, route.message
            ));
        }
        if let Some(target) = &state.target_name {
            output.push_str(&format!("target: {target}\n"));
        }
        if let Some(provider) = &state.target_provider {
            output.push_str(&format!("provider: {provider}\n"));
        }
        if let Some(upstream) = &state.upstream {
            output.push_str(&format!("upstream: {upstream}\n"));
        }
    }
    if let Some(proxy) = &view.proxy {
        output.push_str(&format!("protection: {}\n", proxy_state_tag(proxy.state)));
        for diagnostic in &proxy.diagnostics {
            output.push_str(&format!(
                "{} {}: {}\n",
                severity_tag(diagnostic.severity),
                diagnostic.code,
                diagnostic.message
            ));
        }
    }
    output
}

fn render_health_report(report: &dam_api::HealthReport) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", health_state_tag(report.state)));
    for component in &report.components {
        output.push_str(&format!(
            "{}: {} - {}\n",
            component.component,
            health_state_tag(component.state),
            component.message
        ));
    }
    for diagnostic in &report.diagnostics {
        output.push_str(&format!(
            "{} {}: {}\n",
            severity_tag(diagnostic.severity),
            diagnostic.code,
            diagnostic.message
        ));
    }
    output
}

fn render_setup_plan(report: &dam_diagnostics::SetupPlan) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", report.state.tag()));
    output.push_str(&format!("message: {}\n", report.message));
    output.push_str(&format!("state_dir: {}\n", report.state_dir.display()));
    output.push_str(&format!(
        "integration_state_dir: {}\n",
        report.integration_state_dir.display()
    ));
    output.push_str(&format!("proxy_url: {}\n", report.proxy_url));
    output.push_str(&format!("network_mode: {}\n", report.network_mode));
    output.push_str(&format!("trust_mode: {}\n", report.trust_mode));
    if let Some(action) = &report.next_action {
        output.push_str(&format!(
            "next_action: {}.{} - {}\n",
            action.kind.tag(),
            action.detail.tag(),
            action.message
        ));
        if let Some(command) = &action.command {
            output.push_str(&format!("next_command: {}\n", shell_command(command)));
        }
    } else {
        output.push_str("next_action: none\n");
    }
    for step in &report.steps {
        output.push_str(&format!(
            "step {}: {}.{} - {}\n",
            step.kind.tag(),
            step.status.tag(),
            step.detail.tag(),
            step.message
        ));
        if let Some(command) = &step.command {
            output.push_str(&format!("  command: {}\n", shell_command(command)));
        }
        output.push_str(&format!(
            "  requires_confirmation: {}\n",
            step.requires_confirmation
        ));
        output.push_str(&format!("  changes_system: {}\n", step.changes_system));
    }
    output
}

fn render_setup_next_action(view: &SetupNextActionView) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", view.state.tag()));
    output.push_str(&format!("message: {}\n", view.message));
    output.push_str(&format!("state_dir: {}\n", view.state_dir.display()));
    output.push_str(&format!("proxy_url: {}\n", view.proxy_url));
    output.push_str(&format!("network_mode: {}\n", view.network_mode));
    output.push_str(&format!("trust_mode: {}\n", view.trust_mode));
    if let Some(action) = &view.next_action {
        output.push_str(&format!("kind: {}\n", action.kind.tag()));
        output.push_str(&format!("status: {}\n", action.status.tag()));
        output.push_str(&format!("detail: {}\n", action.detail.tag()));
        output.push_str(&format!("action: {}\n", action.message));
        if let Some(command) = &action.command {
            output.push_str(&format!("command: {}\n", shell_command(command)));
        }
        output.push_str(&format!(
            "requires_confirmation: {}\n",
            action.requires_confirmation
        ));
        output.push_str(&format!("changes_system: {}\n", action.changes_system));
    } else {
        output.push_str("action: none\n");
    }
    output
}

fn print_setup_rescue_view(view: &dam_diagnostics::SetupRescue, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(view)
                .map_err(|error| format!("failed to serialize setup rescue result: {error}"))?
        );
    } else {
        print!("{}", render_setup_rescue_view(view));
    }
    Ok(())
}

fn render_setup_rescue_view(view: &dam_diagnostics::SetupRescue) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", view.state));
    output.push_str(&format!("message: {}\n", view.message));
    output.push_str(&format!("state_dir: {}\n", view.state_dir.display()));
    for action in &view.actions {
        output.push_str(&format!(
            "action {}: {} - {}\n",
            action.id, action.state, action.message
        ));
        output.push_str(&format!("  changes_system: {}\n", action.changes_system));
    }
    output
}

fn render_setup_repair_view(view: &dam_diagnostics::SetupRepair) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", view.state));
    output.push_str(&format!("message: {}\n", view.message));
    output.push_str("\nrescue:\n");
    for action in &view.rescue.actions {
        output.push_str(&format!(
            "  action {}: {} - {}\n",
            action.id, action.state, action.message
        ));
    }
    output.push_str("\nsetup:\n");
    output.push_str(&render_setup_plan(&view.setup_plan));
    output
}

fn render_setup_diagnostics_export(view: &dam_diagnostics::SetupDiagnosticsExport) -> String {
    let mut output = String::new();
    output.push_str(&format!("generated_at_unix: {}\n", view.generated_at_unix));
    output.push_str(&format!("doctor: {:?}\n", view.doctor.state));
    output.push_str(&format!("setup: {}\n", view.setup_plan.state.tag()));
    output.push_str(&format!("rescue: {}\n", view.rescue_preview.state));
    output.push_str("Use --json for the complete offline diagnostics payload.\n");
    output
}

fn print_profile_status_view(view: &ProfileStatusView, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(view)
                .map_err(|error| format!("failed to serialize profile status: {error}"))?
        );
    } else {
        print!("{}", render_profile_status_view(view));
    }
    Ok(())
}

fn print_network_result(
    result: &dam_net_macos::MacosSystemProxyResult,
    json: bool,
    approved: bool,
) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(result)
                .map_err(|error| format!("failed to serialize network result: {error}"))?
        );
    } else {
        print!("{}", render_network_result(result, approved));
    }
    Ok(())
}

fn print_network_extension_result(
    result: &dam_net_macos::MacosNetworkExtensionResult,
    json: bool,
    approved: bool,
) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(result).map_err(|error| format!(
                "failed to serialize network extension result: {error}"
            ))?
        );
    } else {
        print!("{}", render_network_extension_result(result, approved));
    }
    Ok(())
}

fn print_unsupported_platform(
    backend: &'static str,
    message: &str,
    json: bool,
) -> Result<(), String> {
    let view = UnsupportedPlatformView {
        state: "unsupported_platform",
        support: "planned",
        platform: std::env::consts::OS,
        backend,
        message: message.to_string(),
        fallback_command: vec![
            "dam".to_string(),
            "connect".to_string(),
            "--network-mode".to_string(),
            "explicit_proxy".to_string(),
            "--trust-mode".to_string(),
            "disabled".to_string(),
        ],
        system_routes_changed: false,
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view).map_err(|error| {
                format!("failed to serialize unsupported platform result: {error}")
            })?
        );
    } else {
        println!("state: {}", view.state);
        println!("support: {}", view.support);
        println!("platform: {}", view.platform);
        println!("backend: {}", view.backend);
        println!("message: {}", view.message);
        println!("fallback: {}", shell_command(&view.fallback_command));
        println!("system_routes_changed: false");
    }
    Ok(())
}

fn render_profile_status_view(view: &ProfileStatusView) -> String {
    let mut output = String::new();
    match &view.active_profile {
        Some(profile) => {
            output.push_str(&format!("active_profile: {}\n", profile.profile_id));
            output.push_str(&format!("selected_at_unix: {}\n", profile.selected_at_unix));
        }
        None => output.push_str("active_profile: none\n"),
    }
    if view.enabled_profiles.is_empty() {
        output.push_str("enabled_profiles: none\n");
    } else {
        output.push_str(&format!(
            "enabled_profiles: {}\n",
            view.enabled_profiles
                .iter()
                .map(|profile| profile.profile_id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    output.push_str(&format!("proxy_url: {}\n", view.proxy_url));
    for apply in &view.applies {
        output.push_str(&format!(
            "profile {} apply_state: {}\n",
            apply.profile_id,
            integration_apply_status_tag(apply.status)
        ));
        output.push_str(&format!(
            "profile {} target: {}\n",
            apply.profile_id,
            apply.target_path.display()
        ));
        output.push_str(&format!(
            "profile {} rollback: {}\n",
            apply.profile_id,
            if apply.rollback_available {
                "available"
            } else {
                "not_available"
            }
        ));
        if apply.rollback_available {
            output.push_str(&format!("rollback_profile: {}\n", apply.profile_id));
        }
        output.push_str(&format!(
            "profile {} message: {}\n",
            apply.profile_id, apply.message
        ));
    }
    if let Some(apply) = view.applies.first() {
        output.push_str(&format!(
            "apply_state: {}\n",
            integration_apply_status_tag(apply.status)
        ));
        output.push_str(&format!("target: {}\n", apply.target_path.display()));
        output.push_str(&format!(
            "rollback: {}\n",
            if apply.rollback_available {
                "available"
            } else {
                "not_available"
            }
        ));
        output.push_str(&format!("message: {}\n", apply.message));
    }
    for error in &view.inspection_errors {
        output.push_str(&format!("warning inspection: {error}\n"));
    }
    output
}

fn render_local_ca_generate_view(view: &LocalCaGenerateView) -> String {
    let artifact = &view.artifact;
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", view.state));
    output.push_str(&format!("id: {}\n", artifact.record.id));
    output.push_str(&format!("label: {}\n", artifact.record.label));
    output.push_str(&format!(
        "fingerprint_sha256: {}\n",
        artifact.record.fingerprint_sha256
    ));
    if let Some(fingerprint_sha1) = &artifact.record.fingerprint_sha1 {
        output.push_str(&format!("fingerprint_sha1: {fingerprint_sha1}\n"));
    }
    output.push_str(&format!(
        "created_at_unix: {}\n",
        artifact.record.created_at_unix
    ));
    output.push_str("installed_at_unix: none\n");
    output.push_str(&format!(
        "manifest: {}\n",
        artifact.paths.manifest_path.display()
    ));
    output.push_str(&format!(
        "certificate: {}\n",
        artifact.paths.certificate_path.display()
    ));
    output.push_str(&format!(
        "private_key: {}\n",
        artifact.paths.private_key_path.display()
    ));
    output.push_str("local_trust: unchanged\n");
    output
}

fn render_local_ca_delete_view(view: &LocalCaDeleteView) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", view.state));
    output.push_str(&format!("deleted: {}\n", view.deleted));
    output.push_str(&format!("state_dir: {}\n", view.state_dir.display()));
    output.push_str("local_trust: unchanged\n");
    output
}

fn render_local_ca_system_trust_result(
    result: &dam_trust::LocalCaSystemTrustResult,
    approved: bool,
) -> String {
    let plan = &result.plan;
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", result.state));
    output.push_str(&format!("action: {}\n", trust_action_tag(plan.action)));
    output.push_str(&format!("message: {}\n", plan.message));
    output.push_str(&format!("support: {}\n", trust_support_tag(plan.support)));
    output.push_str(&format!("platform_store: {}\n", plan.platform_store));
    output.push_str(&format!("requires_admin: {}\n", plan.requires_admin));
    output.push_str(&format!(
        "changes_local_trust: {}\n",
        plan.changes_system_trust
    ));
    output.push_str(&format!(
        "requires_user_consent: {}\n",
        plan.requires_user_consent
    ));
    output.push_str(&format!(
        "will_generate_artifact: {}\n",
        plan.will_generate_artifact
    ));
    output.push_str(&format!("can_execute: {}\n", plan.can_execute));
    output.push_str(&format!("system_store: {}\n", plan.system_store));
    output.push_str(&format!(
        "certificate: {}\n",
        plan.certificate_path.display()
    ));
    if let Some(artifact) = &result.artifact {
        output.push_str(&format!("id: {}\n", artifact.record.id));
        output.push_str(&format!(
            "fingerprint_sha256: {}\n",
            artifact.record.fingerprint_sha256
        ));
        if let Some(fingerprint_sha1) = &artifact.record.fingerprint_sha1 {
            output.push_str(&format!("fingerprint_sha1: {fingerprint_sha1}\n"));
        }
        output.push_str(&format!(
            "installed_at_unix: {}\n",
            artifact
                .record
                .installed_at_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string())
        ));
    }
    for command in &plan.commands {
        output.push_str(&format!(
            "command: {} {}\n",
            command.program,
            command.args.join(" ")
        ));
    }
    output.push_str(&format!(
        "local_trust: {}\n",
        if result.system_trust_changed {
            "changed"
        } else {
            "unchanged"
        }
    ));
    if !approved && plan.requires_user_consent {
        output.push_str("approval: rerun with --yes to apply this local trust change\n");
    }
    output
}

fn render_network_result(result: &dam_net_macos::MacosSystemProxyResult, approved: bool) -> String {
    let plan = &result.plan;
    let mut output = String::new();
    output.push_str(&format!(
        "state: {}\n",
        network_result_state_tag(result.state)
    ));
    output.push_str(&format!("action: {}\n", network_action_tag(plan.action)));
    output.push_str(&format!("message: {}\n", plan.message));
    output.push_str(&format!("support: {}\n", network_support_tag(plan.support)));
    output.push_str(&format!("proxy_url: {}\n", plan.proxy_url));
    output.push_str(&format!("pac_url: {}\n", plan.pac_url));
    output.push_str(&format!("pac_path: {}\n", plan.paths.pac_path.display()));
    output.push_str(&format!("services: {}\n", plan.services.len()));
    for service in &plan.services {
        output.push_str(&format!(
            "service {}: auto_proxy={} url={}\n",
            service.service_name,
            service.auto_proxy_enabled,
            service.auto_proxy_url.as_deref().unwrap_or("none")
        ));
    }
    for command in &plan.commands {
        output.push_str(&format!(
            "command: {} {}\n",
            command.program,
            command.args.join(" ")
        ));
    }
    output.push_str(&format!("can_execute: {}\n", plan.can_execute));
    output.push_str(&format!(
        "system_routes: {}\n",
        if result.system_routes_changed {
            "changed"
        } else {
            "unchanged"
        }
    ));
    if !approved && plan.can_execute {
        output.push_str("approval: rerun with --yes to apply this network change\n");
    }
    output
}

fn render_network_extension_result(
    result: &dam_net_macos::MacosNetworkExtensionResult,
    approved: bool,
) -> String {
    let plan = &result.plan;
    let mut output = String::new();
    output.push_str(&format!(
        "state: {}\n",
        network_extension_result_state_tag(result.state)
    ));
    output.push_str(&format!(
        "action: {}\n",
        network_extension_action_tag(plan.action)
    ));
    output.push_str(&format!("message: {}\n", plan.message));
    output.push_str(&format!(
        "support: {}\n",
        network_extension_support_tag(plan.support)
    ));
    output.push_str(&format!("bundle_id: {}\n", plan.bundle_identifier));
    output.push_str(&format!(
        "team_id: {}\n",
        plan.team_identifier.as_deref().unwrap_or("none")
    ));
    output.push_str(&format!("backend: {}\n", plan.backend_status.kind.tag()));
    output.push_str(&format!(
        "backend_readiness: {}\n",
        plan.backend_status.readiness.tag()
    ));
    output.push_str(&format!("backend_active: {}\n", plan.backend_status.active));
    output.push_str(&format!(
        "protected_hosts: {}\n",
        plan.protected_hosts.join(", ")
    ));
    for command in &plan.commands {
        output.push_str(&format!(
            "command: {} {}\n",
            command.program,
            command.args.join(" ")
        ));
    }
    output.push_str(&format!("can_execute: {}\n", plan.can_execute));
    output.push_str(&format!(
        "system_routes: {}\n",
        if result.system_routes_changed {
            "changed"
        } else {
            "unchanged"
        }
    ));
    if let Some(record) = &result.record {
        output.push_str(&format!(
            "activation_method: {}\n",
            record.activation_method
        ));
        output.push_str(&format!(
            "installed_at_unix: {}\n",
            record.installed_at_unix
        ));
    }
    if result.state == dam_net_macos::MacosNetworkExtensionResultState::NeedsApproval {
        output.push_str(
            "approval: approve DAM Network Protection in System Settings, then click Connect/Resume again\n",
        );
    }
    if !approved && plan.can_execute {
        output.push_str("approval: rerun with --yes to apply this Network Extension change\n");
    }
    output
}

fn network_result_state_tag(state: dam_net_macos::MacosSystemProxyResultState) -> &'static str {
    match state {
        dam_net_macos::MacosSystemProxyResultState::Preview => "preview",
        dam_net_macos::MacosSystemProxyResultState::Installed => "installed",
        dam_net_macos::MacosSystemProxyResultState::AlreadyInstalled => "already_installed",
        dam_net_macos::MacosSystemProxyResultState::Removed => "removed",
        dam_net_macos::MacosSystemProxyResultState::NotInstalled => "not_installed",
    }
}

fn network_action_tag(action: dam_net_macos::MacosSystemProxyAction) -> &'static str {
    match action {
        dam_net_macos::MacosSystemProxyAction::Install => "install",
        dam_net_macos::MacosSystemProxyAction::Remove => "remove",
    }
}

fn network_support_tag(support: dam_net_macos::MacosSystemProxySupport) -> &'static str {
    match support {
        dam_net_macos::MacosSystemProxySupport::Implemented => "implemented",
        dam_net_macos::MacosSystemProxySupport::Planned => "planned",
    }
}

fn network_extension_result_state_tag(
    state: dam_net_macos::MacosNetworkExtensionResultState,
) -> &'static str {
    match state {
        dam_net_macos::MacosNetworkExtensionResultState::Preview => "preview",
        dam_net_macos::MacosNetworkExtensionResultState::Installed => "installed",
        dam_net_macos::MacosNetworkExtensionResultState::AlreadyInstalled => "already_installed",
        dam_net_macos::MacosNetworkExtensionResultState::NeedsApproval => "needs_approval",
        dam_net_macos::MacosNetworkExtensionResultState::Removed => "removed",
        dam_net_macos::MacosNetworkExtensionResultState::NotInstalled => "not_installed",
        dam_net_macos::MacosNetworkExtensionResultState::Status => "status",
    }
}

fn network_extension_action_tag(
    action: dam_net_macos::MacosNetworkExtensionAction,
) -> &'static str {
    match action {
        dam_net_macos::MacosNetworkExtensionAction::Install => "install",
        dam_net_macos::MacosNetworkExtensionAction::Remove => "remove",
        dam_net_macos::MacosNetworkExtensionAction::Status => "status",
    }
}

fn network_extension_support_tag(
    support: dam_net_macos::MacosNetworkExtensionSupport,
) -> &'static str {
    match support {
        dam_net_macos::MacosNetworkExtensionSupport::Implemented => "implemented",
        dam_net_macos::MacosNetworkExtensionSupport::Planned => "planned",
    }
}

fn trust_action_tag(action: dam_trust::TrustAction) -> &'static str {
    match action {
        dam_trust::TrustAction::Inspect => "inspect",
        dam_trust::TrustAction::InstallLocalCa => "install_local_ca",
        dam_trust::TrustAction::RemoveLocalCa => "remove_local_ca",
    }
}

fn trust_support_tag(support: dam_trust::TrustSupport) -> &'static str {
    match support {
        dam_trust::TrustSupport::Implemented => "implemented",
        dam_trust::TrustSupport::Planned => "planned",
    }
}

fn profile_status_view(state_dir: &std::path::Path) -> Result<ProfileStatusView, String> {
    let active_profile = dam_integrations::read_active_profile(state_dir)?;
    let enabled_profiles = dam_integrations::read_effective_enabled_integrations(state_dir)?;
    let proxy_url = integration_proxy_url(None);
    let mut applies = Vec::new();
    let mut inspection_errors = Vec::new();
    for profile in &enabled_profiles {
        match default_integration_target_path(&profile.profile_id, state_dir).and_then(
            |target_path| {
                dam_integrations::inspect_apply_in_state(
                    &profile.profile_id,
                    &proxy_url,
                    target_path,
                    state_dir,
                    state_dir,
                )
            },
        ) {
            Ok(inspection) => applies.push(inspection),
            Err(error) => inspection_errors.push(format!("{}: {error}", profile.profile_id)),
        }
    }

    Ok(ProfileStatusView {
        active_profile,
        enabled_profiles,
        proxy_url,
        applies,
        inspection_errors,
    })
}

fn generate_local_ca_output(state_dir: &std::path::Path, json: bool) -> Result<String, String> {
    let artifact =
        dam_trust::generate_local_ca_artifact(state_dir).map_err(|error| error.to_string())?;
    let view = LocalCaGenerateView {
        state: "generated",
        artifact,
    };
    if json {
        serde_json::to_string_pretty(&view)
            .map(|value| format!("{value}\n"))
            .map_err(|error| format!("failed to serialize local CA result: {error}"))
    } else {
        Ok(render_local_ca_generate_view(&view))
    }
}

fn delete_local_ca_output(state_dir: &std::path::Path, json: bool) -> Result<String, String> {
    let deleted =
        dam_trust::delete_local_ca_artifact(state_dir).map_err(|error| error.to_string())?;
    let view = LocalCaDeleteView {
        state: if deleted { "deleted" } else { "missing" },
        deleted,
        state_dir: state_dir.to_path_buf(),
    };
    if json {
        serde_json::to_string_pretty(&view)
            .map(|value| format!("{value}\n"))
            .map_err(|error| format!("failed to serialize local CA delete result: {error}"))
    } else {
        Ok(render_local_ca_delete_view(&view))
    }
}

fn install_local_ca_output(
    state_dir: &std::path::Path,
    json: bool,
    yes: bool,
) -> Result<String, String> {
    let result = if yes {
        dam_trust::install_local_ca_system_trust(state_dir)
    } else {
        dam_trust::preview_local_ca_install(state_dir)
    }
    .map_err(|error| error.to_string())?;
    if json {
        serde_json::to_string_pretty(&result)
            .map(|value| format!("{value}\n"))
            .map_err(|error| format!("failed to serialize local CA install result: {error}"))
    } else {
        Ok(render_local_ca_system_trust_result(&result, yes))
    }
}

fn remove_local_ca_output(
    state_dir: &std::path::Path,
    json: bool,
    yes: bool,
) -> Result<String, String> {
    let result = if yes {
        dam_trust::remove_local_ca_system_trust(state_dir)
    } else {
        dam_trust::preview_local_ca_remove(state_dir)
    }
    .map_err(|error| error.to_string())?;
    if json {
        serde_json::to_string_pretty(&result)
            .map(|value| format!("{value}\n"))
            .map_err(|error| format!("failed to serialize local CA remove result: {error}"))
    } else {
        Ok(render_local_ca_system_trust_result(&result, yes))
    }
}

fn active_profile_for_status() -> (Option<dam_integrations::ActiveProfileState>, Option<String>) {
    let state_dir = match integration_state_dir() {
        Ok(path) => path,
        Err(error) => return (None, Some(error)),
    };
    match dam_integrations::read_active_profile(&state_dir) {
        Ok(profile) => (profile, None),
        Err(error) => (None, Some(error)),
    }
}

fn enabled_profiles_for_connect_parse(args: &[String]) -> Result<ConnectProfileSelection, String> {
    if !matches!(args.first().map(String::as_str), Some("connect")) {
        return Ok(ConnectProfileSelection::default());
    }
    let connect_args = &args[1..];
    if matches!(
        connect_args.first().map(String::as_str),
        Some("-h" | "--help")
    ) {
        return Ok(ConnectProfileSelection::default());
    }
    if connect_args.iter().any(|arg| arg == "--profile") {
        return Ok(ConnectProfileSelection {
            integration_state_dir: Some(integration_state_dir()?),
            ..ConnectProfileSelection::default()
        });
    }
    if connect_args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "--target-name"
                | "--provider"
                | "--upstream"
                | "--target"
                | "--traffic-app"
                | "--no-traffic-apps"
        )
    }) {
        return Ok(ConnectProfileSelection::default());
    }

    let state_dir = integration_state_dir()?;
    let runtime_profiles = dam_integrations::runtime_enabled_profile_ids(&state_dir)?;
    let profiles = runtime_profiles.clone().unwrap_or_default();
    if profiles.is_empty() && connect_args.iter().any(|arg| arg == "--apply") {
        Err(
            "--apply requires --profile <id> or enabled profiles in `dam profile status`"
                .to_string(),
        )
    } else {
        Ok(ConnectProfileSelection {
            profile_ids: profiles,
            explicit_selection: runtime_profiles.is_some(),
            integration_state_dir: Some(state_dir),
        })
    }
}

fn integration_proxy_url(proxy_url: Option<String>) -> String {
    if let Some(proxy_url) = proxy_url {
        return proxy_url;
    }

    match dam_daemon::daemon_status() {
        Ok(dam_daemon::DaemonStatus::Connected(state)) => state.proxy_url,
        Ok(dam_daemon::DaemonStatus::Disconnected | dam_daemon::DaemonStatus::Stale(_))
        | Err(_) => dam_integrations::DEFAULT_PROXY_URL.to_string(),
    }
}

fn render_integration_list(
    profiles: &[dam_integrations::IntegrationProfile],
    proxy_url: &str,
) -> String {
    let mut output = String::new();
    output.push_str(&format!("proxy_url: {proxy_url}\n"));
    output.push_str("profiles:\n");
    for profile in profiles {
        output.push_str(&format!(
            "  {:<18} {} - {}\n",
            profile.id, profile.provider, profile.summary
        ));
    }
    output.push_str("\nUse `dam integrations show <profile>` for setup details.\n");
    output
}

fn render_integration_profile(
    profile: &dam_integrations::IntegrationProfile,
    proxy_url: &str,
) -> String {
    let mut output = String::new();
    output.push_str(&format!("profile: {}\n", profile.id));
    output.push_str(&format!("name: {}\n", profile.name));
    output.push_str(&format!("provider: {}\n", profile.provider));
    output.push_str(&format!("proxy_url: {proxy_url}\n"));
    output.push_str(&format!("summary: {}\n", profile.summary));

    if !profile.connect_args.is_empty() {
        let mut command = vec!["dam".to_string(), "connect".to_string()];
        command.extend(profile.connect_args.iter().cloned());
        output.push_str("\nconnect:\n");
        output.push_str(&format!("  {}\n", shell_command(&command)));
    }

    if !profile.settings.is_empty() {
        output.push_str("\nsettings:\n");
        for setting in &profile.settings {
            output.push_str(&format!(
                "  {}={}  # {}\n",
                setting.key,
                shell_quote(&setting.value),
                setting.description
            ));
        }
    }

    if !profile.commands.is_empty() {
        output.push_str("\ncommands:\n");
        for command in &profile.commands {
            output.push_str(&format!("  {}:\n", command.label));
            output.push_str(&format!("    {}\n", shell_command(&command.command)));
        }
    }

    if !profile.notes.is_empty() {
        output.push_str("\nnotes:\n");
        for note in &profile.notes {
            output.push_str(&format!("  - {note}\n"));
        }
    }

    output
}

fn integration_state_dir() -> Result<PathBuf, String> {
    dam_daemon::state_paths()
        .map(|paths| paths.state_dir.join("integrations"))
        .map_err(|error| error.to_string())
}

fn default_integration_target_path(
    profile_id: &str,
    state_dir: &std::path::Path,
) -> Result<PathBuf, String> {
    dam_integrations::default_apply_path(profile_id, state_dir)
}

fn apply_connect_profile(profile_id: &str, proxy_url: &str) -> Result<ConnectApplyOutcome, String> {
    let state_dir = integration_state_dir()?;
    let target_path = default_integration_target_path(profile_id, &state_dir)?;
    let inspection = dam_integrations::inspect_apply_in_state(
        profile_id,
        proxy_url,
        target_path.clone(),
        &state_dir,
        &state_dir,
    )?;

    if let Some(error) = &inspection.record_error {
        return Err(format!(
            "integration profile {profile_id} cannot be applied safely: {}\nrollback record issue: {error}\nRun `damctl integrations check {profile_id}` for details or `dam integrations rollback {profile_id}` to restore the last DAM change.",
            inspection.message
        ));
    }

    if inspection.status == dam_integrations::IntegrationApplyStatus::Modified {
        return Err(format!(
            "integration profile {profile_id} was previously applied, but the target no longer matches DAM's desired content: {}\nrefusing to overwrite during `dam connect --apply`; run `damctl integrations check {profile_id}` for details or `dam integrations rollback {profile_id}` to restore the last DAM change.",
            inspection.target_path.display()
        ));
    }

    let rollback_available_before_apply = inspection.rollback_available;
    let prepared =
        dam_integrations::prepare_apply_in_state(profile_id, proxy_url, target_path, &state_dir)?;
    let result = dam_integrations::run_apply(prepared, false, &state_dir)?;
    let rollback_available = rollback_available_before_apply || result.record_path.is_some();

    Ok(ConnectApplyOutcome {
        result,
        rollback_available,
    })
}

fn ensure_connect_transparent_prerequisites(
    proxy: &dam_daemon::ProxyOptions,
    config: &dam_config::DamConfig,
    state_dir: Option<PathBuf>,
) -> Result<(), String> {
    if proxy.network_mode == dam_net::CaptureMode::ExplicitProxy
        && proxy.trust_mode == dam_trust::TrustMode::Disabled
    {
        return Ok(());
    }
    let plan = dam_diagnostics::setup_plan(
        config,
        &dam_diagnostics::SetupPlanOptions {
            state_dir,
            config_path: proxy.config_path.clone(),
            proxy_url: Some(proxy_url_for_connect_apply(proxy)?),
            network_mode: proxy.network_mode,
            trust_mode: proxy.trust_mode,
        },
    )?;
    for step in &plan.steps {
        let enforced = matches!(
            step.kind,
            dam_diagnostics::SetupStepKind::SystemProxy
                | dam_diagnostics::SetupStepKind::NetworkExtension
                | dam_diagnostics::SetupStepKind::NetworkExtensionConfiguration
                | dam_diagnostics::SetupStepKind::NetworkExtensionEnable
                | dam_diagnostics::SetupStepKind::NetworkExtensionStart
                | dam_diagnostics::SetupStepKind::LinuxTransparentProxy
                | dam_diagnostics::SetupStepKind::WindowsFilteringPlatform
                | dam_diagnostics::SetupStepKind::LocalCa
        );
        if enforced
            && matches!(
                step.status,
                dam_diagnostics::SetupStepStatus::Needed
                    | dam_diagnostics::SetupStepStatus::Blocked
            )
        {
            return Err(render_connect_prerequisite_error(step));
        }
    }

    Ok(())
}

fn render_connect_prerequisite_error(step: &dam_diagnostics::SetupStep) -> String {
    let mut message = format!(
        "DAM cannot start this transparent setup yet: {}",
        step.message
    );
    if let Some(command) = &step.command {
        message.push_str(&format!("\nRun `{}` first.", command.join(" ")));
    }
    message
}

fn validate_connect_apply_profile_matches_proxy(
    profile_id: &str,
    proxy: &dam_daemon::ProxyOptions,
    integration_state_dir: Option<&Path>,
) -> Result<(), String> {
    let expected_app_ids =
        traffic_app_ids_for_profiles(&[profile_id.to_string()], integration_state_dir)?;
    if let Some(active_app_ids) = &proxy.traffic_app_ids
        && !expected_app_ids
            .iter()
            .any(|expected| active_app_ids.contains(expected))
    {
        return Err(format!(
            "profile {profile_id} is not included in the active traffic app selection"
        ));
    }
    if let Some(targets) = &proxy.targets {
        let expected_targets = proxy_targets_for_traffic_app_ids(&expected_app_ids);
        if !expected_targets.iter().any(|expected| {
            targets
                .iter()
                .any(|target| target.name == expected.name && target.provider == expected.provider)
        }) {
            return Err(format!(
                "profile {profile_id} is not included in the configured traffic targets"
            ));
        }
    }
    Ok(())
}

fn proxy_targets_for_profiles(
    profile_ids: &[String],
    integration_state_dir: Option<&Path>,
) -> Result<Vec<dam_config::ProxyTargetConfig>, String> {
    Ok(proxy_targets_for_traffic_app_ids(
        &traffic_app_ids_for_profiles(profile_ids, integration_state_dir)?,
    ))
}

fn proxy_targets_for_traffic_app_ids(app_ids: &[String]) -> Vec<dam_config::ProxyTargetConfig> {
    let profile = dam_net::llm_mvp_profile().with_runtime_enabled_apps(app_ids);
    let routes = dam_net::traffic_routes_from_profile(&profile);
    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();
    for route in routes {
        let key = (
            route.target_name.clone(),
            route.provider.clone(),
            route.upstream.clone(),
        );
        if !seen.insert(key.clone()) {
            continue;
        }
        let (name, provider, upstream) = key;
        targets.push(dam_config::ProxyTargetConfig {
            name,
            provider,
            upstream,
            auth: route.auth,
            failure_mode: None,
            api_key_env: None,
            api_key: None,
        });
    }
    targets
}

fn override_single_profile_target_upstream(
    targets: &mut [dam_config::ProxyTargetConfig],
    upstream: &str,
) -> Result<(), String> {
    let distinct_targets = targets
        .iter()
        .map(|target| (target.name.clone(), target.provider.clone()))
        .collect::<BTreeSet<_>>();
    if distinct_targets.len() > 1 {
        return Err(
            "--upstream can override only single-target profiles; use --target for multi-target profile tests"
                .to_string(),
        );
    }
    for target in targets {
        target.upstream = upstream.to_string();
    }
    Ok(())
}

fn last_option_value(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .rev()
        .filter(|window| window[0] == option)
        .map(|window| window[1].clone())
        .next()
}

fn traffic_app_ids_for_profiles(
    profile_ids: &[String],
    integration_state_dir: Option<&Path>,
) -> Result<Vec<String>, String> {
    let mut app_ids = Vec::new();
    for profile_id in profile_ids {
        let profile = integration_profile_for_state(
            profile_id,
            dam_integrations::DEFAULT_PROXY_URL,
            integration_state_dir,
        )?
        .ok_or_else(|| {
            format!(
                "unknown enabled integration profile: {profile_id}\nknown profiles: {}",
                dam_integrations::profile_ids().join(", ")
            )
        })?;
        for app_id in profile.traffic_app_ids {
            if !app_ids.contains(&app_id) {
                app_ids.push(app_id);
            }
        }
    }
    Ok(app_ids)
}

fn proxy_url_for_connect_apply(options: &dam_daemon::ProxyOptions) -> Result<String, String> {
    let addr = options
        .listen
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid --listen address {}: {error}", options.listen))?;
    if addr.port() == 0 {
        return Err(
            "dam connect --apply requires a fixed --listen port; port 0 cannot be written into a harness profile"
                .to_string(),
        );
    }
    Ok(dam_daemon::local_base_url(addr))
}

fn render_integration_apply_result(result: &dam_integrations::IntegrationApplyResult) -> String {
    let mut output = String::new();
    output.push_str(&format!("profile: {}\n", result.profile_id));
    output.push_str(&format!("state: {}\n", result.message));
    output.push_str(&format!("proxy_url: {}\n", result.proxy_url));
    if let Some(record_path) = &result.record_path {
        output.push_str(&format!("rollback_record: {}\n", record_path.display()));
    }
    for change in &result.changes {
        output.push_str(&format!(
            "{}: {} - {}\n",
            change.action.tag(),
            change.path.display(),
            change.description
        ));
    }
    output
}

fn render_connect_apply_outcome(outcome: &ConnectApplyOutcome) -> String {
    let mut output = render_integration_apply_result(&outcome.result);
    if outcome.rollback_available {
        output.push_str(&format!(
            "rollback: dam integrations rollback {}\n",
            outcome.result.profile_id
        ));
    }
    output
}

fn render_integration_rollback_result(
    result: &dam_integrations::IntegrationRollbackResult,
) -> String {
    let mut output = String::new();
    output.push_str(&format!("profile: {}\n", result.profile_id));
    output.push_str(&format!("state: {}\n", result.message));
    for change in &result.changes {
        output.push_str(&format!(
            "{}: {} - {}\n",
            change.action.tag(),
            change.path.display(),
            change.description
        ));
    }
    output
}

fn shell_command(command: &[String]) -> String {
    command
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "/:._=-".contains(ch))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
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

fn integration_apply_status_tag(status: dam_integrations::IntegrationApplyStatus) -> &'static str {
    match status {
        dam_integrations::IntegrationApplyStatus::Applied => "applied",
        dam_integrations::IntegrationApplyStatus::NeedsApply => "needs_apply",
        dam_integrations::IntegrationApplyStatus::Modified => "modified",
    }
}

fn severity_tag(severity: dam_api::DiagnosticSeverity) -> &'static str {
    match severity {
        dam_api::DiagnosticSeverity::Info => "info",
        dam_api::DiagnosticSeverity::Warning => "warning",
        dam_api::DiagnosticSeverity::Error => "error",
    }
}

fn health_state_tag(state: dam_api::HealthState) -> &'static str {
    match state {
        dam_api::HealthState::Healthy => "healthy",
        dam_api::HealthState::Degraded => "degraded",
        dam_api::HealthState::Unhealthy => "unhealthy",
        dam_api::HealthState::Unknown => "unknown",
    }
}

fn usage() -> &'static str {
    "Usage: dam <command>\n\nCommands:\n  connect       Start or resume the background DAM proxy daemon\n  web           Start the local DAM web UI\n  doctor        Run machine-readable local readiness diagnostics\n  status        Show background DAM protection status\n  setup         Inspect or recover the idempotent local setup plan\n  logs          Show concise local DAM operation logs\n  profile       Select and inspect the active harness profile\n  trust         Manage local trust artifacts and approved local trust changes\n  network       Manage local network routing plans and approved changes\n  startup       Inspect or record local startup setup choices\n  disconnect    Pause DAM protection, or stop the daemon with --stop\n  integrations  List and inspect known harness integration profiles\n\nRun `dam connect --help`, `dam web --help`, `dam doctor --help`, `dam setup --help`, `dam logs --help`, `dam profile --help`, `dam trust --help`, `dam network --help`, `dam startup --help`, or `dam integrations --help` for command options."
}

fn usage_connect() -> String {
    format!(
        "Usage: dam connect [--profile PROFILE] [--apply] [--json] [DAM_OPTIONS]\n\nStarts a background DAM proxy daemon for proxy/interception routing. Enabled app profiles select daemon targets automatically. --apply additionally ensures selected DAM profile files before connecting.\n\nDAM options:\n  --profile <id>          Use integration profile daemon defaults\n  --apply                 Ensure selected or enabled DAM profile files before connecting\n  --json                  Print a stable machine-readable connect result\n  --config <path>         Load DAM config file before daemon overrides\n  --listen <addr>         Local proxy listen address (default: 127.0.0.1:7828)\n  --network-mode <mode>   Control-plane network mode: explicit_proxy, system_proxy, or tun\n  --trust-mode <mode>     Control-plane trust mode: disabled or local_ca\n  --target-name <name>    Low-level proxy target name\n  --provider <provider>   Low-level target label\n  --upstream <url>        Low-level target upstream URL\n  --target <json>         Internal repeated target JSON\n  --db <path>             Vault SQLite path (default: vault.db)\n  --log <path>            Log SQLite path (default: log.db)\n  --consent-db <path>     Consent SQLite path (default: consent.db)\n  --no-log                Disable DAM log writes\n  --no-resolve-inbound    Leave DAM references unresolved in inbound responses\n  --resolve-inbound       Restore DAM references in inbound responses (default)\n\nKnown profiles: {}",
        dam_integrations::profile_ids().join(", ")
    )
}

fn usage_status() -> &'static str {
    "Usage: dam status [--json]"
}

fn usage_doctor() -> &'static str {
    "Usage: dam doctor [--config PATH] [--state-dir PATH] [--proxy-url URL] [--network-mode explicit_proxy|system_proxy|tun] [--trust-mode disabled|local_ca] [--json]\n\nRuns local readiness diagnostics without calling remote providers. JSON output is stable for agents and installers."
}

fn usage_setup() -> &'static str {
    "Usage: dam setup <command>\n\nCommands:\n  status              Alias for plan; print the full idempotent setup checklist\n  plan                Print the full idempotent setup checklist\n  next-action         Print only the next setup action\n  resume              Alias for next-action after restart or interrupted setup\n  rescue              Preview or apply local setup rescue actions\n  repair              Preview or apply rescue, then print the current setup plan\n  export-diagnostics  Export offline setup diagnostics"
}

fn usage_setup_plan(command: &str) -> String {
    let description = match command {
        "status" => "Print the full idempotent setup checklist.",
        "plan" => "Print the full idempotent setup checklist.",
        "next-action" => "Print only the next setup action.",
        "resume" => "Print the next setup action after restart or interrupted setup.",
        "export-diagnostics" => "Export offline setup diagnostics and next recovery guidance.",
        "repair" => "Preview repair setup planning options.",
        _ => "Print setup planning information.",
    };
    format!(
        "Usage: dam setup {command} [--config PATH] [--state-dir PATH] [--proxy-url URL] [--network-mode explicit_proxy|system_proxy|tun] [--trust-mode disabled|local_ca] [--json]\n\n{description}"
    )
}

fn usage_setup_rescue() -> &'static str {
    "Usage: dam setup rescue [--state-dir PATH] [--dry-run|--yes] [--json]\n\nPreviews local recovery by default. Use --yes to stop the DAM daemon and remove DAM-managed macOS routing state so normal networking can resume."
}

fn usage_setup_repair() -> &'static str {
    "Usage: dam setup repair [--config PATH] [--state-dir PATH] [--proxy-url URL] [--network-mode explicit_proxy|system_proxy|tun] [--trust-mode disabled|local_ca] [--dry-run|--yes] [--json]\n\nPreviews rescue plus the current setup plan by default. Use --yes to apply local rescue first, then follow the returned setup_plan.next_action."
}

fn usage_logs() -> &'static str {
    "Usage: dam logs [--limit N] [--after-id ID] [--operation OPERATION_ID] [--events] [--json]\n\nShows concise non-sensitive operation summaries by default. Use --operation to inspect one operation's event timeline, or --events to show raw log event rows without grouping."
}

fn usage_disconnect() -> &'static str {
    "Usage: dam disconnect [--stop] [--json]\n\nBy default, `dam disconnect` pauses protection while leaving the daemon in pass-through mode so existing clients keep working. Use --stop after restoring routing or app profile setup when the daemon should exit."
}

fn usage_profile() -> &'static str {
    "Usage: dam profile <command>\n\nCommands:\n  status  Show the active harness profile and apply state\n  set     Select the active harness profile\n  clear   Clear the active harness profile"
}

fn usage_profile_status() -> &'static str {
    "Usage: dam profile status [--json]"
}

fn usage_profile_set() -> &'static str {
    "Usage: dam profile set <profile> [--json]"
}

fn usage_profile_clear() -> &'static str {
    "Usage: dam profile clear [--json]"
}

fn usage_trust() -> &'static str {
    "Usage: dam trust <command>\n\nCommands:\n  generate-local-ca  Generate local CA certificate/key artifacts without installing trust\n  delete-local-ca    Delete uninstalled local CA artifacts\n  install-local-ca   Preview or install the DAM local CA in local trust\n  remove-local-ca    Preview or remove the DAM local CA from local trust"
}

fn usage_trust_generate_local_ca() -> &'static str {
    "Usage: dam trust generate-local-ca [--json]\n\nCreates local CA certificate/key artifacts under the DAM state directory. This does not install a CA or change local trust."
}

fn usage_trust_delete_local_ca() -> &'static str {
    "Usage: dam trust delete-local-ca [--json]\n\nDeletes DAM-managed local CA artifacts only when they are not marked installed. This does not change local trust."
}

fn usage_trust_install_local_ca() -> &'static str {
    "Usage: dam trust install-local-ca [--dry-run|--yes] [--json]\n\nPreviews the local trust change by default. Use --yes to install the DAM local CA into the macOS user login keychain."
}

fn usage_trust_remove_local_ca() -> &'static str {
    "Usage: dam trust remove-local-ca [--dry-run|--yes] [--json]\n\nPreviews the local trust removal by default. Use --yes to remove the recorded DAM local CA from the macOS user login keychain."
}

fn usage_network() -> &'static str {
    "Usage: dam network <command>\n\nCommands:\n  install-system-proxy       Preview or install macOS PAC routing for proxy-capable traffic\n  remove-system-proxy        Preview or remove DAM macOS PAC routing and restore prior settings\n  install-network-extension  Preview or install macOS Network Extension capture for tun mode\n  remove-network-extension   Preview or remove DAM macOS Network Extension capture\n  status                     Show macOS capture backend status"
}

fn usage_network_install_system_proxy() -> &'static str {
    "Usage: dam network install-system-proxy [--config PATH] [--dry-run|--yes] [--json]\n\nPreviews macOS PAC system proxy routing by default. Use --yes to route proxy-capable HTTP and HTTPS traffic to DAM. Unknown hosts pass through untouched; active traffic profile hosts are protected only when routing, trust, consent, and the TLS adapter are all ready."
}

fn usage_network_remove_system_proxy() -> &'static str {
    "Usage: dam network remove-system-proxy [--dry-run|--yes] [--json]\n\nPreviews macOS PAC system proxy rollback by default. Use --yes to restore the prior auto-proxy settings recorded before DAM changed them."
}

fn usage_network_install_network_extension() -> &'static str {
    "Usage: dam network install-network-extension [--config PATH] [--dry-run|--yes] [--json]\n\nPreviews macOS Network Extension capture by default. Use --yes to activate the packaged Network Extension backend for DAM tun mode. In source builds without a packaged helper, DAM records control-plane state only; release builds must supply the native helper through DAM_MACOS_NE_HELPER or the app bundle."
}

fn usage_network_remove_network_extension() -> &'static str {
    "Usage: dam network remove-network-extension [--dry-run|--yes] [--json]\n\nPreviews macOS Network Extension removal by default. Use --yes to deactivate the packaged capture backend and clear DAM rollback state."
}

fn usage_network_status() -> &'static str {
    "Usage: dam network status [--json]\n\nShows macOS Network Extension capture state for DAM tun mode."
}

fn usage_startup() -> &'static str {
    "Usage: dam startup <command>\n\nCommands:\n  status              Show the local startup setup choice\n  skip-open-at-login  Record that Open at Login was intentionally skipped"
}

fn usage_startup_status() -> &'static str {
    "Usage: dam startup status [--json]\n\nShows whether DAM startup setup is registered, skipped, or still unconfigured."
}

fn usage_startup_skip_open_at_login() -> &'static str {
    "Usage: dam startup skip-open-at-login [--json]\n\nRecords the same choice as the tray Skip button so scripted installs can continue setup without adding DAM to Open at Login."
}

fn usage_integrations() -> &'static str {
    "Usage: dam integrations <command>\n\nCommands:\n  list      List known integration profiles\n  show      Show setup details for one integration profile\n  apply     Apply a harness integration profile with backup support\n  rollback  Roll back the last DAM integration profile change"
}

fn usage_integrations_list() -> &'static str {
    "Usage: dam integrations list [--proxy-url http://127.0.0.1:7828] [--json]"
}

fn usage_integrations_show() -> &'static str {
    "Usage: dam integrations show <profile> [--proxy-url http://127.0.0.1:7828] [--json]"
}

fn usage_integrations_apply() -> &'static str {
    "Usage: dam integrations apply <profile> [--write|--dry-run] [--proxy-url http://127.0.0.1:7828] [--target-path PATH] [--json]\n\nPreviews a profile file operation by default. Use --write to ensure the DAM catalog profile, or combine --write with --target-path to write a rendered JSON export with rollback support."
}

fn usage_integrations_rollback() -> &'static str {
    "Usage: dam integrations rollback <profile> [--json]"
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
