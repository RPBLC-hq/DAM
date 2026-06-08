use super::*;

#[test]
fn transforms_escaped_reference_in_json_string_value() {
    let body = Bytes::from_static(br#"{"text":"\\[email:abc123\\]"}"#);

    let output = transform_json_string_body(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace(r"\[email:abc123\]", "banana@example.test"))
    })
    .unwrap();
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(!output.contains(r"\\[email:abc123\\]"));
}

#[test]
fn transforms_all_nested_json_string_values() {
    let body =
        Bytes::from_static(br#"{"items":["\\[email:abc123\\]",{"text":"\\[phone:def456\\]"}]}"#);

    let output = transform_json_string_body(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(
            text.replace(r"\[email:abc123\]", "banana@example.test")
                .replace(r"\[phone:def456\]", "+1 555 0100"),
        )
    })
    .unwrap();
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(output.contains("+1 555 0100"));
    assert!(!output.contains(r"\\[email:abc123\\]"));
    assert!(!output.contains(r"\\[phone:def456\\]"));
}

#[test]
fn preserves_object_key_order_when_json_changes() {
    let body = Bytes::from_static(br#"{"z":"safe","a":"\\[email:abc123\\]","m":"safe"}"#);

    let output = transform_json_string_body(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace(r"\[email:abc123\]", "banana@example.test"))
    })
    .unwrap();
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert_eq!(
        output,
        r#"{"z":"safe","a":"banana@example.test","m":"safe"}"#
    );
}

#[test]
fn transforms_line_delimited_json_string_values() {
    let body = Bytes::from_static(
        br#"{"type":"delta","text":"\\[email:abc123\\]"}
{"type":"delta","nested":{"text":"\\[phone:def456\\]"}}
"#,
    );

    let output = transform_json_string_body(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(
            text.replace(r"\[email:abc123\]", "banana@example.test")
                .replace(r"\[phone:def456\]", "+1 555 0100"),
        )
    })
    .unwrap();
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("banana@example.test"));
    assert!(output.contains("+1 555 0100"));
    assert!(!output.contains(r"\\[email:abc123\\]"));
    assert!(!output.contains(r"\\[phone:def456\\]"));
    assert!(output.ends_with('\n'));
}

#[test]
fn preserves_non_json_lines_while_transforming_json_lines() {
    let body = Bytes::from_static(
        br#"event: delta
{"text":"\\[email:abc123\\]"}
"#,
    );

    let output = transform_json_string_body(body, |chunk| {
        let text = String::from_utf8(chunk.to_vec()).unwrap();
        Bytes::from(text.replace(r"\[email:abc123\]", "banana@example.test"))
    })
    .unwrap();
    let output = String::from_utf8(output.to_vec()).unwrap();

    assert!(output.contains("event: delta"));
    assert!(output.contains("banana@example.test"));
}

#[test]
fn returns_none_when_json_has_no_string_changes() {
    let body = Bytes::from_static(br#"{"text":"safe"}"#);

    let output = transform_json_string_body(body, |chunk| chunk);

    assert!(output.is_none());
}

#[test]
fn returns_none_for_non_json_body() {
    let body = Bytes::from_static(b"raw [email:abc123]");

    let output = transform_json_string_body(body, |chunk| chunk);

    assert!(output.is_none());
}
