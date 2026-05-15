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
#[path = "lib_tests.rs"]
mod tests;
