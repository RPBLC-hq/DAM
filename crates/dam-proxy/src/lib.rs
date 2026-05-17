use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use dam_core::{
    EventSink, LogEventType, LogLevel, VaultReadError, VaultReader, VaultRecord, VaultWriter,
};
use http_body_util::BodyExt;
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    future::Future,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex, Once, RwLock},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{
    TlsAcceptor, TlsConnector,
    rustls::{
        ClientConfig, RootCertStore, ServerConfig,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
    },
};

mod events;
mod providers;
mod websocket;

use events::{log_intercepted_response_write, log_provider_response, record_proxy_event};
use providers::ProviderAdapters;

const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;
const MAX_INTERCEPTED_HEADER_BYTES: usize = 64 * 1024;
const PASSTHROUGH_RESUME_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectBypassReason {
    UnmatchedRoute,
    ProtectionPaused,
}

impl ConnectBypassReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::UnmatchedRoute => "unmatched_route",
            Self::ProtectionPaused => "protection_paused",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("proxy is disabled")]
    Disabled,

    #[error("proxy target is missing")]
    MissingTarget,

    #[error("invalid proxy listen address {addr}: {source}")]
    InvalidListen {
        addr: String,
        source: std::net::AddrParseError,
    },

    #[error("proxy listen address must be loopback: {0}")]
    NonLoopbackListen(SocketAddr),

    #[error("failed to bind proxy listener {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        source: std::io::Error,
    },

    #[error("proxy server failed: {0}")]
    Server(std::io::Error),

    #[error("failed to initialize provider: {0}")]
    ProviderInit(String),

    #[error("vault backend is unavailable and fail-closed is configured: {0}")]
    VaultUnavailable(String),

    #[error("log backend is unavailable and fail-closed is configured: {0}")]
    LogUnavailable(String),

    #[error("consent backend is unavailable: {0}")]
    ConsentUnavailable(String),
}

pub struct ProxyState {
    routes: dam_router::RouteTable,
    resolve_inbound: bool,
    route_resolve_inbound: HashMap<String, bool>,
    route_protect_inbound: HashMap<String, bool>,
    vault: Arc<dyn ProxyVault>,
    consent_store: Option<Arc<dam_consent::ConsentStore>>,
    log_sink: Option<Arc<dyn EventSink>>,
    policy: dam_policy::StaticPolicy,
    replacement_options: dam_core::ReplacementPlanOptions,
    providers: ProviderAdapters,
    transparent_interception: Option<TransparentInterceptionConfig>,
    tls_acceptor_cache: Mutex<HashMap<String, Arc<ServerConfig>>>,
}

#[derive(Clone)]
pub struct TransparentInterceptionConfig {
    pub state_dir: PathBuf,
    pub network_mode: dam_net::CaptureMode,
    pub system_proxy_active: bool,
    pub tun_active: bool,
    pub routes: Vec<dam_net::TrafficRoute>,
    pub trust: dam_trust::TrustState,
    pub user_consented: bool,
    pub protection_control_path: Option<PathBuf>,
}

impl From<dam_router::RouteError> for ProxyError {
    fn from(error: dam_router::RouteError) -> Self {
        match error {
            dam_router::RouteError::MissingTarget => Self::MissingTarget,
        }
    }
}

trait ProxyVault: VaultWriter + VaultReader {}

impl<T> ProxyVault for T where T: VaultWriter + VaultReader {}

impl ProxyState {
    fn protection_enabled(&self) -> bool {
        self.transparent_interception
            .as_ref()
            .and_then(|config| config.protection_control_path.as_ref())
            .map(protection_control_enabled)
            .unwrap_or(true)
    }
}

struct FailingVault {
    message: String,
}

impl VaultWriter for FailingVault {
    fn write_with_options(
        &self,
        _record: &VaultRecord,
        _options: dam_core::VaultWriteOptions,
    ) -> Result<dam_core::Reference, dam_core::VaultWriteError> {
        Err(dam_core::VaultWriteError::new(self.message.clone()))
    }
}

impl VaultReader for FailingVault {
    fn read(&self, _reference: &dam_core::Reference) -> Result<Option<String>, VaultReadError> {
        Err(VaultReadError::new(self.message.clone()))
    }
}

pub async fn run(config: dam_config::DamConfig) -> Result<(), ProxyError> {
    let addr: SocketAddr =
        config
            .proxy
            .listen
            .parse()
            .map_err(|source| ProxyError::InvalidListen {
                addr: config.proxy.listen.clone(),
                source,
            })?;
    if !addr.ip().is_loopback() {
        return Err(ProxyError::NonLoopbackListen(addr));
    }
    let app = build_app(config)?;
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| ProxyError::Bind { addr, source })?;

    axum::serve(listener, app).await.map_err(ProxyError::Server)
}

pub fn build_app(config: dam_config::DamConfig) -> Result<Router, ProxyError> {
    build_app_with_interception(config, None)
}

pub fn build_app_with_interception(
    config: dam_config::DamConfig,
    transparent_interception: Option<TransparentInterceptionConfig>,
) -> Result<Router, ProxyError> {
    let state = build_state(config, transparent_interception)?;

    Ok(Router::new()
        .route("/health", get(health))
        .fallback(proxy)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state))
}

fn build_state(
    config: dam_config::DamConfig,
    transparent_interception: Option<TransparentInterceptionConfig>,
) -> Result<Arc<ProxyState>, ProxyError> {
    if !config.proxy.enabled {
        return Err(ProxyError::Disabled);
    }

    let routes = dam_router::RouteTable::from_proxy_config(&config.proxy)?;
    let providers = ProviderAdapters::new()?;

    let replacement_options = dam_core::ReplacementPlanOptions {
        deduplicate_replacements: config.policy.deduplicate_replacements,
    };

    Ok(Arc::new(ProxyState {
        routes,
        resolve_inbound: config.proxy.resolve_inbound,
        route_resolve_inbound: route_resolve_inbound(&config.traffic.effective_profile()),
        route_protect_inbound: route_protect_inbound(&config.traffic.effective_profile()),
        vault: open_vault(&config)?,
        consent_store: open_consent_store(&config)?,
        log_sink: open_log_sink(&config)?,
        policy: dam_policy::StaticPolicy::from(config.policy),
        replacement_options,
        providers,
        transparent_interception,
        tls_acceptor_cache: Mutex::new(HashMap::new()),
    }))
}

pub async fn serve_transparent_with_shutdown<F>(
    listener: TcpListener,
    config: dam_config::DamConfig,
    transparent_interception: TransparentInterceptionConfig,
    shutdown: F,
) -> Result<(), ProxyError>
where
    F: Future<Output = ()> + Send,
{
    let addr = listener.local_addr().map_err(ProxyError::Server)?;
    if !addr.ip().is_loopback() {
        return Err(ProxyError::NonLoopbackListen(addr));
    }
    let state = build_state(config, Some(transparent_interception))?;
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            accepted = listener.accept() => {
                let (stream, _) = accepted.map_err(ProxyError::Server)?;
                let state = state.clone();
                tokio::spawn(async move {
                    let _ = handle_raw_proxy_connection(state, stream).await;
                });
            }
        }
    }
}

fn open_vault(config: &dam_config::DamConfig) -> Result<Arc<dyn ProxyVault>, ProxyError> {
    match config.vault.backend {
        dam_config::VaultBackend::Sqlite => match dam_vault::Vault::open(&config.vault.sqlite_path)
        {
            Ok(vault) => Ok(Arc::new(vault)),
            Err(error)
                if config.failure.vault_write == dam_config::VaultWriteFailureMode::RedactOnly =>
            {
                Ok(Arc::new(FailingVault {
                    message: error.to_string(),
                }))
            }
            Err(error) => Err(ProxyError::VaultUnavailable(error.to_string())),
        },
        dam_config::VaultBackend::Remote
            if config.failure.vault_write == dam_config::VaultWriteFailureMode::RedactOnly =>
        {
            Ok(Arc::new(FailingVault {
                message: "remote vault backend is not implemented".to_string(),
            }))
        }
        dam_config::VaultBackend::Remote => Err(ProxyError::VaultUnavailable(
            "remote vault backend is not implemented".to_string(),
        )),
    }
}

fn open_consent_store(
    config: &dam_config::DamConfig,
) -> Result<Option<Arc<dam_consent::ConsentStore>>, ProxyError> {
    if !config.consent.enabled {
        return Ok(None);
    }

    match config.consent.backend {
        dam_config::ConsentBackend::Sqlite => {
            dam_consent::ConsentStore::open(&config.consent.sqlite_path)
                .map(Arc::new)
                .map(Some)
                .map_err(|error| ProxyError::ConsentUnavailable(error.to_string()))
        }
    }
}

fn open_log_sink(config: &dam_config::DamConfig) -> Result<Option<Arc<dyn EventSink>>, ProxyError> {
    if !config.log.enabled || config.log.backend == dam_config::LogBackend::None {
        return Ok(None);
    }

    match config.log.backend {
        dam_config::LogBackend::Sqlite => match dam_log::LogStore::open(&config.log.sqlite_path) {
            Ok(store) => Ok(Some(Arc::new(store))),
            Err(_) if config.failure.log_write == dam_config::LogWriteFailureMode::WarnContinue => {
                Ok(None)
            }
            Err(error) => Err(ProxyError::LogUnavailable(error.to_string())),
        },
        dam_config::LogBackend::Remote
            if config.failure.log_write == dam_config::LogWriteFailureMode::WarnContinue =>
        {
            Ok(None)
        }
        dam_config::LogBackend::Remote => Err(ProxyError::LogUnavailable(
            "remote log backend is not implemented".to_string(),
        )),
        dam_config::LogBackend::None => Ok(None),
    }
}

async fn health(State(state): State<Arc<ProxyState>>) -> Response {
    health_response(&state)
}

fn health_response(state: &ProxyState) -> Response {
    let route = state.routes.decide(&HeaderMap::new(), None);
    let (proxy_state, message) = if !state.protection_enabled() {
        (
            dam_api::ProxyState::Bypassing,
            "protection is paused; traffic is passed through".to_string(),
        )
    } else if route.config_required() {
        (
            dam_api::ProxyState::ConfigRequired,
            "target API key is missing".to_string(),
        )
    } else {
        (dam_api::ProxyState::Protected, "proxy is ready".to_string())
    };

    status_response(StatusCode::OK, proxy_state, message, None, route.target())
}

async fn handle_raw_proxy_connection(
    state: Arc<ProxyState>,
    mut stream: TcpStream,
) -> Result<(), String> {
    let operation_id = dam_core::generate_operation_id();
    let request = match read_intercepted_http_request(&mut stream).await {
        Ok(Some(request)) => request,
        Ok(None) => return Ok(()),
        Err(error) => {
            write_intercepted_error(&mut stream, StatusCode::BAD_REQUEST, &error).await?;
            return Err(error);
        }
    };

    if request.method == Method::CONNECT {
        return handle_raw_connect_request(state, operation_id, request, stream).await;
    }

    if request.method == Method::GET && request.uri.path() == "/health" {
        let response = health_response(&state);
        return write_intercepted_http_response(&mut stream, response).await;
    }

    if is_forward_proxy_http_request(&request.uri)
        && !should_protect_forward_proxy_http_request(&state, &request)
    {
        return handle_raw_http_pass_through(state, operation_id, request, stream).await;
    }

    let response = proxy_http_request(
        state.clone(),
        request.method,
        request.uri,
        request.headers,
        request.body,
        operation_id.clone(),
    )
    .await;
    log_intercepted_response_write(&state, &operation_id, &response);
    write_intercepted_http_response(&mut stream, response).await
}

async fn handle_raw_connect_request(
    state: Arc<ProxyState>,
    operation_id: String,
    request: InterceptedHttpRequest,
    mut stream: TcpStream,
) -> Result<(), String> {
    let route = state.routes.decide(&request.headers, Some(&request.uri));
    let Some(authority) = connect_authority(&request.uri, &request.headers) else {
        let response = connect_blocked_response(
            &state,
            route,
            &operation_id,
            StatusCode::BAD_REQUEST,
            "CONNECT target host is missing",
        );
        write_intercepted_http_response(&mut stream, response).await?;
        return Ok(());
    };

    let Some(interception) = state.transparent_interception.clone() else {
        let response = connect_blocked_response(
            &state,
            route,
            &operation_id,
            StatusCode::NOT_IMPLEMENTED,
            "transparent CONNECT traffic requires the TLS interception runtime",
        );
        write_intercepted_http_response(&mut stream, response).await?;
        return Ok(());
    };
    let traffic_route =
        dam_net::classify_traffic_host_with_routes(&authority.host, &interception.routes);
    let protection_paused = !state.protection_enabled();
    if traffic_route.is_none() || protection_paused {
        let bypass_reason = if traffic_route.is_some() && protection_paused {
            ConnectBypassReason::ProtectionPaused
        } else {
            ConnectBypassReason::UnmatchedRoute
        };
        return handle_raw_connect_tunnel(
            state,
            operation_id,
            authority,
            bypass_reason,
            stream,
            traffic_route.is_some() && protection_paused,
        )
        .await;
    }
    let traffic_route = traffic_route.unwrap();
    let route = state
        .routes
        .decide_for_traffic_route(&request.headers, &traffic_route);
    if !route_matches_traffic_target(route, &traffic_route) {
        let response = connect_blocked_response(
            &state,
            route,
            &operation_id,
            StatusCode::FORBIDDEN,
            "CONNECT target does not match the configured proxy target",
        );
        write_intercepted_http_response(&mut stream, response).await?;
        return Ok(());
    }

    let readiness = transparent_interception_readiness(&interception, traffic_route);
    if readiness.readiness != dam_intercept::TlsInterceptionReadiness::Ready {
        record_proxy_event(
            &state,
            &operation_id,
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "blocked",
            "transparent TLS interception is not ready",
        );
        let response = status_response(
            StatusCode::SERVICE_UNAVAILABLE,
            dam_api::ProxyState::Blocked,
            readiness.message,
            Some(operation_id),
            route.target(),
        );
        write_intercepted_http_response(&mut stream, response).await?;
        return Ok(());
    }

    let acceptor = match tls_acceptor_for_host(&state, &interception, &authority.host) {
        Ok(acceptor) => acceptor,
        Err(message) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Error,
                LogEventType::ProxyFailure,
                "blocked",
                "failed to prepare transparent TLS interception",
            );
            let response = status_response(
                StatusCode::BAD_GATEWAY,
                dam_api::ProxyState::Blocked,
                message,
                Some(operation_id),
                route.target(),
            );
            write_intercepted_http_response(&mut stream, response).await?;
            return Ok(());
        }
    };

    handle_intercepted_tls_io(state, &operation_id, stream, acceptor, true).await
}

async fn proxy(State(state): State<Arc<ProxyState>>, mut request: Request) -> Response {
    let operation_id = dam_core::generate_operation_id();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    let route = route_for_request(&state, &headers, &uri);

    if method == Method::CONNECT {
        return handle_connect_request(
            state.clone(),
            route,
            operation_id,
            &uri,
            &headers,
            &mut request,
        )
        .await;
    }

    let body = match to_bytes(request.into_body(), MAX_REQUEST_BYTES).await {
        Ok(body) => body,
        Err(_) => {
            return handle_protection_failure(
                state.clone(),
                route,
                operation_id,
                "request body exceeds the supported size",
            );
        }
    };

    proxy_http_request(state, method, uri, headers, body, operation_id).await
}

async fn handle_connect_request(
    state: Arc<ProxyState>,
    route: dam_router::RouteDecision<'_>,
    operation_id: String,
    uri: &Uri,
    headers: &HeaderMap,
    request: &mut Request,
) -> Response {
    let Some(authority) = connect_authority(uri, headers) else {
        return connect_blocked_response(
            &state,
            route,
            &operation_id,
            StatusCode::BAD_REQUEST,
            "CONNECT target host is missing",
        );
    };

    let Some(interception) = state.transparent_interception.clone() else {
        return connect_blocked_response(
            &state,
            route,
            &operation_id,
            StatusCode::NOT_IMPLEMENTED,
            "transparent CONNECT traffic requires the TLS interception runtime",
        );
    };
    let traffic_route =
        dam_net::classify_traffic_host_with_routes(&authority.host, &interception.routes);
    let protection_paused = !state.protection_enabled();
    if traffic_route.is_none() || protection_paused {
        let bypass_reason = if traffic_route.is_some() && protection_paused {
            ConnectBypassReason::ProtectionPaused
        } else {
            ConnectBypassReason::UnmatchedRoute
        };
        return handle_connect_tunnel_request(
            state,
            route,
            operation_id,
            authority,
            bypass_reason,
            request,
            traffic_route.is_some() && protection_paused,
        )
        .await;
    }
    let traffic_route = traffic_route.unwrap();

    let route = state
        .routes
        .decide_for_traffic_route(headers, &traffic_route);
    if !route_matches_traffic_target(route, &traffic_route) {
        return connect_blocked_response(
            &state,
            route,
            &operation_id,
            StatusCode::FORBIDDEN,
            "CONNECT target does not match the configured proxy target",
        );
    }

    let readiness = transparent_interception_readiness(&interception, traffic_route);
    if readiness.readiness != dam_intercept::TlsInterceptionReadiness::Ready {
        record_proxy_event(
            &state,
            &operation_id,
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "blocked",
            "transparent TLS interception is not ready",
        );
        return status_response(
            StatusCode::SERVICE_UNAVAILABLE,
            dam_api::ProxyState::Blocked,
            readiness.message,
            Some(operation_id),
            route.target(),
        );
    }

    let acceptor = match tls_acceptor_for_host(&state, &interception, &authority.host) {
        Ok(acceptor) => acceptor,
        Err(message) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Error,
                LogEventType::ProxyFailure,
                "blocked",
                "failed to prepare transparent TLS interception",
            );
            return status_response(
                StatusCode::BAD_GATEWAY,
                dam_api::ProxyState::Blocked,
                message,
                Some(operation_id),
                route.target(),
            );
        }
    };

    if request
        .extensions()
        .get::<hyper::upgrade::OnUpgrade>()
        .is_none()
    {
        record_proxy_event(
            &state,
            &operation_id,
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "blocked",
            "CONNECT request cannot be upgraded",
        );
        return status_response(
            StatusCode::BAD_GATEWAY,
            dam_api::ProxyState::Blocked,
            "CONNECT request cannot be upgraded".to_string(),
            Some(operation_id),
            route.target(),
        );
    }

    let upgrade = hyper::upgrade::on(request);
    tokio::spawn(handle_upgraded_connect(
        state,
        operation_id,
        upgrade,
        acceptor,
    ));

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::OK.into_response())
}

fn connect_blocked_response(
    state: &ProxyState,
    route: dam_router::RouteDecision<'_>,
    operation_id: &str,
    status: StatusCode,
    message: &'static str,
) -> Response {
    record_proxy_event(
        state,
        operation_id,
        LogLevel::Error,
        LogEventType::ProxyFailure,
        "blocked",
        message,
    );
    status_response(
        status,
        dam_api::ProxyState::Blocked,
        message.to_string(),
        Some(operation_id.to_string()),
        route.target(),
    )
}

async fn handle_connect_tunnel_request(
    state: Arc<ProxyState>,
    route: dam_router::RouteDecision<'_>,
    operation_id: String,
    authority: TargetAuthority,
    bypass_reason: ConnectBypassReason,
    request: &mut Request,
    close_on_protection_resume: bool,
) -> Response {
    if request
        .extensions()
        .get::<hyper::upgrade::OnUpgrade>()
        .is_none()
    {
        record_proxy_event(
            &state,
            &operation_id,
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "blocked",
            "CONNECT request cannot be upgraded",
        );
        return status_response(
            StatusCode::BAD_GATEWAY,
            dam_api::ProxyState::Blocked,
            "CONNECT request cannot be upgraded".to_string(),
            Some(operation_id),
            route.target(),
        );
    }

    let upstream = match connect_target(&authority).await {
        Ok(upstream) => upstream,
        Err(error) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Error,
                LogEventType::ProxyFailure,
                "provider_down",
                "CONNECT passthrough target is unavailable",
            );
            return status_response(
                StatusCode::BAD_GATEWAY,
                dam_api::ProxyState::ProviderDown,
                error,
                Some(operation_id),
                route.target(),
            );
        }
    };

    let upgrade = hyper::upgrade::on(request);
    tokio::spawn(handle_upgraded_tunnel(
        state,
        operation_id,
        authority,
        bypass_reason,
        upgrade,
        upstream,
        close_on_protection_resume,
    ));

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::OK.into_response())
}

async fn handle_raw_connect_tunnel(
    state: Arc<ProxyState>,
    operation_id: String,
    authority: TargetAuthority,
    bypass_reason: ConnectBypassReason,
    mut stream: TcpStream,
    close_on_protection_resume: bool,
) -> Result<(), String> {
    let mut upstream = match connect_target(&authority).await {
        Ok(upstream) => upstream,
        Err(error) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Error,
                LogEventType::ProxyFailure,
                "provider_down",
                "CONNECT passthrough target is unavailable",
            );
            write_intercepted_error(&mut stream, StatusCode::BAD_GATEWAY, &error).await?;
            return Ok(());
        }
    };

    stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .map_err(|error| format!("failed to acknowledge CONNECT tunnel: {error}"))?;
    stream
        .flush()
        .await
        .map_err(|error| format!("failed to flush CONNECT tunnel: {error}"))?;
    record_proxy_event(
        &state,
        &operation_id,
        LogLevel::Info,
        LogEventType::ProxyBypass,
        "bypassing",
        format!(
            "CONNECT tunnel passed through without inspection target={}:{} reason={}",
            authority.host,
            authority.port,
            bypass_reason.as_str()
        ),
    );
    match copy_passthrough_tunnel(
        &state,
        &operation_id,
        &mut stream,
        &mut upstream,
        close_on_protection_resume,
    )
    .await
    {
        Ok(PassthroughTunnelOutcome::Completed) => Ok(()),
        Ok(PassthroughTunnelOutcome::ClosedOnProtectionResume) => Ok(()),
        Err(error) => Err(format!("CONNECT passthrough failed: {error}")),
    }
}

async fn handle_raw_http_pass_through(
    state: Arc<ProxyState>,
    operation_id: String,
    request: InterceptedHttpRequest,
    mut stream: TcpStream,
) -> Result<(), String> {
    let Some(authority) = http_authority(&request.uri, &request.headers) else {
        write_intercepted_error(
            &mut stream,
            StatusCode::BAD_REQUEST,
            "HTTP proxy target host is missing",
        )
        .await?;
        return Ok(());
    };
    let mut upstream = match connect_target(&authority).await {
        Ok(upstream) => upstream,
        Err(error) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Error,
                LogEventType::ProxyFailure,
                "provider_down",
                "HTTP passthrough target is unavailable",
            );
            write_intercepted_error(&mut stream, StatusCode::BAD_GATEWAY, &error).await?;
            return Ok(());
        }
    };

    write_forward_proxy_request(&mut upstream, &request, &authority).await?;
    record_proxy_event(
        &state,
        &operation_id,
        LogLevel::Info,
        LogEventType::ProxyBypass,
        "bypassing",
        "HTTP request passed through without inspection",
    );
    tokio::io::copy(&mut upstream, &mut stream)
        .await
        .map(|_| ())
        .map_err(|error| format!("HTTP passthrough failed: {error}"))
}

async fn proxy_http_request(
    state: Arc<ProxyState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
    operation_id: String,
) -> Response {
    let route = route_for_request(&state, &headers, &uri);
    let protection_enabled = state.protection_enabled();
    let inbound_plan = InboundTransformPlan {
        resolve_references: protection_enabled && state.resolve_inbound_for_route(route),
        protect_sensitive_data: protection_enabled && state.protect_inbound_for_route(route),
    };
    let consent_scopes = Arc::new(consent_scopes_for_target(route.target()));
    record_proxy_event(
        &state,
        &operation_id,
        LogLevel::Info,
        LogEventType::ProxyForward,
        "route_decision",
        format!(
            "route target={} provider={} method={} path={} protection_enabled={} resolve_inbound={} protect_inbound={} request_bytes={}",
            route.target().name,
            route.target().provider,
            method,
            uri.path(),
            protection_enabled,
            inbound_plan.resolve_references,
            inbound_plan.protect_sensitive_data,
            body.len()
        ),
    );

    if route.config_required() {
        record_proxy_event(
            &state,
            &operation_id,
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "config_required",
            "proxy target API key is missing",
        );
        return status_response(
            StatusCode::SERVICE_UNAVAILABLE,
            dam_api::ProxyState::ConfigRequired,
            "proxy target API key is missing".to_string(),
            Some(operation_id),
            route.target(),
        );
    }

    if !protection_enabled {
        return forward_or_provider_down(
            state.clone(),
            route,
            ForwardAttempt {
                method,
                uri,
                headers,
                body,
                operation_id,
                action: "bypassing",
                related_domains: Arc::new(Vec::new()),
                consent_scopes,
                inbound_plan,
            },
        )
        .await;
    }

    if request_has_unsupported_content_encoding(&headers) {
        record_proxy_event(
            &state,
            &operation_id,
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "blocked",
            "encoded request bodies are not supported",
        );
        return status_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            dam_api::ProxyState::Blocked,
            "encoded request bodies are not supported".to_string(),
            Some(operation_id),
            route.target(),
        );
    }

    let body_text = match std::str::from_utf8(&body) {
        Ok(text) => text,
        Err(_) => {
            return handle_protection_failure(
                state.clone(),
                route,
                operation_id,
                "request body is not utf-8",
            );
        }
    };

    let protected = match dam_pipeline::protect_text(
        body_text,
        &operation_id,
        &state.policy,
        state.vault.as_ref(),
        dam_pipeline::ProtectTextContext {
            reference_vault: Some(state.vault.as_ref()),
            consent_store: state.consent_store.as_deref(),
            consent_scopes: consent_scopes.as_slice(),
            event_sink: state.log_sink.as_deref(),
            ..dam_pipeline::ProtectTextContext::default()
        },
        state.replacement_options,
    ) {
        Ok(result) => result,
        Err(_) => {
            return handle_protection_failure(
                state.clone(),
                route,
                operation_id,
                "request protection failed",
            );
        }
    };
    record_proxy_event(
        &state,
        &operation_id,
        LogLevel::Info,
        LogEventType::ProxyForward,
        "request_protection",
        format!(
            "request protection detections={} replacements={} tokenized={} blocked={}",
            protected.detections.len(),
            protected.plan.replacements.len(),
            protected.plan.tokenized_count(),
            protected.plan.blocked_count()
        ),
    );

    if protected.is_blocked() {
        record_proxy_event(
            &state,
            &operation_id,
            LogLevel::Warn,
            LogEventType::ProxyFailure,
            "blocked",
            "proxy request blocked by policy",
        );
        return status_response(
            StatusCode::FORBIDDEN,
            dam_api::ProxyState::Blocked,
            "proxy request blocked by policy".to_string(),
            Some(operation_id),
            route.target(),
        );
    }

    let Some(protected_body) = protected.output else {
        return handle_protection_failure(
            state.clone(),
            route,
            operation_id,
            "request protection did not produce output",
        );
    };
    let related_domains = Arc::new(related_domains_from_detections(&protected.detections));
    let body_changed = protected_body.as_str() != body_text;
    let mut protected_headers = headers;
    if body_changed {
        let removed = strip_body_integrity_headers(&mut protected_headers);
        if removed > 0 {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Info,
                LogEventType::ProxyForward,
                "request_integrity_headers_removed",
                format!("removed body integrity headers count={removed}"),
            );
        }
    }
    forward_or_provider_down(
        state.clone(),
        route,
        ForwardAttempt {
            method,
            uri,
            headers: protected_headers,
            body: Bytes::from(protected_body),
            operation_id,
            action: "protected",
            related_domains,
            consent_scopes,
            inbound_plan,
        },
    )
    .await
}

fn related_domains_from_detections(detections: &[dam_core::Detection]) -> Vec<String> {
    let mut domains = BTreeSet::new();
    for detection in detections
        .iter()
        .filter(|detection| detection.kind == dam_core::SensitiveType::Email)
    {
        let email =
            dam_core::canonical_sensitive_value(dam_core::SensitiveType::Email, &detection.value);
        let Some((_, domain)) = email.rsplit_once('@') else {
            continue;
        };
        let domain = dam_core::canonical_sensitive_value(dam_core::SensitiveType::Domain, domain);
        if domain.contains('.') {
            domains.insert(domain);
        }
    }
    domains.into_iter().collect()
}

async fn handle_upgraded_connect(
    state: Arc<ProxyState>,
    operation_id: String,
    upgrade: hyper::upgrade::OnUpgrade,
    acceptor: TlsAcceptor,
) {
    let upgraded = match upgrade.await {
        Ok(upgraded) => upgraded,
        Err(_) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Error,
                LogEventType::ProxyFailure,
                "blocked",
                "CONNECT upgrade failed",
            );
            return;
        }
    };

    if let Err(error) =
        handle_intercepted_tls_connection(state.clone(), &operation_id, upgraded, acceptor).await
    {
        record_proxy_event(
            &state,
            &operation_id,
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "blocked",
            "intercepted TLS request failed",
        );
        let _ = error;
    }
}

async fn handle_upgraded_tunnel(
    state: Arc<ProxyState>,
    operation_id: String,
    authority: TargetAuthority,
    bypass_reason: ConnectBypassReason,
    upgrade: hyper::upgrade::OnUpgrade,
    mut upstream: TcpStream,
    close_on_protection_resume: bool,
) {
    let upgraded = match upgrade.await {
        Ok(upgraded) => upgraded,
        Err(_) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Error,
                LogEventType::ProxyFailure,
                "blocked",
                "CONNECT upgrade failed",
            );
            return;
        }
    };
    let mut client = TokioIo::new(upgraded);
    record_proxy_event(
        &state,
        &operation_id,
        LogLevel::Info,
        LogEventType::ProxyBypass,
        "bypassing",
        format!(
            "CONNECT tunnel passed through without inspection target={}:{} reason={}",
            authority.host,
            authority.port,
            bypass_reason.as_str()
        ),
    );
    match copy_passthrough_tunnel(
        &state,
        &operation_id,
        &mut client,
        &mut upstream,
        close_on_protection_resume,
    )
    .await
    {
        Ok(PassthroughTunnelOutcome::Completed)
        | Ok(PassthroughTunnelOutcome::ClosedOnProtectionResume) => {}
        Err(_) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Warn,
                LogEventType::ProxyFailure,
                "bypassing",
                "CONNECT passthrough ended with an I/O error",
            );
        }
    }
}

enum PassthroughTunnelOutcome {
    Completed,
    ClosedOnProtectionResume,
}

async fn copy_passthrough_tunnel<C, U>(
    state: &ProxyState,
    operation_id: &str,
    client: &mut C,
    upstream: &mut U,
    close_on_protection_resume: bool,
) -> Result<PassthroughTunnelOutcome, std::io::Error>
where
    C: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    if !close_on_protection_resume {
        tokio::io::copy_bidirectional(client, upstream).await?;
        return Ok(PassthroughTunnelOutcome::Completed);
    }

    let copy = tokio::io::copy_bidirectional(client, upstream);
    tokio::pin!(copy);
    let mut interval = tokio::time::interval(PASSTHROUGH_RESUME_POLL_INTERVAL);
    loop {
        tokio::select! {
            result = &mut copy => {
                result?;
                return Ok(PassthroughTunnelOutcome::Completed);
            }
            _ = interval.tick() => {
                if state.protection_enabled() {
                    record_proxy_event(
                        state,
                        operation_id,
                        LogLevel::Info,
                        LogEventType::ProxyBypass,
                        "bypassing",
                        "paused AI CONNECT tunnel closed because protection resumed",
                    );
                    return Ok(PassthroughTunnelOutcome::ClosedOnProtectionResume);
                }
            }
        }
    }
}

async fn handle_intercepted_tls_connection(
    state: Arc<ProxyState>,
    operation_id: &str,
    upgraded: Upgraded,
    acceptor: TlsAcceptor,
) -> Result<(), String> {
    handle_intercepted_tls_io(state, operation_id, TokioIo::new(upgraded), acceptor, true).await
}

async fn handle_intercepted_tls_io<T>(
    state: Arc<ProxyState>,
    operation_id: &str,
    mut io: T,
    acceptor: TlsAcceptor,
    acknowledge_connect: bool,
) -> Result<(), String>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    if acknowledge_connect {
        io.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .map_err(|error| format!("failed to acknowledge CONNECT tunnel: {error}"))?;
        io.flush()
            .await
            .map_err(|error| format!("failed to flush CONNECT tunnel: {error}"))?;
    }
    let mut tls = acceptor
        .accept(io)
        .await
        .map_err(|error| format!("TLS handshake failed: {error}"))?;

    let request = match read_intercepted_http_request(&mut tls).await {
        Ok(Some(request)) => request,
        Ok(None) => return Ok(()),
        Err(error) => {
            write_intercepted_error(&mut tls, StatusCode::BAD_REQUEST, &error).await?;
            return Err(error);
        }
    };

    if websocket::is_upgrade_request(&request.method, &request.headers) {
        return handle_intercepted_websocket(state, operation_id, request, tls).await;
    }

    let response = proxy_http_request(
        state.clone(),
        request.method,
        request.uri,
        request.headers,
        request.body,
        operation_id.to_string(),
    )
    .await;

    log_intercepted_response_write(&state, operation_id, &response);
    if let Err(error) = write_intercepted_http_response(&mut tls, response).await {
        let _ = write_intercepted_error(&mut tls, StatusCode::BAD_GATEWAY, &error).await;
        return Err(error);
    }
    let _ = tls.shutdown().await;
    Ok(())
}

async fn handle_intercepted_websocket<T>(
    state: Arc<ProxyState>,
    operation_id: &str,
    request: InterceptedHttpRequest,
    mut client_tls: tokio_rustls::server::TlsStream<T>,
) -> Result<(), String>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let route = route_for_request(&state, &request.headers, &request.uri);
    let protection_enabled = state.protection_enabled();
    let consent_scopes = Arc::new(consent_scopes_for_target(route.target()));
    if route.config_required() {
        record_proxy_event(
            &state,
            operation_id,
            LogLevel::Error,
            LogEventType::ProxyFailure,
            "config_required",
            "WebSocket target API key is missing",
        );
        write_intercepted_error(
            &mut client_tls,
            StatusCode::SERVICE_UNAVAILABLE,
            "WebSocket target API key is missing",
        )
        .await?;
        return Ok(());
    }

    let Some(authority) = https_authority(&request.uri, &request.headers) else {
        write_intercepted_error(
            &mut client_tls,
            StatusCode::BAD_REQUEST,
            "WebSocket target host is missing",
        )
        .await?;
        return Ok(());
    };
    let upstream_tcp = connect_target(&authority).await?;
    let connector = upstream_tls_connector();
    let server_name = ServerName::try_from(authority.host.clone())
        .map_err(|_| "WebSocket target host is not a valid TLS server name".to_string())?;
    let mut upstream_tls = connector
        .connect(server_name, upstream_tcp)
        .await
        .map_err(|error| format!("WebSocket upstream TLS handshake failed: {error}"))?;

    write_websocket_upgrade_request(&mut upstream_tls, &request, &authority).await?;
    let upstream_head = read_intercepted_response_head(&mut upstream_tls).await?;
    if !websocket::response_is_switching_protocols(&upstream_head)? {
        write_intercepted_error(
            &mut client_tls,
            StatusCode::BAD_GATEWAY,
            "WebSocket upstream did not switch protocols",
        )
        .await?;
        return Ok(());
    }
    let response_head = websocket::filter_response_header_bytes(&upstream_head)?;
    client_tls
        .write_all(&response_head)
        .await
        .map_err(|error| format!("failed to write WebSocket upgrade response: {error}"))?;
    client_tls
        .flush()
        .await
        .map_err(|error| format!("failed to flush WebSocket upgrade response: {error}"))?;

    record_proxy_event(
        &state,
        operation_id,
        LogLevel::Info,
        LogEventType::ProxyForward,
        if protection_enabled {
            "protected"
        } else {
            "bypassing"
        },
        if protection_enabled {
            "WebSocket tunnel established with connection protection snapshot enabled"
        } else {
            "WebSocket tunnel established with connection protection snapshot disabled"
        },
    );

    proxy_websocket_frames(
        state,
        operation_id,
        client_tls,
        upstream_tls,
        protection_enabled,
        consent_scopes,
    )
    .await
}

async fn proxy_websocket_frames<C, U>(
    state: Arc<ProxyState>,
    operation_id: &str,
    client_tls: C,
    upstream_tls: U,
    protection_enabled: bool,
    consent_scopes: Arc<Vec<String>>,
) -> Result<(), String>
where
    C: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    let (mut client_reader, mut client_writer) = tokio::io::split(client_tls);
    let (mut upstream_reader, mut upstream_writer) = tokio::io::split(upstream_tls);
    let related_domains = Arc::new(RwLock::new(Vec::new()));
    let outcome = {
        let client_to_upstream = proxy_websocket_client_frames(
            state.clone(),
            operation_id.to_string(),
            &mut client_reader,
            &mut upstream_writer,
            related_domains.clone(),
            protection_enabled,
            Arc::clone(&consent_scopes),
        );
        let upstream_to_client = proxy_websocket_upstream_frames(
            state.clone(),
            operation_id.to_string(),
            &mut upstream_reader,
            &mut client_writer,
            related_domains,
            protection_enabled,
            Arc::clone(&consent_scopes),
        );

        tokio::select! {
            result = client_to_upstream => result,
            result = upstream_to_client => result,
        }
    }?;

    if matches!(outcome, WebSocketClientFrameOutcome::PolicyBlocked) {
        let close = websocket::WebSocketFrame::close(1008, "blocked by DAM policy");
        websocket::write_unmasked_frame(&mut client_writer, &close).await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebSocketClientFrameOutcome {
    Completed,
    PolicyBlocked,
}

async fn proxy_websocket_client_frames<R, W>(
    state: Arc<ProxyState>,
    operation_id: String,
    reader: &mut R,
    writer: &mut W,
    related_domains: Arc<RwLock<Vec<String>>>,
    protection_enabled: bool,
    consent_scopes: Arc<Vec<String>>,
) -> Result<WebSocketClientFrameOutcome, String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    loop {
        let Some(mut frame) = (match websocket::read_frame(reader).await {
            Ok(frame) => frame,
            Err(error) if protection_enabled && websocket_frame_error_is_unsupported(&error) => {
                record_proxy_event(
                    &state,
                    &operation_id,
                    LogLevel::Warn,
                    LogEventType::ProxyFailure,
                    "unsupported_websocket_frame",
                    "WebSocket request frame closed because unsupported protected frame shape was received",
                );
                return Ok(WebSocketClientFrameOutcome::PolicyBlocked);
            }
            Err(error) => return Err(error),
        }) else {
            return Ok(WebSocketClientFrameOutcome::Completed);
        };
        if frame.is_unfragmented_text() && protection_enabled {
            let text = std::str::from_utf8(&frame.payload)
                .map_err(|_| "WebSocket text frame is not utf-8".to_string())?;
            let protected = dam_pipeline::protect_text(
                text,
                &operation_id,
                &state.policy,
                state.vault.as_ref(),
                dam_pipeline::ProtectTextContext {
                    reference_vault: Some(state.vault.as_ref()),
                    consent_store: state.consent_store.as_deref(),
                    consent_scopes: consent_scopes.as_slice(),
                    event_sink: state.log_sink.as_deref(),
                    ..dam_pipeline::ProtectTextContext::default()
                },
                state.replacement_options,
            )
            .map_err(|_| "WebSocket request frame protection failed".to_string())?;
            if protected.is_blocked() {
                record_proxy_event(
                    &state,
                    &operation_id,
                    LogLevel::Warn,
                    LogEventType::ProxyFailure,
                    "blocked",
                    "WebSocket request frame blocked by policy",
                );
                return Ok(WebSocketClientFrameOutcome::PolicyBlocked);
            }
            remember_related_domains(&related_domains, &protected.detections)?;
            let Some(output) = protected.output else {
                return Err("WebSocket request frame protection did not produce output".to_string());
            };
            frame.payload = output.into_bytes();
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Info,
                LogEventType::ProxyForward,
                "protected",
                "WebSocket request text frame protected",
            );
        } else if protection_enabled && websocket_frame_requires_body_protection(&frame) {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Warn,
                LogEventType::ProxyFailure,
                "unsupported_websocket_frame",
                "WebSocket request frame closed because fragmented/binary protection is parked",
            );
            return Ok(WebSocketClientFrameOutcome::PolicyBlocked);
        }
        let is_close = frame.opcode == websocket::OPCODE_CLOSE;
        websocket::write_masked_frame(writer, &frame).await?;
        if is_close {
            return Ok(WebSocketClientFrameOutcome::Completed);
        }
    }
}

async fn proxy_websocket_upstream_frames<R, W>(
    state: Arc<ProxyState>,
    operation_id: String,
    reader: &mut R,
    writer: &mut W,
    related_domains: Arc<RwLock<Vec<String>>>,
    protection_enabled: bool,
    consent_scopes: Arc<Vec<String>>,
) -> Result<WebSocketClientFrameOutcome, String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    loop {
        let Some(mut frame) = (match websocket::read_frame(reader).await {
            Ok(frame) => frame,
            Err(error) if protection_enabled && websocket_frame_error_is_unsupported(&error) => {
                record_proxy_event(
                    &state,
                    &operation_id,
                    LogLevel::Warn,
                    LogEventType::ProxyFailure,
                    "unsupported_websocket_frame",
                    "WebSocket response frame closed because unsupported protected frame shape was received",
                );
                return Ok(WebSocketClientFrameOutcome::PolicyBlocked);
            }
            Err(error) => return Err(error),
        }) else {
            return Ok(WebSocketClientFrameOutcome::Completed);
        };
        if frame.is_unfragmented_text() && protection_enabled {
            let text = std::str::from_utf8(&frame.payload)
                .map_err(|_| "WebSocket response text frame is not utf-8".to_string())?;
            let domains = related_domains.read().map_err(|_| {
                "WebSocket related-domain state is unavailable after a prior failure".to_string()
            })?;
            let protected = dam_pipeline::protect_text(
                text,
                &operation_id,
                &state.policy,
                state.vault.as_ref(),
                dam_pipeline::ProtectTextContext {
                    reference_vault: Some(state.vault.as_ref()),
                    consent_store: state.consent_store.as_deref(),
                    consent_scopes: consent_scopes.as_slice(),
                    event_sink: state.log_sink.as_deref(),
                    related_domains: domains.as_slice(),
                },
                state.replacement_options,
            )
            .map_err(|_| "WebSocket response frame protection failed".to_string())?;
            if protected.is_blocked() {
                record_proxy_event(
                    &state,
                    &operation_id,
                    LogLevel::Warn,
                    LogEventType::ProxyFailure,
                    "inbound_blocked",
                    "WebSocket response frame blocked by policy",
                );
                return Ok(WebSocketClientFrameOutcome::PolicyBlocked);
            } else {
                let Some(output) = protected.output else {
                    return Err(
                        "WebSocket response frame protection did not produce output".to_string()
                    );
                };
                frame.payload = output.into_bytes();
            }
            if !protected.detections.is_empty() {
                record_proxy_event(
                    &state,
                    &operation_id,
                    LogLevel::Info,
                    LogEventType::ProxyForward,
                    "inbound_protection",
                    format!(
                        "WebSocket response text frame protected detections={} replacements={} tokenized={} blocked={}",
                        protected.detections.len(),
                        protected.plan.replacements.len(),
                        protected.plan.tokenized_count(),
                        protected.plan.blocked_count()
                    ),
                );
            }
        } else if protection_enabled && websocket_frame_requires_body_protection(&frame) {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Warn,
                LogEventType::ProxyFailure,
                "unsupported_websocket_frame",
                "WebSocket response frame closed because fragmented/binary protection is parked",
            );
            return Ok(WebSocketClientFrameOutcome::PolicyBlocked);
        }
        let is_close = frame.opcode == websocket::OPCODE_CLOSE;
        websocket::write_unmasked_frame(writer, &frame).await?;
        if is_close {
            return Ok(WebSocketClientFrameOutcome::Completed);
        }
    }
}

fn remember_related_domains(
    related_domains: &Arc<RwLock<Vec<String>>>,
    detections: &[dam_core::Detection],
) -> Result<(), String> {
    let mut related_domains = related_domains.write().map_err(|_| {
        "WebSocket related-domain state is unavailable after a prior failure".to_string()
    })?;
    for domain in related_domains_from_detections(detections) {
        if !related_domains.contains(&domain) {
            related_domains.push(domain);
        }
    }
    Ok(())
}

fn websocket_frame_requires_body_protection(frame: &websocket::WebSocketFrame) -> bool {
    frame.is_fragmented_text_or_continuation() || frame.is_binary()
}

fn websocket_frame_error_is_unsupported(error: &str) -> bool {
    error.contains("compressed or extension WebSocket frames are not supported")
}

async fn write_websocket_upgrade_request<T>(
    upstream: &mut T,
    request: &InterceptedHttpRequest,
    authority: &TargetAuthority,
) -> Result<(), String>
where
    T: AsyncWrite + Unpin,
{
    let target = origin_form_target(&request.uri);
    upstream
        .write_all(format!("{} {target} HTTP/1.1\r\n", request.method).as_bytes())
        .await
        .map_err(|error| format!("failed to write WebSocket upgrade request: {error}"))?;
    upstream
        .write_all(format!("host: {}\r\n", authority_header_value(authority)).as_bytes())
        .await
        .map_err(|error| format!("failed to write WebSocket upgrade request: {error}"))?;
    upstream
        .write_all(b"connection: Upgrade\r\nupgrade: websocket\r\n")
        .await
        .map_err(|error| format!("failed to write WebSocket upgrade request: {error}"))?;
    for (name, value) in request.headers.iter() {
        if websocket::request_header_should_skip(name) {
            continue;
        }
        upstream
            .write_all(name.as_str().as_bytes())
            .await
            .map_err(|error| format!("failed to write WebSocket upgrade request: {error}"))?;
        upstream
            .write_all(b": ")
            .await
            .map_err(|error| format!("failed to write WebSocket upgrade request: {error}"))?;
        upstream
            .write_all(value.as_bytes())
            .await
            .map_err(|error| format!("failed to write WebSocket upgrade request: {error}"))?;
        upstream
            .write_all(b"\r\n")
            .await
            .map_err(|error| format!("failed to write WebSocket upgrade request: {error}"))?;
    }
    upstream
        .write_all(b"\r\n")
        .await
        .map_err(|error| format!("failed to finish WebSocket upgrade request: {error}"))?;
    upstream
        .flush()
        .await
        .map_err(|error| format!("failed to flush WebSocket upgrade request: {error}"))
}

async fn read_intercepted_response_head<T>(stream: &mut T) -> Result<Vec<u8>, String>
where
    T: AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        if find_header_end(&buffer).is_some() {
            return Ok(buffer);
        }
        if buffer.len() >= MAX_INTERCEPTED_HEADER_BYTES {
            return Err("WebSocket upstream response headers are too large".to_string());
        }
        let read = stream
            .read(&mut byte)
            .await
            .map_err(|error| format!("failed to read WebSocket upstream response: {error}"))?;
        if read == 0 {
            return Err("WebSocket upstream response ended before headers completed".to_string());
        }
        buffer.extend_from_slice(&byte[..read]);
    }
}

fn upstream_tls_connector() -> TlsConnector {
    ensure_rustls_crypto_provider();
    let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

struct InterceptedHttpRequest {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
}

fn tls_server_config(issued: dam_trust::LocalCaIssuedCertificate) -> Result<ServerConfig, String> {
    ensure_rustls_crypto_provider();
    let cert_chain = vec![CertificateDer::from(issued.certificate_der)];
    let private_key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(issued.private_key_der));
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|error| format!("failed to configure TLS certificate: {error}"))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

fn ensure_rustls_crypto_provider() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    });
}

fn tls_acceptor_for_host(
    state: &ProxyState,
    interception: &TransparentInterceptionConfig,
    host: &str,
) -> Result<TlsAcceptor, String> {
    let host = dam_net::normalize_traffic_host(host);
    if host.is_empty() {
        return Err("failed to issue local TLS certificate: host is empty".to_string());
    }
    if let Some(config) = state
        .tls_acceptor_cache
        .lock()
        .map_err(|_| "TLS certificate cache is unavailable".to_string())?
        .get(&host)
        .cloned()
    {
        return Ok(TlsAcceptor::from(config));
    }

    let issued = dam_trust::issue_local_ca_leaf_certificate(&interception.state_dir, &host)
        .map_err(|error| format!("failed to issue local TLS certificate: {error}"))?;
    let config = Arc::new(tls_server_config(issued)?);
    state
        .tls_acceptor_cache
        .lock()
        .map_err(|_| "TLS certificate cache is unavailable".to_string())?
        .insert(host, config.clone());
    Ok(TlsAcceptor::from(config))
}

fn transparent_interception_readiness(
    interception: &TransparentInterceptionConfig,
    traffic_route: dam_net::TrafficRoute,
) -> dam_intercept::RouteTlsInterceptionReadiness {
    let routing = dam_net::transparent_route_capture_readiness(
        traffic_route.clone(),
        dam_net::TrafficProtocol::Https,
        interception.network_mode,
        interception.system_proxy_active,
        interception.tun_active,
    );
    let trust_report = dam_trust::readiness_for_route(
        &dam_net::decide_transparent_route_with_routes(
            &dam_net::TrafficObservation::new(
                traffic_route.host.clone(),
                dam_net::TrafficProtocol::Https,
            ),
            &interception.routes,
        ),
        &interception.trust,
        interception.user_consented,
    );
    let route_trust = dam_trust::RouteTrustReadiness {
        route: traffic_route.clone(),
        protocol: dam_net::TrafficProtocol::Https,
        readiness: trust_report.readiness,
        message: trust_report.message,
    };

    dam_intercept::readiness_for_route(
        &routing,
        &route_trust,
        interception.user_consented,
        dam_intercept::TlsInterceptionAdapter::new(true),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetAuthority {
    host: String,
    port: u16,
}

fn connect_authority(uri: &Uri, headers: &HeaderMap) -> Option<TargetAuthority> {
    uri.authority()
        .map(|authority| authority.as_str())
        .or_else(|| {
            headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .and_then(|value| parse_target_authority(value, 443))
}

fn http_authority(uri: &Uri, headers: &HeaderMap) -> Option<TargetAuthority> {
    if matches!(uri.scheme_str(), Some(scheme) if !scheme.eq_ignore_ascii_case("http")) {
        return None;
    }
    uri.authority()
        .map(|authority| authority.as_str())
        .or_else(|| {
            headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .and_then(|value| parse_target_authority(value, 80))
}

fn https_authority(uri: &Uri, headers: &HeaderMap) -> Option<TargetAuthority> {
    if matches!(uri.scheme_str(), Some(scheme) if !scheme.eq_ignore_ascii_case("https")) {
        return None;
    }
    uri.authority()
        .map(|authority| authority.as_str())
        .or_else(|| {
            headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .and_then(|value| parse_target_authority(value, 443))
}

fn parse_target_authority(value: &str, default_port: u16) -> Option<TargetAuthority> {
    let value = value
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or_default()
        .trim();
    if value.is_empty() {
        return None;
    }

    if let Some(rest) = value.strip_prefix('[') {
        let (host, remainder) = rest.split_once(']')?;
        let port = remainder
            .strip_prefix(':')
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or(default_port);
        return Some(TargetAuthority {
            host: host.to_ascii_lowercase(),
            port,
        });
    }

    let (host, port) = value
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
        .unwrap_or((value, default_port));
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(TargetAuthority { host, port })
}

async fn connect_target(authority: &TargetAuthority) -> Result<TcpStream, String> {
    TcpStream::connect((authority.host.as_str(), authority.port))
        .await
        .map_err(|error| {
            format!(
                "failed to connect to {}:{}: {error}",
                authority.host, authority.port
            )
        })
}

fn is_forward_proxy_http_request(uri: &Uri) -> bool {
    uri.scheme().is_some() && uri.authority().is_some()
}

fn should_protect_forward_proxy_http_request(
    state: &ProxyState,
    request: &InterceptedHttpRequest,
) -> bool {
    if !state.protection_enabled() {
        return false;
    }
    let Some(interception) = state.transparent_interception.as_ref() else {
        return false;
    };
    http_authority(&request.uri, &request.headers)
        .and_then(|authority| {
            dam_net::classify_traffic_host_with_routes(&authority.host, &interception.routes)
        })
        .is_some()
}

fn route_matches_traffic_target(
    route: dam_router::RouteDecision<'_>,
    traffic_route: &dam_net::TrafficRoute,
) -> bool {
    let target = route.target();
    target.provider == traffic_route.provider
        && (target.name == traffic_route.target_name
            || normalize_host(&target.upstream) == normalize_host(&traffic_route.upstream))
}

fn consent_scopes_for_target(target: &dam_config::ProxyTargetConfig) -> Vec<String> {
    vec![dam_consent::target_scope(&target.name)]
}

impl ProxyState {
    fn resolve_inbound_for_route(&self, route: dam_router::RouteDecision<'_>) -> bool {
        self.resolve_inbound
            && self
                .route_resolve_inbound
                .get(&route.target().name)
                .copied()
                .unwrap_or(true)
    }

    fn protect_inbound_for_route(&self, route: dam_router::RouteDecision<'_>) -> bool {
        self.route_protect_inbound
            .get(&route.target().name)
            .copied()
            .unwrap_or(false)
    }
}

fn route_resolve_inbound(profile: &dam_net::TrafficProfile) -> HashMap<String, bool> {
    let mut policies = HashMap::new();
    for app in &profile.apps {
        if !app.enabled || app.action != dam_net::TrafficAction::Inspect {
            continue;
        }
        let target_name = app
            .target_name
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&app.id);
        policies.insert(target_name.clone(), app.inbound.resolve_references);
    }
    policies
}

fn route_protect_inbound(profile: &dam_net::TrafficProfile) -> HashMap<String, bool> {
    let mut policies = HashMap::new();
    for app in &profile.apps {
        if !app.enabled || app.action != dam_net::TrafficAction::Inspect {
            continue;
        }
        let target_name = app
            .target_name
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&app.id);
        policies.insert(target_name.clone(), app.inbound.protect_sensitive_data);
    }
    policies
}

fn route_for_request<'a>(
    state: &'a ProxyState,
    headers: &HeaderMap,
    uri: &Uri,
) -> dam_router::RouteDecision<'a> {
    if let Some(traffic_route) = profile_route_for_request(state, headers, uri) {
        return state
            .routes
            .decide_for_traffic_route(headers, &traffic_route);
    }

    state.routes.decide(headers, Some(uri))
}

fn profile_route_for_request(
    state: &ProxyState,
    headers: &HeaderMap,
    uri: &Uri,
) -> Option<dam_net::TrafficRoute> {
    let interception = state.transparent_interception.as_ref()?;
    let authority = https_authority(uri, headers).or_else(|| http_authority(uri, headers))?;
    dam_net::classify_traffic_host_with_routes(&authority.host, &interception.routes)
}

async fn read_intercepted_http_request<T>(
    stream: &mut T,
) -> Result<Option<InterceptedHttpRequest>, String>
where
    T: AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    let mut scratch = [0_u8; 1024];
    let header_end = loop {
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
        if buffer.len() >= MAX_INTERCEPTED_HEADER_BYTES {
            return Err("intercepted request headers are too large".to_string());
        }
        let read = stream
            .read(&mut scratch)
            .await
            .map_err(|error| format!("failed to read intercepted request: {error}"))?;
        if read == 0 {
            if buffer.is_empty() {
                return Ok(None);
            }
            return Err("intercepted request ended before headers completed".to_string());
        }
        buffer.extend_from_slice(&scratch[..read]);
    };

    let head = std::str::from_utf8(&buffer[..header_end])
        .map_err(|_| "intercepted request headers are not utf-8".to_string())?;
    let mut lines = head.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "intercepted request line is missing".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| "intercepted request method is missing".to_string())?
        .parse::<Method>()
        .map_err(|_| "intercepted request method is invalid".to_string())?;
    let target = request_parts
        .next()
        .ok_or_else(|| "intercepted request target is missing".to_string())?;
    let version = request_parts
        .next()
        .ok_or_else(|| "intercepted HTTP version is missing".to_string())?;
    if request_parts.next().is_some() || version != "HTTP/1.1" {
        return Err("only HTTP/1.1 intercepted requests are supported".to_string());
    }
    let uri = parse_intercepted_request_target(target)?;

    let mut headers = HeaderMap::new();
    let mut content_length_count = 0;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err("folded intercepted request headers are not supported".to_string());
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err("intercepted request header is invalid".to_string());
        };
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| "intercepted request header name is invalid".to_string())?;
        if name == header::CONTENT_LENGTH {
            content_length_count += 1;
        }
        let value = HeaderValue::from_str(value.trim())
            .map_err(|_| "intercepted request header value is invalid".to_string())?;
        headers.append(name, value);
    }

    if headers.contains_key(header::TRANSFER_ENCODING) {
        return Err("chunked intercepted requests are not supported".to_string());
    }
    if content_length_count > 1 {
        return Err("multiple content-length headers are not supported".to_string());
    }
    let content_length = headers
        .get(header::CONTENT_LENGTH)
        .map(|value| {
            value
                .to_str()
                .map_err(|_| "content-length is invalid".to_string())
                .and_then(|value| {
                    value
                        .parse::<usize>()
                        .map_err(|_| "content-length is invalid".to_string())
                })
        })
        .transpose()?
        .unwrap_or(0);
    if content_length > MAX_REQUEST_BYTES {
        return Err("intercepted request body exceeds the supported size".to_string());
    }

    let body_start = header_end + 4;
    let mut body = buffer[body_start..].to_vec();
    if body.len() > content_length {
        body.truncate(content_length);
    }
    while body.len() < content_length {
        let mut chunk = vec![0_u8; content_length - body.len()];
        stream
            .read_exact(&mut chunk)
            .await
            .map_err(|error| format!("failed to read intercepted request body: {error}"))?;
        body.extend_from_slice(&chunk);
    }

    Ok(Some(InterceptedHttpRequest {
        method,
        uri,
        headers,
        body: Bytes::from(body),
    }))
}

async fn write_forward_proxy_request<T>(
    upstream: &mut T,
    request: &InterceptedHttpRequest,
    authority: &TargetAuthority,
) -> Result<(), String>
where
    T: AsyncWrite + Unpin,
{
    let target = origin_form_target(&request.uri);
    upstream
        .write_all(format!("{} {target} HTTP/1.1\r\n", request.method).as_bytes())
        .await
        .map_err(|error| format!("failed to write passthrough request: {error}"))?;
    upstream
        .write_all(format!("host: {}\r\n", authority_header_value(authority)).as_bytes())
        .await
        .map_err(|error| format!("failed to write passthrough request: {error}"))?;
    for (name, value) in request.headers.iter() {
        if passthrough_request_should_skip_header(name) {
            continue;
        }
        upstream
            .write_all(name.as_str().as_bytes())
            .await
            .map_err(|error| format!("failed to write passthrough request: {error}"))?;
        upstream
            .write_all(b": ")
            .await
            .map_err(|error| format!("failed to write passthrough request: {error}"))?;
        upstream
            .write_all(value.as_bytes())
            .await
            .map_err(|error| format!("failed to write passthrough request: {error}"))?;
        upstream
            .write_all(b"\r\n")
            .await
            .map_err(|error| format!("failed to write passthrough request: {error}"))?;
    }
    upstream
        .write_all(b"connection: close\r\n\r\n")
        .await
        .map_err(|error| format!("failed to write passthrough request: {error}"))?;
    upstream
        .write_all(&request.body)
        .await
        .map_err(|error| format!("failed to write passthrough request body: {error}"))?;
    upstream
        .flush()
        .await
        .map_err(|error| format!("failed to flush passthrough request: {error}"))
}

fn origin_form_target(uri: &Uri) -> String {
    uri.path_and_query()
        .map(|value| value.as_str().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/".to_string())
}

fn authority_header_value(authority: &TargetAuthority) -> String {
    if authority.port == 80 {
        authority.host.clone()
    } else if authority.host.contains(':') {
        format!("[{}]:{}", authority.host, authority.port)
    } else {
        format!("{}:{}", authority.host, authority.port)
    }
}

async fn write_intercepted_http_response<T>(
    stream: &mut T,
    response: Response,
) -> Result<(), String>
where
    T: AsyncWrite + Unpin,
{
    let streaming = response_is_streaming(&response);
    let (parts, body) = response.into_parts();
    let reason = parts.status.canonical_reason().unwrap_or("");
    stream
        .write_all(format!("HTTP/1.1 {} {reason}\r\n", parts.status.as_u16()).as_bytes())
        .await
        .map_err(|error| format!("failed to write intercepted response: {error}"))?;
    for (name, value) in parts.headers.iter() {
        if intercepted_response_should_skip_header(name) {
            continue;
        }
        stream
            .write_all(name.as_str().as_bytes())
            .await
            .map_err(|error| format!("failed to write intercepted response: {error}"))?;
        stream
            .write_all(b": ")
            .await
            .map_err(|error| format!("failed to write intercepted response: {error}"))?;
        stream
            .write_all(value.as_bytes())
            .await
            .map_err(|error| format!("failed to write intercepted response: {error}"))?;
        stream
            .write_all(b"\r\n")
            .await
            .map_err(|error| format!("failed to write intercepted response: {error}"))?;
    }
    if streaming {
        stream
            .write_all(b"transfer-encoding: chunked\r\nconnection: close\r\n\r\n")
            .await
            .map_err(|error| format!("failed to write intercepted response: {error}"))?;
        write_intercepted_chunked_body(stream, body).await?;
        return Ok(());
    }

    let body = to_bytes(body, MAX_REQUEST_BYTES)
        .await
        .map_err(|_| "intercepted response body exceeds the supported size".to_string())?;
    stream
        .write_all(
            format!(
                "content-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .map_err(|error| format!("failed to write intercepted response: {error}"))?;
    stream
        .write_all(&body)
        .await
        .map_err(|error| format!("failed to write intercepted response: {error}"))?;
    Ok(())
}

async fn write_intercepted_chunked_body<T>(stream: &mut T, mut body: Body) -> Result<(), String>
where
    T: AsyncWrite + Unpin,
{
    while let Some(frame) = body.frame().await {
        let frame = frame
            .map_err(|error| format!("failed to read intercepted streaming response: {error}"))?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        if data.is_empty() {
            continue;
        }
        stream
            .write_all(format!("{:x}\r\n", data.len()).as_bytes())
            .await
            .map_err(|error| format!("failed to write intercepted streaming response: {error}"))?;
        stream
            .write_all(&data)
            .await
            .map_err(|error| format!("failed to write intercepted streaming response: {error}"))?;
        stream
            .write_all(b"\r\n")
            .await
            .map_err(|error| format!("failed to write intercepted streaming response: {error}"))?;
    }
    stream
        .write_all(b"0\r\n\r\n")
        .await
        .map_err(|error| format!("failed to finish intercepted streaming response: {error}"))
}

async fn write_intercepted_error<T>(
    stream: &mut T,
    status: StatusCode,
    message: &str,
) -> Result<(), String>
where
    T: AsyncWrite + Unpin,
{
    let safe_message = if message.is_empty() {
        "intercepted request failed"
    } else {
        message
    };
    let reason = status.canonical_reason().unwrap_or("Error");
    let body = format!("{safe_message}\n");
    let response = format!(
        "HTTP/1.1 {} {reason}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        status.as_u16(),
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| format!("failed to write intercepted error response: {error}"))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_intercepted_request_target(target: &str) -> Result<Uri, String> {
    target
        .parse::<Uri>()
        .or_else(|_| format!("http://{target}").parse::<Uri>())
        .map_err(|_| "intercepted request target is invalid".to_string())
}

fn response_is_streaming(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .any(|part| part.trim().eq_ignore_ascii_case("text/event-stream"))
        })
}

fn intercepted_response_should_skip_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "content-length" | "connection" | "transfer-encoding" | "keep-alive" | "upgrade"
    )
}

fn passthrough_request_should_skip_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "host"
            | "connection"
            | "proxy-connection"
            | "proxy-authorization"
            | "keep-alive"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn protection_control_enabled(path: &PathBuf) -> bool {
    let Ok(value) = fs::read_to_string(path) else {
        return true;
    };
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&value)
        && let Some(enabled) = json.get("enabled").and_then(serde_json::Value::as_bool)
    {
        return enabled;
    }
    !value.trim().eq_ignore_ascii_case("disabled")
}

fn normalize_host(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('.');
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("wss://"))
        .or_else(|| trimmed.strip_prefix("ws://"))
        .unwrap_or(trimmed);
    let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
    host_port
        .strip_prefix('[')
        .and_then(|value| value.split_once(']').map(|(host, _)| host))
        .unwrap_or_else(|| {
            host_port
                .split_once(':')
                .map(|(host, _)| host)
                .unwrap_or(host_port)
        })
        .to_ascii_lowercase()
}

fn handle_protection_failure(
    state: Arc<ProxyState>,
    route: dam_router::RouteDecision<'_>,
    operation_id: String,
    message: &'static str,
) -> Response {
    record_proxy_event(
        &state,
        &operation_id,
        LogLevel::Error,
        LogEventType::ProxyFailure,
        "blocked",
        message,
    );
    status_response(
        StatusCode::BAD_GATEWAY,
        dam_api::ProxyState::Blocked,
        message.to_string(),
        Some(operation_id),
        route.target(),
    )
}

struct ForwardAttempt {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
    operation_id: String,
    action: &'static str,
    related_domains: Arc<Vec<String>>,
    consent_scopes: Arc<Vec<String>>,
    inbound_plan: InboundTransformPlan,
}

#[derive(Debug, Clone, Copy)]
struct InboundTransformPlan {
    resolve_references: bool,
    protect_sensitive_data: bool,
}

struct ForwardRequestInput<'a> {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
    operation_id: &'a str,
    related_domains: Arc<Vec<String>>,
    consent_scopes: Arc<Vec<String>>,
    inbound_plan: InboundTransformPlan,
}

async fn forward_or_provider_down(
    state: Arc<ProxyState>,
    route: dam_router::RouteDecision<'_>,
    attempt: ForwardAttempt,
) -> Response {
    let ForwardAttempt {
        method,
        uri,
        headers,
        body,
        operation_id,
        action,
        related_domains,
        consent_scopes,
        inbound_plan,
    } = attempt;
    match forward_request(
        &state,
        route,
        ForwardRequestInput {
            method,
            uri,
            headers,
            body,
            operation_id: &operation_id,
            related_domains: Arc::clone(&related_domains),
            consent_scopes: Arc::clone(&consent_scopes),
            inbound_plan,
        },
    )
    .await
    {
        Ok(response) => {
            let event_type = if action == "bypassing" {
                LogEventType::ProxyBypass
            } else {
                LogEventType::ProxyForward
            };
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Info,
                event_type,
                action,
                "proxy request forwarded",
            );
            response
        }
        Err(error) => {
            record_proxy_event(
                &state,
                &operation_id,
                LogLevel::Error,
                LogEventType::ProxyFailure,
                "provider_down",
                "upstream provider is unavailable",
            );
            status_response(
                StatusCode::BAD_GATEWAY,
                dam_api::ProxyState::ProviderDown,
                error,
                Some(operation_id),
                route.target(),
            )
        }
    }
}

async fn forward_request(
    state: &Arc<ProxyState>,
    route: dam_router::RouteDecision<'_>,
    input: ForwardRequestInput<'_>,
) -> Result<Response, String> {
    let ForwardRequestInput {
        method,
        uri,
        headers,
        body,
        operation_id,
        related_domains,
        consent_scopes,
        inbound_plan,
    } = input;
    let target_api_key = route.target_api_key();
    let transform_inbound = inbound_plan.resolve_references || inbound_plan.protect_sensitive_data;
    let target = route.target();
    let target_name = target.name.clone();
    let target_provider = target.provider.clone();
    let target_api_key_injection = target_api_key
        .and(target.auth.inject.as_ref())
        .map(|inject| dam_http_adapter::AuthInjection {
            header: inject.header.as_str(),
            scheme: inject.scheme.as_deref(),
            strip_headers: inject.strip_headers.as_slice(),
        });
    let response_state = Arc::clone(state);
    let response_operation_id = operation_id.to_owned();
    let response_related_domains = Arc::clone(&related_domains);
    let response_consent_scopes = Arc::clone(&consent_scopes);
    let request = dam_http_adapter::ForwardRequest {
        upstream: &target.upstream,
        method,
        uri,
        headers,
        body,
        target_api_key,
        target_api_key_injection,
        transform_streaming_response: transform_inbound,
    };
    record_proxy_event(
        state,
        operation_id,
        LogLevel::Info,
        LogEventType::ProxyForward,
        "provider_forward_start",
        format!(
            "provider forward start target={target_name} provider={target_provider} resolve_inbound={} transform_streaming={}",
            inbound_plan.resolve_references, transform_inbound
        ),
    );
    let response = state
        .providers
        .http()
        .forward(request, move |response_body| {
            resolve_response_body(
                &response_state,
                &response_operation_id,
                response_body,
                inbound_plan,
                response_related_domains.as_slice(),
                response_consent_scopes.as_slice(),
            )
        })
        .await
        .map_err(|error| error.to_string())?;
    log_provider_response(state, operation_id, &response);
    Ok(response)
}

fn resolve_response_body(
    state: &ProxyState,
    operation_id: &str,
    body: Bytes,
    inbound_plan: InboundTransformPlan,
    related_domains: &[String],
    consent_scopes: &[String],
) -> Bytes {
    if !inbound_plan.resolve_references {
        record_proxy_event(
            state,
            operation_id,
            LogLevel::Info,
            LogEventType::ProxyForward,
            "resolve_disabled",
            format!("inbound resolution disabled response_bytes={}", body.len()),
        );
        if !inbound_plan.protect_sensitive_data {
            return body;
        }
        let body_text = match std::str::from_utf8(body.as_ref()) {
            Ok(text) => text,
            Err(_) => return body,
        };
        return protect_inbound_response_body(
            state,
            operation_id,
            body_text,
            related_domains,
            consent_scopes,
        )
        .map(Bytes::from)
        .unwrap_or(body);
    }

    let body_text = match std::str::from_utf8(body.as_ref()) {
        Ok(text) => text,
        Err(_) => {
            record_proxy_event(
                state,
                operation_id,
                LogLevel::Warn,
                LogEventType::Resolve,
                "resolve_non_utf8",
                format!(
                    "inbound resolution skipped non_utf8 response_bytes={}",
                    body.len()
                ),
            );
            return body;
        }
    };
    let result = dam_pipeline::resolve_text(
        body_text,
        operation_id,
        state.vault.as_ref(),
        state.log_sink.as_deref(),
    );
    record_proxy_event(
        state,
        operation_id,
        LogLevel::Info,
        LogEventType::Resolve,
        "resolve_attempt",
        format!(
            "inbound resolution references={} resolved={} missing={} read_failures={} response_bytes={}",
            result.plan.references.len(),
            result.plan.resolved_count(),
            result.plan.missing_count(),
            result.plan.read_failure_count(),
            body.len()
        ),
    );
    if let Some(output) = result.output {
        return Bytes::from(output);
    }

    if inbound_plan.protect_sensitive_data {
        protect_inbound_response_body(
            state,
            operation_id,
            body_text,
            related_domains,
            consent_scopes,
        )
        .map(Bytes::from)
        .unwrap_or(body)
    } else {
        body
    }
}

fn protect_inbound_response_body(
    state: &ProxyState,
    operation_id: &str,
    body_text: &str,
    related_domains: &[String],
    consent_scopes: &[String],
) -> Option<String> {
    if !state.protection_enabled() {
        return None;
    }

    let protected = match dam_pipeline::protect_text(
        body_text,
        operation_id,
        &state.policy,
        state.vault.as_ref(),
        dam_pipeline::ProtectTextContext {
            reference_vault: Some(state.vault.as_ref()),
            consent_store: state.consent_store.as_deref(),
            consent_scopes,
            event_sink: state.log_sink.as_deref(),
            related_domains,
        },
        state.replacement_options,
    ) {
        Ok(result) => result,
        Err(_) => {
            record_proxy_event(
                state,
                operation_id,
                LogLevel::Warn,
                LogEventType::ProxyFailure,
                "inbound_protection_failed",
                "inbound response protection failed",
            );
            return None;
        }
    };

    if protected.detections.is_empty() {
        return None;
    }

    record_proxy_event(
        state,
        operation_id,
        LogLevel::Info,
        LogEventType::ProxyForward,
        "inbound_protection",
        format!(
            "inbound protection detections={} replacements={} tokenized={} blocked={}",
            protected.detections.len(),
            protected.plan.replacements.len(),
            protected.plan.tokenized_count(),
            protected.plan.blocked_count()
        ),
    );

    if protected.is_blocked() {
        record_proxy_event(
            state,
            operation_id,
            LogLevel::Warn,
            LogEventType::ProxyFailure,
            "inbound_blocked",
            "inbound response blocked by policy",
        );
        return Some("[blocked by DAM policy]".to_string());
    }

    protected.output
}

fn request_has_unsupported_content_encoding(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .any(|part| !part.eq_ignore_ascii_case("identity"))
        })
}

fn strip_body_integrity_headers(headers: &mut HeaderMap) -> usize {
    let mut removed = 0;
    for name in [
        "content-digest",
        "content-md5",
        "digest",
        "repr-digest",
        "signature",
        "signature-input",
        "x-content-digest",
        "x-content-md5",
        "x-body-digest",
        "x-body-sha256",
        "x-payload-digest",
        "x-payload-sha256",
        "x-signature",
    ] {
        if headers.remove(name).is_some() {
            removed += 1;
        }
    }
    removed
}

fn status_response(
    status: StatusCode,
    state: dam_api::ProxyState,
    message: String,
    operation_id: Option<String>,
    target: &dam_config::ProxyTargetConfig,
) -> Response {
    let diagnostics = proxy_diagnostics(state, &message);

    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(dam_api::ProxyReport {
            operation_id,
            target: Some(target.name.clone()),
            upstream: Some(target.upstream.clone()),
            state,
            message,
            diagnostics,
        }),
    )
        .into_response()
}

fn proxy_diagnostics(state: dam_api::ProxyState, message: &str) -> Vec<dam_api::Diagnostic> {
    match state {
        dam_api::ProxyState::Protected => Vec::new(),
        dam_api::ProxyState::Bypassing => vec![dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Warning,
            "bypassing",
            message,
        )],
        dam_api::ProxyState::Blocked => vec![dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Error,
            "blocked",
            message,
        )],
        dam_api::ProxyState::ProviderDown => vec![dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Error,
            "provider_down",
            message,
        )],
        dam_api::ProxyState::ConfigRequired => vec![dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Error,
            "config_required",
            message,
        )],
        dam_api::ProxyState::DamDown => vec![dam_api::Diagnostic::new(
            dam_api::DiagnosticSeverity::Error,
            "dam_down",
            message,
        )],
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
