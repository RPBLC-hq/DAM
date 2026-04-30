use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

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
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Self::LinuxNssOrSystemStore
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", unix)))]
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
                message: "trust inspection is available without changing system trust".to_string(),
            },
            TrustAction::InstallLocalCa => Self {
                action,
                support: TrustSupport::Planned,
                mode: TrustMode::LocalCa,
                platform_store,
                requires_admin: true,
                changes_system_trust: true,
                requires_user_consent: true,
                rollback_required: true,
                message: "local CA installation is planned and must require explicit user approval"
                    .to_string(),
            },
            TrustAction::RemoveLocalCa => Self {
                action,
                support: TrustSupport::Planned,
                mode: TrustMode::LocalCa,
                platform_store,
                requires_admin: true,
                changes_system_trust: true,
                requires_user_consent: true,
                rollback_required: false,
                message: "local CA removal is planned and must leave no trusted DAM root behind"
                    .to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalCaRecord {
    pub id: String,
    pub label: String,
    pub fingerprint_sha256: String,
    pub created_at_unix: u64,
    pub installed_at_unix: Option<u64>,
}

impl LocalCaRecord {
    pub fn installed(&self) -> bool {
        self.installed_at_unix.is_some()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsInterceptionReadiness {
    NotAiTraffic,
    NotRequired,
    Disabled,
    HostNotAllowed,
    NeedsUserConsent,
    NeedsLocalCa,
    Ready,
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
        dam_net::TransparentRouteDecision::NonAiTraffic { .. } => TrustReadinessReport {
            readiness: TlsInterceptionReadiness::NotAiTraffic,
            message: "non-AI traffic is outside the trust scope".to_string(),
        },
        dam_net::TransparentRouteDecision::IdentifiedAi {
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
        dam_net::TransparentRouteDecision::IdentifiedAi { route, .. } => {
            if trust.mode == TrustMode::Disabled {
                return TrustReadinessReport {
                    readiness: TlsInterceptionReadiness::Disabled,
                    message: "TLS interception is disabled".to_string(),
                };
            }
            if !trust.host_allowed(&route.upstream) {
                return TrustReadinessReport {
                    readiness: TlsInterceptionReadiness::HostNotAllowed,
                    message: format!("{} is not in the trusted AI host scope", route.upstream),
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

pub fn default_allowed_hosts() -> Vec<String> {
    dam_net::known_ai_hosts()
        .into_iter()
        .map(str::to_string)
        .collect()
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
mod tests {
    use super::*;

    fn https_openai_decision() -> dam_net::TransparentRouteDecision {
        dam_net::decide_transparent_route(&dam_net::TrafficObservation::new(
            "api.openai.com",
            dam_net::TrafficProtocol::Https,
        ))
    }

    #[test]
    fn parses_trust_modes() {
        assert_eq!("off".parse::<TrustMode>().unwrap(), TrustMode::Disabled);
        assert_eq!("local-ca".parse::<TrustMode>().unwrap(), TrustMode::LocalCa);
    }

    #[test]
    fn trust_action_plans_do_not_mark_local_ca_install_as_implemented() {
        let inspect =
            TrustActionPlan::for_action(TrustAction::Inspect, PlatformTrustStore::MacosKeychain);
        let install = TrustActionPlan::for_action(
            TrustAction::InstallLocalCa,
            PlatformTrustStore::MacosKeychain,
        );

        assert_eq!(inspect.support, TrustSupport::Implemented);
        assert_eq!(install.support, TrustSupport::Planned);
        assert!(install.requires_user_consent);
        assert!(install.rollback_required);
    }

    #[test]
    fn default_trust_state_allows_known_ai_hosts_but_is_disabled() {
        let state = TrustState::default();

        assert_eq!(state.mode, TrustMode::Disabled);
        assert!(state.host_allowed("https://api.openai.com/v1/responses"));
        assert!(!state.host_allowed("example.com"));
    }

    #[test]
    fn https_ai_traffic_needs_trust_when_interception_is_disabled() {
        let report = readiness_for_route(&https_openai_decision(), &TrustState::default(), false);

        assert_eq!(report.readiness, TlsInterceptionReadiness::Disabled);
    }

    #[test]
    fn local_ca_mode_requires_user_consent_before_ca_check() {
        let state = TrustState {
            mode: TrustMode::LocalCa,
            ..TrustState::default()
        };

        let report = readiness_for_route(&https_openai_decision(), &state, false);

        assert_eq!(report.readiness, TlsInterceptionReadiness::NeedsUserConsent);
    }

    #[test]
    fn local_ca_mode_requires_installed_ca_after_user_consent() {
        let state = TrustState {
            mode: TrustMode::LocalCa,
            ..TrustState::default()
        };

        let report = readiness_for_route(&https_openai_decision(), &state, true);

        assert_eq!(report.readiness, TlsInterceptionReadiness::NeedsLocalCa);
    }

    #[test]
    fn installed_local_ca_and_user_consent_make_known_ai_tls_route_ready() {
        let state = TrustState {
            mode: TrustMode::LocalCa,
            local_ca: Some(LocalCaRecord {
                id: "dam-local-ca".to_string(),
                label: "DAM Local CA".to_string(),
                fingerprint_sha256: "abc123".to_string(),
                created_at_unix: 1,
                installed_at_unix: Some(2),
            }),
            ..TrustState::default()
        };

        let report = readiness_for_route(&https_openai_decision(), &state, true);

        assert_eq!(report.readiness, TlsInterceptionReadiness::Ready);
    }
}
