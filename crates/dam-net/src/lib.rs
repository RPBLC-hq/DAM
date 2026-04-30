use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

pub const OPENAI_COMPATIBLE_PROVIDER: &str = "openai-compatible";
pub const ANTHROPIC_PROVIDER: &str = "anthropic";

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
                tls_visibility: TlsVisibility::NotRequired,
                message: "selected AI clients must point at DAM's local app-layer endpoint"
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
pub enum TrafficProtocol {
    Http,
    Https,
    WebSocket,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiTrafficKind {
    OpenAiApi,
    AnthropicApi,
    XaiApi,
    ChatGptCodexBackend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiRoute {
    pub kind: AiTrafficKind,
    pub provider: String,
    pub target_name: String,
    pub upstream: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum TransparentRouteDecision {
    NonAiTraffic {
        reason: String,
    },
    IdentifiedAi {
        route: AiRoute,
        tls_visibility: TlsVisibility,
        protectable_without_tls: bool,
    },
}

pub fn classify_ai_host(host: &str) -> Option<AiRoute> {
    let normalized = normalize_host(host);
    match normalized.as_str() {
        "api.openai.com" => Some(AiRoute {
            kind: AiTrafficKind::OpenAiApi,
            provider: OPENAI_COMPATIBLE_PROVIDER.to_string(),
            target_name: "openai".to_string(),
            upstream: "https://api.openai.com".to_string(),
        }),
        "api.anthropic.com" => Some(AiRoute {
            kind: AiTrafficKind::AnthropicApi,
            provider: ANTHROPIC_PROVIDER.to_string(),
            target_name: "anthropic".to_string(),
            upstream: "https://api.anthropic.com".to_string(),
        }),
        "api.x.ai" => Some(AiRoute {
            kind: AiTrafficKind::XaiApi,
            provider: OPENAI_COMPATIBLE_PROVIDER.to_string(),
            target_name: "xai".to_string(),
            upstream: "https://api.x.ai".to_string(),
        }),
        "chatgpt.com" => Some(AiRoute {
            kind: AiTrafficKind::ChatGptCodexBackend,
            provider: OPENAI_COMPATIBLE_PROVIDER.to_string(),
            target_name: "chatgpt-codex".to_string(),
            upstream: "https://chatgpt.com".to_string(),
        }),
        _ => None,
    }
}

pub fn known_ai_routes() -> Vec<AiRoute> {
    known_ai_hosts()
        .into_iter()
        .filter_map(classify_ai_host)
        .collect()
}

pub fn known_ai_hosts() -> Vec<&'static str> {
    vec![
        "api.openai.com",
        "api.anthropic.com",
        "api.x.ai",
        "chatgpt.com",
    ]
}

pub fn decide_transparent_route(observation: &TrafficObservation) -> TransparentRouteDecision {
    let Some(route) = classify_ai_host(&observation.host) else {
        return TransparentRouteDecision::NonAiTraffic {
            reason: "host is not a known AI provider endpoint".to_string(),
        };
    };

    if observation.protocol.is_tls() {
        TransparentRouteDecision::IdentifiedAi {
            route,
            tls_visibility: TlsVisibility::RequiresInterception,
            protectable_without_tls: false,
        }
    } else {
        TransparentRouteDecision::IdentifiedAi {
            route,
            tls_visibility: TlsVisibility::NotRequired,
            protectable_without_tls: true,
        }
    }
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
mod tests {
    use super::*;

    #[test]
    fn parses_capture_modes_with_user_friendly_aliases() {
        assert_eq!(
            "explicit".parse::<CaptureMode>().unwrap(),
            CaptureMode::ExplicitProxy
        );
        assert_eq!(
            "system-proxy".parse::<CaptureMode>().unwrap(),
            CaptureMode::SystemProxy
        );
        assert_eq!("vpn".parse::<CaptureMode>().unwrap(), CaptureMode::Tun);
    }

    #[test]
    fn capture_plan_marks_only_explicit_proxy_as_implemented() {
        assert_eq!(
            CapturePlan::for_mode(CaptureMode::ExplicitProxy).support,
            CaptureSupport::Implemented
        );
        assert_eq!(
            CapturePlan::for_mode(CaptureMode::SystemProxy).support,
            CaptureSupport::Planned
        );
        assert!(CapturePlan::for_mode(CaptureMode::Tun).requires_admin);
    }

    #[test]
    fn classifies_known_ai_provider_hosts() {
        assert_eq!(
            classify_ai_host("https://api.openai.com/v1/responses")
                .unwrap()
                .target_name,
            "openai"
        );
        assert_eq!(
            classify_ai_host("api.anthropic.com:443").unwrap().provider,
            ANTHROPIC_PROVIDER.to_string()
        );
        assert_eq!(classify_ai_host("API.X.AI.").unwrap().target_name, "xai");
        assert_eq!(
            classify_ai_host("chatgpt.com").unwrap().kind,
            AiTrafficKind::ChatGptCodexBackend
        );
        assert!(classify_ai_host("example.com").is_none());
    }

    #[test]
    fn known_ai_routes_are_unique_and_non_empty() {
        let routes = known_ai_routes();

        assert_eq!(routes.len(), 4);
        assert_eq!(routes[0].target_name, "openai");
        assert_eq!(known_ai_hosts()[0], "api.openai.com");
    }

    #[test]
    fn transparent_https_ai_traffic_is_identified_but_not_protectable_without_tls() {
        let decision = decide_transparent_route(&TrafficObservation::new(
            "api.openai.com",
            TrafficProtocol::Https,
        ));

        assert_eq!(
            decision,
            TransparentRouteDecision::IdentifiedAi {
                route: classify_ai_host("api.openai.com").unwrap(),
                tls_visibility: TlsVisibility::RequiresInterception,
                protectable_without_tls: false,
            }
        );
    }

    #[test]
    fn transparent_http_ai_traffic_is_protectable_without_tls() {
        let decision = decide_transparent_route(&TrafficObservation::new(
            "api.anthropic.com",
            TrafficProtocol::Http,
        ));

        assert_eq!(
            decision,
            TransparentRouteDecision::IdentifiedAi {
                route: classify_ai_host("api.anthropic.com").unwrap(),
                tls_visibility: TlsVisibility::NotRequired,
                protectable_without_tls: true,
            }
        );
    }
}
