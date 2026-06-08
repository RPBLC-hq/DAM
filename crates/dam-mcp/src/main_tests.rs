use super::*;

#[test]
fn lists_tools() {
    let config = dam_config::DamConfig::default();
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
    let response = handle_message(&config, &request).unwrap();

    assert!(response.to_string().contains("dam_consent_grant"));
    assert!(response.to_string().contains("dam_consent_revoke"));
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
