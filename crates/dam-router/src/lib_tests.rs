use super::*;
use http::{HeaderValue, header};

const OPENAI_PROVIDER: &str = "openai-compatible";
const ANTHROPIC_PROVIDER: &str = "anthropic";
const GENERIC_PROVIDER: &str = "generic-http";

fn target(provider: &str) -> dam_config::ProxyTargetConfig {
    dam_config::ProxyTargetConfig {
        name: "test".to_string(),
        provider: provider.to_string(),
        upstream: "https://upstream.example.test".to_string(),
        auth: auth_for_provider(provider),
        failure_mode: None,
        api_key_env: None,
        api_key: None,
    }
}

fn auth_for_provider(provider: &str) -> dam_net::UpstreamAuthConfig {
    match provider {
        OPENAI_PROVIDER => dam_net::UpstreamAuthConfig {
            caller_headers: vec!["authorization".to_string()],
            inject: Some(dam_net::UpstreamAuthInjection {
                header: "authorization".to_string(),
                scheme: Some("Bearer".to_string()),
                strip_headers: vec!["authorization".to_string()],
            }),
        },
        ANTHROPIC_PROVIDER => dam_net::UpstreamAuthConfig {
            caller_headers: vec!["x-api-key".to_string(), "authorization".to_string()],
            inject: Some(dam_net::UpstreamAuthInjection {
                header: "x-api-key".to_string(),
                scheme: None,
                strip_headers: vec!["x-api-key".to_string(), "authorization".to_string()],
            }),
        },
        _ => dam_net::UpstreamAuthConfig::default(),
    }
}

fn proxy_config(target: dam_config::ProxyTargetConfig) -> dam_config::ProxyConfig {
    let mut config = dam_config::ProxyConfig::default();
    config.targets.push(target);
    config
}

#[test]
fn selects_first_target_and_effective_failure_mode() {
    let mut first = target(OPENAI_PROVIDER);
    first.name = "first".to_string();
    first.failure_mode = Some(dam_config::ProxyFailureMode::BlockOnError);
    let mut second = target(ANTHROPIC_PROVIDER);
    second.name = "second".to_string();

    let mut config = proxy_config(first);
    config.targets.push(second);
    config.default_failure_mode = dam_config::ProxyFailureMode::BypassOnError;

    let route = RoutePlan::from_proxy_config(&config).unwrap();

    assert_eq!(route.target().name, "first");
    assert_eq!(
        route.failure_mode(),
        dam_config::ProxyFailureMode::BlockOnError
    );
}

#[test]
fn uses_default_failure_mode_when_target_does_not_override() {
    let mut config = proxy_config(target(OPENAI_PROVIDER));
    config.default_failure_mode = dam_config::ProxyFailureMode::RedactOnly;

    let route = RoutePlan::from_proxy_config(&config).unwrap();

    assert_eq!(
        route.failure_mode(),
        dam_config::ProxyFailureMode::RedactOnly
    );
}

#[test]
fn missing_target_is_reported() {
    let config = dam_config::ProxyConfig::default();

    assert_eq!(
        RoutePlan::from_proxy_config(&config).unwrap_err(),
        RouteError::MissingTarget
    );
}

#[test]
fn route_table_does_not_infer_provider_from_request_shape() {
    let mut openai = target(OPENAI_PROVIDER);
    openai.name = "openai".to_string();
    let mut anthropic = target(ANTHROPIC_PROVIDER);
    anthropic.name = "anthropic".to_string();
    let mut config = proxy_config(openai);
    config.targets.push(anthropic);
    let table = RouteTable::from_proxy_config(&config).unwrap();

    let openai_uri = "/v1/responses".parse::<http::Uri>().unwrap();
    let anthropic_uri = "/v1/messages".parse::<http::Uri>().unwrap();

    assert_eq!(
        table
            .decide(&HeaderMap::new(), Some(&openai_uri))
            .target()
            .name,
        "openai"
    );
    assert_eq!(
        table
            .decide(&HeaderMap::new(), Some(&anthropic_uri))
            .target()
            .name,
        "openai"
    );
}

#[test]
fn route_table_does_not_infer_provider_from_headers() {
    let mut openai = target(OPENAI_PROVIDER);
    openai.name = "openai".to_string();
    let mut anthropic = target(ANTHROPIC_PROVIDER);
    anthropic.name = "anthropic".to_string();
    let mut config = proxy_config(openai);
    config.targets.push(anthropic);
    let table = RouteTable::from_proxy_config(&config).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    assert_eq!(table.decide(&headers, None).target().name, "openai");
}

#[test]
fn route_table_selects_target_from_traffic_route() {
    let mut anthropic = target(ANTHROPIC_PROVIDER);
    anthropic.name = "anthropic".to_string();
    anthropic.upstream = "https://api.anthropic.com".to_string();
    let mut chatgpt = target(OPENAI_PROVIDER);
    chatgpt.name = "chatgpt-codex".to_string();
    chatgpt.upstream = "https://chatgpt.com".to_string();
    let mut config = proxy_config(anthropic);
    config.targets.push(chatgpt);
    let table = RouteTable::from_proxy_config(&config).unwrap();
    let traffic_route = dam_net::TrafficRoute::new(
        dam_net::ProtocolAdapterKind::WebSocket,
        "chatgpt.com",
        OPENAI_PROVIDER,
        "chatgpt-codex",
        "https://chatgpt.com",
    );

    let decision = table.decide_for_traffic_route(&HeaderMap::new(), &traffic_route);

    assert_eq!(decision.target().name, "chatgpt-codex");
}

#[test]
fn target_requires_config_when_env_key_is_missing_and_configured_caller_auth_is_absent() {
    let mut target = target(OPENAI_PROVIDER);
    target.api_key_env = Some("OPENAI_API_KEY".to_string());
    let route = RoutePlan::from_proxy_config(&proxy_config(target)).unwrap();

    let decision = route.decide(&HeaderMap::new());

    assert_eq!(decision.auth(), RouteAuth::ConfigRequired);
    assert!(decision.config_required());
    assert!(decision.target_api_key().is_none());
}

#[test]
fn target_accepts_configured_caller_authorization_when_env_key_is_missing() {
    let mut target = target(OPENAI_PROVIDER);
    target.api_key_env = Some("OPENAI_API_KEY".to_string());
    let route = RoutePlan::from_proxy_config(&proxy_config(target)).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer caller"),
    );

    let decision = route.decide(&headers);

    assert_eq!(decision.auth(), RouteAuth::CallerPassthrough);
    assert!(!decision.config_required());
}

#[test]
fn target_api_key_wins_over_caller_auth() {
    let mut target = target(OPENAI_PROVIDER);
    target.api_key_env = Some("OPENAI_API_KEY".to_string());
    target.api_key = Some(dam_config::SecretValue::new(
        "OPENAI_API_KEY",
        "target-secret",
    ));
    let route = RoutePlan::from_proxy_config(&proxy_config(target)).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer caller"),
    );

    let decision = route.decide(&headers);

    assert_eq!(decision.auth(), RouteAuth::TargetApiKey);
    assert_eq!(decision.target_api_key(), Some("target-secret"));
}

#[test]
fn target_without_api_key_env_uses_pass_through_even_without_caller_auth() {
    let route = RoutePlan::from_proxy_config(&proxy_config(target(OPENAI_PROVIDER))).unwrap();

    let decision = route.decide(&HeaderMap::new());

    assert_eq!(decision.auth(), RouteAuth::CallerPassthrough);
    assert!(!decision.config_required());
}

#[test]
fn target_with_no_caller_auth_headers_uses_pass_through_for_missing_env_key() {
    let mut target = target(GENERIC_PROVIDER);
    target.api_key_env = Some("OPTIONAL_KEY".to_string());
    let route = RoutePlan::from_proxy_config(&proxy_config(target)).unwrap();

    let decision = route.decide(&HeaderMap::new());

    assert_eq!(decision.auth(), RouteAuth::CallerPassthrough);
    assert!(!decision.config_required());
    assert_eq!(decision.target_api_key(), None);
}

#[test]
fn anthropic_target_accepts_x_api_key_or_authorization_as_caller_auth() {
    let mut target = target(ANTHROPIC_PROVIDER);
    target.api_key_env = Some("ANTHROPIC_API_KEY".to_string());
    let route = RoutePlan::from_proxy_config(&proxy_config(target)).unwrap();

    let mut x_api_key_headers = HeaderMap::new();
    x_api_key_headers.insert("x-api-key", HeaderValue::from_static("caller"));
    assert_eq!(
        route.decide(&x_api_key_headers).auth(),
        RouteAuth::CallerPassthrough
    );

    let mut authorization_headers = HeaderMap::new();
    authorization_headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer a"));
    assert_eq!(
        route.decide(&authorization_headers).auth(),
        RouteAuth::CallerPassthrough
    );
}
