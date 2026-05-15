use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsInterceptionReadiness {
    NotTransparentMode,
    NeedsRouting,
    NeedsUserConsent,
    NeedsTrust,
    NeedsAdapter,
    Ready,
}

impl TlsInterceptionReadiness {
    pub fn tag(self) -> &'static str {
        match self {
            Self::NotTransparentMode => "not_transparent_mode",
            Self::NeedsRouting => "needs_routing",
            Self::NeedsUserConsent => "needs_user_consent",
            Self::NeedsTrust => "needs_trust",
            Self::NeedsAdapter => "needs_adapter",
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
pub struct RouteTlsInterceptionReadiness {
    pub route: dam_net::TrafficRoute,
    pub protocol: dam_net::TrafficProtocol,
    pub network_mode: dam_net::CaptureMode,
    pub routing_readiness: dam_net::RouteCaptureReadiness,
    pub trust_readiness: dam_trust::TlsInterceptionReadiness,
    pub user_consented: bool,
    pub adapter_available: bool,
    pub readiness: TlsInterceptionReadiness,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsInterceptionActivationState {
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsInterceptionActivation {
    pub state: TlsInterceptionActivationState,
    pub route: dam_net::TrafficRoute,
    pub network_mode: dam_net::CaptureMode,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TlsInterceptionError {
    #[error("TLS interception is not ready for {target}: {reason}")]
    NotReady { target: String, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlsInterceptionAdapter {
    adapter_available: bool,
}

impl TlsInterceptionAdapter {
    pub fn new(adapter_available: bool) -> Self {
        Self { adapter_available }
    }

    pub fn unavailable() -> Self {
        Self::new(false)
    }

    pub fn adapter_available(self) -> bool {
        self.adapter_available
    }

    pub fn activate(
        self,
        readiness: &RouteTlsInterceptionReadiness,
    ) -> Result<TlsInterceptionActivation, TlsInterceptionError> {
        if !self.adapter_available {
            return Err(TlsInterceptionError::NotReady {
                target: readiness.route.target_name.clone(),
                reason: "TLS interception adapter runtime is not available".to_string(),
            });
        }
        if readiness.readiness != TlsInterceptionReadiness::Ready {
            return Err(TlsInterceptionError::NotReady {
                target: readiness.route.target_name.clone(),
                reason: readiness.message.clone(),
            });
        }

        Ok(TlsInterceptionActivation {
            state: TlsInterceptionActivationState::Active,
            route: readiness.route.clone(),
            network_mode: readiness.network_mode,
            message: format!(
                "TLS interception adapter active for {}",
                readiness.route.target_name
            ),
        })
    }
}

pub fn readiness_for_default_routes(
    network_mode: dam_net::CaptureMode,
    system_proxy_active: bool,
    tun_active: bool,
    trust: &dam_trust::TrustState,
    user_consented: bool,
    adapter: TlsInterceptionAdapter,
) -> Vec<RouteTlsInterceptionReadiness> {
    readiness_for_routes(
        &dam_net::default_traffic_routes(),
        network_mode,
        system_proxy_active,
        tun_active,
        trust,
        user_consented,
        adapter,
    )
}

pub fn readiness_for_routes(
    routes: &[dam_net::TrafficRoute],
    network_mode: dam_net::CaptureMode,
    system_proxy_active: bool,
    tun_active: bool,
    trust: &dam_trust::TrustState,
    user_consented: bool,
    adapter: TlsInterceptionAdapter,
) -> Vec<RouteTlsInterceptionReadiness> {
    let routing = dam_net::transparent_capture_readiness_for_routes(
        routes,
        network_mode,
        system_proxy_active,
        tun_active,
    );
    let trust = dam_trust::readiness_for_routes(routes, trust, user_consented);

    routing
        .iter()
        .zip(trust.iter())
        .map(|(routing, trust)| readiness_for_route(routing, trust, user_consented, adapter))
        .collect()
}

pub fn readiness_for_route(
    routing: &dam_net::TransparentRouteCaptureReadiness,
    trust: &dam_trust::RouteTrustReadiness,
    user_consented: bool,
    adapter: TlsInterceptionAdapter,
) -> RouteTlsInterceptionReadiness {
    let (readiness, message) =
        if routing.readiness == dam_net::RouteCaptureReadiness::NotTransparentMode {
            (
                TlsInterceptionReadiness::NotTransparentMode,
                "route capture is inactive for this mode".to_string(),
            )
        } else if routing.readiness != dam_net::RouteCaptureReadiness::Ready {
            (
                TlsInterceptionReadiness::NeedsRouting,
                routing.message.clone(),
            )
        } else if !user_consented {
            (
                TlsInterceptionReadiness::NeedsUserConsent,
                "TLS interception requires explicit user approval".to_string(),
            )
        } else if trust.readiness != dam_trust::TlsInterceptionReadiness::Ready {
            (TlsInterceptionReadiness::NeedsTrust, trust.message.clone())
        } else if !adapter.adapter_available() {
            (
                TlsInterceptionReadiness::NeedsAdapter,
                "TLS interception adapter runtime is not available".to_string(),
            )
        } else {
            (
                TlsInterceptionReadiness::Ready,
                format!(
                    "{} traffic is ready for guarded TLS interception",
                    routing.route.target_name
                ),
            )
        };

    RouteTlsInterceptionReadiness {
        route: routing.route.clone(),
        protocol: routing.protocol,
        network_mode: routing.mode,
        routing_readiness: routing.readiness,
        trust_readiness: trust.readiness,
        user_consented,
        adapter_available: adapter.adapter_available(),
        readiness,
        message,
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
