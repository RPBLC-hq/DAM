mod action;
mod builtin;
mod matcher;

use std::collections::{BTreeMap, BTreeSet};

pub use action::{
    SensitiveDataAction, TrafficAction, TrafficDirectionPolicy, TrafficFilterPolicy,
    TrafficInboundPolicy,
};
pub use builtin::llm_mvp_profile;
pub use matcher::TrafficMatch;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ProtocolAdapterKind, TrafficObservation, TrafficRoute, UpstreamAuthConfig};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficProfile {
    #[serde(default = "default_profile_version")]
    pub version: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub default_action: TrafficAction,
    #[serde(default)]
    pub apps: Vec<TrafficAppProfile>,
}

impl Default for TrafficProfile {
    fn default() -> Self {
        Self {
            version: default_profile_version(),
            name: None,
            default_action: TrafficAction::Bypass,
            apps: Vec::new(),
        }
    }
}

impl TrafficProfile {
    pub fn with_runtime_enabled_apps(&self, app_ids: &[String]) -> Self {
        let app_ids = app_ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let mut profile = self.clone();
        for app in &mut profile.apps {
            app.enabled = app.enabled && app_ids.contains(app.id.as_str());
        }
        profile
    }

    pub fn app(&self, id: &str) -> Option<&TrafficAppProfile> {
        self.apps.iter().find(|app| app.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficAppProfile {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(rename = "match", default)]
    pub match_rules: TrafficMatch,
    #[serde(default)]
    pub action: TrafficAction,
    #[serde(default)]
    pub adapter: ProtocolAdapterKind,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub target_name: Option<String>,
    #[serde(default)]
    pub upstream: Option<String>,
    #[serde(default)]
    pub auth: UpstreamAuthConfig,
    #[serde(default)]
    pub steps: Vec<TrafficPipelineStep>,
    #[serde(default)]
    pub outbound: TrafficDirectionPolicy,
    #[serde(default)]
    pub inbound: TrafficInboundPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficPipelineStep {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub direction: TrafficStepDirection,
    #[serde(default)]
    pub config: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrafficStepDirection {
    #[default]
    Both,
    Outbound,
    Inbound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum TrafficProfileDecision {
    Matched {
        app_id: String,
        action: TrafficAction,
        adapter: ProtocolAdapterKind,
    },
    Default {
        action: TrafficAction,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum TrafficProfileError {
    #[error("failed to parse traffic profile JSON: {0}")]
    Parse(serde_json::Error),

    #[error("invalid traffic profile app {app_id}: {message}")]
    InvalidApp { app_id: String, message: String },

    #[error("duplicate traffic profile app id: {0}")]
    DuplicateAppId(String),
}

pub fn traffic_profile_from_json_str(raw: &str) -> Result<TrafficProfile, TrafficProfileError> {
    let profile =
        serde_json::from_str::<TrafficProfile>(raw).map_err(TrafficProfileError::Parse)?;
    validate_traffic_profile(&profile)?;
    Ok(profile)
}

pub fn validate_traffic_profile(profile: &TrafficProfile) -> Result<(), TrafficProfileError> {
    let mut ids = BTreeSet::new();
    for app in &profile.apps {
        if app.id.trim().is_empty() {
            return Err(TrafficProfileError::InvalidApp {
                app_id: app.id.clone(),
                message: "id is required".to_string(),
            });
        }
        if !ids.insert(app.id.clone()) {
            return Err(TrafficProfileError::DuplicateAppId(app.id.clone()));
        }
        if app.enabled && app.action == TrafficAction::Inspect && app.match_rules.is_empty() {
            return Err(TrafficProfileError::InvalidApp {
                app_id: app.id.clone(),
                message: "inspect apps require at least one match rule".to_string(),
            });
        }
        if app.enabled && app.action == TrafficAction::Inspect {
            require_non_empty(app, "provider", app.provider.as_deref())?;
            require_non_empty(app, "upstream", app.upstream.as_deref())?;
        }
    }
    Ok(())
}

pub fn decide_profile_traffic(
    profile: &TrafficProfile,
    observation: &TrafficObservation,
) -> TrafficProfileDecision {
    let matched = profile
        .apps
        .iter()
        .filter(|app| app.enabled && app.match_rules.matches(observation))
        .max_by_key(|app| app.priority);

    match matched {
        Some(app) => TrafficProfileDecision::Matched {
            app_id: app.id.clone(),
            action: app.action,
            adapter: app.adapter,
        },
        None => TrafficProfileDecision::Default {
            action: profile.default_action,
        },
    }
}

pub fn traffic_routes_from_profile(profile: &TrafficProfile) -> Vec<TrafficRoute> {
    let mut routes = Vec::new();
    for app in &profile.apps {
        if !app.enabled || app.action != TrafficAction::Inspect {
            continue;
        }
        let Some(provider) = app
            .provider
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let Some(upstream) = app
            .upstream
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let target_name = app
            .target_name
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&app.id);
        for domain in app.match_rules.normalized_route_hosts() {
            routes.push(TrafficRoute::new_with_auth(
                app.adapter,
                domain,
                provider.clone(),
                target_name.clone(),
                upstream.clone(),
                app.auth.clone(),
            ));
        }
    }
    dedupe_routes(routes)
}

fn dedupe_routes(routes: Vec<TrafficRoute>) -> Vec<TrafficRoute> {
    let mut deduped = Vec::<TrafficRoute>::new();
    for route in routes {
        if let Some(existing) = deduped
            .iter_mut()
            .find(|existing| existing.host == route.host)
        {
            *existing = route;
        } else {
            deduped.push(route);
        }
    }
    deduped
}

fn require_non_empty(
    app: &TrafficAppProfile,
    field: &'static str,
    value: Option<&str>,
) -> Result<(), TrafficProfileError> {
    if value.is_some_and(|value| !value.trim().is_empty()) {
        Ok(())
    } else {
        Err(TrafficProfileError::InvalidApp {
            app_id: app.id.clone(),
            message: format!("inspect apps require {field}"),
        })
    }
}

fn default_profile_version() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
