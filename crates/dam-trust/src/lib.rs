use std::{
    env, fmt, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};

#[cfg(unix)]
use std::os::unix::{fs::OpenOptionsExt, fs::PermissionsExt};

const LOCAL_CA_DIR: &str = "trust/local-ca";
const LOCAL_CA_MANIFEST: &str = "manifest.json";
const LOCAL_CA_CERTIFICATE: &str = "ca.pem";
const LOCAL_CA_PRIVATE_KEY: &str = "ca-key.pem";
const LOCAL_CA_LABEL: &str = "DAM Local CA";
const LOCAL_CA_MANIFEST_VERSION: u32 = 1;
const MACOS_SECURITY: &str = "/usr/bin/security";
const MACOS_LOGIN_KEYCHAIN_DB: &str = "Library/Keychains/login.keychain-db";
const MACOS_LOGIN_KEYCHAIN_LEGACY: &str = "Library/Keychains/login.keychain";
const LINUX_SUDO: &str = "/usr/bin/sudo";
const LINUX_TRUST: &str = "/usr/bin/trust";

#[derive(Debug, thiserror::Error)]
pub enum TrustArtifactError {
    #[error("failed to create local CA directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("local CA artifact already exists at {0}")]
    AlreadyExists(PathBuf),

    #[error("failed to generate local CA material: {0}")]
    Generate(#[from] rcgen::Error),

    #[error("failed to serialize local CA manifest: {0}")]
    Serialize(serde_json::Error),

    #[error("failed to parse local CA manifest {path}: {source}")]
    ParseManifest {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("failed to read local CA manifest {path}: {source}")]
    ReadManifest {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write local CA artifact {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to read local CA artifact {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("local CA artifact is missing {path}")]
    MissingFile { path: PathBuf },

    #[error("failed to parse local CA certificate {path}: {source}")]
    ParseCertificate {
        path: PathBuf,
        source: pem::PemError,
    },

    #[error("refusing to delete installed local CA artifact {0}; remove local trust first")]
    Installed(String),

    #[error("failed to delete local CA artifact {path}: {source}")]
    DeleteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("local CA trust-store mutation is not implemented for {0}")]
    UnsupportedPlatform(PlatformTrustStore),

    #[error("failed to run local trust command {program}: {source}")]
    RunCommand {
        program: String,
        source: std::io::Error,
    },

    #[error("local trust command failed ({status}): {program} {args}; {stderr}")]
    CommandFailed {
        program: String,
        args: String,
        status: String,
        stderr: String,
    },

    #[error("system clock is before unix epoch")]
    Clock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrustMode {
    #[default]
    Disabled,
    LocalCa,
}

impl TrustMode {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::LocalCa => "local_ca",
        }
    }
}

impl fmt::Display for TrustMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.tag())
    }
}

impl FromStr for TrustMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().replace('-', "_").as_str() {
            "disabled" | "off" | "none" => Ok(Self::Disabled),
            "local_ca" | "ca" | "trust" => Ok(Self::LocalCa),
            _ => Err(format!(
                "unsupported trust mode: {value}; expected disabled or local_ca"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustSupport {
    Implemented,
    Planned,
}

impl TrustSupport {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::Planned => "planned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformTrustStore {
    MacosKeychain,
    WindowsRootStore,
    LinuxNssOrSystemStore,
    Unknown,
}

impl PlatformTrustStore {
    pub fn tag(self) -> &'static str {
        match self {
            Self::MacosKeychain => "macos_keychain",
            Self::WindowsRootStore => "windows_root_store",
            Self::LinuxNssOrSystemStore => "linux_nss_or_system_store",
            Self::Unknown => "unknown",
        }
    }

    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::MacosKeychain
        }
        #[cfg(target_os = "windows")]
        {
            Self::WindowsRootStore
        }
        #[cfg(target_os = "linux")]
        {
            Self::LinuxNssOrSystemStore
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Self::Unknown
        }
    }
}

impl fmt::Display for PlatformTrustStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.tag())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustAction {
    Inspect,
    InstallLocalCa,
    RemoveLocalCa,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustActionPlan {
    pub action: TrustAction,
    pub support: TrustSupport,
    pub mode: TrustMode,
    pub platform_store: PlatformTrustStore,
    pub requires_admin: bool,
    pub changes_system_trust: bool,
    pub requires_user_consent: bool,
    pub rollback_required: bool,
    pub message: String,
}

impl TrustActionPlan {
    pub fn for_action(action: TrustAction, platform_store: PlatformTrustStore) -> Self {
        match action {
            TrustAction::Inspect => Self {
                action,
                support: TrustSupport::Implemented,
                mode: TrustMode::Disabled,
                platform_store,
                requires_admin: false,
                changes_system_trust: false,
                requires_user_consent: false,
                rollback_required: false,
                message: "trust inspection is available without changing local trust".to_string(),
            },
            TrustAction::InstallLocalCa => Self {
                action,
                support: local_ca_platform_support(platform_store),
                mode: TrustMode::LocalCa,
                platform_store,
                requires_admin: local_ca_requires_admin(platform_store),
                changes_system_trust: true,
                requires_user_consent: true,
                rollback_required: true,
                message: match local_ca_platform_support(platform_store) {
                    TrustSupport::Implemented => {
                        "local CA installation is implemented and requires explicit user approval"
                    }
                    TrustSupport::Planned => "local CA installation is planned for this platform",
                }
                .to_string(),
            },
            TrustAction::RemoveLocalCa => Self {
                action,
                support: local_ca_platform_support(platform_store),
                mode: TrustMode::LocalCa,
                platform_store,
                requires_admin: local_ca_requires_admin(platform_store),
                changes_system_trust: true,
                requires_user_consent: true,
                rollback_required: false,
                message: match local_ca_platform_support(platform_store) {
                    TrustSupport::Implemented => {
                        "local CA removal is implemented and requires explicit user approval"
                    }
                    TrustSupport::Planned => "local CA removal is planned for this platform",
                }
                .to_string(),
            },
        }
    }
}

fn local_ca_requires_admin(platform_store: PlatformTrustStore) -> bool {
    match platform_store {
        PlatformTrustStore::MacosKeychain => false,
        PlatformTrustStore::WindowsRootStore
        | PlatformTrustStore::LinuxNssOrSystemStore
        | PlatformTrustStore::Unknown => true,
    }
}

fn local_ca_platform_support(platform_store: PlatformTrustStore) -> TrustSupport {
    match platform_store {
        PlatformTrustStore::MacosKeychain => TrustSupport::Implemented,
        PlatformTrustStore::LinuxNssOrSystemStore => {
            if cfg!(target_os = "linux") {
                TrustSupport::Implemented
            } else {
                TrustSupport::Planned
            }
        }
        PlatformTrustStore::WindowsRootStore | PlatformTrustStore::Unknown => TrustSupport::Planned,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalCaRecord {
    pub id: String,
    pub label: String,
    pub fingerprint_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_sha1: Option<String>,
    pub created_at_unix: u64,
    pub installed_at_unix: Option<u64>,
}

impl LocalCaRecord {
    pub fn installed(&self) -> bool {
        self.installed_at_unix.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalCaPaths {
    pub directory: PathBuf,
    pub manifest_path: PathBuf,
    pub certificate_path: PathBuf,
    pub private_key_path: PathBuf,
}

impl LocalCaPaths {
    pub fn for_state_dir(state_dir: impl AsRef<Path>) -> Self {
        let directory = state_dir.as_ref().join(LOCAL_CA_DIR);
        Self {
            manifest_path: directory.join(LOCAL_CA_MANIFEST),
            certificate_path: directory.join(LOCAL_CA_CERTIFICATE),
            private_key_path: directory.join(LOCAL_CA_PRIVATE_KEY),
            directory,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalCaArtifact {
    pub record: LocalCaRecord,
    pub paths: LocalCaPaths,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalCaIssuedCertificate {
    pub host: String,
    pub certificate_der: Vec<u8>,
    pub certificate_pem: String,
    pub private_key_der: Vec<u8>,
    pub private_key_pem: String,
    pub ca_certificate_der: Vec<u8>,
    pub ca_certificate_pem: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemTrustCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalCaSystemTrustPlan {
    pub action: TrustAction,
    pub support: TrustSupport,
    pub platform_store: PlatformTrustStore,
    pub artifact: Option<LocalCaRecord>,
    pub certificate_path: PathBuf,
    pub system_store: String,
    pub commands: Vec<SystemTrustCommand>,
    pub will_generate_artifact: bool,
    pub requires_admin: bool,
    pub changes_system_trust: bool,
    pub requires_user_consent: bool,
    pub can_execute: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalCaSystemTrustResultState {
    Preview,
    Installed,
    AlreadyInstalled,
    Removed,
    NotInstalled,
}

impl LocalCaSystemTrustResultState {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Installed => "installed",
            Self::AlreadyInstalled => "already_installed",
            Self::Removed => "removed",
            Self::NotInstalled => "not_installed",
        }
    }
}

impl fmt::Display for LocalCaSystemTrustResultState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.tag())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalCaSystemTrustResult {
    pub state: LocalCaSystemTrustResultState,
    pub plan: LocalCaSystemTrustPlan,
    pub artifact: Option<LocalCaArtifact>,
    pub generated_artifact: bool,
    pub system_trust_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LocalCaManifest {
    version: u32,
    record: LocalCaRecord,
    certificate_file: String,
    private_key_file: String,
}

impl LocalCaManifest {
    fn new(record: LocalCaRecord) -> Self {
        Self {
            version: LOCAL_CA_MANIFEST_VERSION,
            record,
            certificate_file: LOCAL_CA_CERTIFICATE.to_string(),
            private_key_file: LOCAL_CA_PRIVATE_KEY.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustState {
    pub mode: TrustMode,
    pub platform_store: PlatformTrustStore,
    pub local_ca: Option<LocalCaRecord>,
    pub allowed_hosts: Vec<String>,
}

impl Default for TrustState {
    fn default() -> Self {
        Self {
            mode: TrustMode::Disabled,
            platform_store: PlatformTrustStore::current(),
            local_ca: None,
            allowed_hosts: default_allowed_hosts(),
        }
    }
}

impl TrustState {
    pub fn local_ca_installed(&self) -> bool {
        self.local_ca
            .as_ref()
            .map(LocalCaRecord::installed)
            .unwrap_or(false)
    }

    pub fn host_allowed(&self, host: &str) -> bool {
        let normalized = normalize_host(host);
        self.allowed_hosts
            .iter()
            .any(|allowed| normalize_host(allowed) == normalized)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteTrustReadiness {
    pub route: dam_net::TrafficRoute,
    pub protocol: dam_net::TrafficProtocol,
    pub readiness: TlsInterceptionReadiness,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsInterceptionReadiness {
    NotConfiguredTraffic,
    NotRequired,
    Disabled,
    HostNotAllowed,
    NeedsUserConsent,
    NeedsLocalCa,
    Ready,
}

impl TlsInterceptionReadiness {
    pub fn tag(self) -> &'static str {
        match self {
            Self::NotConfiguredTraffic => "not_configured_traffic",
            Self::NotRequired => "not_required",
            Self::Disabled => "disabled",
            Self::HostNotAllowed => "host_not_allowed",
            Self::NeedsUserConsent => "needs_user_consent",
            Self::NeedsLocalCa => "needs_local_ca",
            Self::Ready => "ready",
        }
    }
}

impl fmt::Display for TlsInterceptionReadiness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.tag())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustReadinessReport {
    pub readiness: TlsInterceptionReadiness,
    pub message: String,
}

pub fn readiness_for_route(
    decision: &dam_net::TransparentRouteDecision,
    trust: &TrustState,
    user_consented: bool,
) -> TrustReadinessReport {
    match decision {
        dam_net::TransparentRouteDecision::Unmatched { .. } => TrustReadinessReport {
            readiness: TlsInterceptionReadiness::NotConfiguredTraffic,
            message: "traffic is outside the configured trust scope".to_string(),
        },
        dam_net::TransparentRouteDecision::Matched {
            route,
            protectable_without_tls,
            ..
        } if *protectable_without_tls => TrustReadinessReport {
            readiness: TlsInterceptionReadiness::NotRequired,
            message: format!(
                "{} traffic is visible without TLS interception",
                route.target_name
            ),
        },
        dam_net::TransparentRouteDecision::Matched { route, .. } => {
            if trust.mode == TrustMode::Disabled {
                return TrustReadinessReport {
                    readiness: TlsInterceptionReadiness::Disabled,
                    message: "TLS interception is disabled".to_string(),
                };
            }
            if !trust.host_allowed(&route.host) {
                return TrustReadinessReport {
                    readiness: TlsInterceptionReadiness::HostNotAllowed,
                    message: format!("{} is not in the trusted host scope", route.host),
                };
            }
            if !user_consented {
                return TrustReadinessReport {
                    readiness: TlsInterceptionReadiness::NeedsUserConsent,
                    message: "TLS interception requires explicit user approval".to_string(),
                };
            }
            if !trust.local_ca_installed() {
                return TrustReadinessReport {
                    readiness: TlsInterceptionReadiness::NeedsLocalCa,
                    message: "TLS interception requires a trusted local DAM CA".to_string(),
                };
            }
            TrustReadinessReport {
                readiness: TlsInterceptionReadiness::Ready,
                message: format!(
                    "{} traffic is ready for TLS interception",
                    route.target_name
                ),
            }
        }
    }
}

pub fn readiness_for_default_routes(
    trust: &TrustState,
    user_consented: bool,
) -> Vec<RouteTrustReadiness> {
    readiness_for_routes(&dam_net::default_traffic_routes(), trust, user_consented)
}

pub fn readiness_for_routes(
    routes: &[dam_net::TrafficRoute],
    trust: &TrustState,
    user_consented: bool,
) -> Vec<RouteTrustReadiness> {
    routes
        .iter()
        .cloned()
        .map(|route| {
            let decision = dam_net::decide_transparent_route_with_routes(
                &dam_net::TrafficObservation::new(
                    route.host.clone(),
                    dam_net::TrafficProtocol::Https,
                ),
                routes,
            );
            let report = readiness_for_route(&decision, trust, user_consented);
            RouteTrustReadiness {
                route,
                protocol: dam_net::TrafficProtocol::Https,
                readiness: report.readiness,
                message: report.message,
            }
        })
        .collect()
}

pub fn trust_state_for_state_dir(
    mode: TrustMode,
    state_dir: impl AsRef<Path>,
) -> Result<TrustState, TrustArtifactError> {
    let mut state = TrustState {
        mode,
        ..TrustState::default()
    };
    if let Some(artifact) = inspect_local_ca_artifact(state_dir)? {
        state.local_ca = Some(artifact.record);
    }
    Ok(state)
}

pub fn local_ca_install_plan(
    state_dir: impl AsRef<Path>,
) -> Result<LocalCaSystemTrustPlan, TrustArtifactError> {
    local_ca_install_plan_for_platform(state_dir, PlatformTrustStore::current())
}

pub fn local_ca_remove_plan(
    state_dir: impl AsRef<Path>,
) -> Result<LocalCaSystemTrustPlan, TrustArtifactError> {
    local_ca_remove_plan_for_platform(state_dir, PlatformTrustStore::current())
}

pub fn local_ca_install_plan_for_platform(
    state_dir: impl AsRef<Path>,
    platform_store: PlatformTrustStore,
) -> Result<LocalCaSystemTrustPlan, TrustArtifactError> {
    let state_dir = state_dir.as_ref();
    let paths = LocalCaPaths::for_state_dir(state_dir);
    let artifact = inspect_local_ca_artifact(state_dir)?;
    let support = local_ca_platform_support(platform_store);
    let certificate_path = artifact
        .as_ref()
        .map(|artifact| artifact.paths.certificate_path.clone())
        .unwrap_or_else(|| paths.certificate_path.clone());
    let already_installed = artifact
        .as_ref()
        .map(|artifact| artifact.record.installed())
        .unwrap_or(false);
    let will_generate_artifact = artifact.is_none();
    let can_execute = support == TrustSupport::Implemented && !already_installed;
    let commands = if support == TrustSupport::Implemented && !already_installed {
        vec![install_command_for_platform(
            platform_store,
            &certificate_path,
        )]
    } else {
        Vec::new()
    };
    let message = match (support, already_installed, will_generate_artifact) {
        (TrustSupport::Planned, _, _) => {
            format!("local CA installation is not implemented for {platform_store}")
        }
        (TrustSupport::Implemented, true, _) => "local CA is already marked installed".to_string(),
        (TrustSupport::Implemented, false, true) => {
            "will generate a DAM local CA artifact, then install it in local platform trust"
                .to_string()
        }
        (TrustSupport::Implemented, false, false) => {
            "will install the DAM local CA certificate in local platform trust".to_string()
        }
    };

    Ok(LocalCaSystemTrustPlan {
        action: TrustAction::InstallLocalCa,
        support,
        platform_store,
        artifact: artifact.map(|artifact| artifact.record),
        certificate_path,
        system_store: system_store_name(platform_store),
        commands,
        will_generate_artifact,
        requires_admin: local_ca_requires_admin(platform_store),
        changes_system_trust: true,
        requires_user_consent: true,
        can_execute,
        message,
    })
}

pub fn local_ca_remove_plan_for_platform(
    state_dir: impl AsRef<Path>,
    platform_store: PlatformTrustStore,
) -> Result<LocalCaSystemTrustPlan, TrustArtifactError> {
    let state_dir = state_dir.as_ref();
    let paths = LocalCaPaths::for_state_dir(state_dir);
    let artifact = inspect_local_ca_artifact(state_dir)?;
    let support = local_ca_platform_support(platform_store);
    let installed = artifact
        .as_ref()
        .map(|artifact| artifact.record.installed())
        .unwrap_or(false);
    let commands = if support == TrustSupport::Implemented && installed {
        let artifact = artifact.as_ref().expect("installed artifact exists");
        vec![remove_command_for_platform(platform_store, artifact)?]
    } else {
        Vec::new()
    };
    let message = match (support, installed) {
        (TrustSupport::Planned, _) => {
            format!("local CA removal is not implemented for {platform_store}")
        }
        (TrustSupport::Implemented, true) => {
            "will remove the DAM local CA certificate from local platform trust".to_string()
        }
        (TrustSupport::Implemented, false) => "no installed DAM local CA is recorded".to_string(),
    };

    Ok(LocalCaSystemTrustPlan {
        action: TrustAction::RemoveLocalCa,
        support,
        platform_store,
        artifact: artifact.as_ref().map(|artifact| artifact.record.clone()),
        certificate_path: artifact
            .as_ref()
            .map(|artifact| artifact.paths.certificate_path.clone())
            .unwrap_or(paths.certificate_path),
        system_store: system_store_name(platform_store),
        commands,
        will_generate_artifact: false,
        requires_admin: local_ca_requires_admin(platform_store),
        changes_system_trust: true,
        requires_user_consent: true,
        can_execute: support == TrustSupport::Implemented && installed,
        message,
    })
}

pub fn preview_local_ca_install(
    state_dir: impl AsRef<Path>,
) -> Result<LocalCaSystemTrustResult, TrustArtifactError> {
    let plan = local_ca_install_plan(&state_dir)?;
    let artifact = inspect_local_ca_artifact(state_dir)?;
    Ok(LocalCaSystemTrustResult {
        state: LocalCaSystemTrustResultState::Preview,
        plan,
        artifact,
        generated_artifact: false,
        system_trust_changed: false,
    })
}

pub fn preview_local_ca_remove(
    state_dir: impl AsRef<Path>,
) -> Result<LocalCaSystemTrustResult, TrustArtifactError> {
    let plan = local_ca_remove_plan(&state_dir)?;
    let artifact = inspect_local_ca_artifact(state_dir)?;
    Ok(LocalCaSystemTrustResult {
        state: LocalCaSystemTrustResultState::Preview,
        plan,
        artifact,
        generated_artifact: false,
        system_trust_changed: false,
    })
}

pub fn install_local_ca_system_trust(
    state_dir: impl AsRef<Path>,
) -> Result<LocalCaSystemTrustResult, TrustArtifactError> {
    let plan = local_ca_install_plan(&state_dir)?;
    if plan.support != TrustSupport::Implemented {
        return Err(TrustArtifactError::UnsupportedPlatform(plan.platform_store));
    }

    let (artifact, generated_artifact) = match inspect_local_ca_artifact(&state_dir)? {
        Some(artifact) => (artifact, false),
        None => (generate_local_ca_artifact(&state_dir)?, true),
    };
    if artifact.record.installed() {
        return Ok(LocalCaSystemTrustResult {
            state: LocalCaSystemTrustResultState::AlreadyInstalled,
            plan: local_ca_install_plan(&state_dir)?,
            artifact: Some(artifact),
            generated_artifact,
            system_trust_changed: false,
        });
    }

    let command =
        install_command_for_platform(plan.platform_store, &artifact.paths.certificate_path);
    run_system_trust_command(&command)?;
    let installed = mark_local_ca_installed_at(&state_dir, unix_timestamp()?)?;

    Ok(LocalCaSystemTrustResult {
        state: LocalCaSystemTrustResultState::Installed,
        plan: local_ca_install_plan(&state_dir)?,
        artifact: Some(installed),
        generated_artifact,
        system_trust_changed: true,
    })
}

pub fn remove_local_ca_system_trust(
    state_dir: impl AsRef<Path>,
) -> Result<LocalCaSystemTrustResult, TrustArtifactError> {
    let plan = local_ca_remove_plan(&state_dir)?;
    if plan.support != TrustSupport::Implemented {
        return Err(TrustArtifactError::UnsupportedPlatform(plan.platform_store));
    }

    let Some(artifact) = inspect_local_ca_artifact(&state_dir)? else {
        return Ok(LocalCaSystemTrustResult {
            state: LocalCaSystemTrustResultState::NotInstalled,
            plan,
            artifact: None,
            generated_artifact: false,
            system_trust_changed: false,
        });
    };
    if !artifact.record.installed() {
        return Ok(LocalCaSystemTrustResult {
            state: LocalCaSystemTrustResultState::NotInstalled,
            plan,
            artifact: Some(artifact),
            generated_artifact: false,
            system_trust_changed: false,
        });
    }

    let command = remove_command_for_platform(plan.platform_store, &artifact)?;
    run_system_trust_command(&command)?;
    let updated = mark_local_ca_uninstalled(&state_dir)?;

    Ok(LocalCaSystemTrustResult {
        state: LocalCaSystemTrustResultState::Removed,
        plan: local_ca_remove_plan(&state_dir)?,
        artifact: Some(updated),
        generated_artifact: false,
        system_trust_changed: true,
    })
}

pub fn generate_local_ca_artifact(
    state_dir: impl AsRef<Path>,
) -> Result<LocalCaArtifact, TrustArtifactError> {
    generate_local_ca_artifact_at(state_dir, unix_timestamp()?)
}

pub fn generate_local_ca_artifact_at(
    state_dir: impl AsRef<Path>,
    created_at_unix: u64,
) -> Result<LocalCaArtifact, TrustArtifactError> {
    let paths = LocalCaPaths::for_state_dir(state_dir);
    if paths.manifest_path.exists()
        || paths.certificate_path.exists()
        || paths.private_key_path.exists()
    {
        return Err(TrustArtifactError::AlreadyExists(paths.directory));
    }
    fs::create_dir_all(&paths.directory).map_err(|source| TrustArtifactError::CreateDir {
        path: paths.directory.clone(),
        source,
    })?;
    set_dir_private(&paths.directory)?;

    let (certificate_pem, private_key_pem) = generate_ca_material()?;
    let certificate_der =
        certificate_der_from_pem(certificate_pem.as_bytes(), Path::new(LOCAL_CA_CERTIFICATE))?;
    let record = LocalCaRecord {
        id: format!("dam-local-ca-{}", uuid::Uuid::new_v4().simple()),
        label: LOCAL_CA_LABEL.to_string(),
        fingerprint_sha256: sha256_hex(&certificate_der),
        fingerprint_sha1: Some(sha1_hex(&certificate_der)),
        created_at_unix,
        installed_at_unix: None,
    };
    let manifest = LocalCaManifest::new(record.clone());
    let manifest_json =
        serde_json::to_vec_pretty(&manifest).map_err(TrustArtifactError::Serialize)?;

    write_atomic(&paths.private_key_path, private_key_pem.as_bytes(), 0o600)?;
    write_atomic(&paths.certificate_path, certificate_pem.as_bytes(), 0o644)?;
    write_atomic(&paths.manifest_path, &manifest_json, 0o600)?;

    Ok(LocalCaArtifact { record, paths })
}

pub fn inspect_local_ca_artifact(
    state_dir: impl AsRef<Path>,
) -> Result<Option<LocalCaArtifact>, TrustArtifactError> {
    let paths = LocalCaPaths::for_state_dir(state_dir);
    if !paths.manifest_path.exists() {
        return Ok(None);
    }
    let bytes =
        fs::read(&paths.manifest_path).map_err(|source| TrustArtifactError::ReadManifest {
            path: paths.manifest_path.clone(),
            source,
        })?;
    let manifest: LocalCaManifest =
        serde_json::from_slice(&bytes).map_err(|source| TrustArtifactError::ParseManifest {
            path: paths.manifest_path.clone(),
            source,
        })?;

    let certificate_path = paths.directory.join(&manifest.certificate_file);
    let private_key_path = paths.directory.join(&manifest.private_key_file);
    if !certificate_path.exists() {
        return Err(TrustArtifactError::MissingFile {
            path: certificate_path,
        });
    }
    if !private_key_path.exists() {
        return Err(TrustArtifactError::MissingFile {
            path: private_key_path,
        });
    }

    Ok(Some(LocalCaArtifact {
        record: manifest.record,
        paths: LocalCaPaths {
            certificate_path,
            private_key_path,
            ..paths
        },
    }))
}

pub fn delete_local_ca_artifact(state_dir: impl AsRef<Path>) -> Result<bool, TrustArtifactError> {
    let Some(artifact) = inspect_local_ca_artifact(state_dir)? else {
        return Ok(false);
    };
    if artifact.record.installed() {
        return Err(TrustArtifactError::Installed(artifact.record.id));
    }

    for path in [
        &artifact.paths.private_key_path,
        &artifact.paths.certificate_path,
        &artifact.paths.manifest_path,
    ] {
        if path.exists() {
            fs::remove_file(path).map_err(|source| TrustArtifactError::DeleteFile {
                path: path.clone(),
                source,
            })?;
        }
    }
    let _ = fs::remove_dir(&artifact.paths.directory);
    Ok(true)
}

pub fn issue_local_ca_leaf_certificate(
    state_dir: impl AsRef<Path>,
    host: &str,
) -> Result<LocalCaIssuedCertificate, TrustArtifactError> {
    let state_dir = state_dir.as_ref();
    let host = normalize_host(host);
    let artifact = inspect_local_ca_artifact(state_dir)?;
    let Some(artifact) = artifact else {
        return Err(TrustArtifactError::MissingFile {
            path: LocalCaPaths::for_state_dir(state_dir).manifest_path,
        });
    };
    issue_local_ca_leaf_certificate_from_artifact(&artifact, &host)
}

fn mark_local_ca_installed_at(
    state_dir: impl AsRef<Path>,
    installed_at_unix: u64,
) -> Result<LocalCaArtifact, TrustArtifactError> {
    update_local_ca_record(state_dir, |record| {
        record.installed_at_unix = Some(installed_at_unix);
    })
}

fn mark_local_ca_uninstalled(
    state_dir: impl AsRef<Path>,
) -> Result<LocalCaArtifact, TrustArtifactError> {
    update_local_ca_record(state_dir, |record| {
        record.installed_at_unix = None;
    })
}

fn issue_local_ca_leaf_certificate_from_artifact(
    artifact: &LocalCaArtifact,
    host: &str,
) -> Result<LocalCaIssuedCertificate, TrustArtifactError> {
    let ca_certificate_pem =
        fs::read_to_string(&artifact.paths.certificate_path).map_err(|source| {
            TrustArtifactError::ReadFile {
                path: artifact.paths.certificate_path.clone(),
                source,
            }
        })?;
    let ca_private_key_pem =
        fs::read_to_string(&artifact.paths.private_key_path).map_err(|source| {
            TrustArtifactError::ReadFile {
                path: artifact.paths.private_key_path.clone(),
                source,
            }
        })?;
    let ca_certificate_der = certificate_der_from_pem(
        ca_certificate_pem.as_bytes(),
        &artifact.paths.certificate_path,
    )?;
    let ca_key_pair = rcgen::KeyPair::from_pem(&ca_private_key_pem)?;
    let ca_certificate = local_ca_params()?.self_signed(&ca_key_pair)?;

    let mut params = rcgen::CertificateParams::new(vec![host.to_string()])?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, host);
    params.is_ca = rcgen::IsCa::NoCa;
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let leaf_key_pair = rcgen::KeyPair::generate()?;
    let leaf_certificate = params.signed_by(&leaf_key_pair, &ca_certificate, &ca_key_pair)?;

    Ok(LocalCaIssuedCertificate {
        host: host.to_string(),
        certificate_der: leaf_certificate.der().to_vec(),
        certificate_pem: leaf_certificate.pem(),
        private_key_der: leaf_key_pair.serialize_der(),
        private_key_pem: leaf_key_pair.serialize_pem(),
        ca_certificate_der,
        ca_certificate_pem,
    })
}

fn update_local_ca_record(
    state_dir: impl AsRef<Path>,
    update: impl FnOnce(&mut LocalCaRecord),
) -> Result<LocalCaArtifact, TrustArtifactError> {
    let state_dir = state_dir.as_ref();
    let paths = LocalCaPaths::for_state_dir(state_dir);
    let bytes =
        fs::read(&paths.manifest_path).map_err(|source| TrustArtifactError::ReadManifest {
            path: paths.manifest_path.clone(),
            source,
        })?;
    let mut manifest: LocalCaManifest =
        serde_json::from_slice(&bytes).map_err(|source| TrustArtifactError::ParseManifest {
            path: paths.manifest_path.clone(),
            source,
        })?;
    update(&mut manifest.record);
    let manifest_json =
        serde_json::to_vec_pretty(&manifest).map_err(TrustArtifactError::Serialize)?;
    write_atomic(&paths.manifest_path, &manifest_json, 0o600)?;
    inspect_local_ca_artifact(state_dir)?.ok_or(TrustArtifactError::MissingFile {
        path: paths.manifest_path,
    })
}

fn macos_install_command(certificate_path: &Path) -> SystemTrustCommand {
    SystemTrustCommand {
        program: MACOS_SECURITY.to_string(),
        args: vec![
            "add-trusted-cert".to_string(),
            "-r".to_string(),
            "trustRoot".to_string(),
            "-k".to_string(),
            macos_user_keychain_path().display().to_string(),
            certificate_path.display().to_string(),
        ],
    }
}

fn linux_install_command(certificate_path: &Path) -> SystemTrustCommand {
    SystemTrustCommand {
        program: LINUX_SUDO.to_string(),
        args: vec![
            LINUX_TRUST.to_string(),
            "anchor".to_string(),
            "--store".to_string(),
            certificate_path.display().to_string(),
        ],
    }
}

fn macos_remove_command(
    artifact: &LocalCaArtifact,
) -> Result<SystemTrustCommand, TrustArtifactError> {
    let fingerprint_sha1 = artifact
        .record
        .fingerprint_sha1
        .clone()
        .map(Ok)
        .unwrap_or_else(|| certificate_sha1_hex(&artifact.paths.certificate_path))?;
    Ok(SystemTrustCommand {
        program: MACOS_SECURITY.to_string(),
        args: vec![
            "delete-certificate".to_string(),
            "-Z".to_string(),
            fingerprint_sha1,
            macos_user_keychain_path().display().to_string(),
        ],
    })
}

fn linux_remove_command(
    artifact: &LocalCaArtifact,
) -> Result<SystemTrustCommand, TrustArtifactError> {
    Ok(SystemTrustCommand {
        program: LINUX_SUDO.to_string(),
        args: vec![
            LINUX_TRUST.to_string(),
            "anchor".to_string(),
            "--remove".to_string(),
            artifact.paths.certificate_path.display().to_string(),
        ],
    })
}

fn install_command_for_platform(
    platform_store: PlatformTrustStore,
    certificate_path: &Path,
) -> SystemTrustCommand {
    match platform_store {
        PlatformTrustStore::MacosKeychain => macos_install_command(certificate_path),
        PlatformTrustStore::LinuxNssOrSystemStore => linux_install_command(certificate_path),
        PlatformTrustStore::WindowsRootStore | PlatformTrustStore::Unknown => {
            unreachable!("unsupported platform store should not request install commands")
        }
    }
}

fn remove_command_for_platform(
    platform_store: PlatformTrustStore,
    artifact: &LocalCaArtifact,
) -> Result<SystemTrustCommand, TrustArtifactError> {
    match platform_store {
        PlatformTrustStore::MacosKeychain => macos_remove_command(artifact),
        PlatformTrustStore::LinuxNssOrSystemStore => linux_remove_command(artifact),
        PlatformTrustStore::WindowsRootStore | PlatformTrustStore::Unknown => {
            unreachable!("unsupported platform store should not request remove commands")
        }
    }
}

fn system_store_name(platform_store: PlatformTrustStore) -> String {
    match platform_store {
        PlatformTrustStore::MacosKeychain => macos_user_keychain_path().display().to_string(),
        PlatformTrustStore::WindowsRootStore => "windows_root_store".to_string(),
        PlatformTrustStore::LinuxNssOrSystemStore => "linux_nss_or_system_store".to_string(),
        PlatformTrustStore::Unknown => "unknown".to_string(),
    }
}

fn macos_user_keychain_path() -> PathBuf {
    let Some(home) = env::var_os("HOME") else {
        return PathBuf::from("login.keychain-db");
    };
    let home = PathBuf::from(home);
    let db = home.join(MACOS_LOGIN_KEYCHAIN_DB);
    if db.exists() {
        db
    } else {
        home.join(MACOS_LOGIN_KEYCHAIN_LEGACY)
    }
}

fn run_system_trust_command(command: &SystemTrustCommand) -> Result<(), TrustArtifactError> {
    match PlatformTrustStore::current() {
        PlatformTrustStore::MacosKeychain | PlatformTrustStore::LinuxNssOrSystemStore => {}
        platform_store => return Err(TrustArtifactError::UnsupportedPlatform(platform_store)),
    }

    if system_trust_command_inherits_stdio(command) {
        let status = Command::new(&command.program)
            .args(&command.args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|source| TrustArtifactError::RunCommand {
                program: command.program.clone(),
                source,
            })?;
        if status.success() {
            return Ok(());
        }

        return Err(TrustArtifactError::CommandFailed {
            program: command.program.clone(),
            args: command.args.join(" "),
            status: status.to_string(),
            stderr: "see terminal stderr".to_string(),
        });
    }

    let output = Command::new(&command.program)
        .args(&command.args)
        .output()
        .map_err(|source| TrustArtifactError::RunCommand {
            program: command.program.clone(),
            source,
        })?;
    if output.status.success() {
        return Ok(());
    }

    Err(TrustArtifactError::CommandFailed {
        program: command.program.clone(),
        args: command.args.join(" "),
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

fn system_trust_command_inherits_stdio(command: &SystemTrustCommand) -> bool {
    command.program == LINUX_SUDO
}

pub fn default_allowed_hosts() -> Vec<String> {
    dam_net::default_traffic_hosts()
}

fn generate_ca_material() -> Result<(String, String), TrustArtifactError> {
    let params = local_ca_params()?;
    let key_pair = rcgen::KeyPair::generate()?;
    let certificate = params.self_signed(&key_pair)?;
    Ok((certificate.pem(), key_pair.serialize_pem()))
}

fn local_ca_params() -> Result<rcgen::CertificateParams, TrustArtifactError> {
    let mut params = rcgen::CertificateParams::new(vec![LOCAL_CA_LABEL.to_string()])?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, LOCAL_CA_LABEL);
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    Ok(params)
}

fn certificate_sha1_hex(path: &Path) -> Result<String, TrustArtifactError> {
    let bytes = fs::read(path).map_err(|source| TrustArtifactError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let der = certificate_der_from_pem(&bytes, path)?;
    Ok(sha1_hex(&der))
}

fn certificate_der_from_pem(bytes: &[u8], path: &Path) -> Result<Vec<u8>, TrustArtifactError> {
    let pem = pem::parse(bytes).map_err(|source| TrustArtifactError::ParseCertificate {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(pem.contents().to_vec())
}

fn unix_timestamp() -> Result<u64, TrustArtifactError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| TrustArtifactError::Clock)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha1_hex(bytes: &[u8]) -> String {
    let digest = Sha1::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn set_dir_private(path: &Path) -> Result<(), TrustArtifactError> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            TrustArtifactError::WriteFile {
                path: path.to_path_buf(),
                source,
            }
        })?;
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8], unix_mode: u32) -> Result<(), TrustArtifactError> {
    let temp_path = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dam-trust"),
        uuid::Uuid::new_v4().simple()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(unix_mode);
    }
    let write_result = (|| -> std::io::Result<()> {
        let mut file = options.open(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp_path, path)?;
        Ok(())
    })();
    if let Err(source) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(TrustArtifactError::WriteFile {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

fn normalize_host(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('.');
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("wss://"))
        .or_else(|| trimmed.strip_prefix("ws://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    host_port
        .split_once(':')
        .map(|(host, _)| host)
        .unwrap_or(host_port)
        .to_ascii_lowercase()
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
