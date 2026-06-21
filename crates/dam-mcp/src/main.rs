use serde_json::{Value, json};
use std::env;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;

const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MIN_DIRECT_ACCESS_DURATION_SECONDS: u64 = 30;

#[derive(Debug, Clone, Default)]
struct CliArgs {
    config: dam_config::ConfigOverrides,
}

#[derive(Debug, Clone)]
struct ActorBinding {
    actor_id: String,
    label: String,
}

fn main() {
    let cli = match parse_args(env::args().skip(1)) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let config = match dam_config::load(&cli.config) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load config: {error}");
            std::process::exit(2);
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    if let Err(error) = run_stdio(&config, stdin.lock(), stdout.lock()) {
        eprintln!("stdio error: {error}");
        std::process::exit(1);
    }
}

fn run_stdio<R: Read, W: Write>(
    config: &dam_config::DamConfig,
    input: R,
    mut output: W,
) -> io::Result<()> {
    let mut reader = BufReader::new(input);
    while let Some(message) = read_message(&mut reader)? {
        if let Some(response) = handle_message(config, &message) {
            write_response(&mut output, &response)?;
        }
    }
    Ok(())
}

fn handle_message(config: &dam_config::DamConfig, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str)?;
    match method {
        "initialize" => Some(success(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "dam-mcp", "version": env!("CARGO_PKG_VERSION") }
            }),
        )),
        "notifications/initialized" => None,
        "tools/list" => Some(success(id, json!({ "tools": tools(config) }))),
        "tools/call" => Some(handle_tool_call(config, id, message)),
        _ => Some(error(id, -32601, "method not found")),
    }
}

fn tools(config: &dam_config::DamConfig) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": "dam_status",
            "description": "Inspect local DAM daemon status.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "dam_setup_plan",
            "description": "Return DAM's read-only local setup plan.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "dam_setup_next_action",
            "description": "Return the next idempotent DAM setup action.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "dam_setup_rescue",
            "description": "Preview or apply local DAM setup rescue. Applying stops DAM and removes DAM-managed network routing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "apply": { "type": "boolean" },
                    "confirm": {
                        "type": "string",
                        "description": "Required as remove_dam_network_setup when apply is true."
                    }
                }
            }
        }),
        json!({
            "name": "dam_setup_repair",
            "description": "Preview or apply rescue, then return the current setup plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "apply": { "type": "boolean" },
                    "confirm": {
                        "type": "string",
                        "description": "Required as remove_dam_network_setup when apply is true."
                    }
                }
            }
        }),
        json!({
            "name": "dam_setup_export_diagnostics",
            "description": "Export offline DAM setup diagnostics for agent repair workflows.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "dam_consent_list",
            "description": "List DAM passthrough consents.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ];

    if config.consent.mcp_write_enabled {
        tools.push(json!({
            "name": "dam_consent_grant",
            "description": "Grant passthrough consent for a DAM vault key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "vault_key": { "type": "string" },
                    "ttl_seconds": { "type": "integer" },
                    "reason": { "type": "string" }
                },
                "required": ["vault_key"]
            }
        }));
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
            "name": "dam_consent_request_status",
            "description": "Inspect a pending or resolved DAM direct value-access request.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "request_id": { "type": "string" },
                    "grant_id": { "type": "string" }
                }
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

fn handle_tool_call(config: &dam_config::DamConfig, id: Option<Value>, message: &Value) -> Value {
    let Some(params) = message.get("params") else {
        return error(id, -32602, "missing params");
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error(id, -32602, "missing tool name");
    };
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match call_tool(config, name, &arguments) {
        Ok(result) => success(
            id,
            json!({ "content": [{ "type": "text", "text": result }] }),
        ),
        Err(message) => success(
            id,
            json!({
                "isError": true,
                "content": [{ "type": "text", "text": message }]
            }),
        ),
    }
}

fn call_tool(
    config: &dam_config::DamConfig,
    name: &str,
    arguments: &Value,
) -> Result<String, String> {
    call_tool_with_actor(config, name, arguments, bound_actor_binding())
}

fn call_tool_with_actor(
    config: &dam_config::DamConfig,
    name: &str,
    arguments: &Value,
    actor: Option<ActorBinding>,
) -> Result<String, String> {
    match name {
        "dam_status" => dam_status_tool(),
        "dam_setup_plan" => dam_setup_plan_tool(config),
        "dam_setup_next_action" => dam_setup_next_action_tool(config),
        "dam_setup_rescue" => dam_setup_rescue_tool(arguments),
        "dam_setup_repair" => dam_setup_repair_tool(config, arguments),
        "dam_setup_export_diagnostics" => dam_setup_export_diagnostics_tool(config),
        "dam_consent_list" => {
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
        "dam_consent_grant" if config.consent.mcp_write_enabled => {
            let store = open_consent_store(config)?;
            let vault_key = arguments
                .get("vault_key")
                .and_then(Value::as_str)
                .ok_or_else(|| "vault_key is required".to_string())?;
            let ttl_seconds = arguments
                .get("ttl_seconds")
                .and_then(Value::as_u64)
                .unwrap_or(config.consent.default_ttl_seconds);
            let reason = arguments
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string);
            let vault = open_vault(config)?;
            let entry = store
                .grant_for_reference(vault_key, &vault, ttl_seconds, "dam-mcp", reason)
                .map_err(|error| error.to_string())?;
            Ok(serde_json::to_string(&entry_to_json(&entry)).unwrap())
        }
        "dam_consent_revoke" if config.consent.mcp_write_enabled => {
            let store = open_consent_store(config)?;
            if let Some(consent_id) = arguments.get("consent_id").and_then(Value::as_str) {
                let revoked = store
                    .revoke(consent_id)
                    .map_err(|error| error.to_string())?;
                return Ok(serde_json::to_string(
                    &json!({ "revoked": revoked, "kind": "passthrough" }),
                )
                .unwrap());
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
        "dam_consent_request" if config.consent.mcp_write_enabled => {
            dam_consent_request_tool(config, arguments, actor)
        }
        "dam_consent_request_status" if config.consent.mcp_write_enabled => {
            dam_consent_request_status_tool(config, arguments)
        }
        "dam_resolve_if_consented" if config.consent.mcp_write_enabled => {
            dam_resolve_if_consented_tool(config, arguments, actor)
        }
        _ => Err("unknown or disabled tool".to_string()),
    }
}

fn dam_setup_rescue_tool(arguments: &Value) -> Result<String, String> {
    let apply = confirmed_apply(arguments)?;
    let rescue = dam_diagnostics::setup_rescue(&dam_diagnostics::SetupRescueOptions {
        state_dir: None,
        proxy_url: None,
        apply,
    })?;
    Ok(serde_json::to_string(&rescue).unwrap())
}

fn dam_setup_repair_tool(
    config: &dam_config::DamConfig,
    arguments: &Value,
) -> Result<String, String> {
    let apply = confirmed_apply(arguments)?;
    let repair = dam_diagnostics::setup_repair(
        config,
        &dam_diagnostics::SetupRepairOptions {
            setup: dam_diagnostics::SetupPlanOptions::default(),
            apply,
        },
    )?;
    Ok(serde_json::to_string(&repair).unwrap())
}

fn dam_setup_export_diagnostics_tool(config: &dam_config::DamConfig) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to start diagnostics runtime: {error}"))?;
    let export = runtime.block_on(dam_diagnostics::setup_diagnostics_export(
        config,
        &dam_diagnostics::DoctorOptions::default(),
        &dam_diagnostics::SetupPlanOptions::default(),
    ))?;
    Ok(serde_json::to_string(&export).unwrap())
}

fn dam_consent_request_tool(
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

fn dam_consent_request_status_tool(
    config: &dam_config::DamConfig,
    arguments: &Value,
) -> Result<String, String> {
    let store = open_consent_store(config)?;
    let request_id = request_lookup_id(arguments)?;
    let request = store
        .direct_access_request(&request_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "request_id or grant_id not found".to_string())?;
    Ok(serde_json::to_string(&direct_access_request_to_json(&request)).unwrap())
}

fn dam_resolve_if_consented_tool(
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

fn confirmed_apply(arguments: &Value) -> Result<bool, String> {
    let apply = arguments
        .get("apply")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if apply && arguments.get("confirm").and_then(Value::as_str) != Some("remove_dam_network_setup")
    {
        return Err("confirm must be remove_dam_network_setup when apply is true".to_string());
    }
    Ok(apply)
}

fn dam_status_tool() -> Result<String, String> {
    let value = match dam_daemon::daemon_status().map_err(|error| error.to_string())? {
        dam_daemon::DaemonStatus::Disconnected => json!({
            "state": "disconnected",
            "message": "DAM is not connected"
        }),
        dam_daemon::DaemonStatus::Stale(state) => json!({
            "state": "stale",
            "message": format!("daemon state points at stopped pid {}", state.pid),
            "daemon": state
        }),
        dam_daemon::DaemonStatus::Connected(state) => json!({
            "state": if state.protection_enabled { "connected" } else { "paused" },
            "message": format!("daemon process {} is running", state.pid),
            "daemon": state
        }),
    };
    Ok(serde_json::to_string(&value).unwrap())
}

fn dam_setup_plan_tool(config: &dam_config::DamConfig) -> Result<String, String> {
    let plan = dam_diagnostics::setup_plan(config, &dam_diagnostics::SetupPlanOptions::default())?;
    Ok(serde_json::to_string(&plan).unwrap())
}

fn dam_setup_next_action_tool(config: &dam_config::DamConfig) -> Result<String, String> {
    let plan = dam_diagnostics::setup_plan(config, &dam_diagnostics::SetupPlanOptions::default())?;
    Ok(serde_json::to_string(&json!({
        "state": plan.state,
        "message": plan.message,
        "state_dir": plan.state_dir,
        "proxy_url": plan.proxy_url,
        "network_mode": plan.network_mode,
        "trust_mode": plan.trust_mode,
        "next_action": plan.next_action
    }))
    .unwrap())
}

fn open_consent_store(config: &dam_config::DamConfig) -> Result<dam_consent::ConsentStore, String> {
    if !config.consent.enabled {
        return Err("consent is disabled".to_string());
    }
    match config.consent.backend {
        dam_config::ConsentBackend::Sqlite => {
            dam_consent::ConsentStore::open(&config.consent.sqlite_path)
                .map_err(|error| error.to_string())
        }
    }
}

fn open_vault(config: &dam_config::DamConfig) -> Result<dam_vault::Vault, String> {
    match config.vault.backend {
        dam_config::VaultBackend::Sqlite => {
            dam_vault::Vault::open(&config.vault.sqlite_path).map_err(|error| error.to_string())
        }
        dam_config::VaultBackend::Remote => {
            Err("remote vault backend is not implemented".to_string())
        }
    }
}

fn bound_actor_binding() -> Option<ActorBinding> {
    let actor_id = env::var("DAM_MCP_ACTOR_ID").ok();
    let label = env::var("DAM_MCP_ACTOR_LABEL").ok();
    match (actor_id, label) {
        (Some(actor_id), Some(label))
            if !actor_id.trim().is_empty() && !label.trim().is_empty() =>
        {
            Some(ActorBinding { actor_id, label })
        }
        (None, Some(label)) if !label.trim().is_empty() => Some(ActorBinding {
            actor_id: format!(
                "mcp-actor:{}",
                dam_consent::fingerprint(dam_core::SensitiveType::Email, &label)
            ),
            label,
        }),
        _ => None,
    }
}

fn request_lookup_id(arguments: &Value) -> Result<String, String> {
    arguments
        .get("request_id")
        .and_then(Value::as_str)
        .or_else(|| arguments.get("grant_id").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or_else(|| "request_id or grant_id is required".to_string())
}

fn entries_to_json(entries: &[dam_consent::ConsentEntry]) -> Vec<Value> {
    entries.iter().map(entry_to_json).collect()
}

fn direct_access_request_to_json(entry: &dam_consent::DirectAccessRequest) -> Value {
    json!({
        "request_id": entry.request_id,
        "grant_id": entry.grant_id,
        "status": entry.status.tag(),
        "kind": entry.kind.tag(),
        "vault_key": entry.vault_key,
        "requesting_actor": entry.requesting_actor,
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

fn entry_to_json(entry: &dam_consent::ConsentEntry) -> Value {
    json!({
        "id": entry.id,
        "kind": entry.kind.tag(),
        "vault_key": entry.vault_key,
        "scope": entry.scope,
        "created_at": entry.created_at,
        "expires_at": entry.expires_at,
        "revoked_at": entry.revoked_at,
        "created_by": entry.created_by,
        "reason": entry.reason,
    })
}

fn success(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Value>> {
    let Some(content_length) = read_content_length(reader)? else {
        return Ok(None);
    };
    if content_length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("MCP message exceeds {MAX_MESSAGE_BYTES} byte limit"),
        ));
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn read_content_length<R: BufRead>(reader: &mut R) -> io::Result<Option<usize>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return if content_length.is_some() {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected EOF in message headers",
                ))
            } else {
                Ok(None)
            };
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            );
        }
    }

    content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header"))
        .map(Some)
}

fn write_response<W: Write>(output: &mut W, response: &Value) -> io::Result<()> {
    let payload = response.to_string();
    write!(
        output,
        "Content-Length: {}\r\n\r\n{}",
        payload.len(),
        payload
    )?;
    output.flush()
}

#[cfg(test)]
fn parse_messages(input: &str) -> Vec<Value> {
    if input.trim_start().starts_with('{') {
        return serde_json::from_str(input)
            .map(|value| vec![value])
            .unwrap_or_default();
    }

    let mut messages = Vec::new();
    let mut rest = input;
    while let Some(header_end) = rest.find("\r\n\r\n") {
        let header = &rest[..header_end];
        let content_length = header.lines().find_map(|line| {
            line.strip_prefix("Content-Length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
        });
        let Some(content_length) = content_length else {
            break;
        };
        let body_start = header_end + 4;
        if rest.len() < body_start + content_length {
            break;
        }
        let body = &rest[body_start..body_start + content_length];
        if let Ok(message) = serde_json::from_str(body) {
            messages.push(message);
        }
        rest = &rest[body_start + content_length..];
    }

    messages
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<CliArgs, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    let mut cli = CliArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--config requires a path".to_string())?;
                cli.config.config_path = Some(PathBuf::from(value));
            }
            "--db" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--db requires a path".to_string())?;
                cli.config.vault_sqlite_path = Some(PathBuf::from(value));
            }
            "--consent-db" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--consent-db requires a path".to_string())?;
                cli.config.consent_sqlite_path = Some(PathBuf::from(value));
            }
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }
    Ok(cli)
}

fn usage() -> &'static str {
    "Usage: dam-mcp [--config dam.toml] [--db vault.db] [--consent-db consent.db]"
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
