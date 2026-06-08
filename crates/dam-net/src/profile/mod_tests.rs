use super::*;
use crate::TrafficProtocol;

#[test]
fn llm_mvp_profile_is_just_traffic_profile_data() {
    let profile = llm_mvp_profile();

    assert_eq!(profile.default_action, TrafficAction::Bypass);
    assert_eq!(profile.apps.len(), 9);
    assert_eq!(profile.apps[0].id, "openai-api");
    assert_eq!(profile.apps[0].action, TrafficAction::Inspect);
    assert_eq!(
        profile.apps[0].provider.as_deref(),
        Some("openai-compatible")
    );
    assert_eq!(
        profile.apps[0].outbound.filter.default_action,
        SensitiveDataAction::Tokenize
    );
    assert!(profile.apps[0].inbound.resolve_references);
}

#[test]
fn profile_decision_matches_arbitrary_web_traffic() {
    let profile = traffic_profile_from_json_str(
        r#"
        {
          "version": 1,
          "default_action": "bypass",
          "apps": [
            {
              "id": "mail-example",
              "match": {
                "domains": ["mail.example.com"],
                "ports": [443],
                "protocols": ["https"]
              },
              "action": "inspect",
              "adapter": "email_imap",
              "provider": "imap",
              "target_name": "mail-example",
              "upstream": "https://mail.example.com",
              "steps": [
                {"id": "detect", "kind": "detect_sensitive_data", "direction": "both"}
              ]
            }
          ]
        }
        "#,
    )
    .unwrap();
    let mut observation = TrafficObservation::new("mail.example.com", TrafficProtocol::Https);
    observation.port = Some(443);

    assert_eq!(
        decide_profile_traffic(&profile, &observation),
        TrafficProfileDecision::Matched {
            app_id: "mail-example".to_string(),
            action: TrafficAction::Inspect,
            adapter: ProtocolAdapterKind::EmailImap,
        }
    );
}

#[test]
fn runtime_enabled_apps_filter_profile_without_rewriting_pipeline() {
    let profile = llm_mvp_profile().with_runtime_enabled_apps(&["anthropic-api".to_string()]);
    let routes = traffic_routes_from_profile(&profile);

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].host, "api.anthropic.com");
}

#[test]
fn explicit_empty_runtime_app_list_disables_profile_apps() {
    let profile = llm_mvp_profile().with_runtime_enabled_apps(&[]);

    assert!(traffic_routes_from_profile(&profile).is_empty());
}

#[test]
fn route_registry_is_derived_from_inspect_apps() {
    let routes = traffic_routes_from_profile(&llm_mvp_profile());

    assert_eq!(routes.len(), 10);
    assert!(routes.iter().any(|route| route.host == "chatgpt.com"));
    assert!(routes.iter().any(|route| route.host == "ab.chatgpt.com"));
    assert!(routes.iter().any(|route| route.host == "chat.openai.com"));
    assert!(routes.iter().any(|route| route.host == "claude.ai"));
    assert!(
        routes
            .iter()
            .any(|route| route.host == "console.anthropic.com")
    );
    assert!(
        routes
            .iter()
            .any(|route| route.host == "mcp-proxy.anthropic.com")
    );
    assert!(
        routes
            .iter()
            .any(|route| route.host == "platform.claude.com")
    );
    assert!(
        routes
            .iter()
            .any(|route| route.host == "platform.openai.com")
    );
}

#[test]
fn route_registry_includes_hosts_from_url_match_rules() {
    let profile = traffic_profile_from_json_str(
        r#"
        {
          "apps": [
            {
              "id": "private-openai",
              "match": {
                "urls": ["https://gateway.example.test/v1/chat/completions"]
              },
              "action": "inspect",
              "adapter": "http",
              "provider": "openai-compatible",
              "target_name": "private-openai",
              "upstream": "https://gateway.example.test"
            }
          ]
        }
        "#,
    )
    .unwrap();

    let routes = traffic_routes_from_profile(&profile);

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].host, "gateway.example.test");
}

#[test]
fn invalid_inspect_app_requires_match_and_upstream_contract() {
    let error = traffic_profile_from_json_str(
        r#"
        {
          "apps": [
            {
              "id": "broken",
              "action": "inspect",
              "provider": "openai-compatible"
            }
          ]
        }
        "#,
    )
    .unwrap_err();

    assert!(matches!(error, TrafficProfileError::InvalidApp { .. }));
}
