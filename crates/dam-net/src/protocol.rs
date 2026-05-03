use serde::{Deserialize, Serialize};

use crate::{AiRoute, AiTrafficKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolAdapterKind {
    Http,
    WebSocket,
    Grpc,
    EmailSmtp,
    EmailImap,
    EmailPop3,
    Media,
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
    pub route: AiRoute,
    pub adapter: ProtocolAdapterKind,
    pub readiness: ProtocolAdapterReadiness,
    pub message: String,
}

pub fn adapter_for_ai_route(route: &AiRoute) -> ProtocolAdapterKind {
    match route.kind {
        AiTrafficKind::ChatGptCodexBackend => ProtocolAdapterKind::WebSocket,
        AiTrafficKind::OpenAiApi
        | AiTrafficKind::AnthropicApi
        | AiTrafficKind::XaiApi
        | AiTrafficKind::Custom => ProtocolAdapterKind::Http,
    }
}

pub fn adapter_status_for_ai_route(route: AiRoute) -> ProtocolAdapterStatus {
    let adapter = adapter_for_ai_route(&route);
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

pub fn adapter_status_for_ai_routes(routes: &[AiRoute]) -> Vec<ProtocolAdapterStatus> {
    routes
        .iter()
        .cloned()
        .map(adapter_status_for_ai_route)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_chatgpt_uses_websocket_adapter() {
        let route = crate::known_ai_routes()
            .into_iter()
            .find(|route| route.kind == AiTrafficKind::ChatGptCodexBackend)
            .unwrap();

        let status = adapter_status_for_ai_route(route);

        assert_eq!(status.adapter, ProtocolAdapterKind::WebSocket);
        assert_eq!(status.readiness, ProtocolAdapterReadiness::Ready);
    }

    #[test]
    fn api_routes_use_http_adapter() {
        let statuses = adapter_status_for_ai_routes(&crate::known_ai_routes());

        assert!(statuses.iter().any(|status| {
            status.route.kind == AiTrafficKind::OpenAiApi
                && status.adapter == ProtocolAdapterKind::Http
        }));
        assert!(statuses.iter().any(|status| {
            status.route.kind == AiTrafficKind::AnthropicApi
                && status.adapter == ProtocolAdapterKind::Http
        }));
    }
}
