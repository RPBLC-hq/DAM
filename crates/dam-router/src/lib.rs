use http::HeaderMap;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("proxy target is missing")]
    MissingTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteAuth {
    CallerPassthrough,
    TargetApiKey,
    ConfigRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlan {
    target: dam_config::ProxyTargetConfig,
    default_failure_mode: dam_config::ProxyFailureMode,
}

impl RoutePlan {
    pub fn from_proxy_config(config: &dam_config::ProxyConfig) -> Result<Self, RouteError> {
        let target = config
            .targets
            .first()
            .cloned()
            .ok_or(RouteError::MissingTarget)?;
        Self::new(target, config.default_failure_mode)
    }

    pub fn new(
        target: dam_config::ProxyTargetConfig,
        default_failure_mode: dam_config::ProxyFailureMode,
    ) -> Result<Self, RouteError> {
        Ok(Self {
            target,
            default_failure_mode,
        })
    }

    pub fn target(&self) -> &dam_config::ProxyTargetConfig {
        &self.target
    }

    pub fn failure_mode(&self) -> dam_config::ProxyFailureMode {
        self.target
            .effective_failure_mode(self.default_failure_mode)
    }

    pub fn decide<'a>(&'a self, headers: &HeaderMap) -> RouteDecision<'a> {
        let auth = if self.target.api_key.is_some() {
            RouteAuth::TargetApiKey
        } else if self.target.api_key_env.is_some()
            && !caller_auth_header_present(&self.target.auth, headers)
        {
            RouteAuth::ConfigRequired
        } else {
            RouteAuth::CallerPassthrough
        };

        RouteDecision {
            target: &self.target,
            failure_mode: self.failure_mode(),
            auth,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteTable {
    routes: Vec<RoutePlan>,
}

impl RouteTable {
    pub fn from_proxy_config(config: &dam_config::ProxyConfig) -> Result<Self, RouteError> {
        if config.targets.is_empty() {
            return Err(RouteError::MissingTarget);
        }
        let routes = config
            .targets
            .iter()
            .cloned()
            .map(|target| RoutePlan::new(target, config.default_failure_mode))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { routes })
    }

    pub fn routes(&self) -> &[RoutePlan] {
        &self.routes
    }

    pub fn first(&self) -> &RoutePlan {
        &self.routes[0]
    }

    pub fn decide<'a>(
        &'a self,
        headers: &HeaderMap,
        _uri: Option<&http::Uri>,
    ) -> RouteDecision<'a> {
        self.first().decide(headers)
    }

    pub fn decide_for_traffic_route<'a>(
        &'a self,
        headers: &HeaderMap,
        traffic_route: &dam_net::TrafficRoute,
    ) -> RouteDecision<'a> {
        let route = self
            .routes
            .iter()
            .find(|route| {
                route.target.provider == traffic_route.provider
                    && target_matches(route.target(), traffic_route)
            })
            .or_else(|| {
                self.routes
                    .iter()
                    .find(|route| route.target.provider == traffic_route.provider)
            })
            .unwrap_or_else(|| self.first());
        route.decide(headers)
    }
}

fn target_matches(
    target: &dam_config::ProxyTargetConfig,
    traffic_route: &dam_net::TrafficRoute,
) -> bool {
    target.name == traffic_route.target_name
        || normalize_host(&target.upstream) == normalize_host(&traffic_route.upstream)
}

fn normalize_host(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn caller_auth_header_present(auth: &dam_net::UpstreamAuthConfig, headers: &HeaderMap) -> bool {
    if auth.caller_headers.is_empty() {
        return true;
    }
    auth.caller_headers
        .iter()
        .map(|header| header.trim())
        .filter(|header| !header.is_empty())
        .any(|header| headers.contains_key(header))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteDecision<'a> {
    target: &'a dam_config::ProxyTargetConfig,
    failure_mode: dam_config::ProxyFailureMode,
    auth: RouteAuth,
}

impl<'a> RouteDecision<'a> {
    pub fn target(self) -> &'a dam_config::ProxyTargetConfig {
        self.target
    }

    pub fn failure_mode(self) -> dam_config::ProxyFailureMode {
        self.failure_mode
    }

    pub fn auth(self) -> RouteAuth {
        self.auth
    }

    pub fn config_required(self) -> bool {
        self.auth == RouteAuth::ConfigRequired
    }

    pub fn target_api_key(self) -> Option<&'a str> {
        match self.auth {
            RouteAuth::TargetApiKey => self.target.api_key.as_ref().map(|key| key.expose()),
            RouteAuth::CallerPassthrough | RouteAuth::ConfigRequired => None,
        }
    }
}

#[cfg(test)]
mod tests {
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
}
