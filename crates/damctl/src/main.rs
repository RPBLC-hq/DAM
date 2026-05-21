use std::env;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Status(StatusArgs),
    Doctor(DoctorArgs),
    Bypass(BypassArgs),
    Daemon(DaemonArgs),
    Trust(TrustArgs),
    Network(NetworkArgs),
    Setup(SetupArgs),
    Integrations(IntegrationsArgs),
    ConfigCheck(ConfigCheckArgs),
    McpConfig(McpConfigArgs),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CommonArgs {
    config: dam_config::ConfigOverrides,
    json: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StatusArgs {
    common: CommonArgs,
    proxy_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DoctorArgs {
    common: CommonArgs,
    proxy_url: Option<String>,
    state_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ConfigCheckArgs {
    common: CommonArgs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BypassArgs {
    command: BypassCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BypassCommand {
    Status(BypassStatusArgs),
}

impl Default for BypassCommand {
    fn default() -> Self {
        Self::Status(BypassStatusArgs::default())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BypassStatusArgs {
    common: CommonArgs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DaemonArgs {
    command: DaemonCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DaemonCommand {
    Inspect(DaemonInspectArgs),
}

impl Default for DaemonCommand {
    fn default() -> Self {
        Self::Inspect(DaemonInspectArgs::default())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DaemonInspectArgs {
    json: bool,
    state_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TrustArgs {
    command: TrustCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TrustCommand {
    Inspect(TrustInspectArgs),
}

impl Default for TrustCommand {
    fn default() -> Self {
        Self::Inspect(TrustInspectArgs::default())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TrustInspectArgs {
    json: bool,
    state_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NetworkArgs {
    command: NetworkCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NetworkCommand {
    Inspect(NetworkInspectArgs),
}

impl Default for NetworkCommand {
    fn default() -> Self {
        Self::Inspect(NetworkInspectArgs::default())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NetworkInspectArgs {
    json: bool,
    state_dir: Option<PathBuf>,
    config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SetupArgs {
    command: SetupCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SetupCommand {
    Plan(SetupPlanArgs),
}

impl Default for SetupCommand {
    fn default() -> Self {
        Self::Plan(SetupPlanArgs::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetupPlanArgs {
    common: CommonArgs,
    state_dir: Option<PathBuf>,
    proxy_url: Option<String>,
    network_mode: dam_net::CaptureMode,
    trust_mode: dam_trust::TrustMode,
}

impl Default for SetupPlanArgs {
    fn default() -> Self {
        Self {
            common: CommonArgs::default(),
            state_dir: None,
            proxy_url: None,
            network_mode: dam_net::CaptureMode::ExplicitProxy,
            trust_mode: dam_trust::TrustMode::Disabled,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct IntegrationsArgs {
    command: IntegrationsCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IntegrationsCommand {
    Check(IntegrationsCheckArgs),
}

impl Default for IntegrationsCommand {
    fn default() -> Self {
        Self::Check(IntegrationsCheckArgs::default())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct IntegrationsCheckArgs {
    profile_id: Option<String>,
    json: bool,
    proxy_url: Option<String>,
    target_path: Option<PathBuf>,
    state_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct McpConfigArgs {
    config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct IntegrationsCheckReport {
    proxy_url: String,
    profiles: Vec<dam_integrations::IntegrationApplyInspection>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct DaemonInspectReport {
    state: &'static str,
    message: String,
    state_dir: PathBuf,
    state_file: PathBuf,
    process_running: Option<bool>,
    daemon: Option<dam_daemon::DaemonState>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct TrustInspectReport {
    state: &'static str,
    message: String,
    source: &'static str,
    state_dir: PathBuf,
    state_file: PathBuf,
    trust: dam_trust::TrustState,
    local_ca_artifact: Option<dam_trust::LocalCaArtifact>,
    route_readiness: Vec<dam_trust::RouteTrustReadiness>,
    actions: Vec<dam_trust::TrustActionPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct NetworkInspectReport {
    state: &'static str,
    message: String,
    state_dir: PathBuf,
    rollback_path: PathBuf,
    pac_path: PathBuf,
    support: &'static str,
    system_proxy_installed: bool,
    configured_hosts: Vec<String>,
    route_readiness: Vec<dam_net::TransparentRouteCaptureReadiness>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct BypassStatusReport {
    state: &'static str,
    message: String,
    reduced_guarantees: bool,
    proxy_enabled: bool,
    proxy_default_failure_mode: String,
    proxy_targets: Vec<BypassTargetStatus>,
    vault_write_failure_mode: String,
    log_write_failure_mode: String,
    diagnostics: Vec<dam_api::Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct BypassTargetStatus {
    name: String,
    provider: String,
    upstream: String,
    configured_failure_mode: Option<String>,
    effective_failure_mode: String,
    reduced_guarantee: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

impl CommandOutput {
    fn fail(code: i32, stderr: impl Into<String>) -> Self {
        Self {
            code,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }
}

#[tokio::main]
async fn main() {
    let output = match parse_args(env::args().skip(1)) {
        Ok(command) => run(command).await,
        Err(message) => CommandOutput::fail(2, format!("{message}\n{}", usage())),
    };

    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    std::process::exit(output.code);
}

async fn run(command: Command) -> CommandOutput {
    match command {
        Command::Status(args) => status(args).await,
        Command::Doctor(args) => doctor(args).await,
        Command::Bypass(args) => bypass(args),
        Command::Daemon(args) => daemon(args),
        Command::Trust(args) => trust(args),
        Command::Network(args) => network(args),
        Command::Setup(args) => setup(args),
        Command::Integrations(args) => integrations(args),
        Command::ConfigCheck(args) => config_check(args),
        Command::McpConfig(args) => mcp_config(args),
    }
}

async fn status(args: StatusArgs) -> CommandOutput {
    let (config, config_warning) = match dam_config::load(&args.common.config) {
        Ok(config) => (Some(config), None),
        Err(error) if args.proxy_url.is_some() => {
            (None, Some(format!("config load failed: {error}")))
        }
        Err(error) => return CommandOutput::fail(2, format!("config load failed: {error}\n")),
    };

    let health_url = match status_url(&args, config.as_ref()) {
        Ok(url) => url,
        Err(message) => return CommandOutput::fail(2, format!("{message}\n")),
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(2_000))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return CommandOutput::fail(1, format!("failed to build http client: {error}\n"));
        }
    };

    let report = match client.get(&health_url).send().await {
        Ok(response) => match response.json::<dam_api::ProxyReport>().await {
            Ok(report) => report,
            Err(error) => dam_down_report(
                config.as_ref(),
                format!("DAM proxy returned an unreadable status response: {error}"),
            ),
        },
        Err(error) => dam_down_report(
            config.as_ref(),
            format!("DAM proxy is not reachable at {health_url}: {error}"),
        ),
    };

    let code = if report.state == dam_api::ProxyState::Protected {
        0
    } else {
        1
    };

    let stdout = if args.common.json {
        json(&report)
    } else {
        render_proxy_report(&report, config_warning.as_deref())
    };

    CommandOutput {
        code,
        stdout,
        stderr: String::new(),
    }
}

async fn doctor(args: DoctorArgs) -> CommandOutput {
    let config = match dam_config::load(&args.common.config) {
        Ok(config) => config,
        Err(error) => {
            let report = config_load_failed_report(error);
            return CommandOutput {
                code: 2,
                stdout: if args.common.json {
                    json(&report)
                } else {
                    render_health_report(&report)
                },
                stderr: String::new(),
            };
        }
    };

    let mut report = dam_diagnostics::doctor_report(
        &config,
        &dam_diagnostics::DoctorOptions {
            proxy_url: args.proxy_url.clone(),
            state_dir: args.state_dir.clone(),
            config_path: args.common.config.config_path.clone(),
            ..dam_diagnostics::DoctorOptions::default()
        },
    )
    .await;
    add_integration_doctor_summary(&mut report, args.state_dir, args.proxy_url);
    let code = if report.state == dam_api::HealthState::Unhealthy {
        1
    } else {
        0
    };
    let stdout = if args.common.json {
        json(&report)
    } else {
        render_health_report(&report)
    };

    CommandOutput {
        code,
        stdout,
        stderr: String::new(),
    }
}

fn config_check(args: ConfigCheckArgs) -> CommandOutput {
    let config = match dam_config::load(&args.common.config) {
        Ok(config) => config,
        Err(error) => {
            let report = config_load_failed_report(error);
            return CommandOutput {
                code: 1,
                stdout: if args.common.json {
                    json(&report)
                } else {
                    render_health_report(&report)
                },
                stderr: String::new(),
            };
        }
    };

    let report = dam_diagnostics::config_report(&config);
    let code = if report.state == dam_api::HealthState::Unhealthy {
        1
    } else {
        0
    };
    let stdout = if args.common.json {
        json(&report)
    } else {
        render_health_report(&report)
    };

    CommandOutput {
        code,
        stdout,
        stderr: String::new(),
    }
}

fn bypass(args: BypassArgs) -> CommandOutput {
    match args.command {
        BypassCommand::Status(args) => bypass_status(args),
    }
}

fn bypass_status(args: BypassStatusArgs) -> CommandOutput {
    let config = match dam_config::load(&args.common.config) {
        Ok(config) => config,
        Err(error) => return CommandOutput::fail(2, format!("config load failed: {error}\n")),
    };
    let report = bypass_status_report(&config);
    let code = if report.reduced_guarantees { 1 } else { 0 };
    let stdout = if args.common.json {
        json(&report)
    } else {
        render_bypass_status_report(&report)
    };

    CommandOutput {
        code,
        stdout,
        stderr: String::new(),
    }
}

fn daemon(args: DaemonArgs) -> CommandOutput {
    match args.command {
        DaemonCommand::Inspect(args) => daemon_inspect(args),
    }
}

fn daemon_inspect(args: DaemonInspectArgs) -> CommandOutput {
    let report = match daemon_inspect_report(&args) {
        Ok(report) => report,
        Err(error) => return CommandOutput::fail(2, format!("{error}\n")),
    };
    let stdout = if args.json {
        json(&report)
    } else {
        render_daemon_inspect_report(&report)
    };

    CommandOutput {
        code: 0,
        stdout,
        stderr: String::new(),
    }
}

fn trust(args: TrustArgs) -> CommandOutput {
    match args.command {
        TrustCommand::Inspect(args) => trust_inspect(args),
    }
}

fn trust_inspect(args: TrustInspectArgs) -> CommandOutput {
    let report = match trust_inspect_report(&args) {
        Ok(report) => report,
        Err(error) => return CommandOutput::fail(2, format!("{error}\n")),
    };
    let stdout = if args.json {
        json(&report)
    } else {
        render_trust_inspect_report(&report)
    };

    CommandOutput {
        code: 0,
        stdout,
        stderr: String::new(),
    }
}

fn network(args: NetworkArgs) -> CommandOutput {
    match args.command {
        NetworkCommand::Inspect(args) => network_inspect(args),
    }
}

fn network_inspect(args: NetworkInspectArgs) -> CommandOutput {
    let report = match network_inspect_report(&args) {
        Ok(report) => report,
        Err(error) => return CommandOutput::fail(2, format!("{error}\n")),
    };
    let stdout = if args.json {
        json(&report)
    } else {
        render_network_inspect_report(&report)
    };

    CommandOutput {
        code: 0,
        stdout,
        stderr: String::new(),
    }
}

fn setup(args: SetupArgs) -> CommandOutput {
    match args.command {
        SetupCommand::Plan(args) => setup_plan(args),
    }
}

fn setup_plan(args: SetupPlanArgs) -> CommandOutput {
    let config_path = args.common.config.config_path.clone();
    let config = match dam_config::load(&args.common.config) {
        Ok(config) => config,
        Err(error) => return CommandOutput::fail(2, format!("config load failed: {error}\n")),
    };
    let report = match dam_diagnostics::setup_plan(
        &config,
        &dam_diagnostics::SetupPlanOptions {
            state_dir: args.state_dir,
            config_path,
            proxy_url: args.proxy_url,
            network_mode: args.network_mode,
            trust_mode: args.trust_mode,
        },
    ) {
        Ok(report) => report,
        Err(error) => return CommandOutput::fail(2, format!("setup plan failed: {error}\n")),
    };
    let code = if report.state == dam_diagnostics::SetupPlanState::Ready {
        0
    } else {
        1
    };
    let stdout = if args.common.json {
        json(&report)
    } else {
        render_setup_plan(&report)
    };

    CommandOutput {
        code,
        stdout,
        stderr: String::new(),
    }
}

fn integrations(args: IntegrationsArgs) -> CommandOutput {
    match args.command {
        IntegrationsCommand::Check(args) => integrations_check(args),
    }
}

fn integrations_check(args: IntegrationsCheckArgs) -> CommandOutput {
    let report = match integrations_check_report(&args) {
        Ok(report) => report,
        Err(error) => return CommandOutput::fail(2, format!("{error}\n")),
    };
    let code = integrations_check_exit_code(&report, args.profile_id.is_some());
    let stdout = if args.json {
        json(&report)
    } else {
        render_integrations_check_report(&report)
    };

    CommandOutput {
        code,
        stdout,
        stderr: String::new(),
    }
}

fn mcp_config(args: McpConfigArgs) -> CommandOutput {
    let mut mcp_args = Vec::new();
    if let Some(config_path) = args.config_path {
        mcp_args.push("--config".to_string());
        mcp_args.push(config_path.display().to_string());
    }

    let value = serde_json::json!({
        "mcpServers": {
            "dam": {
                "command": "dam-mcp",
                "args": mcp_args
            }
        }
    });

    CommandOutput {
        code: 0,
        stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
        stderr: String::new(),
    }
}

fn add_integration_doctor_summary(
    report: &mut dam_api::HealthReport,
    state_dir: Option<PathBuf>,
    proxy_url: Option<String>,
) {
    let args = IntegrationsCheckArgs {
        state_dir,
        proxy_url,
        ..IntegrationsCheckArgs::default()
    };
    let check = match integrations_check_report(&args) {
        Ok(report) => report,
        Err(error) => {
            report.components.push(dam_api::ComponentHealth {
                component: "integrations".to_string(),
                state: dam_api::HealthState::Degraded,
                message: format!("integration profile checks unavailable: {error}"),
            });
            report.diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Warning,
                "integrations_check_unavailable",
                error,
            ));
            report.state = aggregate_health_state(&report.components);
            return;
        }
    };

    let applied = check
        .profiles
        .iter()
        .filter(|profile| {
            profile.status == dam_integrations::IntegrationApplyStatus::Applied
                && profile.rollback_available
        })
        .count();
    let modified = check
        .profiles
        .iter()
        .filter(|profile| profile.status == dam_integrations::IntegrationApplyStatus::Modified)
        .count();
    let record_errors = check
        .profiles
        .iter()
        .filter(|profile| profile.record_error.is_some())
        .count();
    let state = if modified > 0 || record_errors > 0 {
        dam_api::HealthState::Degraded
    } else if applied > 0 {
        dam_api::HealthState::Healthy
    } else {
        dam_api::HealthState::Degraded
    };
    report.components.push(dam_api::ComponentHealth {
        component: "integrations".to_string(),
        state,
        message: format!(
            "{applied}/{} profile(s) applied, {modified} modified target(s), {record_errors} rollback record issue(s)",
            check.profiles.len()
        ),
    });
    for profile in check
        .profiles
        .iter()
        .filter(|profile| profile.status == dam_integrations::IntegrationApplyStatus::Modified)
    {
        report.diagnostics.push(dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Warning,
            "integration_profile_modified",
            format!(
                "integration profile {} target {} no longer matches DAM's desired content",
                profile.profile_id,
                profile.target_path.display()
            ),
        ));
    }
    for (profile, error) in check
        .profiles
        .iter()
        .filter_map(|profile| profile.record_error.as_ref().map(|error| (profile, error)))
    {
        report.diagnostics.push(dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Warning,
            "integration_rollback_record_unreadable",
            format!("{}: {error}", profile.profile_id),
        ));
    }
    report.state = aggregate_health_state(&report.components);
}

fn config_load_failed_report(error: dam_config::ConfigError) -> dam_api::HealthReport {
    dam_api::HealthReport {
        state: dam_api::HealthState::Unhealthy,
        components: vec![dam_api::ComponentHealth {
            component: "config".to_string(),
            state: dam_api::HealthState::Unhealthy,
            message: format!("config load failed: {error}"),
        }],
        diagnostics: vec![dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Error,
            "config_load_failed",
            error.to_string(),
        )],
    }
}

fn daemon_inspect_report(args: &DaemonInspectArgs) -> Result<DaemonInspectReport, String> {
    let paths = daemon_state_paths(args.state_dir.clone())?;
    let status = match args.state_dir.as_ref() {
        Some(_) => daemon_status_from_file(&paths.state_file)?,
        None => dam_daemon::daemon_status().map_err(|error| error.to_string())?,
    };

    let report = match status {
        dam_daemon::DaemonStatus::Disconnected => DaemonInspectReport {
            state: "disconnected",
            message: "no daemon state file".to_string(),
            state_dir: paths.state_dir,
            state_file: paths.state_file,
            process_running: None,
            daemon: None,
        },
        dam_daemon::DaemonStatus::Stale(state) => DaemonInspectReport {
            state: "stale",
            message: format!("daemon state points at stopped pid {}", state.pid),
            state_dir: paths.state_dir,
            state_file: paths.state_file,
            process_running: Some(false),
            daemon: Some(state),
        },
        dam_daemon::DaemonStatus::Connected(state) => DaemonInspectReport {
            state: "connected",
            message: format!("daemon process {} is running", state.pid),
            state_dir: paths.state_dir,
            state_file: paths.state_file,
            process_running: Some(true),
            daemon: Some(state),
        },
    };

    Ok(report)
}

fn daemon_state_paths(state_dir: Option<PathBuf>) -> Result<dam_daemon::StatePaths, String> {
    match state_dir {
        Some(state_dir) => Ok(dam_daemon::StatePaths {
            state_file: state_dir.join("daemon.json"),
            state_dir,
        }),
        None => dam_daemon::state_paths().map_err(|error| error.to_string()),
    }
}

fn daemon_status_from_file(path: &std::path::Path) -> Result<dam_daemon::DaemonStatus, String> {
    let Some(state) = dam_daemon::read_state_from(path).map_err(|error| error.to_string())? else {
        return Ok(dam_daemon::DaemonStatus::Disconnected);
    };
    if dam_daemon::process_is_running(state.pid) {
        Ok(dam_daemon::DaemonStatus::Connected(state))
    } else {
        Ok(dam_daemon::DaemonStatus::Stale(state))
    }
}

fn trust_inspect_report(args: &TrustInspectArgs) -> Result<TrustInspectReport, String> {
    let paths = daemon_state_paths(args.state_dir.clone())?;
    let status = match args.state_dir.as_ref() {
        Some(_) => daemon_status_from_file(&paths.state_file)?,
        None => dam_daemon::daemon_status().map_err(|error| error.to_string())?,
    };
    let (source, trust) = match status {
        dam_daemon::DaemonStatus::Connected(state) => ("daemon", state.trust),
        dam_daemon::DaemonStatus::Stale(state) => ("stale_daemon", state.trust),
        dam_daemon::DaemonStatus::Disconnected => (
            "default",
            dam_trust::trust_state_for_state_dir(dam_trust::TrustMode::Disabled, &paths.state_dir)
                .map_err(|error| error.to_string())?,
        ),
    };
    let local_ca_artifact = dam_trust::inspect_local_ca_artifact(&paths.state_dir)
        .map_err(|error| error.to_string())?;
    let route_readiness = dam_trust::readiness_for_default_routes(
        &trust,
        trust.mode == dam_trust::TrustMode::LocalCa,
    );
    let actions = [
        dam_trust::TrustAction::Inspect,
        dam_trust::TrustAction::InstallLocalCa,
        dam_trust::TrustAction::RemoveLocalCa,
    ]
    .into_iter()
    .map(|action| dam_trust::TrustActionPlan::for_action(action, trust.platform_store))
    .collect();

    Ok(TrustInspectReport {
        state: "inspectable",
        message: "trust inspection is read-only; install/remove require explicit approval"
            .to_string(),
        source,
        state_dir: paths.state_dir,
        state_file: paths.state_file,
        trust,
        local_ca_artifact,
        route_readiness,
        actions,
    })
}

fn network_inspect_report(args: &NetworkInspectArgs) -> Result<NetworkInspectReport, String> {
    let paths = daemon_state_paths(args.state_dir.clone())?;
    let network_paths = dam_net_macos::MacosNetworkPaths::for_state_dir(&paths.state_dir);
    let system_proxy_installed = dam_net_macos::system_proxy_installed(&paths.state_dir);
    let traffic_routes = network_inspect_traffic_routes(args.config_path.clone())?;
    let route_readiness = dam_net::transparent_capture_readiness_for_routes(
        &traffic_routes,
        dam_net::CaptureMode::SystemProxy,
        system_proxy_installed,
        false,
    );
    let configured_hosts = traffic_routes
        .iter()
        .map(|route| route.host.clone())
        .collect::<Vec<_>>();

    Ok(NetworkInspectReport {
        state: if system_proxy_installed {
            "installed"
        } else {
            "not_installed"
        },
        message: if system_proxy_installed {
            "macOS system proxy routing rollback state is present".to_string()
        } else {
            "macOS system proxy routing is not installed by DAM".to_string()
        },
        state_dir: paths.state_dir,
        rollback_path: network_paths.rollback_path,
        pac_path: network_paths.pac_path,
        support: if cfg!(target_os = "macos") {
            "implemented"
        } else {
            "planned"
        },
        system_proxy_installed,
        configured_hosts,
        route_readiness,
    })
}

fn network_inspect_traffic_routes(
    config_path: Option<PathBuf>,
) -> Result<Vec<dam_net::TrafficRoute>, String> {
    let Some(config_path) = config_path else {
        return Ok(dam_net::default_traffic_routes());
    };
    let config = dam_config::load(&dam_config::ConfigOverrides {
        config_path: Some(config_path),
        ..dam_config::ConfigOverrides::default()
    })
    .map_err(|error| error.to_string())?;
    Ok(dam_net::traffic_routes_from_profile(
        &config.traffic.effective_profile(),
    ))
}

fn integrations_check_report(
    args: &IntegrationsCheckArgs,
) -> Result<IntegrationsCheckReport, String> {
    let proxy_url = integration_proxy_url(args.proxy_url.clone());
    let state_dir = args
        .state_dir
        .clone()
        .map(|path| path.join("integrations"))
        .map(Ok)
        .unwrap_or_else(integration_state_dir)?;
    let profile_ids = match &args.profile_id {
        Some(profile_id) => vec![profile_id.clone()],
        None => dam_integrations::profiles_from_state(&proxy_url, &state_dir)?
            .into_iter()
            .map(|profile| profile.id)
            .collect(),
    };
    let mut profiles = Vec::new();

    for profile_id in profile_ids {
        let target_path = match &args.target_path {
            Some(path) if args.profile_id.is_some() => path.clone(),
            Some(_) => {
                return Err("--target-path can only be used when checking one profile".to_string());
            }
            None => dam_integrations::default_apply_path(&profile_id, &state_dir)?,
        };
        profiles.push(dam_integrations::inspect_apply_in_state(
            &profile_id,
            &proxy_url,
            target_path,
            &state_dir,
            &state_dir,
        )?);
    }

    Ok(IntegrationsCheckReport {
        proxy_url,
        profiles,
    })
}

fn bypass_status_report(config: &dam_config::DamConfig) -> BypassStatusReport {
    let diagnostics = bypass_status_diagnostics(config);
    let proxy_targets = config
        .proxy
        .targets
        .iter()
        .map(|target| {
            let effective = target.effective_failure_mode(config.proxy.default_failure_mode);
            BypassTargetStatus {
                name: target.name.clone(),
                provider: target.provider.clone(),
                upstream: target.upstream.clone(),
                configured_failure_mode: target.failure_mode.map(|mode| mode.tag().to_string()),
                effective_failure_mode: effective.tag().to_string(),
                reduced_guarantee: proxy_failure_mode_reduces_guarantees(effective),
            }
        })
        .collect::<Vec<_>>();
    let reduced_guarantees = !diagnostics.is_empty();

    BypassStatusReport {
        state: if reduced_guarantees {
            "reduced"
        } else {
            "strict"
        },
        message: if reduced_guarantees {
            "reduced-protection modes are enabled".to_string()
        } else {
            "failure modes are strict".to_string()
        },
        reduced_guarantees,
        proxy_enabled: config.proxy.enabled,
        proxy_default_failure_mode: config.proxy.default_failure_mode.tag().to_string(),
        proxy_targets,
        vault_write_failure_mode: config.failure.vault_write.tag().to_string(),
        log_write_failure_mode: config.failure.log_write.tag().to_string(),
        diagnostics,
    }
}

fn bypass_status_diagnostics(config: &dam_config::DamConfig) -> Vec<dam_api::Diagnostic> {
    let mut diagnostics = Vec::new();

    match config.proxy.default_failure_mode {
        dam_config::ProxyFailureMode::BypassOnError => {
            diagnostics.push(dam_api::Diagnostic::new(
                dam_api::DiagnosticSeverity::Warning,
                "proxy_bypass_on_error",
                "proxy default failure mode can forward unprotected traffic when protection fails",
            ));
        }
        dam_config::ProxyFailureMode::RedactOnly => {
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
        diagnostics.push(dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Warning,
            "vault_redact_only",
            "vault write failures fall back to irreversible redaction",
        ));
    }
    if config.failure.log_write == dam_config::LogWriteFailureMode::WarnContinue {
        diagnostics.push(dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Warning,
            "log_warn_continue",
            "log write failures do not fail the protected path",
        ));
    }

    diagnostics
}

fn proxy_failure_mode_reduces_guarantees(mode: dam_config::ProxyFailureMode) -> bool {
    matches!(
        mode,
        dam_config::ProxyFailureMode::BypassOnError | dam_config::ProxyFailureMode::RedactOnly
    )
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

fn integration_state_dir() -> Result<PathBuf, String> {
    dam_daemon::state_paths()
        .map(|paths| paths.state_dir.join("integrations"))
        .map_err(|error| error.to_string())
}

fn integrations_check_exit_code(report: &IntegrationsCheckReport, specific_profile: bool) -> i32 {
    let has_modified_or_record_error = report.profiles.iter().any(|profile| {
        profile.status == dam_integrations::IntegrationApplyStatus::Modified
            || profile.record_error.is_some()
    });
    let has_needs_apply = report
        .profiles
        .iter()
        .any(|profile| profile.status == dam_integrations::IntegrationApplyStatus::NeedsApply);

    if has_modified_or_record_error || (specific_profile && has_needs_apply) {
        1
    } else {
        0
    }
}

fn render_integrations_check_report(report: &IntegrationsCheckReport) -> String {
    let mut output = String::new();
    output.push_str(&format!("proxy_url: {}\n", report.proxy_url));
    for profile in &report.profiles {
        output.push_str(&format!("profile: {}\n", profile.profile_id));
        output.push_str(&format!(
            "  state: {}\n",
            integration_apply_status_tag(profile.status)
        ));
        output.push_str(&format!("  target: {}\n", profile.target_path.display()));
        output.push_str(&format!(
            "  planned_action: {}\n",
            profile.planned_action.tag()
        ));
        output.push_str(&format!(
            "  rollback: {}\n",
            if profile.rollback_available {
                "available"
            } else {
                "not_available"
            }
        ));
        output.push_str(&format!(
            "  rollback_record: {}\n",
            profile.rollback_record_path.display()
        ));
        output.push_str(&format!("  message: {}\n", profile.message));
        if let Some(error) = &profile.record_error {
            output.push_str(&format!("  record_error: {error}\n"));
        }
    }
    output
}

fn render_bypass_status_report(report: &BypassStatusReport) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", report.state));
    output.push_str(&format!("message: {}\n", report.message));
    output.push_str(&format!(
        "reduced_guarantees: {}\n",
        report.reduced_guarantees
    ));
    output.push_str(&format!("proxy_enabled: {}\n", report.proxy_enabled));
    output.push_str(&format!(
        "proxy_default_failure_mode: {}\n",
        report.proxy_default_failure_mode
    ));
    output.push_str(&format!(
        "vault_write_failure_mode: {}\n",
        report.vault_write_failure_mode
    ));
    output.push_str(&format!(
        "log_write_failure_mode: {}\n",
        report.log_write_failure_mode
    ));
    for target in &report.proxy_targets {
        output.push_str(&format!("target: {}\n", target.name));
        output.push_str(&format!("  provider: {}\n", target.provider));
        output.push_str(&format!("  upstream: {}\n", target.upstream));
        output.push_str(&format!(
            "  configured_failure_mode: {}\n",
            target
                .configured_failure_mode
                .as_deref()
                .unwrap_or("default")
        ));
        output.push_str(&format!(
            "  effective_failure_mode: {}\n",
            target.effective_failure_mode
        ));
        output.push_str(&format!(
            "  reduced_guarantee: {}\n",
            target.reduced_guarantee
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

fn render_daemon_inspect_report(report: &DaemonInspectReport) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", report.state));
    output.push_str(&format!("message: {}\n", report.message));
    output.push_str(&format!("state_dir: {}\n", report.state_dir.display()));
    output.push_str(&format!("state_file: {}\n", report.state_file.display()));
    if let Some(process_running) = report.process_running {
        output.push_str(&format!(
            "process: {}\n",
            if process_running {
                "running"
            } else {
                "not_running"
            }
        ));
    }
    if let Some(state) = &report.daemon {
        output.push_str(&format!("pid: {}\n", state.pid));
        output.push_str(&format!("listen: {}\n", state.listen));
        output.push_str(&format!("proxy: {}\n", state.proxy_url));
        output.push_str(&format!("network_mode: {}\n", state.network_mode));
        output.push_str(&format!(
            "protection_enabled: {}\n",
            state.protection_enabled
        ));
        output.push_str(&format!(
            "transparent_routes: {}\n",
            state.transparent_routes.len()
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
        output.push_str(&format!("trust_store: {}\n", state.trust.platform_store));
        output.push_str(&format!(
            "local_ca_installed: {}\n",
            state.trust.local_ca_installed()
        ));
        output.push_str(&format!(
            "trusted_hosts: {}\n",
            state.trust.allowed_hosts.len()
        ));
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
        if let Some(config_path) = &state.config_path {
            output.push_str(&format!("config: {}\n", config_path.display()));
        }
        output.push_str(&format!("vault: {}\n", state.vault_path.display()));
        match &state.log_path {
            Some(path) => output.push_str(&format!("log: {}\n", path.display())),
            None => output.push_str("log: disabled\n"),
        }
        if let Some(path) = &state.consent_path {
            output.push_str(&format!("consent: {}\n", path.display()));
        }
        output.push_str(&format!("resolve_inbound: {}\n", state.resolve_inbound));
        output.push_str(&format!("started_at_unix: {}\n", state.started_at_unix));
    }
    output
}

fn render_trust_inspect_report(report: &TrustInspectReport) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", report.state));
    output.push_str(&format!("message: {}\n", report.message));
    output.push_str(&format!("source: {}\n", report.source));
    output.push_str(&format!("state_dir: {}\n", report.state_dir.display()));
    output.push_str(&format!("state_file: {}\n", report.state_file.display()));
    output.push_str(&format!("trust_mode: {}\n", report.trust.mode));
    output.push_str(&format!("trust_store: {}\n", report.trust.platform_store));
    output.push_str(&format!(
        "local_ca_installed: {}\n",
        report.trust.local_ca_installed()
    ));
    output.push_str(&format!(
        "trusted_hosts: {}\n",
        report.trust.allowed_hosts.len()
    ));
    match &report.local_ca_artifact {
        Some(artifact) => {
            output.push_str("local_ca_artifact: present\n");
            output.push_str(&format!(
                "local_ca_manifest: {}\n",
                artifact.paths.manifest_path.display()
            ));
            output.push_str(&format!(
                "local_ca_certificate: {}\n",
                artifact.paths.certificate_path.display()
            ));
            output.push_str(&format!(
                "local_ca_private_key: {}\n",
                artifact.paths.private_key_path.display()
            ));
        }
        None => output.push_str("local_ca_artifact: missing\n"),
    }
    output.push_str(&format!("trust_routes: {}\n", report.route_readiness.len()));
    for route in &report.route_readiness {
        output.push_str(&format!(
            "trust_route {}: {} - {}\n",
            route.route.target_name, route.readiness, route.message
        ));
    }
    for action in &report.actions {
        output.push_str(&format!(
            "action {}: {} admin={} local_trust={} user_consent={} rollback={}\n",
            trust_action_tag(action.action),
            trust_support_tag(action.support),
            action.requires_admin,
            action.changes_system_trust,
            action.requires_user_consent,
            action.rollback_required
        ));
    }
    output
}

fn render_network_inspect_report(report: &NetworkInspectReport) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", report.state));
    output.push_str(&format!("message: {}\n", report.message));
    output.push_str(&format!("state_dir: {}\n", report.state_dir.display()));
    output.push_str(&format!("support: {}\n", report.support));
    output.push_str(&format!(
        "system_proxy_installed: {}\n",
        report.system_proxy_installed
    ));
    output.push_str(&format!(
        "rollback_record: {}\n",
        report.rollback_path.display()
    ));
    output.push_str(&format!("pac_file: {}\n", report.pac_path.display()));
    output.push_str(&format!(
        "configured_hosts: {}\n",
        report.configured_hosts.len()
    ));
    for host in &report.configured_hosts {
        output.push_str(&format!("configured_host: {host}\n"));
    }
    output.push_str(&format!(
        "routing_routes: {}\n",
        report.route_readiness.len()
    ));
    for route in &report.route_readiness {
        output.push_str(&format!(
            "routing_route {}: {} - {}\n",
            route.route.target_name, route.readiness, route.message
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
    output.push_str(&format!(
        "active_profile: {}\n",
        report
            .active_profile
            .as_ref()
            .map(|profile| profile.profile_id.as_str())
            .unwrap_or("none")
    ));
    for step in &report.steps {
        output.push_str(&format!(
            "step {}: {}.{} - {}\n",
            step.kind.tag(),
            step.status.tag(),
            step.detail.tag(),
            step.message
        ));
        if let Some(command) = &step.command {
            output.push_str(&format!("  command: {}\n", command.join(" ")));
        }
        output.push_str(&format!(
            "  requires_confirmation: {}\n",
            step.requires_confirmation
        ));
        output.push_str(&format!("  changes_system: {}\n", step.changes_system));
    }
    output
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

fn integration_apply_status_tag(status: dam_integrations::IntegrationApplyStatus) -> &'static str {
    match status {
        dam_integrations::IntegrationApplyStatus::Applied => "applied",
        dam_integrations::IntegrationApplyStatus::NeedsApply => "needs_apply",
        dam_integrations::IntegrationApplyStatus::Modified => "modified",
    }
}

fn aggregate_health_state(components: &[dam_api::ComponentHealth]) -> dam_api::HealthState {
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

fn status_url(args: &StatusArgs, config: Option<&dam_config::DamConfig>) -> Result<String, String> {
    if let Some(proxy_url) = &args.proxy_url {
        return dam_diagnostics::proxy_health_url(
            &dam_config::DamConfig::default(),
            Some(proxy_url),
        );
    }

    let config =
        config.ok_or_else(|| "config is required when --proxy-url is omitted".to_string())?;
    dam_diagnostics::proxy_health_url(config, None)
}

fn dam_down_report(
    config: Option<&dam_config::DamConfig>,
    message: String,
) -> dam_api::ProxyReport {
    let target = config.and_then(|config| config.proxy.targets.first());
    dam_api::ProxyReport {
        operation_id: None,
        target: target.map(|target| target.name.clone()),
        upstream: target.map(|target| target.upstream.clone()),
        state: dam_api::ProxyState::DamDown,
        message: message.clone(),
        diagnostics: vec![dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Error,
            "dam_down",
            message,
        )],
    }
}

fn render_proxy_report(report: &dam_api::ProxyReport, config_warning: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str(&format!("state: {}\n", proxy_state_tag(report.state)));
    output.push_str(&format!("message: {}\n", report.message));
    if let Some(target) = &report.target {
        output.push_str(&format!("target: {target}\n"));
    }
    if let Some(upstream) = &report.upstream {
        output.push_str(&format!("upstream: {upstream}\n"));
    }
    if let Some(operation_id) = &report.operation_id {
        output.push_str(&format!("operation_id: {operation_id}\n"));
    }
    if let Some(config_warning) = config_warning {
        output.push_str(&format!("warning: {config_warning}\n"));
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

fn json<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_string_pretty(value) {
        Ok(json) => format!("{json}\n"),
        Err(error) => format!(
            "{{\"state\":\"unhealthy\",\"diagnostics\":[{{\"severity\":\"error\",\"code\":\"json_serialize_failed\",\"message\":\"{error}\"}}]}}\n"
        ),
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

fn health_state_tag(state: dam_api::HealthState) -> &'static str {
    match state {
        dam_api::HealthState::Healthy => "healthy",
        dam_api::HealthState::Degraded => "degraded",
        dam_api::HealthState::Unhealthy => "unhealthy",
        dam_api::HealthState::Unknown => "unknown",
    }
}

fn severity_tag(severity: dam_api::DiagnosticSeverity) -> &'static str {
    match severity {
        dam_api::DiagnosticSeverity::Info => "info",
        dam_api::DiagnosticSeverity::Warning => "warning",
        dam_api::DiagnosticSeverity::Error => "error",
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    if args.is_empty() {
        return Err("missing command".to_string());
    }

    match args[0].as_str() {
        "status" => parse_status_args(&args[1..]),
        "doctor" => parse_doctor_args(&args[1..]),
        "bypass" => parse_bypass_args(&args[1..]),
        "daemon" => parse_daemon_args(&args[1..]),
        "trust" => parse_trust_args(&args[1..]),
        "network" => parse_network_args(&args[1..]),
        "setup" => parse_setup_args(&args[1..]),
        "integrations" => parse_integrations_args(&args[1..]),
        "config" => parse_config_args(&args[1..]),
        "mcp" => parse_mcp_args(&args[1..]),
        "-h" | "--help" => {
            println!("{}", usage());
            std::process::exit(0);
        }
        command => Err(format!("unknown command: {command}")),
    }
}

fn parse_bypass_args(args: &[String]) -> Result<Command, String> {
    if args.first().map(String::as_str) != Some("status") {
        return Err("expected bypass status".to_string());
    }

    let mut parsed = BypassStatusArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                parsed.common.config.config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--json" => parsed.common.json = true,
            "-h" | "--help" => {
                println!("{}", usage_bypass_status());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown bypass status argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::Bypass(BypassArgs {
        command: BypassCommand::Status(parsed),
    }))
}

fn parse_daemon_args(args: &[String]) -> Result<Command, String> {
    if args.first().map(String::as_str) != Some("inspect") {
        return Err("expected daemon inspect".to_string());
    }

    let mut parsed = DaemonInspectArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--state-dir" => {
                i += 1;
                parsed.state_dir = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--state-dir requires a path".to_string())?,
                ));
            }
            "--json" => parsed.json = true,
            "-h" | "--help" => {
                println!("{}", usage_daemon_inspect());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown daemon inspect argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::Daemon(DaemonArgs {
        command: DaemonCommand::Inspect(parsed),
    }))
}

fn parse_trust_args(args: &[String]) -> Result<Command, String> {
    if args.first().map(String::as_str) != Some("inspect") {
        return Err("expected trust inspect".to_string());
    }

    let mut parsed = TrustInspectArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--state-dir" => {
                i += 1;
                parsed.state_dir = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--state-dir requires a path".to_string())?,
                ));
            }
            "--json" => parsed.json = true,
            "-h" | "--help" => {
                println!("{}", usage_trust_inspect());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown trust inspect argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::Trust(TrustArgs {
        command: TrustCommand::Inspect(parsed),
    }))
}

fn parse_network_args(args: &[String]) -> Result<Command, String> {
    if args.first().map(String::as_str) != Some("inspect") {
        return Err("expected network inspect".to_string());
    }

    let mut parsed = NetworkInspectArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                parsed.config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--state-dir" => {
                i += 1;
                parsed.state_dir = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--state-dir requires a path".to_string())?,
                ));
            }
            "--json" => parsed.json = true,
            "-h" | "--help" => {
                println!("{}", usage_network_inspect());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown network inspect argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::Network(NetworkArgs {
        command: NetworkCommand::Inspect(parsed),
    }))
}

fn parse_setup_args(args: &[String]) -> Result<Command, String> {
    if args.first().map(String::as_str) != Some("plan") {
        return Err("expected setup plan".to_string());
    }

    let mut parsed = SetupPlanArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                parsed.common.config.config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--state-dir" => {
                i += 1;
                parsed.state_dir = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--state-dir requires a path".to_string())?,
                ));
            }
            "--proxy-url" => {
                i += 1;
                parsed.proxy_url = Some(
                    args.get(i)
                        .ok_or_else(|| "--proxy-url requires a URL".to_string())?
                        .clone(),
                );
            }
            "--network-mode" => {
                i += 1;
                parsed.network_mode = args
                    .get(i)
                    .ok_or_else(|| "--network-mode requires a mode".to_string())?
                    .parse::<dam_net::CaptureMode>()?;
            }
            "--trust-mode" => {
                i += 1;
                parsed.trust_mode = args
                    .get(i)
                    .ok_or_else(|| "--trust-mode requires a mode".to_string())?
                    .parse::<dam_trust::TrustMode>()?;
            }
            "--json" => parsed.common.json = true,
            "-h" | "--help" => {
                println!("{}", usage_setup_plan());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown setup plan argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::Setup(SetupArgs {
        command: SetupCommand::Plan(parsed),
    }))
}

fn parse_integrations_args(args: &[String]) -> Result<Command, String> {
    if args.first().map(String::as_str) != Some("check") {
        return Err("expected integrations check".to_string());
    }

    let mut parsed = IntegrationsCheckArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--proxy-url" => {
                i += 1;
                parsed.proxy_url = Some(
                    args.get(i)
                        .ok_or_else(|| "--proxy-url requires a URL".to_string())?
                        .clone(),
                );
            }
            "--target-path" => {
                i += 1;
                parsed.target_path =
                    Some(PathBuf::from(args.get(i).ok_or_else(|| {
                        "--target-path requires a path".to_string()
                    })?));
            }
            "--json" => parsed.json = true,
            "-h" | "--help" => {
                println!("{}", usage_integrations_check());
                std::process::exit(0);
            }
            arg if parsed.profile_id.is_none() => {
                parsed.profile_id = Some(arg.to_string());
            }
            arg => return Err(format!("unexpected integrations check argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::Integrations(IntegrationsArgs {
        command: IntegrationsCommand::Check(parsed),
    }))
}

fn parse_mcp_args(args: &[String]) -> Result<Command, String> {
    if args.first().map(String::as_str) != Some("config") {
        return Err("expected mcp config".to_string());
    }

    let mut parsed = McpConfigArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                parsed.config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "-h" | "--help" => {
                println!("{}", usage_mcp_config());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown mcp config argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::McpConfig(parsed))
}

fn parse_status_args(args: &[String]) -> Result<Command, String> {
    let mut parsed = StatusArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                parsed.common.config.config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--proxy-url" => {
                i += 1;
                parsed.proxy_url = Some(
                    args.get(i)
                        .ok_or_else(|| "--proxy-url requires a URL".to_string())?
                        .clone(),
                );
            }
            "--json" => parsed.common.json = true,
            "-h" | "--help" => {
                println!("{}", usage_status());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown status argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::Status(parsed))
}

fn parse_doctor_args(args: &[String]) -> Result<Command, String> {
    let mut parsed = DoctorArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                parsed.common.config.config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--proxy-url" => {
                i += 1;
                parsed.proxy_url = Some(
                    args.get(i)
                        .ok_or_else(|| "--proxy-url requires a URL".to_string())?
                        .clone(),
                );
            }
            "--state-dir" => {
                i += 1;
                parsed.state_dir = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--state-dir requires a path".to_string())?,
                ));
            }
            "--json" => parsed.common.json = true,
            "-h" | "--help" => {
                println!("{}", usage_doctor());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown doctor argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::Doctor(parsed))
}

fn parse_config_args(args: &[String]) -> Result<Command, String> {
    if args.first().map(String::as_str) != Some("check") {
        return Err("expected config check".to_string());
    }

    let mut parsed = ConfigCheckArgs::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                parsed.common.config.config_path = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--json" => parsed.common.json = true,
            "-h" | "--help" => {
                println!("{}", usage_config_check());
                std::process::exit(0);
            }
            arg => return Err(format!("unknown config check argument: {arg}")),
        }
        i += 1;
    }

    Ok(Command::ConfigCheck(parsed))
}

fn usage() -> &'static str {
    "Usage: damctl <command>\n\nCommands:\n  status              Check the local DAM proxy health endpoint\n  doctor              Run local readiness checks for the protected UX\n  bypass status       Show reduced-protection/bypass failure modes\n  daemon inspect      Inspect local daemon state without changing it\n  trust inspect       Inspect local TLS trust readiness without changing local trust\n  network inspect     Inspect local network routing readiness without changing system routes\n  setup plan          Show the next read-only setup action for local protection\n  integrations check  Inspect integration profile apply state\n  config check        Validate local DAM config for the current implementation\n  mcp config          Print MCP server config for DAM"
}

fn usage_status() -> &'static str {
    "Usage: damctl status [--config dam.toml] [--proxy-url http://127.0.0.1:7828] [--json]"
}

fn usage_doctor() -> &'static str {
    "Usage: damctl doctor [--config dam.toml] [--proxy-url http://127.0.0.1:7828] [--state-dir PATH] [--json]"
}

fn usage_bypass_status() -> &'static str {
    "Usage: damctl bypass status [--config dam.toml] [--json]"
}

fn usage_config_check() -> &'static str {
    "Usage: damctl config check [--config dam.toml] [--json]"
}

fn usage_daemon_inspect() -> &'static str {
    "Usage: damctl daemon inspect [--state-dir PATH] [--json]"
}

fn usage_trust_inspect() -> &'static str {
    "Usage: damctl trust inspect [--state-dir PATH] [--json]"
}

fn usage_network_inspect() -> &'static str {
    "Usage: damctl network inspect [--config dam.toml] [--state-dir PATH] [--json]"
}

fn usage_setup_plan() -> &'static str {
    "Usage: damctl setup plan [--config dam.toml] [--state-dir PATH] [--proxy-url http://127.0.0.1:7828] [--network-mode explicit_proxy|system_proxy|tun] [--trust-mode disabled|local_ca] [--json]"
}

fn usage_integrations_check() -> &'static str {
    "Usage: damctl integrations check [profile] [--proxy-url http://127.0.0.1:7828] [--target-path PATH] [--json]"
}

fn usage_mcp_config() -> &'static str {
    "Usage: damctl mcp config [--config dam.toml]"
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
