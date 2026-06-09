use super::*;
use axum::routing::post;
use futures_util::stream;
use std::sync::{Arc, Mutex};

type CapturedHeaders = Arc<Mutex<Vec<(String, String)>>>;
type CapturedBody = Arc<Mutex<Option<String>>>;
type CapturedHeadersAndBody = (CapturedHeaders, CapturedBody);
const PROBE_EMAIL: &str = "scrabb@jnjjj.com";
const PROBE_EMAIL_MIDDLE_SPACED: &str = "scrabb@j njjj.com";
const PROBE_API_KEY: &str = "sk-test000000000000000000000000";

fn proxy_config(upstream: String) -> dam_config::DamConfig {
    proxy_config_with_provider(upstream, "openai-compatible")
}

fn proxy_config_with_provider(upstream: String, provider: &str) -> dam_config::DamConfig {
    let dir = tempfile::tempdir().unwrap().keep();
    let mut config = dam_config::DamConfig::default();
    config.vault.sqlite_path = dir.join("vault.db");
    config.consent.sqlite_path = dir.join("consent.db");
    config.log.enabled = true;
    config.log.sqlite_path = dir.join("log.db");
    config.proxy.enabled = true;
    config.proxy.targets.push(dam_config::ProxyTargetConfig {
        name: "test-openai".to_string(),
        provider: provider.to_string(),
        upstream,
        auth: auth_for_provider(provider),
        failure_mode: None,
        api_key_env: None,
        api_key: None,
    });
    config
}

fn auth_for_provider(provider: &str) -> dam_net::UpstreamAuthConfig {
    match provider {
        "openai-compatible" => dam_net::UpstreamAuthConfig {
            caller_headers: vec!["authorization".to_string()],
            inject: Some(dam_net::UpstreamAuthInjection {
                header: "authorization".to_string(),
                scheme: Some("Bearer".to_string()),
                strip_headers: vec!["authorization".to_string()],
            }),
        },
        "anthropic" => dam_net::UpstreamAuthConfig {
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

fn anthropic_proxy_config(upstream: String) -> dam_config::DamConfig {
    let mut config = proxy_config_with_provider(upstream, "anthropic");
    config.proxy.targets[0].name = "test-anthropic".to_string();
    config
}

fn set_test_target_inbound_policy(
    config: &mut dam_config::DamConfig,
    resolve_references: bool,
    protect_sensitive_data: bool,
) {
    let target = config.proxy.targets[0].clone();
    config
        .traffic
        .profile
        .apps
        .push(dam_net::TrafficAppProfile {
            id: format!("{}-test-route", target.name),
            name: None,
            enabled: true,
            priority: 100,
            match_rules: dam_net::TrafficMatch {
                domains: vec![target.upstream.clone()],
                ..dam_net::TrafficMatch::default()
            },
            action: dam_net::TrafficAction::Inspect,
            adapter: dam_net::ProtocolAdapterKind::Http,
            provider: Some(target.provider),
            target_name: Some(target.name),
            upstream: Some(target.upstream),
            auth: target.auth,
            steps: Vec::new(),
            outbound: dam_net::TrafficDirectionPolicy::default(),
            inbound: dam_net::TrafficInboundPolicy {
                resolve_references,
                protect_sensitive_data,
                ..dam_net::TrafficInboundPolicy::default()
            },
        });
}

fn set_test_target_outbound_action(
    config: &mut dam_config::DamConfig,
    action: dam_net::SensitiveDataAction,
) {
    let target = config.proxy.targets[0].clone();
    if !config.traffic.profile.apps.iter().any(|app| {
        app.target_name.as_deref() == Some(target.name.as_str())
            && app.provider.as_deref() == Some(target.provider.as_str())
    }) {
        set_test_target_inbound_policy(config, true, false);
    }
    let app = config
        .traffic
        .profile
        .apps
        .iter_mut()
        .find(|app| {
            app.target_name.as_deref() == Some(target.name.as_str())
                && app.provider.as_deref() == Some(target.provider.as_str())
        })
        .expect("test target route should exist");
    app.outbound.filter.default_action = action;
}

fn inbound_plan(resolve_references: bool, protect_sensitive_data: bool) -> InboundTransformPlan {
    InboundTransformPlan {
        resolve_references,
        protect_sensitive_data,
    }
}

fn websocket_protection(enabled: bool, inbound_plan: InboundTransformPlan) -> WebSocketProtection {
    WebSocketProtection {
        target_name: "test-openai".to_string(),
        enabled,
        inbound_plan,
        consent_scopes: Arc::new(Vec::new()),
    }
}

#[test]
fn extracts_related_domains_from_email_detections() {
    let detections = vec![
        dam_core::Detection {
            kind: dam_core::SensitiveType::Email,
            span: dam_core::Span { start: 0, end: 17 },
            value: "alice@Example.COM".to_string(),
        },
        dam_core::Detection {
            kind: dam_core::SensitiveType::Email,
            span: dam_core::Span { start: 18, end: 36 },
            value: "bob@example .com".to_string(),
        },
        dam_core::Detection {
            kind: dam_core::SensitiveType::Phone,
            span: dam_core::Span { start: 37, end: 49 },
            value: "415-555-1212".to_string(),
        },
    ];

    assert_eq!(
        related_domains_from_detections(&detections),
        vec!["example.com".to_string()]
    );
}

async fn spawn_app(app: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn websocket_response_text_frames_are_redacted_without_wallet_write_before_client() {
    let config = proxy_config("https://chatgpt.com".to_string());
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let state = build_state(config, None).unwrap();
    let mut upstream = Vec::new();
    websocket::write_unmasked_frame(
        &mut upstream,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: br#"{"content":"banana@example.com"}"#.to_vec(),
        },
    )
    .await
    .unwrap();
    websocket::write_unmasked_frame(&mut upstream, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut client = Vec::new();

    proxy_websocket_upstream_frames(
        state,
        "websocket-inbound-test".to_string(),
        &mut upstream.as_slice(),
        &mut client,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(true, inbound_plan(false, true)),
    )
    .await
    .unwrap();

    let frame = websocket::read_frame(&mut client.as_slice())
        .await
        .unwrap()
        .unwrap();
    let body = String::from_utf8(frame.payload).unwrap();
    assert!(!body.contains("banana@example.com"));
    assert!(body.contains("[email]"));
    assert!(!body.contains("[email:"));
    assert_eq!(
        dam_vault::Vault::open(vault_path).unwrap().count().unwrap(),
        0
    );
    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.action.as_deref() == Some("inbound_protection")
            && entry
                .message
                .contains("WebSocket response text frame protected")
    }));
    assert!(!logs.iter().any(|entry| {
        entry.event_type == "vault_write"
            || entry.event_type == "vault_write_failed"
            || entry.action.as_deref() == Some("tokenized")
    }));
}

#[tokio::test]
async fn websocket_response_redacts_related_domains_from_request_context() {
    let state = build_state(proxy_config("https://chatgpt.com".to_string()), None).unwrap();
    let mut upstream = Vec::new();
    websocket::write_unmasked_frame(
        &mut upstream,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: br#"{"content":"wolol3o22.com"}"#.to_vec(),
        },
    )
    .await
    .unwrap();
    websocket::write_unmasked_frame(&mut upstream, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut client = Vec::new();

    proxy_websocket_upstream_frames(
        state,
        "websocket-related-domain-test".to_string(),
        &mut upstream.as_slice(),
        &mut client,
        Arc::new(RwLock::new(vec!["wolol3o22.com".to_string()])),
        websocket_protection(true, inbound_plan(false, true)),
    )
    .await
    .unwrap();

    let frame = websocket::read_frame(&mut client.as_slice())
        .await
        .unwrap()
        .unwrap();
    let body = String::from_utf8(frame.payload).unwrap();
    assert!(!body.contains("wolol3o22.com"));
    assert!(body.contains("[domain]"));
    assert!(!body.contains("[domain:"));
}

#[tokio::test]
async fn websocket_outbound_probe_tokenizes_email_split_across_text_delta_frames() {
    let config = proxy_config("https://chatgpt.com".to_string());
    let state = build_state(config, None).unwrap();
    let related_domains = Arc::new(RwLock::new(Vec::new()));
    let mut client = Vec::new();
    websocket::write_masked_frame(
        &mut client,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: br#"{"type":"input.delta","delta":"scrabb@j"}"#.to_vec(),
        },
    )
    .await
    .unwrap();
    websocket::write_masked_frame(
        &mut client,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: br#"{"type":"input.delta","delta":"njjj.com echo this message"}"#.to_vec(),
        },
    )
    .await
    .unwrap();
    websocket::write_masked_frame(&mut client, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut upstream = Vec::new();

    let outcome = proxy_websocket_client_frames(
        state.clone(),
        "websocket-outbound-probe-test".to_string(),
        &mut client.as_slice(),
        &mut upstream,
        related_domains.clone(),
        websocket_protection(true, inbound_plan(true, false)),
    )
    .await
    .unwrap();

    assert_eq!(outcome, WebSocketClientFrameOutcome::Completed);
    let mut forwarded_frames = upstream.as_slice();
    let first_frame = websocket::read_frame(&mut forwarded_frames)
        .await
        .unwrap()
        .unwrap();
    let second_frame = websocket::read_frame(&mut forwarded_frames)
        .await
        .unwrap()
        .unwrap();
    let first_delta = websocket_text_delta_from_frame(&first_frame)
        .unwrap()
        .unwrap()
        .text;
    let second_delta = websocket_text_delta_from_frame(&second_frame)
        .unwrap()
        .unwrap()
        .text;
    let forwarded_text = format!("{first_delta}{second_delta}");
    assert!(!forwarded_text.contains(PROBE_EMAIL));
    assert!(forwarded_text.contains("[email:"));

    let provider_value = first_reference_or_probe_email(&forwarded_text);
    let provider_response = serde_json::json!({
        "verbatim": provider_value,
        "middle_spaced": insert_middle_space(&provider_value)
    })
    .to_string();
    let mut provider = Vec::new();
    websocket::write_unmasked_frame(
        &mut provider,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: provider_response.into_bytes(),
        },
    )
    .await
    .unwrap();
    websocket::write_unmasked_frame(&mut provider, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut visible = Vec::new();

    proxy_websocket_upstream_frames(
        state,
        "websocket-outbound-probe-response-test".to_string(),
        &mut provider.as_slice(),
        &mut visible,
        related_domains,
        websocket_protection(true, inbound_plan(true, false)),
    )
    .await
    .unwrap();

    let frame = websocket::read_frame(&mut visible.as_slice())
        .await
        .unwrap()
        .unwrap();
    assert_probe_response_proves_upstream_was_tokenized(&String::from_utf8(frame.payload).unwrap());
}

#[tokio::test]
async fn websocket_outbound_probe_tokenizes_email_split_across_raw_text_frames() {
    let config = proxy_config("https://chatgpt.com".to_string());
    let state = build_state(config, None).unwrap();
    let mut client = Vec::new();
    websocket::write_masked_frame(
        &mut client,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: b"scrabb@j".to_vec(),
        },
    )
    .await
    .unwrap();
    websocket::write_masked_frame(
        &mut client,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: b"njjj.com echo this message".to_vec(),
        },
    )
    .await
    .unwrap();
    websocket::write_masked_frame(&mut client, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut upstream = Vec::new();

    proxy_websocket_client_frames(
        state,
        "websocket-outbound-raw-probe-test".to_string(),
        &mut client.as_slice(),
        &mut upstream,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(true, inbound_plan(true, false)),
    )
    .await
    .unwrap();

    let mut forwarded_frames = upstream.as_slice();
    let first_frame = websocket::read_frame(&mut forwarded_frames)
        .await
        .unwrap()
        .unwrap();
    let second_frame = websocket::read_frame(&mut forwarded_frames)
        .await
        .unwrap()
        .unwrap();
    let forwarded_text = format!(
        "{}{}",
        String::from_utf8(first_frame.payload).unwrap(),
        String::from_utf8(second_frame.payload).unwrap()
    );
    assert!(!forwarded_text.contains(PROBE_EMAIL));
    assert!(forwarded_text.contains("[email:"));
}

#[tokio::test]
async fn websocket_outbound_json_decodes_escaped_newline_before_tokenizing() {
    let config = proxy_config("https://chatgpt.com".to_string());
    let state = build_state(config, None).unwrap();
    let payload = serde_json::json!({
        "type": "conversation.item.create",
        "item": {
            "content": [
                {
                    "type": "input_text",
                    "text": format!("line\n{PROBE_EMAIL} echo this message")
                }
            ]
        }
    })
    .to_string();
    let mut client = Vec::new();
    websocket::write_masked_frame(
        &mut client,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: payload.into_bytes(),
        },
    )
    .await
    .unwrap();
    websocket::write_masked_frame(&mut client, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut upstream = Vec::new();

    let outcome = proxy_websocket_client_frames(
        state,
        "websocket-json-escaped-newline-probe-test".to_string(),
        &mut client.as_slice(),
        &mut upstream,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(true, inbound_plan(true, false)),
    )
    .await
    .unwrap();

    assert_eq!(outcome, WebSocketClientFrameOutcome::Completed);
    let mut forwarded_frames = upstream.as_slice();
    let frame = websocket::read_frame(&mut forwarded_frames)
        .await
        .unwrap()
        .unwrap();
    let forwarded_body = String::from_utf8(frame.payload).unwrap();
    let forwarded_json: serde_json::Value = serde_json::from_str(&forwarded_body).unwrap();
    let forwarded_text = forwarded_json
        .pointer("/item/content/0/text")
        .and_then(serde_json::Value::as_str)
        .unwrap();

    assert!(!forwarded_text.contains(PROBE_EMAIL));
    assert!(forwarded_text.contains("[email:"));
    assert!(forwarded_text.starts_with("line\n[email:"));
    assert!(!forwarded_body.contains(r"\[email:"));
    assert!(!forwarded_body.contains("nscrabb@jnjjj.com"));
}

#[tokio::test]
async fn websocket_response_resolves_references_split_across_text_delta_frames() {
    let config = proxy_config("https://chatgpt.com".to_string());
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let reference = dam_core::Reference::generate(dam_core::SensitiveType::Email);
    dam_vault::Vault::open(&vault_path)
        .unwrap()
        .put(&reference.key(), "scrabb@jnjjj.com")
        .unwrap();
    let state = build_state(config, None).unwrap();
    let display = reference.display();
    let split_at = display.len() - 8;
    let first = &display[..split_at];
    let second = format!("{} echo this message", &display[split_at..]);
    let mut upstream = Vec::new();
    websocket::write_unmasked_frame(
        &mut upstream,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: format!(r#"{{"type":"response.output_text.delta","delta":"{first}"}}"#)
                .into_bytes(),
        },
    )
    .await
    .unwrap();
    websocket::write_unmasked_frame(
        &mut upstream,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: format!(r#"{{"type":"response.output_text.delta","delta":"{second}"}}"#)
                .into_bytes(),
        },
    )
    .await
    .unwrap();
    websocket::write_unmasked_frame(&mut upstream, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut client = Vec::new();

    proxy_websocket_upstream_frames(
        state,
        "websocket-split-resolve-test".to_string(),
        &mut upstream.as_slice(),
        &mut client,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(true, inbound_plan(true, false)),
    )
    .await
    .unwrap();

    let mut client_frames = client.as_slice();
    let first_frame = websocket::read_frame(&mut client_frames)
        .await
        .unwrap()
        .unwrap();
    let second_frame = websocket::read_frame(&mut client_frames)
        .await
        .unwrap()
        .unwrap();
    let first_json: serde_json::Value = serde_json::from_slice(&first_frame.payload).unwrap();
    let second_json: serde_json::Value = serde_json::from_slice(&second_frame.payload).unwrap();
    let visible_text = format!(
        "{}{}",
        first_json
            .get("delta")
            .and_then(serde_json::Value::as_str)
            .unwrap(),
        second_json
            .get("delta")
            .and_then(serde_json::Value::as_str)
            .unwrap()
    );
    assert_eq!(visible_text, "scrabb@jnjjj.com echo this message");
    assert!(!String::from_utf8_lossy(&client).contains(&display));

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.event_type == "resolve"
            && entry.action.as_deref() == Some("resolve_attempt")
            && entry.message.contains("references=1 resolved=1")
    }));
    assert!(logs.iter().any(|entry| {
        entry.event_type == "vault_read" && entry.action.as_deref() == Some("vault_read_succeeded")
    }));
}

#[tokio::test]
async fn websocket_response_does_not_redact_after_reference_resolution() {
    let config = proxy_config("https://chatgpt.com".to_string());
    let vault_path = config.vault.sqlite_path.clone();
    let reference = dam_core::Reference::generate(dam_core::SensitiveType::Email);
    dam_vault::Vault::open(&vault_path)
        .unwrap()
        .put(&reference.key(), "scrabb@jnjjj.com")
        .unwrap();
    let state = build_state(config, None).unwrap();
    let mut upstream = Vec::new();
    websocket::write_unmasked_frame(
        &mut upstream,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: format!(r#"{{"content":"{}"}}"#, reference.display()).into_bytes(),
        },
    )
    .await
    .unwrap();
    websocket::write_unmasked_frame(&mut upstream, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut client = Vec::new();

    proxy_websocket_upstream_frames(
        state,
        "websocket-resolve-before-protect-test".to_string(),
        &mut upstream.as_slice(),
        &mut client,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(true, inbound_plan(true, true)),
    )
    .await
    .unwrap();

    let frame = websocket::read_frame(&mut client.as_slice())
        .await
        .unwrap()
        .unwrap();
    let body = String::from_utf8(frame.payload).unwrap();
    assert!(body.contains("scrabb@jnjjj.com"));
    assert!(!body.contains("[email]"));
    assert!(!body.contains("[email:"));
}

#[tokio::test]
async fn websocket_response_respects_connection_protection_snapshot() {
    let state = build_state(proxy_config("https://chatgpt.com".to_string()), None).unwrap();
    let mut upstream = Vec::new();
    websocket::write_unmasked_frame(
        &mut upstream,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: br#"{"content":"banana@example.com"}"#.to_vec(),
        },
    )
    .await
    .unwrap();
    websocket::write_unmasked_frame(&mut upstream, &websocket::WebSocketFrame::close(1000, ""))
        .await
        .unwrap();
    let mut client = Vec::new();

    proxy_websocket_upstream_frames(
        state,
        "websocket-snapshot-test".to_string(),
        &mut upstream.as_slice(),
        &mut client,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(false, inbound_plan(false, false)),
    )
    .await
    .unwrap();

    let frame = websocket::read_frame(&mut client.as_slice())
        .await
        .unwrap()
        .unwrap();
    let body = String::from_utf8(frame.payload).unwrap();
    assert!(body.contains("banana@example.com"));
    assert!(!body.contains("[email:"));
}

#[tokio::test]
async fn websocket_response_fragmented_text_fails_closed_when_protected() {
    let state = build_state(proxy_config("https://chatgpt.com".to_string()), None).unwrap();
    let mut upstream = Vec::new();
    websocket::write_unmasked_frame(
        &mut upstream,
        &websocket::WebSocketFrame {
            fin: false,
            opcode: websocket::OPCODE_TEXT,
            payload: br#"{"content":"banana@example.com"}"#.to_vec(),
        },
    )
    .await
    .unwrap();
    let mut client = Vec::new();

    let outcome = proxy_websocket_upstream_frames(
        state,
        "websocket-fragmented-inbound-test".to_string(),
        &mut upstream.as_slice(),
        &mut client,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(true, inbound_plan(false, true)),
    )
    .await
    .unwrap();

    assert_eq!(outcome, WebSocketClientFrameOutcome::PolicyBlocked);
    assert!(client.is_empty());
}

#[tokio::test]
async fn websocket_response_compressed_frame_fails_closed_when_protected() {
    let state = build_state(proxy_config("https://chatgpt.com".to_string()), None).unwrap();
    let mut upstream = vec![0x80 | 0x40 | websocket::OPCODE_TEXT, 5];
    upstream.extend_from_slice(b"hello");
    let mut client = Vec::new();

    let outcome = proxy_websocket_upstream_frames(
        state,
        "websocket-compressed-inbound-test".to_string(),
        &mut upstream.as_slice(),
        &mut client,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(true, inbound_plan(false, true)),
    )
    .await
    .unwrap();

    assert_eq!(outcome, WebSocketClientFrameOutcome::PolicyBlocked);
    assert!(client.is_empty());
}

#[tokio::test]
async fn websocket_response_policy_block_fails_closed() {
    let mut config = proxy_config("https://chatgpt.com".to_string());
    config.policy.default_action = dam_core::PolicyAction::Block;
    let state = build_state(config, None).unwrap();
    let mut upstream = Vec::new();
    websocket::write_unmasked_frame(
        &mut upstream,
        &websocket::WebSocketFrame {
            fin: true,
            opcode: websocket::OPCODE_TEXT,
            payload: br#"{"content":"banana@example.com"}"#.to_vec(),
        },
    )
    .await
    .unwrap();
    let mut client = Vec::new();

    let outcome = proxy_websocket_upstream_frames(
        state,
        "websocket-inbound-block-test".to_string(),
        &mut upstream.as_slice(),
        &mut client,
        Arc::new(RwLock::new(Vec::new())),
        websocket_protection(true, inbound_plan(false, true)),
    )
    .await
    .unwrap();

    assert_eq!(outcome, WebSocketClientFrameOutcome::PolicyBlocked);
    assert!(client.is_empty());
}

#[tokio::test]
async fn websocket_upgrade_request_strips_extension_negotiation() {
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, "chatgpt.com".parse().unwrap());
    headers.insert("sec-websocket-key", "abc".parse().unwrap());
    headers.insert(
        "sec-websocket-extensions",
        "permessage-deflate".parse().unwrap(),
    );
    let request = InterceptedHttpRequest {
        method: Method::GET,
        uri: Uri::from_static("https://chatgpt.com/backend-api/ws?x=1"),
        headers,
        body: Bytes::new(),
    };
    let mut output = Vec::new();

    write_websocket_upgrade_request(
        &mut output,
        &request,
        &TargetAuthority {
            host: "chatgpt.com".to_string(),
            port: 443,
        },
    )
    .await
    .unwrap();
    let text = String::from_utf8(output).unwrap();

    assert!(text.starts_with("GET /backend-api/ws?x=1 HTTP/1.1\r\n"));
    assert!(text.contains("sec-websocket-key: abc\r\n"));
    assert!(
        !text
            .to_ascii_lowercase()
            .contains("sec-websocket-extensions")
    );
}

async fn spawn_echo_upstream() -> String {
    async fn echo(body: Bytes) -> Response {
        (StatusCode::OK, body).into_response()
    }

    spawn_app(Router::new().route("/v1/chat/completions", post(echo))).await
}

async fn spawn_capture_echo_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn echo(State(seen_body): State<Arc<Mutex<Option<String>>>>, body: Bytes) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        (StatusCode::OK, body_text).into_response()
    }

    spawn_app(
        Router::new()
            .route("/v1/chat/completions", post(echo))
            .with_state(seen_body),
    )
    .await
}

async fn spawn_value_probe_upstream(
    seen_body: Arc<Mutex<Option<String>>>,
    path: &'static str,
) -> String {
    async fn probe(State(seen_body): State<Arc<Mutex<Option<String>>>>, body: Bytes) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        let value = first_reference_or_probe_email(&body_text);
        let response = serde_json::json!({
            "verbatim": value,
            "middle_spaced": insert_middle_space(&value)
        });

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(response.to_string()))
            .unwrap()
    }

    spawn_app(Router::new().route(path, post(probe)).with_state(seen_body)).await
}

fn first_reference_or_probe_email(input: &str) -> String {
    dam_core::find_references(input)
        .into_iter()
        .next()
        .map(|match_| match_.reference.display())
        .or_else(|| input.contains(PROBE_EMAIL).then(|| PROBE_EMAIL.to_string()))
        .unwrap_or_else(|| "missing-value".to_string())
}

fn insert_middle_space(value: &str) -> String {
    let midpoint = value.len() / 2;
    format!("{} {}", &value[..midpoint], &value[midpoint..])
}

fn assert_probe_response_proves_upstream_was_tokenized(response_body: &str) {
    let response: serde_json::Value = serde_json::from_str(response_body).unwrap();
    assert_eq!(response["verbatim"].as_str(), Some(PROBE_EMAIL));
    let middle_spaced = response["middle_spaced"].as_str().unwrap();
    assert_ne!(middle_spaced, PROBE_EMAIL_MIDDLE_SPACED);
    assert!(
        middle_spaced.contains("[email"),
        "middle-spaced provider echo should be derived from the token, got {middle_spaced}"
    );
}

async fn spawn_raw_sensitive_response_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn raw_response(
        State(seen_body): State<Arc<Mutex<Option<String>>>>,
        body: Bytes,
    ) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"message":{"content":"provider returned leak@example.com"}}"#,
            ))
            .unwrap()
    }

    spawn_app(
        Router::new()
            .route("/v1/chat/completions", post(raw_response))
            .with_state(seen_body),
    )
    .await
}

async fn spawn_raw_domain_response_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn raw_response(
        State(seen_body): State<Arc<Mutex<Option<String>>>>,
        body: Bytes,
    ) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"message":{"content":"provider returned leak.example"}}"#,
            ))
            .unwrap()
    }

    spawn_app(
        Router::new()
            .route("/v1/chat/completions", post(raw_response))
            .with_state(seen_body),
    )
    .await
}

async fn spawn_capture_codex_compact_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn compact(State(seen_body): State<Arc<Mutex<Option<String>>>>, body: Bytes) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        (StatusCode::OK, body_text).into_response()
    }

    spawn_app(
        Router::new()
            .route("/backend-api/codex/responses/compact", post(compact))
            .with_state(seen_body),
    )
    .await
}

async fn spawn_json_escaped_reference_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn json_response(
        State(seen_body): State<Arc<Mutex<Option<String>>>>,
        body: Bytes,
    ) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        let reference = dam_core::find_references(&body_text)
            .into_iter()
            .next()
            .expect("protected upstream body should contain a reference")
            .reference
            .display();
        let escaped_reference = reference.replace('[', r"\\[").replace(']', r"\\]");
        let response = format!(r#"{{"message":{{"content":"{escaped_reference}"}}}}"#);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(response))
            .unwrap()
    }

    spawn_app(
        Router::new()
            .route("/v1/chat/completions", post(json_response))
            .with_state(seen_body),
    )
    .await
}

async fn spawn_ndjson_escaped_reference_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn ndjson_response(
        State(seen_body): State<Arc<Mutex<Option<String>>>>,
        body: Bytes,
    ) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        let reference = dam_core::find_references(&body_text)
            .into_iter()
            .next()
            .expect("protected upstream body should contain a reference")
            .reference
            .display();
        let escaped_reference = reference.replace('[', r"\\[").replace(']', r"\\]");
        let response = format!(r#"{{"type":"delta","text":"{escaped_reference}"}}"#);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/x-ndjson")
            .body(Body::from(format!("{response}\n")))
            .unwrap()
    }

    spawn_app(
        Router::new()
            .route("/v1/chat/completions", post(ndjson_response))
            .with_state(seen_body),
    )
    .await
}

async fn spawn_capture_headers_upstream(seen_headers: CapturedHeaders) -> String {
    async fn echo(State(seen_headers): State<CapturedHeaders>, headers: HeaderMap) -> Response {
        *seen_headers.lock().unwrap() = headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect();
        (StatusCode::OK, "{}").into_response()
    }

    spawn_app(
        Router::new()
            .route("/v1/chat/completions", post(echo))
            .with_state(seen_headers),
    )
    .await
}

async fn spawn_capture_headers_and_body_upstream(
    seen_headers: CapturedHeaders,
    seen_body: CapturedBody,
) -> String {
    async fn echo(
        State((seen_headers, seen_body)): State<CapturedHeadersAndBody>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        *seen_headers.lock().unwrap() = headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect();
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        (StatusCode::OK, body_text).into_response()
    }

    spawn_app(
        Router::new()
            .route("/v1/chat/completions", post(echo))
            .with_state((seen_headers, seen_body)),
    )
    .await
}

async fn spawn_capture_anthropic_headers_upstream(
    seen_headers: CapturedHeaders,
    seen_body: CapturedBody,
) -> String {
    async fn echo(
        State((seen_headers, seen_body)): State<CapturedHeadersAndBody>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        *seen_headers.lock().unwrap() = headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect();
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        (StatusCode::OK, body_text).into_response()
    }

    spawn_app(
        Router::new()
            .route("/v1/messages", post(echo))
            .with_state((seen_headers, seen_body)),
    )
    .await
}

async fn spawn_capture_anthropic_sse_text_delta_upstream(
    seen_body: Arc<Mutex<Option<String>>>,
) -> String {
    async fn sse(State(seen_body): State<Arc<Mutex<Option<String>>>>, body: Bytes) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        let start = body_text
            .find("[email:")
            .expect("protected upstream body should contain email reference");
        let end = start
            + body_text[start..]
                .find(']')
                .expect("email reference should be closed")
            + 1;
        let reference = &body_text[start..end];
        let split_at = reference.len() / 2;
        let first = &reference[..split_at];
        let second = &reference[split_at..];
        let first_event = format!(
            "event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"delta\":{{\"type\":\"text_delta\",\"text\":\"{first}\"}}}}\n\n"
        );
        let second_event = format!(
            "event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"delta\":{{\"type\":\"text_delta\",\"text\":\"{second}\"}}}}\n\n"
        );
        let chunks = stream::iter([
            Ok::<_, std::io::Error>(Bytes::from(first_event)),
            Ok(Bytes::from(second_event)),
            Ok(Bytes::from_static(
                b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            )),
        ]);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(chunks))
            .unwrap()
    }

    spawn_app(
        Router::new()
            .route("/v1/messages", post(sse))
            .with_state(seen_body),
    )
    .await
}

async fn spawn_anthropic_sse_raw_domain_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn sse(State(seen_body): State<Arc<Mutex<Option<String>>>>, body: Bytes) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text);
        let chunks = stream::iter([
            Ok::<_, std::io::Error>(Bytes::from_static(
                br#"event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"splonk.io"}}

"#,
            )),
            Ok(Bytes::from_static(
                b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            )),
        ]);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(chunks))
            .unwrap()
    }

    spawn_app(
        Router::new()
            .route("/v1/messages", post(sse))
            .with_state(seen_body),
    )
    .await
}

async fn spawn_capture_sse_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn sse(State(seen_body): State<Arc<Mutex<Option<String>>>>, body: Bytes) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        let event = format!("event: response.output_text.delta\ndata: {body_text}\n\n");
        let split_at = event
            .find("[email:")
            .map(|index| index + "[email:".len() + 8)
            .unwrap_or(event.len());
        let chunks = stream::iter([
            Ok::<_, std::io::Error>(Bytes::from(event[..split_at].to_string())),
            Ok(Bytes::from(event[split_at..].to_string())),
        ]);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(chunks))
            .unwrap()
    }

    spawn_app(
        Router::new()
            .route("/v1/responses", post(sse))
            .with_state(seen_body),
    )
    .await
}

async fn proxy_report(response: reqwest::Response) -> dam_api::ProxyReport {
    response.json().await.expect("proxy report json")
}

fn transparent_config(state_dir: PathBuf) -> TransparentInterceptionConfig {
    TransparentInterceptionConfig {
        state_dir,
        network_mode: dam_net::CaptureMode::SystemProxy,
        system_proxy_active: true,
        tun_active: false,
        routes: dam_net::default_traffic_routes(),
        trust: dam_trust::TrustState::default(),
        user_consented: true,
        protection_control_path: None,
    }
}

#[tokio::test]
async fn transparent_connect_requests_fail_closed_without_tls_runtime() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;
    let addr = proxy.strip_prefix("http://").unwrap();
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            b"CONNECT api.openai.com:443 HTTP/1.1\r\nHost: api.openai.com:443\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();

    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();

    assert!(response.starts_with("HTTP/1.1 501"));
    assert!(response.contains("transparent CONNECT traffic requires the TLS interception runtime"));
    assert!(upstream_seen.lock().unwrap().is_none());
    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.event_type == "proxy_failure"
            && entry.action.as_deref() == Some("blocked")
            && entry
                .message
                .contains("transparent CONNECT traffic requires")
    }));
}

#[tokio::test]
async fn transparent_connect_passes_unknown_hosts_through_without_inspection() {
    let seen = Arc::new(Mutex::new(Vec::<u8>::new()));
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_addr = origin.local_addr().unwrap();
    let seen_for_origin = seen.clone();
    tokio::spawn(async move {
        let (mut stream, _) = origin.accept().await.unwrap();
        let mut buffer = [0_u8; 4];
        tokio::io::AsyncReadExt::read_exact(&mut stream, &mut buffer)
            .await
            .unwrap();
        *seen_for_origin.lock().unwrap() = buffer.to_vec();
        tokio::io::AsyncWriteExt::write_all(&mut stream, b"pong")
            .await
            .unwrap();
    });

    let upstream = spawn_capture_echo_upstream(Arc::new(Mutex::new(None::<String>))).await;
    let config = proxy_config(upstream);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        serve_transparent_with_shutdown(
            listener,
            config,
            transparent_config(tempfile::tempdir().unwrap().keep()),
            async {
                let _ = shutdown_rx.await;
            },
        )
        .await
        .unwrap();
    });

    let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        format!(
            "CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n",
            origin_addr, origin_addr
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let connect_response = read_until_headers(&mut stream).await;
    assert!(String::from_utf8_lossy(&connect_response).starts_with("HTTP/1.1 200"));

    tokio::io::AsyncWriteExt::write_all(&mut stream, b"ping")
        .await
        .unwrap();
    let mut response = [0_u8; 4];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut response)
        .await
        .unwrap();

    assert_eq!(&response, b"pong");
    assert_eq!(&*seen.lock().unwrap(), b"ping");
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn paused_profile_connect_tunnel_closes_when_protection_resumes() {
    let seen = Arc::new(Mutex::new(Vec::<u8>::new()));
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_addr = origin.local_addr().unwrap();
    let seen_for_origin = seen.clone();
    tokio::spawn(async move {
        let (mut stream, _) = origin.accept().await.unwrap();
        let mut buffer = [0_u8; 4];
        tokio::io::AsyncReadExt::read_exact(&mut stream, &mut buffer)
            .await
            .unwrap();
        *seen_for_origin.lock().unwrap() = buffer.to_vec();
        tokio::io::AsyncWriteExt::write_all(&mut stream, b"pong")
            .await
            .unwrap();
        let mut keepalive = [0_u8; 1];
        let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut keepalive).await;
    });

    let upstream = spawn_capture_echo_upstream(Arc::new(Mutex::new(None::<String>))).await;
    let mut config = proxy_config(upstream);
    config.proxy.targets[0].name = "test-openai".to_string();
    let log_path = config.log.sqlite_path.clone();
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("protection-control");
    fs::write(&control_path, "disabled\n").unwrap();
    let mut interception = transparent_config(dir.path().to_path_buf());
    interception.protection_control_path = Some(control_path.clone());
    interception.routes = vec![dam_net::TrafficRoute::custom(
        "127.0.0.1",
        "openai-compatible",
        "test-openai",
        "https://127.0.0.1",
    )];

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        serve_transparent_with_shutdown(listener, config, interception, async {
            let _ = shutdown_rx.await;
        })
        .await
        .unwrap();
    });

    let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        format!(
            "CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n",
            origin_addr, origin_addr
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let connect_response = read_until_headers(&mut stream).await;
    assert!(String::from_utf8_lossy(&connect_response).starts_with("HTTP/1.1 200"));

    tokio::io::AsyncWriteExt::write_all(&mut stream, b"ping")
        .await
        .unwrap();
    let mut response = [0_u8; 4];
    tokio::io::AsyncReadExt::read_exact(&mut stream, &mut response)
        .await
        .unwrap();
    assert_eq!(&response, b"pong");

    fs::write(&control_path, "enabled\n").unwrap();
    let mut one_byte = [0_u8; 1];
    let closed = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut one_byte)).await;
    match closed {
        Ok(Ok(0)) | Ok(Err(_)) => {}
        Ok(Ok(count)) => panic!("expected paused AI tunnel to close, read {count} bytes"),
        Err(_) => panic!("paused AI tunnel stayed open after protection resumed"),
    }

    assert_eq!(&*seen.lock().unwrap(), b"ping");
    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.event_type == "proxy_bypass"
            && entry.message.contains(&format!("target={origin_addr}"))
            && entry.message.contains("reason=protection_paused")
    }));
    assert!(logs.iter().any(|entry| {
        entry.event_type == "proxy_bypass"
            && entry.message.contains("closed because protection resumed")
    }));
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn transparent_plain_http_passes_unknown_hosts_through_without_redaction() {
    let seen = Arc::new(Mutex::new(None::<String>));
    let origin = spawn_capture_echo_upstream(seen.clone()).await;
    let origin_addr = origin.strip_prefix("http://").unwrap().to_string();
    let config = proxy_config(origin.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        serve_transparent_with_shutdown(
            listener,
            config,
            transparent_config(tempfile::tempdir().unwrap().keep()),
            async {
                let _ = shutdown_rx.await;
            },
        )
        .await
        .unwrap();
    });

    let body = r#"{"input":"alice@example.com"}"#;
    let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            format!(
                "POST http://{origin_addr}/v1/chat/completions HTTP/1.1\r\nHost: {origin_addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .as_bytes(),
        )
        .await
        .unwrap();

    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();

    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("alice@example.com"));
    assert_eq!(seen.lock().unwrap().as_deref(), Some(body));
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn paused_protection_bypasses_explicit_provider_requests() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("protection.state");
    std::fs::write(&control_path, "disabled\n").unwrap();
    let mut interception = transparent_config(dir.path().to_path_buf());
    interception.protection_control_path = Some(control_path);
    let proxy = spawn_app(build_app_with_interception(config, Some(interception)).unwrap()).await;

    let report = proxy_report(reqwest::get(format!("{proxy}/health")).await.unwrap()).await;
    assert_eq!(report.state, dam_api::ProxyState::Bypassing);

    let body = r#"{"input":"alice@example.com"}"#;
    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .header(header::AUTHORIZATION, "Bearer local")
        .body(body)
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(upstream_seen.lock().unwrap().as_deref(), Some(body));
}

#[test]
fn protection_control_reads_json_and_legacy_disabled_state() {
    let dir = tempfile::tempdir().unwrap();
    let control_path = dir.path().join("protection.state");

    std::fs::write(&control_path, "{\"enabled\": false}\n").unwrap();
    assert!(!protection_control_enabled(&control_path));

    std::fs::write(&control_path, "{\"enabled\": true}\n").unwrap();
    assert!(protection_control_enabled(&control_path));

    std::fs::write(&control_path, "disabled\n").unwrap();
    assert!(!protection_control_enabled(&control_path));
}

#[tokio::test]
async fn transparent_connect_requests_fail_closed_when_interception_is_not_ready() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let mut config = proxy_config(upstream);
    config.proxy.targets[0].name = "openai".to_string();
    let interception = TransparentInterceptionConfig {
        state_dir: tempfile::tempdir().unwrap().keep(),
        network_mode: dam_net::CaptureMode::SystemProxy,
        system_proxy_active: true,
        tun_active: false,
        routes: dam_net::default_traffic_routes(),
        trust: dam_trust::TrustState::default(),
        user_consented: true,
        protection_control_path: None,
    };
    let proxy = spawn_app(build_app_with_interception(config, Some(interception)).unwrap()).await;
    let addr = proxy.strip_prefix("http://").unwrap();
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            b"CONNECT api.openai.com:443 HTTP/1.1\r\nHost: api.openai.com:443\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();

    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();

    assert!(response.starts_with("HTTP/1.1 503"));
    assert!(response.contains("TLS interception is disabled"));
    assert!(upstream_seen.lock().unwrap().is_none());
}

#[tokio::test]
async fn transparent_connect_uses_configured_route_registry() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let mut config = proxy_config(upstream);
    config.proxy.targets[0].name = "enterprise-ai".to_string();
    let traffic_routes = vec![dam_net::TrafficRoute::custom(
        "api.enterprise-ai.example",
        "openai-compatible",
        "enterprise-ai",
        "https://api.enterprise-ai.example",
    )];
    let interception = TransparentInterceptionConfig {
        state_dir: tempfile::tempdir().unwrap().keep(),
        network_mode: dam_net::CaptureMode::SystemProxy,
        system_proxy_active: true,
        tun_active: false,
        routes: traffic_routes,
        trust: dam_trust::TrustState::default(),
        user_consented: true,
        protection_control_path: None,
    };
    let proxy = spawn_app(build_app_with_interception(config, Some(interception)).unwrap()).await;
    let addr = proxy.strip_prefix("http://").unwrap();
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            b"CONNECT api.enterprise-ai.example:443 HTTP/1.1\r\nHost: api.enterprise-ai.example:443\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();

    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();

    assert!(response.starts_with("HTTP/1.1 503"));
    assert!(response.contains("TLS interception is disabled"));
    assert!(!response.contains("not in the configured route scope"));
    assert!(upstream_seen.lock().unwrap().is_none());
}

#[tokio::test]
async fn proxy_value_probe_confirms_http_upstream_receives_token_not_raw_value() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_value_probe_upstream(upstream_seen.clone(), "/v1/chat/completions").await;
    let proxy = spawn_app(build_app(proxy_config(upstream)).unwrap()).await;

    let body = format!(r#"{{"messages":[{{"content":"{PROBE_EMAIL} echo this message"}}]}}"#);
    let response_body = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(body)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains(PROBE_EMAIL));
    assert!(upstream_body.contains("[email:"));
    assert_probe_response_proves_upstream_was_tokenized(&response_body);
}

#[tokio::test]
async fn proxy_http_request_tokenizes_labeled_api_key_before_upstream() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let proxy = spawn_app(build_app(proxy_config(upstream)).unwrap()).await;

    let body = format!(r#"{{"input":"OPENAI_API_KEY={PROBE_API_KEY} echo this message"}}"#);
    let response_body = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(body)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains(PROBE_API_KEY));
    assert!(upstream_body.contains("OPENAI_API_KEY=[api_key:"));
    assert!(response_body.contains(PROBE_API_KEY));
}

#[tokio::test]
async fn proxy_http_request_tokenizes_direct_openai_key_before_upstream() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let proxy = spawn_app(build_app(proxy_config(upstream)).unwrap()).await;

    let body = format!(r#"{{"input":"token {PROBE_API_KEY} echo this message"}}"#);
    let response_body = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(body)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains(PROBE_API_KEY));
    assert!(upstream_body.contains("token [api_key:"));
    assert!(response_body.contains(PROBE_API_KEY));
}

#[tokio::test]
async fn proxy_json_request_decodes_escaped_newline_before_tokenizing() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_value_probe_upstream(upstream_seen.clone(), "/v1/chat/completions").await;
    let proxy = spawn_app(build_app(proxy_config(upstream)).unwrap()).await;
    let body = serde_json::json!({
        "messages": [
            {
                "content": format!("line\n{PROBE_EMAIL} echo this message")
            }
        ]
    })
    .to_string();

    let response_body = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(body)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    let upstream_json: serde_json::Value = serde_json::from_str(&upstream_body).unwrap();
    let upstream_text = upstream_json
        .pointer("/messages/0/content")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    assert!(!upstream_text.contains(PROBE_EMAIL));
    assert!(upstream_text.contains("[email:"));
    assert!(upstream_text.starts_with("line\n[email:"));
    assert!(!upstream_body.contains(r"\[email:"));
    assert!(!upstream_body.contains("nscrabb@jnjjj.com"));
    assert_probe_response_proves_upstream_was_tokenized(&response_body);
}

#[tokio::test]
async fn transparent_connect_tls_http1_requests_are_protected() {
    use tokio_rustls::TlsConnector;
    use tokio_rustls::rustls::{
        ClientConfig, RootCertStore,
        pki_types::{CertificateDer, ServerName},
    };

    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let mut config = proxy_config(upstream);
    config.proxy.targets[0].name = "openai".to_string();
    let vault_path = config.vault.sqlite_path.clone();

    let dir = tempfile::tempdir().unwrap();
    let artifact = dam_trust::generate_local_ca_artifact_at(dir.path(), 1).unwrap();
    let mut record = artifact.record.clone();
    record.installed_at_unix = Some(2);
    let trust = dam_trust::TrustState {
        mode: dam_trust::TrustMode::LocalCa,
        local_ca: Some(record),
        ..dam_trust::TrustState::default()
    };
    let interception = TransparentInterceptionConfig {
        state_dir: dir.path().to_path_buf(),
        network_mode: dam_net::CaptureMode::SystemProxy,
        system_proxy_active: true,
        tun_active: false,
        routes: dam_net::default_traffic_routes(),
        trust,
        user_consented: true,
        protection_control_path: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        serve_transparent_with_shutdown(listener, config, interception, async {
            let _ = shutdown_rx.await;
        })
        .await
        .unwrap();
    });
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"CONNECT api.openai.com:443 HTTP/1.1\r\nHost: api.openai.com:443\r\n\r\n",
    )
    .await
    .unwrap();
    let connect_response = read_until_headers(&mut stream).await;
    assert!(String::from_utf8_lossy(&connect_response).starts_with("HTTP/1.1 200"));

    let mut roots = RootCertStore::empty();
    let ca_der = dam_trust::issue_local_ca_leaf_certificate(dir.path(), "api.openai.com")
        .unwrap()
        .ca_certificate_der;
    roots.add(CertificateDer::from(ca_der)).unwrap();
    let client_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from("api.openai.com".to_string()).unwrap();
    let mut tls = connector.connect(server_name, stream).await.unwrap();

    let body = r#"{"input":"email alice@example.com"}"#;
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: api.openai.com\r\nAuthorization: Bearer local\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    tokio::io::AsyncWriteExt::write_all(&mut tls, request.as_bytes())
        .await
        .unwrap();
    let response = read_intercepted_test_response(&mut tls).await;

    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("alice@example.com"));
    assert!(!response.contains("[email:"));
    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("alice@example.com"));
    assert!(upstream_body.contains("[email:"));
    let vault = dam_vault::Vault::open(vault_path).unwrap();
    assert_eq!(vault.count().unwrap(), 1);
    assert_eq!(vault.wallet_count().unwrap(), 0);
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn proxy_value_probe_confirms_https_interception_forwards_token_not_raw_value() {
    use tokio_rustls::TlsConnector;
    use tokio_rustls::rustls::{
        ClientConfig, RootCertStore,
        pki_types::{CertificateDer, ServerName},
    };

    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_value_probe_upstream(upstream_seen.clone(), "/v1/chat/completions").await;
    let mut config = proxy_config(upstream);
    config.proxy.targets[0].name = "openai".to_string();

    let dir = tempfile::tempdir().unwrap();
    let artifact = dam_trust::generate_local_ca_artifact_at(dir.path(), 1).unwrap();
    let mut record = artifact.record.clone();
    record.installed_at_unix = Some(2);
    let trust = dam_trust::TrustState {
        mode: dam_trust::TrustMode::LocalCa,
        local_ca: Some(record),
        ..dam_trust::TrustState::default()
    };
    let interception = TransparentInterceptionConfig {
        state_dir: dir.path().to_path_buf(),
        network_mode: dam_net::CaptureMode::SystemProxy,
        system_proxy_active: true,
        tun_active: false,
        routes: dam_net::default_traffic_routes(),
        trust,
        user_consented: true,
        protection_control_path: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        serve_transparent_with_shutdown(listener, config, interception, async {
            let _ = shutdown_rx.await;
        })
        .await
        .unwrap();
    });
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"CONNECT api.openai.com:443 HTTP/1.1\r\nHost: api.openai.com:443\r\n\r\n",
    )
    .await
    .unwrap();
    let connect_response = read_until_headers(&mut stream).await;
    assert!(String::from_utf8_lossy(&connect_response).starts_with("HTTP/1.1 200"));

    let mut roots = RootCertStore::empty();
    let ca_der = dam_trust::issue_local_ca_leaf_certificate(dir.path(), "api.openai.com")
        .unwrap()
        .ca_certificate_der;
    roots.add(CertificateDer::from(ca_der)).unwrap();
    let client_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from("api.openai.com".to_string()).unwrap();
    let mut tls = connector.connect(server_name, stream).await.unwrap();

    let body = format!(r#"{{"input":"{PROBE_EMAIL} echo this message"}}"#);
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: api.openai.com\r\nAuthorization: Bearer local\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    tokio::io::AsyncWriteExt::write_all(&mut tls, request.as_bytes())
        .await
        .unwrap();
    let response = read_intercepted_test_response(&mut tls).await;

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains(PROBE_EMAIL));
    assert!(upstream_body.contains("[email:"));
    let response_body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
    assert_probe_response_proves_upstream_was_tokenized(response_body);
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn transparent_chatgpt_backend_http_requests_use_route_target() {
    use tokio_rustls::TlsConnector;
    use tokio_rustls::rustls::{
        ClientConfig, RootCertStore,
        pki_types::{CertificateDer, ServerName},
    };

    let fallback_seen = Arc::new(Mutex::new(None::<String>));
    let fallback_upstream = spawn_capture_echo_upstream(fallback_seen.clone()).await;
    let chatgpt_seen = Arc::new(Mutex::new(None::<String>));
    let chatgpt_upstream = spawn_capture_codex_compact_upstream(chatgpt_seen.clone()).await;

    let mut config = proxy_config_with_provider(fallback_upstream, "anthropic");
    config.proxy.targets[0].name = "anthropic".to_string();
    config.proxy.targets.push(dam_config::ProxyTargetConfig {
        name: "chatgpt-web".to_string(),
        provider: "openai-compatible".to_string(),
        upstream: chatgpt_upstream,
        auth: dam_net::UpstreamAuthConfig::default(),
        failure_mode: None,
        api_key_env: None,
        api_key: None,
    });
    let log_path = config.log.sqlite_path.clone();
    let vault_path = config.vault.sqlite_path.clone();

    let dir = tempfile::tempdir().unwrap();
    let artifact = dam_trust::generate_local_ca_artifact_at(dir.path(), 1).unwrap();
    let mut record = artifact.record.clone();
    record.installed_at_unix = Some(2);
    let trust = dam_trust::TrustState {
        mode: dam_trust::TrustMode::LocalCa,
        local_ca: Some(record),
        ..dam_trust::TrustState::default()
    };
    let interception = TransparentInterceptionConfig {
        state_dir: dir.path().to_path_buf(),
        network_mode: dam_net::CaptureMode::SystemProxy,
        system_proxy_active: true,
        tun_active: false,
        routes: dam_net::default_traffic_routes(),
        trust,
        user_consented: true,
        protection_control_path: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        serve_transparent_with_shutdown(listener, config, interception, async {
            let _ = shutdown_rx.await;
        })
        .await
        .unwrap();
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"CONNECT chatgpt.com:443 HTTP/1.1\r\nHost: chatgpt.com:443\r\n\r\n",
    )
    .await
    .unwrap();
    let connect_response = read_until_headers(&mut stream).await;
    assert!(String::from_utf8_lossy(&connect_response).starts_with("HTTP/1.1 200"));

    let mut roots = RootCertStore::empty();
    let ca_der = dam_trust::issue_local_ca_leaf_certificate(dir.path(), "chatgpt.com")
        .unwrap()
        .ca_certificate_der;
    roots.add(CertificateDer::from(ca_der)).unwrap();
    let client_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client_config));
    let server_name = ServerName::try_from("chatgpt.com".to_string()).unwrap();
    let mut tls = connector.connect(server_name, stream).await.unwrap();

    let body = r#"{"input":"email codex@example.com"}"#;
    let request = format!(
        "POST /backend-api/codex/responses/compact HTTP/1.1\r\nHost: chatgpt.com\r\nCookie: test=session\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    tokio::io::AsyncWriteExt::write_all(&mut tls, request.as_bytes())
        .await
        .unwrap();
    let response = read_intercepted_test_response(&mut tls).await;

    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("codex@example.com"));
    assert!(!response.contains("[email:"));
    assert!(fallback_seen.lock().unwrap().is_none());
    let upstream_body = chatgpt_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("codex@example.com"));
    assert!(upstream_body.contains("[email:"));
    let vault = dam_vault::Vault::open(vault_path).unwrap();
    assert_eq!(vault.count().unwrap(), 1);
    assert_eq!(vault.wallet_count().unwrap(), 0);
    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.action.as_deref() == Some("route_decision")
            && entry.message.contains("target=chatgpt-web")
    }));
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn transparent_plain_http_resolves_event_stream_responses() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_sse_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let log_path = config.log.sqlite_path.clone();
    let interception = TransparentInterceptionConfig {
        state_dir: tempfile::tempdir().unwrap().keep(),
        network_mode: dam_net::CaptureMode::SystemProxy,
        system_proxy_active: true,
        tun_active: false,
        routes: dam_net::default_traffic_routes(),
        trust: dam_trust::TrustState::default(),
        user_consented: true,
        protection_control_path: None,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        serve_transparent_with_shutdown(listener, config, interception, async {
            let _ = shutdown_rx.await;
        })
        .await
        .unwrap();
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body = r#"{"input":[{"content":"email erin@example.com"}],"stream":true}"#;
    let request = format!(
        "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer local\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    let response = read_intercepted_test_response(&mut stream).await;

    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("content-type: text/event-stream"));
    assert!(response.contains("transfer-encoding: chunked"));
    assert!(response.contains("erin@example.com"));
    assert!(!response.contains("[email:"));
    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("erin@example.com"));
    assert!(upstream_body.contains("[email:"));
    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    for action in [
        "route_decision",
        "request_protection",
        "provider_forward_start",
        "provider_response",
        "intercepted_response_write",
        "resolve_attempt",
    ] {
        assert!(
            logs.iter()
                .any(|entry| entry.action.as_deref() == Some(action)),
            "missing proxy diagnostic action {action}"
        );
    }
    assert!(
        logs.iter().all(|entry| {
            !entry.message.contains("erin@example.com")
                && !entry
                    .reference
                    .as_deref()
                    .unwrap_or_default()
                    .contains("erin@example.com")
        }),
        "logs must not contain raw sensitive values"
    );
    let _ = shutdown_tx.send(());
}

async fn read_until_headers<T>(stream: &mut T) -> Vec<u8>
where
    T: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = tokio::io::AsyncReadExt::read(stream, &mut chunk)
            .await
            .unwrap();
        assert!(
            read != 0,
            "connection closed before headers completed: {}",
            String::from_utf8_lossy(&buffer)
        );
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.ends_with(b"\r\n\r\n") {
            return buffer;
        }
    }
}

async fn read_intercepted_test_response<T>(stream: &mut T) -> String
where
    T: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        match tokio::io::AsyncReadExt::read(stream, &mut chunk).await {
            Ok(0) => break,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => panic!("failed to read intercepted response: {error}"),
        }
    }
    String::from_utf8(buffer).expect("intercepted response should be utf-8")
}

#[tokio::test]
async fn redacts_outbound_request_and_resolves_inbound_response() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let mut config = proxy_config(upstream);
    config.proxy.resolve_inbound = true;
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"messages":[{"content":"email alice@example.com"}]}"#)
        .send()
        .await
        .unwrap();

    let body = response.text().await.unwrap();
    assert!(body.contains("alice@example.com"));
    assert!(!body.contains("[email:"));

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("alice@example.com"));
    assert!(upstream_body.contains("[email:"));
    let vault = dam_vault::Vault::open(vault_path).unwrap();
    assert_eq!(vault.count().unwrap(), 1);
    assert_eq!(vault.wallet_count().unwrap(), 0);
    assert!(dam_log::LogStore::open(log_path).unwrap().count().unwrap() > 0);
}

#[tokio::test]
async fn traffic_route_redacts_outbound_without_wallet_write_when_profile_says_redact() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let mut config = proxy_config(upstream.clone());
    config.proxy.targets[0].name = "test-openai".to_string();
    set_test_target_outbound_action(&mut config, dam_net::SensitiveDataAction::Redact);
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let mut interception = transparent_config(tempfile::tempdir().unwrap().keep());
    interception.routes = dam_net::traffic_routes_from_profile(&config.traffic.effective_profile());
    let state = build_state(config, Some(interception)).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1"));
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer local"),
    );

    let response = proxy_http_request(
        state,
        Method::POST,
        "/v1/chat/completions".parse().unwrap(),
        headers,
        Bytes::from_static(br#"{"input":"email alice@example.com"}"#),
        "profile-redact-test".to_string(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("alice@example.com"));
    assert!(upstream_body.contains("[email]"));
    assert!(!upstream_body.contains("[email:"));
    assert_eq!(
        dam_vault::Vault::open(vault_path).unwrap().count().unwrap(),
        0
    );
    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.event_type == "redaction" && entry.action.as_deref() == Some("redacted")
    }));
    assert!(!logs.iter().any(|entry| {
        entry.event_type == "vault_write" || entry.action.as_deref() == Some("tokenized")
    }));
}

#[tokio::test]
async fn reuses_references_for_duplicate_outbound_values_by_default() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"input":"email alice@example.com again alice@example.com"}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    let references = dam_core::find_references(&upstream_body);
    assert_eq!(references.len(), 2);
    assert_eq!(references[0].reference, references[1].reference);
    assert_eq!(
        dam_vault::Vault::open(&vault_path)
            .unwrap()
            .count()
            .unwrap(),
        1
    );

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert_eq!(
        logs.iter()
            .filter(|entry| entry.event_type == "vault_write")
            .count(),
        1
    );
    assert_eq!(
        logs.iter()
            .filter(|entry| entry.event_type == "redaction")
            .count(),
        2
    );
}

#[tokio::test]
async fn redacts_spaced_email_variants_from_outbound_history() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let vault_path = config.vault.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
            .post(format!("{proxy}/v1/chat/completions"))
            .body(
                r#"{"messages":[{"role":"assistant","content":"wololo@ w.com"},{"role":"user","content":"wololo @w.com"}]}"#,
            )
            .send()
            .await
            .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("wololo@ w.com"));
    assert!(!upstream_body.contains("wololo @w.com"));
    assert!(upstream_body.contains("[email:"));
    let references = dam_core::find_references(&upstream_body);
    assert_eq!(references.len(), 2);
    assert_eq!(references[0].reference, references[1].reference);
    assert_eq!(
        dam_vault::Vault::open(&vault_path)
            .unwrap()
            .count()
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn active_consent_allows_outbound_value() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let consent_path = config.consent.sqlite_path.clone();
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    dam_consent::ConsentStore::open(&consent_path)
        .unwrap()
        .grant(&dam_consent::GrantConsent {
            kind: dam_core::SensitiveType::Email,
            value: "alice@example.com".to_string(),
            vault_key: None,
            ttl_seconds: 60,
            created_by: "test".to_string(),
            reason: None,
        })
        .unwrap();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"input":"email alice@example.com"}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(upstream_body.contains("alice@example.com"));
    assert_eq!(
        dam_vault::Vault::open(vault_path).unwrap().count().unwrap(),
        0
    );
    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.event_type == "consent"
            && entry
                .action
                .as_deref()
                .is_some_and(|a| a.starts_with("allow:"))
    }));
}

#[tokio::test]
async fn active_consent_expands_allowed_references_from_outbound_history() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let vault_path = config.vault.sqlite_path.clone();
    let consent_path = config.consent.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"input":"email history@example.com"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let first_upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    let reference = dam_core::find_references(&first_upstream_body)
        .into_iter()
        .next()
        .expect("first request should be tokenized")
        .reference;

    let vault = dam_vault::Vault::open(&vault_path).unwrap();
    dam_consent::ConsentStore::open(&consent_path)
        .unwrap()
        .grant_for_reference(&reference.key(), &vault, 60, "test", None)
        .unwrap();
    let history = format!(r#"{{"input":"repeat {}"}}"#, reference.display());

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(history)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let second_upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(second_upstream_body.contains("history@example.com"));
    assert!(!second_upstream_body.contains(&reference.display()));
}

#[tokio::test]
async fn target_api_key_replaces_inbound_authorization() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let upstream = spawn_capture_headers_upstream(seen_headers.clone()).await;
    let mut config = proxy_config(upstream);
    config.proxy.targets[0].api_key_env = Some("TEST_UPSTREAM_KEY".to_string());
    config.proxy.targets[0].api_key = Some(dam_config::SecretValue::new(
        "TEST_UPSTREAM_KEY",
        "upstream-secret",
    ));
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .header(header::AUTHORIZATION, "Bearer local-agent-secret")
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let authorization_values = seen_headers
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    assert_eq!(authorization_values, ["Bearer upstream-secret"]);
}

#[tokio::test]
async fn anthropic_provider_forwards_caller_x_api_key_and_protects_body() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let seen_body = Arc::new(Mutex::new(None::<String>));
    let upstream =
        spawn_capture_anthropic_headers_upstream(seen_headers.clone(), seen_body.clone()).await;
    let proxy = spawn_app(build_app(anthropic_proxy_config(upstream)).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/messages"))
        .header("x-api-key", "caller-secret")
        .body(r#"{"messages":[{"content":"email alice@example.com"}]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let x_api_key_values = seen_headers
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("x-api-key"))
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    assert_eq!(x_api_key_values, ["caller-secret"]);

    let upstream_body = seen_body.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("alice@example.com"));
    assert!(upstream_body.contains("[email:"));
}

#[tokio::test]
async fn anthropic_provider_resolves_references_split_across_text_delta_events() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_anthropic_sse_text_delta_upstream(upstream_seen.clone()).await;
    let config = anthropic_proxy_config(upstream);
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/messages"))
        .header("x-api-key", "caller-secret")
        .body(r#"{"messages":[{"content":"email banana@example.test"}],"stream":true}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = response.text().await.unwrap();
    assert!(body.contains("banana@example.test"));
    assert!(!body.contains("[email:"));

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("banana@example.test"));
    assert!(upstream_body.contains("[email:"));
    assert_eq!(
        dam_vault::Vault::open(vault_path).unwrap().count().unwrap(),
        1
    );

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| entry.event_type == "vault_read"));
    assert!(logs.iter().any(|entry| entry.event_type == "resolve"));
}

#[tokio::test]
async fn anthropic_target_api_key_replaces_inbound_x_api_key_and_authorization() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let seen_body = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_anthropic_headers_upstream(seen_headers.clone(), seen_body).await;
    let mut config = anthropic_proxy_config(upstream);
    config.proxy.targets[0].api_key_env = Some("TEST_ANTHROPIC_KEY".to_string());
    config.proxy.targets[0].api_key = Some(dam_config::SecretValue::new(
        "TEST_ANTHROPIC_KEY",
        "upstream-secret",
    ));
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/messages"))
        .header("x-api-key", "local-agent-secret")
        .header(header::AUTHORIZATION, "Bearer local-authorization")
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let headers = seen_headers.lock().unwrap();
    let x_api_key_values = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("x-api-key"))
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    assert_eq!(x_api_key_values, ["upstream-secret"]);
    assert!(
        !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
    );
}

#[tokio::test]
async fn anthropic_missing_target_api_key_accepts_caller_x_api_key() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let seen_body = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_anthropic_headers_upstream(seen_headers.clone(), seen_body).await;
    let mut config = anthropic_proxy_config(upstream);
    config.proxy.targets[0].api_key_env = Some("MISSING_ANTHROPIC_KEY".to_string());
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/messages"))
        .header("x-api-key", "caller-secret")
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn hop_by_hop_and_connection_listed_headers_are_not_forwarded() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let upstream = spawn_capture_headers_upstream(seen_headers.clone()).await;
    let proxy = spawn_app(build_app(proxy_config(upstream)).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .header(header::CONNECTION, "x-drop-me, keep-alive")
        .header("x-drop-me", "secret")
        .header("te", "trailers")
        .header("trailer", "x-trailer")
        .header("upgrade", "websocket")
        .header("proxy-authorization", "Basic local")
        .header("x-keep-me", "ok")
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let headers = seen_headers.lock().unwrap();
    assert!(
        headers
            .iter()
            .any(|(name, value)| { name.eq_ignore_ascii_case("x-keep-me") && value == "ok" })
    );
    for blocked in [
        "connection",
        "x-drop-me",
        "te",
        "trailer",
        "upgrade",
        "proxy-authorization",
    ] {
        assert!(
            !headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(blocked)),
            "{blocked} should not be forwarded"
        );
    }
}

#[tokio::test]
async fn protected_body_strips_body_integrity_headers() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let seen_body = Arc::new(Mutex::new(None::<String>));
    let upstream =
        spawn_capture_headers_and_body_upstream(seen_headers.clone(), seen_body.clone()).await;
    let proxy = spawn_app(build_app(proxy_config(upstream)).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .header("content-digest", "sha-256=:original:")
        .header("digest", "sha-256=original")
        .header("content-md5", "original")
        .header("signature", "sig1=:original:")
        .header("signature-input", "sig1=(\"content-digest\")")
        .header("x-keep-me", "ok")
        .body(r#"{"messages":[{"content":"email alice@example.com"}]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let upstream_body = seen_body.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("alice@example.com"));
    assert!(upstream_body.contains("[email:"));

    let headers = seen_headers.lock().unwrap();
    assert!(
        headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("x-keep-me") && value == "ok")
    );
    for stripped in [
        "content-digest",
        "digest",
        "content-md5",
        "signature",
        "signature-input",
    ] {
        assert!(
            !headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(stripped)),
            "{stripped} should be stripped after body mutation"
        );
    }
}

#[tokio::test]
async fn unchanged_body_keeps_body_integrity_headers() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let seen_body = Arc::new(Mutex::new(None::<String>));
    let upstream =
        spawn_capture_headers_and_body_upstream(seen_headers.clone(), seen_body.clone()).await;
    let proxy = spawn_app(build_app(proxy_config(upstream)).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .header("content-digest", "sha-256=:original:")
        .header("x-keep-me", "ok")
        .body(r#"{"messages":[{"content":"hello"}]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        seen_body.lock().unwrap().as_deref(),
        Some(r#"{"messages":[{"content":"hello"}]}"#)
    );

    let headers = seen_headers.lock().unwrap();
    assert!(
        headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-digest") && value == "sha-256=:original:"
        }),
        "content-digest should stay when the body is not changed"
    );
    assert!(
        headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("x-keep-me") && value == "ok")
    );
}

#[tokio::test]
async fn resolves_inbound_response_references_by_default() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"messages":[{"content":"email alice@example.com"}]}"#)
        .send()
        .await
        .unwrap();

    let body = response.text().await.unwrap();
    assert!(body.contains("alice@example.com"));
    assert!(!body.contains("[email:"));

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("alice@example.com"));
    assert!(upstream_body.contains("[email:"));
    assert_eq!(
        dam_vault::Vault::open(vault_path).unwrap().count().unwrap(),
        1
    );

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| entry.event_type == "vault_read"));
    assert!(logs.iter().any(|entry| entry.event_type == "resolve"));
}

#[tokio::test]
async fn resolves_json_escaped_inbound_response_references_by_default() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_json_escaped_reference_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"messages":[{"content":"email alice@example.com"}]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let body = response.text().await.unwrap();
    assert!(body.contains("alice@example.com"));
    assert!(!body.contains("[email:"));
    assert!(!body.contains(r"\\[email:"));

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("alice@example.com"));
    assert!(upstream_body.contains("[email:"));
    assert_eq!(
        dam_vault::Vault::open(vault_path).unwrap().count().unwrap(),
        1
    );

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| entry.event_type == "vault_read"));
    assert!(logs.iter().any(|entry| entry.event_type == "resolve"));
}

#[tokio::test]
async fn resolves_ndjson_escaped_inbound_response_references_by_default() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_ndjson_escaped_reference_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"messages":[{"content":"email alice@example.com"}]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/x-ndjson")
    );
    let body = response.text().await.unwrap();
    assert!(body.contains("alice@example.com"));
    assert!(!body.contains("[email:"));
    assert!(!body.contains(r"\\[email:"));

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| entry.event_type == "vault_read"));
    assert!(logs.iter().any(|entry| entry.event_type == "resolve"));
}

#[tokio::test]
async fn passes_raw_sensitive_inbound_response_without_explicit_inbound_protection() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_raw_sensitive_response_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"messages":[{"content":"hello"}]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(body.contains("leak@example.com"));
    assert!(!body.contains("[email:"));

    assert_eq!(
        upstream_seen.lock().unwrap().as_deref(),
        Some(r#"{"messages":[{"content":"hello"}]}"#)
    );

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(!logs.iter().any(|entry| {
        entry.event_type == "proxy_forward" && entry.action.as_deref() == Some("inbound_protection")
    }));
}

#[tokio::test]
async fn redacts_raw_sensitive_inbound_response_without_wallet_write_when_route_opts_in() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_raw_sensitive_response_upstream(upstream_seen.clone()).await;
    let mut config = proxy_config(upstream);
    set_test_target_inbound_policy(&mut config, true, true);
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"messages":[{"content":"hello"}]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(!body.contains("leak@example.com"));
    assert!(body.contains("[email]"));
    assert!(!body.contains("[email:"));

    assert_eq!(
        upstream_seen.lock().unwrap().as_deref(),
        Some(r#"{"messages":[{"content":"hello"}]}"#)
    );
    assert_eq!(
        dam_vault::Vault::open(vault_path).unwrap().count().unwrap(),
        0
    );

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.event_type == "proxy_forward" && entry.action.as_deref() == Some("inbound_protection")
    }));
    assert!(logs.iter().any(|entry| {
        entry.event_type == "redaction" && entry.action.as_deref() == Some("redacted")
    }));
    assert!(!logs.iter().any(|entry| {
        entry.event_type == "vault_write"
            || entry.event_type == "vault_write_failed"
            || entry.action.as_deref() == Some("tokenized")
    }));
}

#[tokio::test]
async fn redacts_email_domain_in_inbound_response_from_request_context() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_raw_domain_response_upstream(upstream_seen.clone()).await;
    let mut config = proxy_config(upstream);
    set_test_target_inbound_policy(&mut config, true, true);
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"messages":[{"content":"email person@leak.example"}]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(!body.contains("leak.example"));
    assert!(body.contains("[domain]"));
    assert!(!body.contains("[domain:"));

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("person@leak.example"));
    assert!(upstream_body.contains("[email:"));

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.event_type == "proxy_forward" && entry.action.as_deref() == Some("inbound_protection")
    }));
    assert!(logs.iter().any(|entry| {
        entry.kind.as_deref() == Some("domain")
            && entry.event_type == "redaction"
            && entry.action.as_deref() == Some("redacted")
    }));
    assert!(!logs.iter().any(|entry| {
        entry.kind.as_deref() == Some("domain") && entry.action.as_deref() == Some("tokenized")
    }));
}

#[tokio::test]
async fn redacts_email_domain_in_anthropic_stream_from_request_context() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_anthropic_sse_raw_domain_upstream(upstream_seen.clone()).await;
    let mut config = anthropic_proxy_config(upstream);
    config.proxy.targets[0].name = "anthropic".to_string();
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/messages"))
        .body(r#"{"messages":[{"content":"email banana@splonk.io"}],"stream":true}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = response.text().await.unwrap();
    assert!(!body.contains("splonk.io"));
    assert!(body.contains("[domain]"));
    assert!(!body.contains("[domain:"));

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("banana@splonk.io"));
    assert!(upstream_body.contains("[email:"));
    let vault = dam_vault::Vault::open(vault_path).unwrap();
    assert_eq!(vault.count().unwrap(), 1);
    assert_eq!(vault.wallet_count().unwrap(), 0);

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| {
        entry.event_type == "resolve" && entry.action.as_deref() == Some("resolve_attempt")
    }));
    assert!(logs.iter().any(|entry| {
        entry.event_type == "proxy_forward" && entry.action.as_deref() == Some("inbound_protection")
    }));
    assert!(logs.iter().any(|entry| {
        entry.kind.as_deref() == Some("domain")
            && entry.event_type == "redaction"
            && entry.action.as_deref() == Some("redacted")
    }));
    assert!(!logs.iter().any(|entry| {
        entry.kind.as_deref() == Some("domain") && entry.action.as_deref() == Some("tokenized")
    }));
}

#[tokio::test]
async fn resolves_event_stream_response_references_by_default() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_sse_upstream(upstream_seen.clone()).await;
    let config = proxy_config(upstream);
    let vault_path = config.vault.sqlite_path.clone();
    let log_path = config.log.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/responses"))
        .body(r#"{"input":[{"content":"email erin@example.com"}],"stream":true}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let body = response.text().await.unwrap();
    assert!(body.contains("erin@example.com"));
    assert!(!body.contains("[email:"));

    let upstream_body = upstream_seen.lock().unwrap().clone().unwrap();
    assert!(!upstream_body.contains("erin@example.com"));
    assert!(upstream_body.contains("[email:"));
    assert_eq!(
        dam_vault::Vault::open(vault_path).unwrap().count().unwrap(),
        1
    );

    let logs = dam_log::LogStore::open(log_path).unwrap().list().unwrap();
    assert!(logs.iter().any(|entry| entry.event_type == "vault_read"));
    assert!(logs.iter().any(|entry| entry.event_type == "resolve"));
}

#[tokio::test]
async fn health_reports_protected_with_dam_api_shape() {
    let upstream = spawn_echo_upstream().await;
    let config = proxy_config(upstream.clone());
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .get(format!("{proxy}/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::Protected);
    assert_eq!(report.target, Some("test-openai".to_string()));
    assert_eq!(report.upstream, Some(upstream));
    assert!(report.operation_id.is_none());
    assert!(report.diagnostics.is_empty());
}

#[tokio::test]
async fn health_reports_config_required_with_dam_api_shape() {
    let upstream = spawn_echo_upstream().await;
    let mut config = proxy_config(upstream);
    config.proxy.targets[0].api_key_env = Some("MISSING_TEST_KEY".to_string());
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .get(format!("{proxy}/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::ConfigRequired);
    assert_eq!(report.diagnostics[0].code, "config_required");
}

#[tokio::test]
async fn blocks_invalid_utf8_even_when_bypass_is_configured() {
    let upstream = spawn_echo_upstream().await;
    let mut config = proxy_config(upstream);
    config.proxy.default_failure_mode = dam_config::ProxyFailureMode::BypassOnError;
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(vec![0xff, b'a'])
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::Blocked);
    assert_eq!(report.diagnostics[0].code, "blocked");
    assert!(report.message.contains("not utf-8"));
}

#[tokio::test]
async fn blocks_invalid_utf8_when_configured() {
    let upstream = spawn_echo_upstream().await;
    let mut config = proxy_config(upstream);
    config.proxy.default_failure_mode = dam_config::ProxyFailureMode::BlockOnError;
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(vec![0xff, b'a'])
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::Blocked);
    assert_eq!(report.diagnostics[0].code, "blocked");
    assert!(report.message.contains("not utf-8"));
}

#[tokio::test]
async fn blocks_consent_errors_even_when_bypass_is_configured() {
    let upstream_seen = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(upstream_seen.clone()).await;
    let mut config = proxy_config(upstream);
    config.proxy.default_failure_mode = dam_config::ProxyFailureMode::BypassOnError;
    let consent_path = config.consent.sqlite_path.clone();
    let proxy = spawn_app(build_app(config).unwrap()).await;
    {
        let conn = rusqlite::Connection::open(consent_path).unwrap();
        conn.execute_batch("DROP TABLE consents;").unwrap();
    }

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body(r#"{"input":"email alice@example.com"}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::Blocked);
    assert_eq!(report.diagnostics[0].code, "blocked");
    assert!(report.message.contains("request protection failed"));
    assert!(upstream_seen.lock().unwrap().is_none());
}

#[tokio::test]
async fn blocks_encoded_request_bodies_before_bypass_policy() {
    let upstream = spawn_echo_upstream().await;
    let mut config = proxy_config(upstream);
    config.proxy.default_failure_mode = dam_config::ProxyFailureMode::BypassOnError;
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .header(header::CONTENT_ENCODING, "gzip")
        .body("not actually gzip")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::Blocked);
    assert!(report.message.contains("encoded request bodies"));
}

#[tokio::test]
async fn policy_block_does_not_forward() {
    let upstream = spawn_echo_upstream().await;
    let mut config = proxy_config(upstream);
    config.policy.default_action = dam_core::PolicyAction::Block;
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body("email alice@example.com")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::Blocked);
    assert_eq!(report.diagnostics[0].code, "blocked");
}

#[tokio::test]
async fn missing_proxy_api_key_reports_config_required() {
    let upstream = spawn_echo_upstream().await;
    let mut config = proxy_config(upstream);
    config.proxy.targets[0].api_key_env = Some("MISSING_TEST_KEY".to_string());
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::ConfigRequired);
    assert_eq!(report.diagnostics[0].code, "config_required");
}

#[tokio::test]
async fn provider_down_is_reported_separately() {
    let config = proxy_config("http://127.0.0.1:1".to_string());
    let proxy = spawn_app(build_app(config).unwrap()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let report = proxy_report(response).await;
    assert_eq!(report.state, dam_api::ProxyState::ProviderDown);
    assert_eq!(report.diagnostics[0].code, "provider_down");
    assert!(!report.message.contains("127.0.0.1:1"));
}

#[test]
fn provider_label_is_configuration_data_not_startup_validation() {
    let mut config = proxy_config("http://127.0.0.1:9999".to_string());
    config.proxy.targets[0].provider = "unknown".to_string();

    assert!(build_app(config).is_ok());
}

#[test]
fn disabled_proxy_fails_at_startup() {
    let mut config = proxy_config("http://127.0.0.1:9999".to_string());
    config.proxy.enabled = false;

    assert!(matches!(
        build_app(config).unwrap_err(),
        ProxyError::Disabled
    ));
}

#[test]
fn fixture_paths_are_temp_files() {
    let config = proxy_config("http://127.0.0.1:9999".to_string());

    assert!(config.vault.sqlite_path.ends_with("vault.db"));
    assert!(config.log.sqlite_path.ends_with("log.db"));
}
