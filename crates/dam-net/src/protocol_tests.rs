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
