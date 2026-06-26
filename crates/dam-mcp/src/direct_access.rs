use super::{MIN_DIRECT_ACCESS_DURATION_SECONDS, entries_to_json, open_consent_store, open_vault};
use serde_json::{Value, json};
use sha2::Digest;
use std::env;

#[derive(Debug, Clone)]
pub(super) struct ActorBinding {
    pub(super) actor_id: String,
    pub(super) label: String,
}

pub(super) fn tool_definitions(config: &dam_config::DamConfig) -> Vec<Value> {
    let mut tools = vec![json!({
        "name": "dam_consent_request_status",
        "description": "Inspect a pending or resolved DAM direct value-access request.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "request_id": { "type": "string" },
                "grant_id": { "type": "string" }
            }
        }
    })];

    if config.consent.mcp_write_enabled {
        tools.push(json!({
            "name": "dam_consent_revoke",
            "description": "Revoke a DAM passthrough consent or direct-access request/grant.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "consent_id": { "type": "string" },
                    "request_id": { "type": "string" },
                    "grant_id": { "type": "string" },
                    "reason": { "type": "string" }
                }
            }
        }));
        tools.push(json!({
            "name": "dam_consent_request",
            "description": "Create a pending DAM direct value-access request bound to the local MCP actor.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "vault_key": { "type": "string" },
                    "purpose": { "type": "string" },
                    "duration_seconds": { "type": "integer" },
                    "reason": { "type": "string" },
                    "correlation_id": { "type": "string" }
                },
                "required": ["vault_key", "purpose", "duration_seconds"]
            }
        }));
        tools.push(json!({
            "name": "dam_resolve_if_consented",
            "description": "Return a raw value only when an active DAM direct-access grant authorizes this MCP actor.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "request_id": { "type": "string" },
                    "grant_id": { "type": "string" }
                }
            }
        }));
    }

    tools
}

pub(super) fn maybe_call_tool(
    config: &dam_config::DamConfig,
    name: &str,
    arguments: &Value,
    actor: Option<ActorBinding>,
) -> Option<Result<String, String>> {
    match name {
        "dam_consent_list" => Some(list_tool(config)),
        "dam_consent_revoke" if config.consent.mcp_write_enabled => {
            Some(revoke_tool(config, arguments))
        }
        "dam_consent_request" if config.consent.mcp_write_enabled => {
            Some(request_tool(config, arguments, actor))
        }
        "dam_consent_request_status" => Some(status_tool(config, arguments)),
        "dam_resolve_if_consented" if config.consent.mcp_write_enabled => {
            Some(resolve_tool(config, arguments, actor))
        }
        _ => None,
    }
}

pub(super) fn bound_actor_binding() -> Option<ActorBinding> {
    bound_actor_binding_from_values(
        env::var("DAM_MCP_ACTOR_ID").ok(),
        env::var("DAM_MCP_ACTOR_LABEL").ok(),
    )
}

pub(super) fn bound_actor_binding_from_values(
    actor_id: Option<String>,
    label: Option<String>,
) -> Option<ActorBinding> {
    let actor_id = actor_id.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });
    let label = label.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });
    match (actor_id, label) {
        (Some(actor_id), Some(label)) => Some(ActorBinding { actor_id, label }),
        (Some(actor_id), None) => Some(ActorBinding {
            label: actor_id.clone(),
            actor_id,
        }),
        (None, Some(label)) => Some(ActorBinding {
            actor_id: label_bound_actor_id(&label),
            label,
        }),
        (None, None) => None,
    }
}

pub(super) fn label_bound_actor_id(label: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"dam-mcp-actor-v1\0");
    hasher.update(label.trim().as_bytes());
    format!(
        "mcp-actor:{}",
        bs58::encode(sha2::Digest::finalize(hasher)).into_string()
    )
}

fn list_tool(config: &dam_config::DamConfig) -> Result<String, String> {
    let store = open_consent_store(config)?;
    let entries = store.list().map_err(|error| error.to_string())?;
    let direct_access = store
        .list_direct_access_requests()
        .map_err(|error| error.to_string())?;
    Ok(serde_json::to_string(&json!({
        "consents": entries_to_json(&entries),
        "direct_access_requests": direct_access.iter().map(direct_access_request_to_json).collect::<Vec<_>>()
    }))
    .unwrap())
}

fn revoke_tool(config: &dam_config::DamConfig, arguments: &Value) -> Result<String, String> {
    let store = open_consent_store(config)?;
    if let Some(consent_id) = arguments.get("consent_id").and_then(Value::as_str) {
        let revoked = store
            .revoke(consent_id)
            .map_err(|error| error.to_string())?;
        return Ok(
            serde_json::to_string(&json!({ "revoked": revoked, "kind": "passthrough" })).unwrap(),
        );
    }
    let request_id = request_lookup_id(arguments)?;
    let revoked = store
        .revoke_direct_access_request(
            &request_id,
            arguments
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string),
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "request_id or grant_id not found".to_string())?;
    Ok(serde_json::to_string(&json!({
        "revoked": true,
        "kind": "direct_access",
        "request": direct_access_request_to_json(&revoked)
    }))
    .unwrap())
}

fn request_tool(
    config: &dam_config::DamConfig,
    arguments: &Value,
    actor: Option<ActorBinding>,
) -> Result<String, String> {
    let actor = actor.ok_or_else(|| {
        "direct value-access tools require DAM_MCP_ACTOR_LABEL or DAM_MCP_ACTOR_ID actor binding"
            .to_string()
    })?;
    let vault_key = arguments
        .get("vault_key")
        .and_then(Value::as_str)
        .ok_or_else(|| "vault_key is required".to_string())?;
    let purpose = arguments
        .get("purpose")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "purpose is required".to_string())?;
    let duration_seconds = arguments
        .get("duration_seconds")
        .and_then(Value::as_u64)
        .ok_or_else(|| "duration_seconds is required".to_string())?;
    if duration_seconds < MIN_DIRECT_ACCESS_DURATION_SECONDS {
        return Err(format!(
            "duration_seconds must be at least {MIN_DIRECT_ACCESS_DURATION_SECONDS}"
        ));
    }
    if duration_seconds > config.consent.max_request_duration_seconds {
        return Err(format!(
            "duration_seconds must be <= {}",
            config.consent.max_request_duration_seconds
        ));
    }

    let store = open_consent_store(config)?;
    let vault = open_vault(config)?;
    let request = store
        .create_direct_access_request(
            &dam_consent::CreateDirectAccessRequest {
                vault_key: vault_key.to_string(),
                actor_id: actor.actor_id,
                requesting_actor: actor.label,
                purpose: purpose.to_string(),
                reason: arguments
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                requested_duration_seconds: duration_seconds,
                pending_timeout_seconds: config.consent.pending_timeout_seconds,
                correlation_id: arguments
                    .get("correlation_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            },
            &vault,
        )
        .map_err(|error| error.to_string())?;
    Ok(serde_json::to_string(&direct_access_request_to_json(&request)).unwrap())
}

fn status_tool(config: &dam_config::DamConfig, arguments: &Value) -> Result<String, String> {
    let store = open_consent_store(config)?;
    let request_id = request_lookup_id(arguments)?;
    let request = store
        .direct_access_request(&request_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "request_id or grant_id not found".to_string())?;
    Ok(serde_json::to_string(&direct_access_request_to_json(&request)).unwrap())
}

fn resolve_tool(
    config: &dam_config::DamConfig,
    arguments: &Value,
    actor: Option<ActorBinding>,
) -> Result<String, String> {
    let actor = actor.ok_or_else(|| {
        "direct value-access tools require DAM_MCP_ACTOR_LABEL or DAM_MCP_ACTOR_ID actor binding"
            .to_string()
    })?;
    let store = open_consent_store(config)?;
    let vault = open_vault(config)?;
    let request_id = request_lookup_id(arguments)?;
    let result = store
        .resolve_direct_access_request(&request_id, &actor.actor_id, &vault)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "request_id or grant_id not found".to_string())?;
    Ok(serde_json::to_string(&json!({
        "request": direct_access_request_to_json(&result.request),
        "outcome_reason": result.outcome_reason,
        "value": result.value,
    }))
    .unwrap())
}

fn request_lookup_id(arguments: &Value) -> Result<String, String> {
    arguments
        .get("request_id")
        .and_then(Value::as_str)
        .or_else(|| arguments.get("grant_id").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| "request_id or grant_id is required".to_string())
}

fn direct_access_request_to_json(entry: &dam_consent::DirectAccessRequest) -> Value {
    json!({
        "request_id": entry.request_id,
        "grant_id": entry.grant_id,
        "status": entry.status.tag(),
        "kind": entry.kind.tag(),
        "vault_key": entry.vault_key,
        "purpose": entry.purpose,
        "reason": entry.reason,
        "decision_reason": entry.decision_reason,
        "requested_duration_seconds": entry.requested_duration_seconds,
        "pending_expires_at": entry.pending_expires_at,
        "grant_expires_at": entry.grant_expires_at,
        "created_at": entry.created_at,
        "decided_at": entry.decided_at,
        "max_resolves": entry.max_resolves,
        "resolve_count": entry.resolve_count,
        "correlation_id": entry.correlation_id,
    })
}
