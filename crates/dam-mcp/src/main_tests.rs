use super::*;
use dam_core::{Reference, SensitiveType, VaultRecord, VaultWriter};

#[test]
fn lists_tools() {
    let config = dam_config::DamConfig::default();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
    let response = handle_message(&config, &request).unwrap();

    assert!(response.to_string().contains("dam_consent_grant"));
    assert!(response.to_string().contains("dam_consent_revoke"));
    assert!(response.to_string().contains("dam_consent_request"));
    assert!(response.to_string().contains("dam_consent_request_status"));
    assert!(response.to_string().contains("dam_resolve_if_consented"));
    assert!(response.to_string().contains("dam_setup_rescue"));
    assert!(response.to_string().contains("dam_setup_repair"));
    assert!(
        response
            .to_string()
            .contains("dam_setup_export_diagnostics")
    );
}

#[test]
fn setup_rescue_apply_requires_explicit_confirmation() {
    let config = dam_config::DamConfig::default();

    let error = call_tool(&config, "dam_setup_rescue", &json!({ "apply": true })).unwrap_err();

    assert!(error.contains("confirm must be remove_dam_network_setup"));
}

#[test]
fn setup_repair_apply_requires_explicit_confirmation() {
    let config = dam_config::DamConfig::default();

    let error = call_tool(&config, "dam_setup_repair", &json!({ "apply": true })).unwrap_err();

    assert!(error.contains("confirm must be remove_dam_network_setup"));
}

#[test]
fn direct_access_tools_require_actor_binding_and_validate_duration() {
    let config = dam_config::DamConfig::default();

    let missing_actor = call_tool_with_actor(
        &config,
        "dam_consent_request",
        &json!({
            "vault_key": "email:abc",
            "purpose": "fill local form",
            "duration_seconds": 30
        }),
        None,
    )
    .unwrap_err();
    assert!(missing_actor.contains("DAM_MCP_ACTOR_LABEL"));

    let short_duration = call_tool_with_actor(
        &config,
        "dam_consent_request",
        &json!({
            "vault_key": "email:abc",
            "purpose": "fill local form",
            "duration_seconds": 10
        }),
        Some(ActorBinding {
            actor_id: "actor-1".to_string(),
            label: "Codex".to_string(),
        }),
    )
    .unwrap_err();
    assert!(short_duration.contains("at least 30"));
}

#[test]
fn direct_access_tools_accept_id_only_actor_binding_and_hash_label_only_binding() {
    let id_only = bound_actor_binding_from_values(Some("actor-1".to_string()), None).unwrap();
    assert_eq!(id_only.actor_id, "actor-1");
    assert_eq!(id_only.label, "actor-1");

    let label_only = bound_actor_binding_from_values(None, Some(" Codex ".to_string())).unwrap();
    assert_eq!(label_only.label, "Codex");
    assert_eq!(label_only.actor_id, label_bound_actor_id("Codex"));
    assert_ne!(
        label_only.actor_id,
        format!(
            "mcp-actor:{}",
            dam_consent::fingerprint(SensitiveType::Email, "Codex")
        )
    );
}

#[test]
fn direct_access_request_status_and_resolve_flow() {
    let dir = tempfile::tempdir().unwrap();
    let vault_path = dir.path().join("vault.db");
    let consent_path = dir.path().join("consent.db");
    let mut config = dam_config::DamConfig::default();
    config.vault.sqlite_path = vault_path.clone();
    config.consent.sqlite_path = consent_path.clone();
    config.consent.pending_timeout_seconds = 60;
    config.consent.max_request_duration_seconds = 300;

    let vault = dam_vault::Vault::open(&vault_path).unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let actor = ActorBinding {
        actor_id: "actor-1".to_string(),
        label: "Codex".to_string(),
    };
    let request_json = call_tool_with_actor(
        &config,
        "dam_consent_request",
        &json!({
            "vault_key": reference.key(),
            "purpose": "fill local form",
            "duration_seconds": 45,
            "reason": "user asked to paste email locally"
        }),
        Some(actor.clone()),
    )
    .unwrap();
    let request: Value = serde_json::from_str(&request_json).unwrap();
    let request_id = request["request_id"].as_str().unwrap().to_string();
    assert_eq!(request["status"], "pending");

    let status_json = call_tool_with_actor(
        &config,
        "dam_consent_request_status",
        &json!({ "request_id": request_id }),
        Some(actor.clone()),
    )
    .unwrap();
    let status: Value = serde_json::from_str(&status_json).unwrap();
    assert_eq!(status["status"], "pending");

    let store = dam_consent::ConsentStore::open(&consent_path).unwrap();
    store
        .approve_direct_access_request(request_id.as_str(), 45, Some("approved".to_string()))
        .unwrap()
        .unwrap();

    let resolve_json = call_tool_with_actor(
        &config,
        "dam_resolve_if_consented",
        &json!({ "request_id": request_id }),
        Some(actor.clone()),
    )
    .unwrap();
    let resolved: Value = serde_json::from_str(&resolve_json).unwrap();
    assert_eq!(resolved["value"], "alice@example.test");
    assert_eq!(resolved["request"]["status"], "consumed");

    let denied_json = call_tool_with_actor(
        &config,
        "dam_resolve_if_consented",
        &json!({ "request_id": request_id }),
        Some(ActorBinding {
            actor_id: "actor-2".to_string(),
            label: "Claude".to_string(),
        }),
    )
    .unwrap();
    let denied: Value = serde_json::from_str(&denied_json).unwrap();
    assert_eq!(denied["value"], Value::Null);
}

#[test]
fn status_tool_available_when_mcp_write_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let vault_path = dir.path().join("vault.db");
    let consent_path = dir.path().join("consent.db");
    let mut config = dam_config::DamConfig::default();
    config.vault.sqlite_path = vault_path.clone();
    config.consent.sqlite_path = consent_path.clone();
    config.consent.pending_timeout_seconds = 60;
    config.consent.max_request_duration_seconds = 300;

    let vault = dam_vault::Vault::open(&vault_path).unwrap();
    let reference = Reference::generate(SensitiveType::Email);
    vault
        .write(&VaultRecord {
            reference: reference.clone(),
            kind: SensitiveType::Email,
            value: "alice@example.test".to_string(),
        })
        .unwrap();

    let store = dam_consent::ConsentStore::open(&consent_path).unwrap();
    let request = store
        .create_direct_access_request(
            &dam_consent::CreateDirectAccessRequest {
                vault_key: reference.key(),
                actor_id: "actor-1".to_string(),
                requesting_actor: "Codex".to_string(),
                purpose: "fill local form".to_string(),
                reason: None,
                requested_duration_seconds: 60,
                pending_timeout_seconds: 60,
                correlation_id: None,
            },
            &vault,
        )
        .unwrap();

    config.consent.mcp_write_enabled = false;

    let tool_list = tools(&config);
    let tool_names: Vec<_> = tool_list
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        tool_names.contains(&"dam_consent_request_status"),
        "status tool should be in list even when mcp_write_enabled=false"
    );
    assert!(
        !tool_names.contains(&"dam_consent_request"),
        "request tool should not be listed when mcp_write_enabled=false"
    );

    let status_json = call_tool_with_actor(
        &config,
        "dam_consent_request_status",
        &json!({ "request_id": request.request_id }),
        None,
    )
    .unwrap();
    let status: Value = serde_json::from_str(&status_json).unwrap();
    assert_eq!(status["status"], "pending");

    let err = call_tool_with_actor(
        &config,
        "dam_consent_request",
        &json!({
            "vault_key": reference.key(),
            "purpose": "fill local form",
            "duration_seconds": 60
        }),
        Some(ActorBinding {
            actor_id: "actor-1".to_string(),
            label: "Codex".to_string(),
        }),
    )
    .unwrap_err();
    assert!(err.contains("unknown or disabled"));
}

#[test]
fn parses_content_length_messages() {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let input = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

    let messages = parse_messages(&input);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["method"], "tools/list");
}

#[test]
fn stdio_handles_framed_messages_in_sequence() {
    let config = dam_config::DamConfig::default();
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
    let tools = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let input = format!(
        "Content-Length: {}\r\n\r\n{}Content-Length: {}\r\n\r\n{}",
        initialize.len(),
        initialize,
        tools.len(),
        tools
    );
    let mut output = Vec::new();

    run_stdio(&config, input.as_bytes(), &mut output).unwrap();

    let output = String::from_utf8(output).unwrap();
    let responses = parse_messages(&output);
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "dam-mcp");
    assert_eq!(responses[1]["id"], 2);
    assert!(responses[1].to_string().contains("dam_consent_list"));
}

#[test]
fn stdio_rejects_oversized_message_frames() {
    let config = dam_config::DamConfig::default();
    let input = format!("Content-Length: {}\r\n\r\n{{}}", MAX_MESSAGE_BYTES + 1);
    let mut output = Vec::new();

    let error = run_stdio(&config, input.as_bytes(), &mut output).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(output.is_empty());
}
