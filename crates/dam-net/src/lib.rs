use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

mod capture;
mod profile;
mod protocol;

pub use capture::*;
pub use profile::*;
pub use protocol::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    #[default]
    ExplicitProxy,
    SystemProxy,
    Tun,
}

impl CaptureMode {
    pub fn tag(self) -> &'static str {
        match self {
            Self::ExplicitProxy => "explicit_proxy",
            Self::SystemProxy => "system_proxy",
            Self::Tun => "tun",
        }
    }
}

impl fmt::Display for CaptureMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.tag())
    }
}

impl FromStr for CaptureMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().replace('-', "_").as_str() {
            "explicit" | "explicit_proxy" | "app_layer" => Ok(Self::ExplicitProxy),
            "system" | "system_proxy" => Ok(Self::SystemProxy),
            "tun" | "vpn" => Ok(Self::Tun),
            _ => Err(format!(
                "unsupported network mode: {value}; expected explicit_proxy, system_proxy, or tun"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSupport {
    Implemented,
    Planned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsVisibility {
    NotRequired,
    HostOnly,
    RequiresInterception,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturePlan {
    pub mode: CaptureMode,
    pub support: CaptureSupport,
    pub requires_admin: bool,
    pub installs_system_routes: bool,
    pub tls_visibility: TlsVisibility,
    pub message: String,
}

impl CapturePlan {
    pub fn for_mode(mode: CaptureMode) -> Self {
        match mode {
            CaptureMode::ExplicitProxy => Self {
                mode,
                support: CaptureSupport::Implemented,
                requires_admin: false,
                installs_system_routes: false,
                tls_visibility: TlsVisibility::RequiresInterception,
                message: "selected AI clients must use DAM as their local HTTP(S) proxy"
                    .to_string(),
            },
            CaptureMode::SystemProxy => Self {
                mode,
                support: CaptureSupport::Planned,
                requires_admin: false,
                installs_system_routes: true,
                tls_visibility: TlsVisibility::HostOnly,
                message:
                    "system proxy routing is planned; HTTPS bodies still require a trust layer"
                        .to_string(),
            },
            CaptureMode::Tun => Self {
                mode,
                support: CaptureSupport::Planned,
                requires_admin: true,
                installs_system_routes: true,
                tls_visibility: TlsVisibility::HostOnly,
                message: "VPN/TUN routing is planned; HTTPS bodies still require a trust layer"
                    .to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteCaptureReadiness {
    NotTransparentMode,
    NeedsSystemProxyInstall,
    NeedsTunInstall,
    Ready,
}

impl RouteCaptureReadiness {
    pub fn tag(self) -> &'static str {
        match self {
            Self::NotTransparentMode => "not_transparent_mode",
            Self::NeedsSystemProxyInstall => "needs_system_proxy_install",
            Self::NeedsTunInstall => "needs_tun_install",
            Self::Ready => "ready",
        }
    }
}

impl fmt::Display for RouteCaptureReadiness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.tag())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransparentRouteCaptureReadiness {
    pub route: TrafficRoute,
    pub protocol: TrafficProtocol,
    pub mode: CaptureMode,
    pub support: CaptureSupport,
    pub readiness: RouteCaptureReadiness,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrafficProtocol {
    Http,
    Https,
    WebSocket,
    #[default]
    Unknown,
}

impl TrafficProtocol {
    pub fn is_tls(self) -> bool {
        matches!(self, Self::Https | Self::WebSocket)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficObservation {
    pub host: String,
    pub port: Option<u16>,
    pub protocol: TrafficProtocol,
    pub path: Option<String>,
    pub process_name: Option<String>,
}

impl TrafficObservation {
    pub fn new(host: impl Into<String>, protocol: TrafficProtocol) -> Self {
        Self {
            host: host.into(),
            port: None,
            protocol,
            path: None,
            process_name: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficRoute {
    pub adapter: ProtocolAdapterKind,
    #[serde(default)]
    pub host: String,
    pub provider: String,
    pub target_name: String,
    pub upstream: String,
    #[serde(default)]
    pub auth: UpstreamAuthConfig,
}

impl TrafficRoute {
    pub fn new(
        adapter: ProtocolAdapterKind,
        host: impl AsRef<str>,
        provider: impl Into<String>,
        target_name: impl Into<String>,
        upstream: impl Into<String>,
    ) -> Self {
        Self::new_with_auth(
            adapter,
            host,
            provider,
            target_name,
            upstream,
            UpstreamAuthConfig::default(),
        )
    }

    pub fn new_with_auth(
        adapter: ProtocolAdapterKind,
        host: impl AsRef<str>,
        provider: impl Into<String>,
        target_name: impl Into<String>,
        upstream: impl Into<String>,
        auth: UpstreamAuthConfig,
    ) -> Self {
        Self {
            adapter,
            host: normalize_host(host.as_ref()),
            provider: provider.into(),
            target_name: target_name.into(),
            upstream: upstream.into(),
            auth,
        }
    }

    pub fn custom(
        host: impl AsRef<str>,
        provider: impl Into<String>,
        target_name: impl Into<String>,
        upstream: impl Into<String>,
    ) -> Self {
        Self::new(
            ProtocolAdapterKind::Http,
            host,
            provider,
            target_name,
            upstream,
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamAuthConfig {
    #[serde(default)]
    pub caller_headers: Vec<String>,
    #[serde(default)]
    pub inject: Option<UpstreamAuthInjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamAuthInjection {
    pub header: String,
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub strip_headers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum TransparentRouteDecision {
    Unmatched {
        reason: String,
    },
    Matched {
        route: TrafficRoute,
        tls_visibility: TlsVisibility,
        protectable_without_tls: bool,
    },
}

pub fn classify_traffic_host(host: &str) -> Option<TrafficRoute> {
    classify_traffic_host_with_routes(host, &default_traffic_routes())
}

pub fn classify_traffic_host_with_routes(
    host: &str,
    routes: &[TrafficRoute],
) -> Option<TrafficRoute> {
    let normalized = normalize_host(host);
    routes
        .iter()
        .find(|route| route.host == normalized)
        .cloned()
}

pub fn default_traffic_routes() -> Vec<TrafficRoute> {
    traffic_routes_from_profile(&llm_mvp_profile())
}

pub fn default_traffic_hosts() -> Vec<String> {
    default_traffic_routes()
        .into_iter()
        .map(|route| route.host)
        .collect()
}

pub fn normalize_traffic_host(host: &str) -> String {
    normalize_host(host)
}

pub fn decide_transparent_route(observation: &TrafficObservation) -> TransparentRouteDecision {
    decide_transparent_route_with_routes(observation, &default_traffic_routes())
}

pub fn decide_transparent_route_with_routes(
    observation: &TrafficObservation,
    routes: &[TrafficRoute],
) -> TransparentRouteDecision {
    let Some(route) = classify_traffic_host_with_routes(&observation.host, routes) else {
        return TransparentRouteDecision::Unmatched {
            reason: "host is not configured for DAM traffic inspection".to_string(),
        };
    };

    if observation.protocol.is_tls() {
        TransparentRouteDecision::Matched {
            route,
            tls_visibility: TlsVisibility::RequiresInterception,
            protectable_without_tls: false,
        }
    } else {
        TransparentRouteDecision::Matched {
            route,
            tls_visibility: TlsVisibility::NotRequired,
            protectable_without_tls: true,
        }
    }
}

pub fn transparent_capture_readiness_for_default_routes(
    mode: CaptureMode,
    system_proxy_active: bool,
    tun_active: bool,
) -> Vec<TransparentRouteCaptureReadiness> {
    transparent_capture_readiness_for_routes(
        &default_traffic_routes(),
        mode,
        system_proxy_active,
        tun_active,
    )
}

pub fn transparent_capture_readiness_for_routes(
    routes: &[TrafficRoute],
    mode: CaptureMode,
    system_proxy_active: bool,
    tun_active: bool,
) -> Vec<TransparentRouteCaptureReadiness> {
    routes
        .iter()
        .cloned()
        .map(|route| {
            transparent_route_capture_readiness(
                route,
                TrafficProtocol::Https,
                mode,
                system_proxy_active,
                tun_active,
            )
        })
        .collect()
}

pub fn transparent_route_capture_readiness(
    route: TrafficRoute,
    protocol: TrafficProtocol,
    mode: CaptureMode,
    system_proxy_active: bool,
    tun_active: bool,
) -> TransparentRouteCaptureReadiness {
    let plan = CapturePlan::for_mode(mode);
    let (support, readiness, message) = match mode {
        CaptureMode::ExplicitProxy => (
            plan.support,
            RouteCaptureReadiness::Ready,
            "explicit proxy routing is active for clients configured to use DAM".to_string(),
        ),
        CaptureMode::SystemProxy if system_proxy_active => (
            CaptureSupport::Implemented,
            RouteCaptureReadiness::Ready,
            format!("system proxy routing is active for {}", route.target_name),
        ),
        CaptureMode::SystemProxy => (
            plan.support,
            RouteCaptureReadiness::NeedsSystemProxyInstall,
            "system proxy routing is not installed".to_string(),
        ),
        CaptureMode::Tun if tun_active => (
            CaptureSupport::Implemented,
            RouteCaptureReadiness::Ready,
            format!("TUN routing is active for {}", route.target_name),
        ),
        CaptureMode::Tun => (
            plan.support,
            RouteCaptureReadiness::NeedsTunInstall,
            "TUN routing is not installed".to_string(),
        ),
    };

    TransparentRouteCaptureReadiness {
        route,
        protocol,
        mode,
        support,
        readiness,
        message,
    }
}

pub(crate) fn normalize_host(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('.');
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("wss://"))
        .or_else(|| trimmed.strip_prefix("ws://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host_only = host_port
        .strip_prefix('[')
        .and_then(|value| value.split_once(']').map(|(host, _)| host))
        .unwrap_or_else(|| {
            host_port
                .split_once(':')
                .map(|(host, _)| host)
                .unwrap_or(host_port)
        });
    host_only.to_ascii_lowercase()
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
