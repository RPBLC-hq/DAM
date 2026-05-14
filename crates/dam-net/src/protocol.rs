use serde::{Deserialize, Serialize};

use crate::TrafficRoute;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolAdapterKind {
    Http,
    WebSocket,
    Grpc,
    EmailSmtp,
    EmailImap,
    EmailPop3,
    Media,
    #[default]
    Unknown,
}

impl ProtocolAdapterKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::WebSocket => "web_socket",
            Self::Grpc => "grpc",
            Self::EmailSmtp => "email_smtp",
            Self::EmailImap => "email_imap",
            Self::EmailPop3 => "email_pop3",
            Self::Media => "media",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolAdapterReadiness {
    Ready,
    Planned,
    Unsupported,
}

impl ProtocolAdapterReadiness {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Planned => "planned",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolAdapterStatus {
    pub route: TrafficRoute,
    pub adapter: ProtocolAdapterKind,
    pub readiness: ProtocolAdapterReadiness,
    pub message: String,
}

pub fn adapter_for_traffic_route(route: &TrafficRoute) -> ProtocolAdapterKind {
    route.adapter
}

pub fn adapter_status_for_traffic_route(route: TrafficRoute) -> ProtocolAdapterStatus {
    let adapter = adapter_for_traffic_route(&route);
    let readiness = match adapter {
        ProtocolAdapterKind::Http | ProtocolAdapterKind::WebSocket => {
            ProtocolAdapterReadiness::Ready
        }
        ProtocolAdapterKind::Grpc
        | ProtocolAdapterKind::EmailSmtp
        | ProtocolAdapterKind::EmailImap
        | ProtocolAdapterKind::EmailPop3
        | ProtocolAdapterKind::Media
        | ProtocolAdapterKind::Unknown => ProtocolAdapterReadiness::Planned,
    };
    let message = match (adapter, readiness) {
        (ProtocolAdapterKind::Http, ProtocolAdapterReadiness::Ready) => {
            format!("HTTP adapter is ready for {}", route.target_name)
        }
        (ProtocolAdapterKind::WebSocket, ProtocolAdapterReadiness::Ready) => {
            format!("WebSocket adapter is ready for {}", route.target_name)
        }
        (_, ProtocolAdapterReadiness::Planned) => {
            format!(
                "{} adapter is planned for {}",
                adapter.tag(),
                route.target_name
            )
        }
        (_, ProtocolAdapterReadiness::Unsupported) => {
            format!(
                "{} adapter is unsupported for {}",
                adapter.tag(),
                route.target_name
            )
        }
        (_, ProtocolAdapterReadiness::Ready) => {
            format!(
                "{} adapter is ready for {}",
                adapter.tag(),
                route.target_name
            )
        }
    };

    ProtocolAdapterStatus {
        route,
        adapter,
        readiness,
        message,
    }
}

pub fn adapter_status_for_traffic_routes(routes: &[TrafficRoute]) -> Vec<ProtocolAdapterStatus> {
    routes
        .iter()
        .cloned()
        .map(adapter_status_for_traffic_route)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_chatgpt_uses_websocket_adapter() {
        let route = crate::default_traffic_routes()
            .into_iter()
            .find(|route| route.target_name == "chatgpt-codex")
            .unwrap();

        let status = adapter_status_for_traffic_route(route);

        assert_eq!(status.adapter, ProtocolAdapterKind::WebSocket);
        assert_eq!(status.readiness, ProtocolAdapterReadiness::Ready);
    }

    #[test]
    fn api_routes_use_http_adapter() {
        let statuses = adapter_status_for_traffic_routes(&crate::default_traffic_routes());

        assert!(statuses.iter().any(|status| {
            status.route.target_name == "openai" && status.adapter == ProtocolAdapterKind::Http
        }));
        assert!(statuses.iter().any(|status| {
            status.route.target_name == "anthropic" && status.adapter == ProtocolAdapterKind::Http
        }));
    }
}
