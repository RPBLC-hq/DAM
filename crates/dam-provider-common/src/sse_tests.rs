use super::*;

#[test]
fn rewrites_references_split_across_anthropic_text_delta_events() {
    let body = Bytes::from_static(
        br#"event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"[email:abc"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"123]"}}

event: message_stop
data: {"type":"message_stop"}

"#,
    );

    let output = rewrite_event_stream_text(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace("[email:abc123]", "banana@example.test"))
    });
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(!output.contains("[email:abc123]"));
    assert!(output.contains(r#"data: {"type":"message_stop"}"#));
}

#[test]
fn rewrites_references_split_across_openai_chat_delta_events() {
    let body = Bytes::from_static(
        br#"data: {"choices":[{"delta":{"content":"[email:abc"}}]}

data: {"choices":[{"delta":{"content":"123]"}}]}

"#,
    );

    let output = rewrite_event_stream_text(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace("[email:abc123]", "banana@example.test"))
    });
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(!output.contains("[email:abc123]"));
    assert!(output.contains(r#""content":"""#));
}

#[test]
fn rewrites_references_split_across_top_level_completion_events() {
    let body = Bytes::from_static(
        br#"data: {"completion":"\\[email:abc"}

data: {"completion":"123\\]"}

"#,
    );

    let output = rewrite_event_stream_text(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace(r"\[email:abc123\]", "banana@example.test"))
    });
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(!output.contains(r"\\[email:abc123\\]"));
    assert!(output.contains(r#""completion":"""#));
}

#[test]
fn rewrites_references_split_across_message_content_text_events() {
    let body = Bytes::from_static(
        br#"data: {"message":{"content":[{"type":"text","text":"\\[email:abc"}]}}

data: {"message":{"content":[{"type":"text","text":"123\\]"}]}}

"#,
    );

    let output = rewrite_event_stream_text(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace(r"\[email:abc123\]", "banana@example.test"))
    });
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(!output.contains(r"\\[email:abc123\\]"));
    assert!(output.contains(r#""text":"""#));
}

#[test]
fn falls_back_to_raw_transform_when_events_have_no_json_text_delta() {
    let body = Bytes::from_static(b"event: delta\ndata: raw [email:abc123]\n\n");

    let output = rewrite_event_stream_text(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace("[email:abc123]", "banana@example.test"))
    });
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(!output.contains("[email:abc123]"));
}

#[test]
fn falls_back_to_json_string_transform_when_known_text_deltas_do_not_change() {
    let body = Bytes::from_static(
        br#"event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"safe text"}}

event: custom_text
data: {"type":"custom_text","text":"\\[email:abc123\\]"}

"#,
    );

    let output = rewrite_event_stream_text(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace(r"\[email:abc123\]", "banana@example.test"))
    });
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(!output.contains(r"\\[email:abc123\\]"));
    assert!(output.contains("safe text"));
}

#[test]
fn transforms_each_json_string_value_in_unrecognized_events() {
    let body = Bytes::from_static(
        br#"event: custom_text
data: {"text":"\\[email:abc123\\]","nested":{"text":"\\[phone:def456\\]"}}

"#,
    );

    let output = rewrite_event_stream_text(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(
            text.replace(r"\[email:abc123\]", "banana@example.test")
                .replace(r"\[phone:def456\]", "+1 555 0100"),
        )
    });
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(output.contains("+1 555 0100"));
    assert!(!output.contains(r"\\[email:abc123\\]"));
    assert!(!output.contains(r"\\[phone:def456\\]"));
}
