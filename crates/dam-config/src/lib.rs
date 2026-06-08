use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const DEFAULT_CONFIG_PATH: &str = "dam.toml";
const DEFAULT_REMOTE_TIMEOUT_MS: u64 = 2_000;
const GENERIC_HTTP_PROVIDER: &str = "generic-http";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DamConfig {
    pub vault: VaultConfig,
    pub log: LogConfig,
    pub consent: ConsentConfig,
    pub policy: PolicyConfig,
    pub failure: FailureConfig,
    pub traffic: TrafficConfig,
    pub web: WebConfig,
    pub proxy: ProxyConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultConfig {
    pub backend: VaultBackend,
    pub sqlite_path: PathBuf,
    pub remote_url: Option<String>,
    pub token_env: Option<String>,
    pub token: Option<SecretValue>,
    pub timeout_ms: u64,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            backend: VaultBackend::Sqlite,
            sqlite_path: PathBuf::from("vault.db"),
            remote_url: None,
            token_env: Some("DAM_VAULT_TOKEN".to_string()),
            token: None,
            timeout_ms: DEFAULT_REMOTE_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultBackend {
    Sqlite,
    Remote,
}

impl VaultBackend {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Remote => "remote",
        }
    }
}

impl FromStr for VaultBackend {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "sqlite" => Ok(Self::Sqlite),
            "remote" => Ok(Self::Remote),
            _ => Err(ConfigError::invalid_value(
                "vault.backend",
                value,
                "expected sqlite or remote",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogConfig {
    pub enabled: bool,
    pub backend: LogBackend,
    pub sqlite_path: PathBuf,
    pub remote_url: Option<String>,
    pub token_env: Option<String>,
    pub token: Option<SecretValue>,
    pub timeout_ms: u64,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: LogBackend::Sqlite,
            sqlite_path: PathBuf::from("log.db"),
            remote_url: None,
            token_env: Some("DAM_LOG_TOKEN".to_string()),
            token: None,
            timeout_ms: DEFAULT_REMOTE_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentConfig {
    pub enabled: bool,
    pub backend: ConsentBackend,
    pub sqlite_path: PathBuf,
    pub default_ttl_seconds: u64,
    pub mcp_write_enabled: bool,
}

impl Default for ConsentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: ConsentBackend::Sqlite,
            sqlite_path: PathBuf::from("consent.db"),
            default_ttl_seconds: 86_400,
            mcp_write_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentBackend {
    Sqlite,
}

impl FromStr for ConsentBackend {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "sqlite" => Ok(Self::Sqlite),
            _ => Err(ConfigError::invalid_value(
                "consent.backend",
                value,
                "expected sqlite",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogBackend {
    None,
    Sqlite,
    Remote,
}

impl LogBackend {
    pub fn tag(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sqlite => "sqlite",
            Self::Remote => "remote",
        }
    }
}

impl FromStr for LogBackend {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "sqlite" => Ok(Self::Sqlite),
            "remote" => Ok(Self::Remote),
            _ => Err(ConfigError::invalid_value(
                "log.backend",
                value,
                "expected none, sqlite, or remote",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyConfig {
    pub default_action: dam_core::PolicyAction,
    pub deduplicate_replacements: bool,
    pub kind_actions: HashMap<dam_core::SensitiveType, dam_core::PolicyAction>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            default_action: dam_core::PolicyAction::Tokenize,
            deduplicate_replacements: true,
            kind_actions: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureConfig {
    pub vault_write: VaultWriteFailureMode,
    pub log_write: LogWriteFailureMode,
}

impl Default for FailureConfig {
    fn default() -> Self {
        Self {
            vault_write: VaultWriteFailureMode::RedactOnly,
            log_write: LogWriteFailureMode::WarnContinue,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultWriteFailureMode {
    RedactOnly,
    FailClosed,
}

impl VaultWriteFailureMode {
    pub fn tag(self) -> &'static str {
        match self {
            Self::RedactOnly => "redact_only",
            Self::FailClosed => "fail_closed",
        }
    }
}

impl FromStr for VaultWriteFailureMode {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "redact_only" => Ok(Self::RedactOnly),
            "fail_closed" => Ok(Self::FailClosed),
            _ => Err(ConfigError::invalid_value(
                "failure.vault_write",
                value,
                "expected redact_only or fail_closed",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogWriteFailureMode {
    WarnContinue,
    FailClosed,
}

impl LogWriteFailureMode {
    pub fn tag(self) -> &'static str {
        match self {
            Self::WarnContinue => "warn_continue",
            Self::FailClosed => "fail_closed",
        }
    }
}

impl FromStr for LogWriteFailureMode {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "warn_continue" => Ok(Self::WarnContinue),
            "fail_closed" => Ok(Self::FailClosed),
            _ => Err(ConfigError::invalid_value(
                "failure.log_write",
                value,
                "expected warn_continue or fail_closed",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrafficConfig {
    pub profile_path: Option<PathBuf>,
    pub profile: dam_net::TrafficProfile,
    pub enabled_app_ids: Option<Vec<String>>,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            profile_path: None,
            profile: dam_net::llm_mvp_profile(),
            enabled_app_ids: None,
        }
    }
}

impl TrafficConfig {
    pub fn effective_profile(&self) -> dam_net::TrafficProfile {
        self.enabled_app_ids
            .as_ref()
            .map(|app_ids| self.profile.with_runtime_enabled_apps(app_ids))
            .unwrap_or_else(|| self.profile.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebConfig {
    pub addr: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:2896".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub listen: String,
    pub mode: ProxyMode,
    pub default_failure_mode: ProxyFailureMode,
    pub resolve_inbound: bool,
    pub targets: Vec<ProxyTargetConfig>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: "127.0.0.1:7828".to_string(),
            mode: ProxyMode::ReverseProxy,
            default_failure_mode: ProxyFailureMode::BypassOnError,
            resolve_inbound: true,
            targets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    ReverseProxy,
}

impl ProxyMode {
    pub fn tag(self) -> &'static str {
        match self {
            Self::ReverseProxy => "reverse_proxy",
        }
    }
}

impl FromStr for ProxyMode {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "reverse_proxy" => Ok(Self::ReverseProxy),
            _ => Err(ConfigError::invalid_value(
                "proxy.mode",
                value,
                "expected reverse_proxy",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyFailureMode {
    BypassOnError,
    RedactOnly,
    BlockOnError,
}

impl ProxyFailureMode {
    pub fn tag(self) -> &'static str {
        match self {
            Self::BypassOnError => "bypass_on_error",
            Self::RedactOnly => "redact_only",
            Self::BlockOnError => "block_on_error",
        }
    }
}

impl FromStr for ProxyFailureMode {
    type Err = ConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "bypass_on_error" => Ok(Self::BypassOnError),
            "redact_only" => Ok(Self::RedactOnly),
            "block_on_error" => Ok(Self::BlockOnError),
            _ => Err(ConfigError::invalid_value(
                "proxy.failure_mode",
                value,
                "expected bypass_on_error, redact_only, or block_on_error",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyTargetConfig {
    pub name: String,
    pub provider: String,
    pub upstream: String,
    pub auth: dam_net::UpstreamAuthConfig,
    pub failure_mode: Option<ProxyFailureMode>,
    pub api_key_env: Option<String>,
    pub api_key: Option<SecretValue>,
}

impl ProxyTargetConfig {
    pub fn default_profile_target() -> Self {
        if let Some(route) = dam_net::default_traffic_routes().into_iter().next() {
            return Self {
                name: route.target_name,
                provider: route.provider,
                upstream: route.upstream,
                auth: route.auth,
                failure_mode: None,
                api_key_env: None,
                api_key: None,
            };
        }
        Self {
            name: "default".to_string(),
            provider: "default".to_string(),
            upstream: String::new(),
            auth: dam_net::UpstreamAuthConfig::default(),
            failure_mode: None,
            api_key_env: None,
            api_key: None,
        }
    }

    pub fn effective_failure_mode(&self, default: ProxyFailureMode) -> ProxyFailureMode {
        self.failure_mode.unwrap_or(default)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue {
    env_var: String,
    value: String,
}

impl SecretValue {
    pub fn new(env_var: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            env_var: env_var.into(),
            value: value.into(),
        }
    }

    pub fn env_var(&self) -> &str {
        &self.env_var
    }

    pub fn expose(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretValue")
            .field("env_var", &self.env_var)
            .field("value", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOverrides {
    pub config_path: Option<PathBuf>,
    pub vault_sqlite_path: Option<PathBuf>,
    pub log_sqlite_path: Option<PathBuf>,
    pub log_enabled: Option<bool>,
    pub consent_sqlite_path: Option<PathBuf>,
    pub consent_enabled: Option<bool>,
    pub traffic_profile_path: Option<PathBuf>,
    pub traffic_enabled_app_ids: Option<Vec<String>>,
    pub web_addr: Option<String>,
    pub proxy_enabled: Option<bool>,
    pub proxy_listen: Option<String>,
    pub proxy_resolve_inbound: Option<bool>,
    pub proxy_target_name: Option<String>,
    pub proxy_target_provider: Option<String>,
    pub proxy_target_upstream: Option<String>,
    pub proxy_target_failure_mode: Option<ProxyFailureMode>,
    pub proxy_target_api_key_env: Option<String>,
    pub proxy_targets: Option<Vec<ProxyTargetConfig>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("failed to read config file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config file {path}: {source}")]
    ParseFile {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("failed to read traffic profile {path}: {source}")]
    ReadTrafficProfile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse traffic profile {path}: {source}")]
    ParseTrafficProfile {
        path: PathBuf,
        source: dam_net::TrafficProfileError,
    },

    #[error("invalid traffic profile {path}: {message}")]
    InvalidTrafficProfile { path: PathBuf, message: String },

    #[error("invalid config value for {field}: {value} ({message})")]
    InvalidValue {
        field: &'static str,
        value: String,
        message: &'static str,
    },

    #[error("missing required config value: {field}")]
    MissingRequired { field: &'static str },
}

impl ConfigError {
    fn invalid_value(field: &'static str, value: impl Into<String>, message: &'static str) -> Self {
        Self::InvalidValue {
            field,
            value: value.into(),
            message,
        }
    }
}

pub fn load(overrides: &ConfigOverrides) -> Result<DamConfig, ConfigError> {
    load_with_env(overrides, std::env::vars())
}

pub fn load_with_env<I, K, V>(overrides: &ConfigOverrides, env: I) -> Result<DamConfig, ConfigError>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let env = env
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect::<BTreeMap<_, _>>();

    let config_path = config_path(overrides, &env);
    let config_path_is_explicit = overrides.config_path.is_some() || env.contains_key("DAM_CONFIG");

    let mut config = DamConfig::default();
    if config_path.exists() {
        merge_file(&mut config, &config_path)?;
    } else if config_path_is_explicit {
        return Err(ConfigError::ConfigNotFound(config_path));
    }

    merge_env(&mut config, &env)?;
    merge_overrides(&mut config, overrides)?;
    resolve_secrets(&mut config, &env);
    validate(&config)?;

    Ok(config)
}

fn config_path(overrides: &ConfigOverrides, env: &BTreeMap<String, String>) -> PathBuf {
    overrides
        .config_path
        .clone()
        .or_else(|| env.get("DAM_CONFIG").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH))
}

fn merge_file(config: &mut DamConfig, path: &Path) -> Result<(), ConfigError> {
    let raw = fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let parsed = toml::from_str::<RawDamConfig>(&raw).map_err(|source| ConfigError::ParseFile {
        path: path.to_path_buf(),
        source,
    })?;

    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    merge_raw(config, parsed, base_dir)
}

fn merge_raw(
    config: &mut DamConfig,
    raw: RawDamConfig,
    base_dir: &Path,
) -> Result<(), ConfigError> {
    if let Some(vault) = raw.vault {
        if let Some(backend) = vault.backend {
            config.vault.backend = backend.parse()?;
        }
        if let Some(path) = vault.path {
            config.vault.sqlite_path = path;
        }
        if let Some(url) = vault.url {
            config.vault.remote_url = non_empty(url);
        }
        if let Some(token_env) = vault.token_env {
            config.vault.token_env = non_empty(token_env);
        }
        if let Some(timeout_ms) = vault.timeout_ms {
            config.vault.timeout_ms = timeout_ms;
        }
    }

    if let Some(log) = raw.log {
        if let Some(enabled) = log.enabled {
            config.log.enabled = enabled;
        }
        if let Some(backend) = log.backend {
            config.log.backend = backend.parse()?;
        }
        if let Some(path) = log.path {
            config.log.sqlite_path = path;
        }
        if let Some(url) = log.url {
            config.log.remote_url = non_empty(url);
        }
        if let Some(token_env) = log.token_env {
            config.log.token_env = non_empty(token_env);
        }
        if let Some(timeout_ms) = log.timeout_ms {
            config.log.timeout_ms = timeout_ms;
        }
    }

    if let Some(consent) = raw.consent {
        if let Some(enabled) = consent.enabled {
            config.consent.enabled = enabled;
        }
        if let Some(backend) = consent.backend {
            config.consent.backend = backend.parse()?;
        }
        if let Some(path) = consent.path {
            config.consent.sqlite_path = path;
        }
        if let Some(default_ttl_seconds) = consent.default_ttl_seconds {
            config.consent.default_ttl_seconds = default_ttl_seconds;
        }
        if let Some(mcp_write_enabled) = consent.mcp_write_enabled {
            config.consent.mcp_write_enabled = mcp_write_enabled;
        }
    }

    if let Some(policy) = raw.policy {
        if let Some(default_action) = policy.default_action {
            config.policy.default_action =
                parse_policy_action("policy.default_action", &default_action)?;
        }
        if let Some(deduplicate_replacements) = policy.deduplicate_replacements {
            config.policy.deduplicate_replacements = deduplicate_replacements;
        }
        if let Some(kind_actions) = policy.kind {
            for (kind, raw_kind_config) in kind_actions {
                let kind = parse_sensitive_type("policy.kind", &kind)?;
                let action = parse_policy_action("policy.kind.action", &raw_kind_config.action)?;
                config.policy.kind_actions.insert(kind, action);
            }
        }
    }

    if let Some(failure) = raw.failure {
        if let Some(vault_write) = failure.vault_write {
            config.failure.vault_write = vault_write.parse()?;
        }
        if let Some(log_write) = failure.log_write {
            config.failure.log_write = log_write.parse()?;
        }
    }

    if let Some(traffic) = raw.traffic {
        if let Some(profile_path) = traffic.profile_path {
            let profile_path = resolve_relative_path(base_dir, profile_path);
            config.traffic.profile = load_traffic_profile(&profile_path)?;
            config.traffic.profile_path = Some(profile_path);
        }
        if let Some(enabled_apps) = traffic.enabled_apps {
            config.traffic.enabled_app_ids = Some(normalized_string_list(enabled_apps));
        }
    }

    if let Some(network) = raw.network
        && network.ai_routes.is_some()
    {
        return Err(ConfigError::invalid_value(
            "network.ai_routes",
            "<configured>",
            "network.ai_routes was removed; define private or enterprise AI endpoints as traffic profile JSON apps and set traffic.profile_path",
        ));
    }

    if let Some(web) = raw.web
        && let Some(addr) = web.addr
    {
        config.web.addr = addr;
    }

    if let Some(proxy) = raw.proxy {
        if let Some(enabled) = proxy.enabled {
            config.proxy.enabled = enabled;
        }
        if let Some(listen) = proxy.listen {
            config.proxy.listen = listen;
        }
        if let Some(mode) = proxy.mode {
            config.proxy.mode = mode.parse()?;
        }
        if let Some(default_failure_mode) = proxy.default_failure_mode {
            config.proxy.default_failure_mode = default_failure_mode.parse()?;
        }
        if let Some(resolve_inbound) = proxy.resolve_inbound {
            config.proxy.resolve_inbound = resolve_inbound;
        }
        if let Some(targets) = proxy.targets {
            config.proxy.targets = targets
                .into_iter()
                .map(parse_proxy_target)
                .collect::<Result<Vec<_>, _>>()?;
        }
    }

    Ok(())
}

fn merge_env(config: &mut DamConfig, env: &BTreeMap<String, String>) -> Result<(), ConfigError> {
    if let Some(value) = env.get("DAM_VAULT_BACKEND") {
        config.vault.backend = value.parse()?;
    }
    if let Some(value) = first_env(env, &["DAM_VAULT_PATH", "DAM_VAULT_SQLITE_PATH"]) {
        config.vault.sqlite_path = PathBuf::from(value);
    }
    if let Some(value) = env.get("DAM_VAULT_URL") {
        config.vault.remote_url = non_empty(value.clone());
    }
    if let Some(value) = env.get("DAM_VAULT_TOKEN_ENV") {
        config.vault.token_env = non_empty(value.clone());
    }
    if let Some(value) = env.get("DAM_VAULT_TIMEOUT_MS") {
        config.vault.timeout_ms = parse_u64("DAM_VAULT_TIMEOUT_MS", value)?;
    }

    if let Some(value) = env.get("DAM_LOG_ENABLED") {
        config.log.enabled = parse_bool("DAM_LOG_ENABLED", value)?;
    }
    if let Some(value) = env.get("DAM_LOG_BACKEND") {
        config.log.backend = value.parse()?;
    }
    if let Some(value) = first_env(env, &["DAM_LOG_PATH", "DAM_LOG_SQLITE_PATH"]) {
        config.log.sqlite_path = PathBuf::from(value);
    }
    if let Some(value) = env.get("DAM_LOG_URL") {
        config.log.remote_url = non_empty(value.clone());
    }
    if let Some(value) = env.get("DAM_LOG_TOKEN_ENV") {
        config.log.token_env = non_empty(value.clone());
    }
    if let Some(value) = env.get("DAM_LOG_TIMEOUT_MS") {
        config.log.timeout_ms = parse_u64("DAM_LOG_TIMEOUT_MS", value)?;
    }

    if let Some(value) = env.get("DAM_CONSENT_ENABLED") {
        config.consent.enabled = parse_bool("DAM_CONSENT_ENABLED", value)?;
    }
    if let Some(value) = env.get("DAM_CONSENT_BACKEND") {
        config.consent.backend = value.parse()?;
    }
    if let Some(value) = first_env(env, &["DAM_CONSENT_PATH", "DAM_CONSENT_SQLITE_PATH"]) {
        config.consent.sqlite_path = PathBuf::from(value);
    }
    if let Some(value) = env.get("DAM_CONSENT_DEFAULT_TTL_SECONDS") {
        config.consent.default_ttl_seconds = parse_u64("DAM_CONSENT_DEFAULT_TTL_SECONDS", value)?;
    }
    if let Some(value) = env.get("DAM_CONSENT_MCP_WRITE_ENABLED") {
        config.consent.mcp_write_enabled = parse_bool("DAM_CONSENT_MCP_WRITE_ENABLED", value)?;
    }

    if let Some(value) = env.get("DAM_POLICY_DEFAULT_ACTION") {
        config.policy.default_action = parse_policy_action("DAM_POLICY_DEFAULT_ACTION", value)?;
    }
    if let Some(value) = env.get("DAM_POLICY_DEDUPLICATE_REPLACEMENTS") {
        config.policy.deduplicate_replacements =
            parse_bool("DAM_POLICY_DEDUPLICATE_REPLACEMENTS", value)?;
    }
    if let Some(value) = env.get("DAM_POLICY_EMAIL_ACTION") {
        config.policy.kind_actions.insert(
            dam_core::SensitiveType::Email,
            parse_policy_action("DAM_POLICY_EMAIL_ACTION", value)?,
        );
    }
    if let Some(value) = env.get("DAM_POLICY_DOMAIN_ACTION") {
        config.policy.kind_actions.insert(
            dam_core::SensitiveType::Domain,
            parse_policy_action("DAM_POLICY_DOMAIN_ACTION", value)?,
        );
    }
    if let Some(value) = env.get("DAM_POLICY_PHONE_ACTION") {
        config.policy.kind_actions.insert(
            dam_core::SensitiveType::Phone,
            parse_policy_action("DAM_POLICY_PHONE_ACTION", value)?,
        );
    }
    if let Some(value) = env.get("DAM_POLICY_SSN_ACTION") {
        config.policy.kind_actions.insert(
            dam_core::SensitiveType::Ssn,
            parse_policy_action("DAM_POLICY_SSN_ACTION", value)?,
        );
    }
    if let Some(value) = first_env(
        env,
        &["DAM_POLICY_CC_ACTION", "DAM_POLICY_CREDIT_CARD_ACTION"],
    ) {
        config.policy.kind_actions.insert(
            dam_core::SensitiveType::CreditCard,
            parse_policy_action("DAM_POLICY_CC_ACTION", value)?,
        );
    }
    if let Some(value) = env.get("DAM_FAILURE_VAULT_WRITE") {
        config.failure.vault_write = value.parse()?;
    }
    if let Some(value) = env.get("DAM_FAILURE_LOG_WRITE") {
        config.failure.log_write = value.parse()?;
    }
    if let Some(value) = env.get("DAM_TRAFFIC_PROFILE") {
        let profile_path = PathBuf::from(value);
        config.traffic.profile = load_traffic_profile(&profile_path)?;
        config.traffic.profile_path = Some(profile_path);
    }
    if let Some(value) = env.get("DAM_TRAFFIC_ENABLED_APPS") {
        config.traffic.enabled_app_ids = Some(parse_csv_list(value));
    }
    if let Some(value) = env.get("DAM_WEB_ADDR") {
        config.web.addr = value.clone();
    }
    if let Some(value) = env.get("DAM_PROXY_ENABLED") {
        config.proxy.enabled = parse_bool("DAM_PROXY_ENABLED", value)?;
    }
    if let Some(value) = env.get("DAM_PROXY_LISTEN") {
        config.proxy.listen = value.clone();
    }
    if let Some(value) = env.get("DAM_PROXY_MODE") {
        config.proxy.mode = value.parse()?;
    }
    if let Some(value) = env.get("DAM_PROXY_DEFAULT_FAILURE_MODE") {
        config.proxy.default_failure_mode = value.parse()?;
    }
    if let Some(value) = env.get("DAM_PROXY_RESOLVE_INBOUND") {
        config.proxy.resolve_inbound = parse_bool("DAM_PROXY_RESOLVE_INBOUND", value)?;
    }
    if proxy_target_env_is_present(env) {
        let target = ensure_first_proxy_target(&mut config.proxy);
        if let Some(value) = env.get("DAM_PROXY_TARGET_NAME") {
            target.name = value.clone();
        }
        if let Some(value) = env.get("DAM_PROXY_TARGET_PROVIDER") {
            target.provider = value.clone();
            if value == GENERIC_HTTP_PROVIDER && !env.contains_key("DAM_PROXY_TARGET_API_KEY_ENV") {
                target.api_key_env = None;
            }
        }
        if let Some(value) = env.get("DAM_PROXY_TARGET_UPSTREAM") {
            target.upstream = value.clone();
        }
        if let Some(value) = env.get("DAM_PROXY_TARGET_FAILURE_MODE") {
            target.failure_mode = Some(value.parse()?);
        }
        if let Some(value) = env.get("DAM_PROXY_TARGET_API_KEY_ENV") {
            target.api_key_env = non_empty(value.clone());
        }
    }

    Ok(())
}

fn merge_overrides(config: &mut DamConfig, overrides: &ConfigOverrides) -> Result<(), ConfigError> {
    if let Some(path) = &overrides.vault_sqlite_path {
        config.vault.backend = VaultBackend::Sqlite;
        config.vault.sqlite_path = path.clone();
    }
    if let Some(path) = &overrides.log_sqlite_path {
        config.log.backend = LogBackend::Sqlite;
        config.log.sqlite_path = path.clone();
        config.log.enabled = true;
    }
    if let Some(enabled) = overrides.log_enabled {
        config.log.enabled = enabled;
    }
    if let Some(path) = &overrides.consent_sqlite_path {
        config.consent.backend = ConsentBackend::Sqlite;
        config.consent.sqlite_path = path.clone();
    }
    if let Some(enabled) = overrides.consent_enabled {
        config.consent.enabled = enabled;
    }
    if let Some(path) = &overrides.traffic_profile_path {
        config.traffic.profile = load_traffic_profile(path)?;
        config.traffic.profile_path = Some(path.clone());
    }
    if let Some(app_ids) = &overrides.traffic_enabled_app_ids {
        config.traffic.enabled_app_ids = Some(normalized_string_list(app_ids.clone()));
    }
    if let Some(addr) = &overrides.web_addr {
        config.web.addr = addr.clone();
    }
    if let Some(enabled) = overrides.proxy_enabled {
        config.proxy.enabled = enabled;
    }
    if let Some(listen) = &overrides.proxy_listen {
        config.proxy.listen = listen.clone();
    }
    if let Some(resolve_inbound) = overrides.proxy_resolve_inbound {
        config.proxy.resolve_inbound = resolve_inbound;
    }
    if let Some(targets) = &overrides.proxy_targets {
        config.proxy.targets = targets.clone();
    } else if proxy_target_override_is_present(overrides) {
        let target = ensure_first_proxy_target(&mut config.proxy);
        if let Some(name) = &overrides.proxy_target_name {
            target.name = name.clone();
        }
        if let Some(provider) = &overrides.proxy_target_provider {
            target.provider = provider.clone();
            if provider == GENERIC_HTTP_PROVIDER && overrides.proxy_target_api_key_env.is_none() {
                target.api_key_env = None;
            }
        }
        if let Some(upstream) = &overrides.proxy_target_upstream {
            target.upstream = upstream.clone();
        }
        if let Some(failure_mode) = overrides.proxy_target_failure_mode {
            target.failure_mode = Some(failure_mode);
        }
        if let Some(api_key_env) = &overrides.proxy_target_api_key_env {
            target.api_key_env = non_empty(api_key_env.clone());
        }
    }
    Ok(())
}

fn resolve_secrets(config: &mut DamConfig, env: &BTreeMap<String, String>) {
    config.vault.token = config.vault.token_env.as_ref().and_then(|env_var| {
        env.get(env_var)
            .map(|value| SecretValue::new(env_var, value))
    });

    config.log.token = config.log.token_env.as_ref().and_then(|env_var| {
        env.get(env_var)
            .map(|value| SecretValue::new(env_var, value))
    });

    for target in &mut config.proxy.targets {
        target.api_key = target.api_key_env.as_ref().and_then(|env_var| {
            env.get(env_var)
                .map(|value| SecretValue::new(env_var, value))
        });
    }
}

fn validate(config: &DamConfig) -> Result<(), ConfigError> {
    match config.vault.backend {
        VaultBackend::Sqlite => require_path("vault.path", &config.vault.sqlite_path)?,
        VaultBackend::Remote => {
            require_some("vault.url", &config.vault.remote_url)?;
            require_some("vault.token", &config.vault.token)?;
            require_non_zero("vault.timeout_ms", config.vault.timeout_ms)?;
        }
    }

    match config.log.backend {
        LogBackend::None => {}
        LogBackend::Sqlite => require_path("log.path", &config.log.sqlite_path)?,
        LogBackend::Remote => {
            require_some("log.url", &config.log.remote_url)?;
            if config.log.enabled {
                require_some("log.token", &config.log.token)?;
            }
            require_non_zero("log.timeout_ms", config.log.timeout_ms)?;
        }
    }

    if config.consent.enabled {
        match config.consent.backend {
            ConsentBackend::Sqlite => require_path("consent.path", &config.consent.sqlite_path)?,
        }
        require_non_zero(
            "consent.default_ttl_seconds",
            config.consent.default_ttl_seconds,
        )?;
    }

    validate_traffic(&config.traffic)?;

    if config.web.addr.trim().is_empty() {
        return Err(ConfigError::MissingRequired { field: "web.addr" });
    }
    require_loopback_socket("web.addr", &config.web.addr)?;

    if config.proxy.listen.trim().is_empty() {
        return Err(ConfigError::MissingRequired {
            field: "proxy.listen",
        });
    }
    require_loopback_socket("proxy.listen", &config.proxy.listen)?;
    if config.proxy.enabled {
        if config.proxy.targets.is_empty() {
            return Err(ConfigError::MissingRequired {
                field: "proxy.targets",
            });
        }
        for target in &config.proxy.targets {
            if target.name.trim().is_empty() {
                return Err(ConfigError::MissingRequired {
                    field: "proxy.targets.name",
                });
            }
            if target.provider.trim().is_empty() {
                return Err(ConfigError::MissingRequired {
                    field: "proxy.targets.provider",
                });
            }
            if target.upstream.trim().is_empty() {
                return Err(ConfigError::MissingRequired {
                    field: "proxy.targets.upstream",
                });
            }
            validate_proxy_target_upstream(&target.upstream)?;
            validate_upstream_auth(&target.auth)?;
        }
    }

    Ok(())
}

fn validate_proxy_target_upstream(upstream: &str) -> Result<(), ConfigError> {
    if upstream.trim() != upstream {
        return Err(ConfigError::invalid_value(
            "proxy.targets.upstream",
            upstream,
            "must not contain leading or trailing whitespace",
        ));
    }

    let Some((scheme, remainder)) = upstream.split_once("://") else {
        return Err(ConfigError::invalid_value(
            "proxy.targets.upstream",
            upstream,
            "must be an absolute http(s) URL",
        ));
    };
    if !matches!(scheme, "http" | "https") {
        return Err(ConfigError::invalid_value(
            "proxy.targets.upstream",
            upstream,
            "scheme must be http or https",
        ));
    }

    let authority = remainder
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.contains('@') {
        return Err(ConfigError::invalid_value(
            "proxy.targets.upstream",
            upstream,
            "userinfo is not allowed",
        ));
    }
    let authority_host = authority;
    if authority.is_empty() || authority.chars().any(char::is_whitespace) {
        return Err(ConfigError::invalid_value(
            "proxy.targets.upstream",
            upstream,
            "host is required",
        ));
    }

    if let Some(remainder) = authority_host.strip_prefix('[') {
        let Some((host, port_suffix)) = remainder.split_once(']') else {
            return Err(ConfigError::invalid_value(
                "proxy.targets.upstream",
                upstream,
                "IPv6 hosts must be enclosed in brackets",
            ));
        };
        if host.trim().is_empty() {
            return Err(ConfigError::invalid_value(
                "proxy.targets.upstream",
                upstream,
                "host is required",
            ));
        }
        if !port_suffix.is_empty() {
            let Some(port) = port_suffix.strip_prefix(':') else {
                return Err(ConfigError::invalid_value(
                    "proxy.targets.upstream",
                    upstream,
                    "IPv6 hosts must be enclosed in brackets",
                ));
            };
            validate_proxy_target_port(upstream, port)?;
        }
        return Ok(());
    }

    let (host, port) = authority_host
        .rsplit_once(':')
        .map(|(host, port)| (host, Some(port)))
        .unwrap_or((authority_host, None));
    if host.trim().is_empty() {
        return Err(ConfigError::invalid_value(
            "proxy.targets.upstream",
            upstream,
            "host is required",
        ));
    }
    if host.contains(':') {
        return Err(ConfigError::invalid_value(
            "proxy.targets.upstream",
            upstream,
            "IPv6 hosts must be enclosed in brackets",
        ));
    }
    if let Some(port) = port {
        validate_proxy_target_port(upstream, port)?;
    }

    Ok(())
}

fn validate_proxy_target_port(upstream: &str, port: &str) -> Result<(), ConfigError> {
    if port.is_empty() || port.parse::<u16>().is_err() {
        return Err(ConfigError::invalid_value(
            "proxy.targets.upstream",
            upstream,
            "port must be a valid numeric TCP port",
        ));
    }

    Ok(())
}

fn validate_upstream_auth(auth: &dam_net::UpstreamAuthConfig) -> Result<(), ConfigError> {
    for header in &auth.caller_headers {
        if header.trim().is_empty() {
            return Err(ConfigError::MissingRequired {
                field: "proxy.targets.auth.caller_headers",
            });
        }
    }
    if let Some(inject) = &auth.inject {
        if inject.header.trim().is_empty() {
            return Err(ConfigError::MissingRequired {
                field: "proxy.targets.auth.inject.header",
            });
        }
        for header in &inject.strip_headers {
            if header.trim().is_empty() {
                return Err(ConfigError::MissingRequired {
                    field: "proxy.targets.auth.inject.strip_headers",
                });
            }
        }
    }
    Ok(())
}

fn validate_traffic(traffic: &TrafficConfig) -> Result<(), ConfigError> {
    let profile_path = traffic
        .profile_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("<builtin>"));
    dam_net::validate_traffic_profile(&traffic.profile).map_err(|error| {
        ConfigError::InvalidTrafficProfile {
            path: profile_path.clone(),
            message: error.to_string(),
        }
    })?;
    if let Some(enabled_app_ids) = &traffic.enabled_app_ids {
        let app_ids = traffic
            .profile
            .apps
            .iter()
            .map(|app| app.id.as_str())
            .collect::<BTreeSet<_>>();
        for app_id in enabled_app_ids {
            if !app_ids.contains(app_id.as_str()) {
                return Err(ConfigError::InvalidTrafficProfile {
                    path: profile_path.clone(),
                    message: format!(
                        "enabled app id {app_id} is not present in the traffic profile"
                    ),
                });
            }
        }
    }

    Ok(())
}

fn require_loopback_socket(field: &'static str, value: &str) -> Result<(), ConfigError> {
    let addr = value
        .parse::<SocketAddr>()
        .map_err(|_| ConfigError::invalid_value(field, value, "expected ip:port socket address"))?;
    if !addr.ip().is_loopback() {
        return Err(ConfigError::invalid_value(
            field,
            value,
            "expected loopback address",
        ));
    }
    Ok(())
}

fn parse_proxy_target(raw: RawProxyTargetConfig) -> Result<ProxyTargetConfig, ConfigError> {
    let failure_mode = raw.failure_mode.map(|mode| mode.parse()).transpose()?;

    Ok(ProxyTargetConfig {
        name: raw.name.unwrap_or_default(),
        provider: raw.provider.unwrap_or_default(),
        upstream: raw.upstream.unwrap_or_default(),
        auth: raw.auth.unwrap_or_default(),
        failure_mode,
        api_key_env: raw.api_key_env.and_then(non_empty),
        api_key: None,
    })
}

fn load_traffic_profile(path: &Path) -> Result<dam_net::TrafficProfile, ConfigError> {
    let raw = fs::read_to_string(path).map_err(|source| ConfigError::ReadTrafficProfile {
        path: path.to_path_buf(),
        source,
    })?;
    dam_net::traffic_profile_from_json_str(&raw).map_err(|source| {
        ConfigError::ParseTrafficProfile {
            path: path.to_path_buf(),
            source,
        }
    })
}

fn resolve_relative_path(base_dir: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn parse_csv_list(value: &str) -> Vec<String> {
    normalized_string_list(value.split(',').map(str::to_string).collect())
}

fn normalized_string_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if !value.is_empty()
            && !normalized
                .iter()
                .any(|existing: &String| existing.as_str() == value)
        {
            normalized.push(value.to_string());
        }
    }
    normalized
}

fn proxy_target_env_is_present(env: &BTreeMap<String, String>) -> bool {
    [
        "DAM_PROXY_TARGET_NAME",
        "DAM_PROXY_TARGET_PROVIDER",
        "DAM_PROXY_TARGET_UPSTREAM",
        "DAM_PROXY_TARGET_FAILURE_MODE",
        "DAM_PROXY_TARGET_API_KEY_ENV",
    ]
    .iter()
    .any(|key| env.contains_key(*key))
}

fn proxy_target_override_is_present(overrides: &ConfigOverrides) -> bool {
    overrides.proxy_target_name.is_some()
        || overrides.proxy_target_provider.is_some()
        || overrides.proxy_target_upstream.is_some()
        || overrides.proxy_target_failure_mode.is_some()
        || overrides.proxy_target_api_key_env.is_some()
}

fn ensure_first_proxy_target(proxy: &mut ProxyConfig) -> &mut ProxyTargetConfig {
    if proxy.targets.is_empty() {
        proxy
            .targets
            .push(ProxyTargetConfig::default_profile_target());
    }

    &mut proxy.targets[0]
}

fn first_env<'a>(env: &'a BTreeMap<String, String>, keys: &[&str]) -> Option<&'a String> {
    keys.iter().find_map(|key| env.get(*key))
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_bool(field: &'static str, value: &str) -> Result<bool, ConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::invalid_value(
            field,
            value,
            "expected true/false, 1/0, yes/no, or on/off",
        )),
    }
}

fn parse_u64(field: &'static str, value: &str) -> Result<u64, ConfigError> {
    value
        .parse::<u64>()
        .map_err(|_| ConfigError::invalid_value(field, value, "expected a positive integer"))
}

fn parse_policy_action(
    field: &'static str,
    value: &str,
) -> Result<dam_core::PolicyAction, ConfigError> {
    dam_core::PolicyAction::from_tag(value).ok_or_else(|| {
        ConfigError::invalid_value(field, value, "expected tokenize, redact, allow, or block")
    })
}

fn parse_sensitive_type(
    field: &'static str,
    value: &str,
) -> Result<dam_core::SensitiveType, ConfigError> {
    dam_core::SensitiveType::from_tag(value).ok_or_else(|| {
        ConfigError::invalid_value(field, value, "expected email, domain, phone, ssn, or cc")
    })
}

fn require_path(field: &'static str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        Err(ConfigError::MissingRequired { field })
    } else {
        Ok(())
    }
}

fn require_some<T>(field: &'static str, value: &Option<T>) -> Result<(), ConfigError> {
    if value.is_none() {
        Err(ConfigError::MissingRequired { field })
    } else {
        Ok(())
    }
}

fn require_non_zero(field: &'static str, value: u64) -> Result<(), ConfigError> {
    if value == 0 {
        Err(ConfigError::invalid_value(
            field,
            value.to_string(),
            "expected a positive integer",
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDamConfig {
    vault: Option<RawVaultConfig>,
    log: Option<RawLogConfig>,
    consent: Option<RawConsentConfig>,
    policy: Option<RawPolicyConfig>,
    failure: Option<RawFailureConfig>,
    traffic: Option<RawTrafficConfig>,
    network: Option<RawNetworkConfig>,
    web: Option<RawWebConfig>,
    proxy: Option<RawProxyConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVaultConfig {
    backend: Option<String>,
    path: Option<PathBuf>,
    url: Option<String>,
    token_env: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLogConfig {
    enabled: Option<bool>,
    backend: Option<String>,
    path: Option<PathBuf>,
    url: Option<String>,
    token_env: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConsentConfig {
    enabled: Option<bool>,
    backend: Option<String>,
    path: Option<PathBuf>,
    default_ttl_seconds: Option<u64>,
    mcp_write_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicyConfig {
    default_action: Option<String>,
    deduplicate_replacements: Option<bool>,
    kind: Option<BTreeMap<String, RawPolicyKindConfig>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicyKindConfig {
    action: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFailureConfig {
    vault_write: Option<String>,
    log_write: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTrafficConfig {
    profile_path: Option<PathBuf>,
    enabled_apps: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNetworkConfig {
    ai_routes: Option<Vec<toml::Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWebConfig {
    addr: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProxyConfig {
    enabled: Option<bool>,
    listen: Option<String>,
    mode: Option<String>,
    default_failure_mode: Option<String>,
    resolve_inbound: Option<bool>,
    targets: Option<Vec<RawProxyTargetConfig>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProxyTargetConfig {
    name: Option<String>,
    provider: Option<String>,
    upstream: Option<String>,
    auth: Option<dam_net::UpstreamAuthConfig>,
    failure_mode: Option<String>,
    api_key_env: Option<String>,
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
