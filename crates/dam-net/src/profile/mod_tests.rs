use super::*;
use crate::TrafficProtocol;

#[test]
fn llm_mvp_profile_is_just_traffic_profile_data() {
    let profile = llm_mvp_profile();

    assert_eq!(profile.default_action, TrafficAction::Bypass);
    assert_eq!(profile.apps.len(), 5);
    assert_eq!(profile.apps[0].id, "openai-api");
    assert_eq!(profile.apps[0].action, TrafficAction::Inspect);
    assert_eq!(
        profile.apps[0].provider.as_deref(),
        Some("openai-compatible")
    );
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

    assert_eq!(routes.len(), 6);
    assert!(routes.iter().any(|route| route.host == "chatgpt.com"));
    assert!(routes.iter().any(|route| route.host == "ab.chatgpt.com"));
    assert!(routes.iter().any(|route| route.host == "claude.ai"));
    assert!(
        routes
            .iter()
            .any(|route| route.host == "console.anthropic.com")
    );
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
