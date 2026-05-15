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
fn classifies_configured_hosts() {
    assert_eq!(
        classify_traffic_host("https://api.openai.com/v1/responses")
            .unwrap()
            .target_name,
        "openai"
    );
    assert_eq!(
        classify_traffic_host("api.anthropic.com:443")
            .unwrap()
            .provider,
        "anthropic".to_string()
    );
    assert_eq!(
        classify_traffic_host("chatgpt.com").unwrap().adapter,
        ProtocolAdapterKind::WebSocket
    );
    assert_eq!(
        classify_traffic_host("ab.chatgpt.com").unwrap().adapter,
        ProtocolAdapterKind::WebSocket
    );
    assert!(classify_traffic_host("API.X.AI.").is_none());
    assert!(classify_traffic_host("example.com").is_none());
}

#[test]
fn default_traffic_routes_are_unique_and_non_empty() {
    let routes = default_traffic_routes();

    assert_eq!(routes.len(), 4);
    assert_eq!(routes[0].host, "api.openai.com");
    assert_eq!(routes[0].target_name, "openai");
    assert_eq!(default_traffic_hosts()[0], "api.openai.com");
}

#[test]
fn traffic_profile_routes_model_private_and_provider_edge_hosts() {
    let profile = traffic_profile_from_json_str(
        r#"
        {
          "version": 1,
          "default_action": "bypass",
          "apps": [
            {
              "id": "internal-ai",
              "match": {"domains": ["api.internal-ai.example:443"]},
              "action": "inspect",
              "adapter": "http",
              "provider": "openai-compatible",
              "target_name": "internal-ai",
              "upstream": "https://api.internal-ai.example"
            },
            {
              "id": "openai-private-edge",
              "match": {"domains": ["https://api.openai.com/v1"]},
              "action": "inspect",
              "adapter": "http",
              "provider": "openai-compatible",
              "target_name": "openai-private-edge",
              "upstream": "https://openai.internal.example"
            }
          ]
        }
        "#,
    )
    .unwrap();
    let routes = traffic_routes_from_profile(&profile);

    assert_eq!(routes.len(), 2);
    assert_eq!(
        classify_traffic_host_with_routes("api.internal-ai.example", &routes)
            .unwrap()
            .target_name,
        "internal-ai"
    );
    assert_eq!(
        classify_traffic_host_with_routes("api.openai.com", &routes)
            .unwrap()
            .upstream,
        "https://openai.internal.example"
    );
}

#[test]
fn transparent_https_profile_traffic_is_identified_but_not_protectable_without_tls() {
    let decision = decide_transparent_route(&TrafficObservation::new(
        "api.openai.com",
        TrafficProtocol::Https,
    ));

    assert_eq!(
        decision,
        TransparentRouteDecision::Matched {
            route: classify_traffic_host("api.openai.com").unwrap(),
            tls_visibility: TlsVisibility::RequiresInterception,
            protectable_without_tls: false,
        }
    );
}

#[test]
fn transparent_http_profile_traffic_is_protectable_without_tls() {
    let decision = decide_transparent_route(&TrafficObservation::new(
        "api.anthropic.com",
        TrafficProtocol::Http,
    ));

    assert_eq!(
        decision,
        TransparentRouteDecision::Matched {
            route: classify_traffic_host("api.anthropic.com").unwrap(),
            tls_visibility: TlsVisibility::NotRequired,
            protectable_without_tls: true,
        }
    );
}

#[test]
fn explicit_proxy_is_ready_for_configured_clients() {
    let readiness =
        transparent_capture_readiness_for_default_routes(CaptureMode::ExplicitProxy, false, false);

    assert_eq!(readiness.len(), 4);
    assert!(
        readiness
            .iter()
            .all(|route| route.readiness == RouteCaptureReadiness::Ready)
    );
}

#[test]
fn system_proxy_and_tun_report_missing_route_installation() {
    let system_proxy =
        transparent_capture_readiness_for_default_routes(CaptureMode::SystemProxy, false, false);
    let tun = transparent_capture_readiness_for_default_routes(CaptureMode::Tun, false, false);

    assert!(
        system_proxy
            .iter()
            .all(|route| route.readiness == RouteCaptureReadiness::NeedsSystemProxyInstall)
    );
    assert!(
        tun.iter()
            .all(|route| route.readiness == RouteCaptureReadiness::NeedsTunInstall)
    );
}

#[test]
fn active_system_proxy_or_tun_marks_transparent_routing_ready() {
    let system_proxy =
        transparent_capture_readiness_for_default_routes(CaptureMode::SystemProxy, true, false);
    let tun = transparent_capture_readiness_for_default_routes(CaptureMode::Tun, false, true);

    assert!(
        system_proxy
            .iter()
            .all(|route| route.readiness == RouteCaptureReadiness::Ready)
    );
    assert!(
        system_proxy
            .iter()
            .all(|route| route.support == CaptureSupport::Implemented)
    );
    assert!(
        tun.iter()
            .all(|route| route.readiness == RouteCaptureReadiness::Ready)
    );
    assert!(
        tun.iter()
            .all(|route| route.support == CaptureSupport::Implemented)
    );
}

#[test]
fn transparent_readiness_accepts_custom_route_sets() {
    let routes = vec![TrafficRoute::custom(
        "api.enterprise-ai.example",
        "openai-compatible",
        "enterprise-ai",
        "https://api.enterprise-ai.example",
    )];

    let readiness =
        transparent_capture_readiness_for_routes(&routes, CaptureMode::SystemProxy, true, false);

    assert_eq!(readiness.len(), 1);
    assert!(
        readiness
            .iter()
            .any(|route| route.route.target_name == "enterprise-ai"
                && route.readiness == RouteCaptureReadiness::Ready)
    );
}
