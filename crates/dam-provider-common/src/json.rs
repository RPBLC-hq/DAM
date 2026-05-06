use axum::body::Bytes;
use serde_json::Value;

pub fn transform_json_string_body<F>(body: Bytes, transform: F) -> Option<Bytes>
where
    F: Fn(Bytes) -> Bytes,
{
    transform_whole_json_body(body.clone(), &transform)
        .or_else(|| transform_json_lines_body(body, &transform))
}

fn transform_whole_json_body<F>(body: Bytes, transform: &F) -> Option<Bytes>
where
    F: Fn(Bytes) -> Bytes,
{
    let mut value = serde_json::from_slice::<Value>(body.as_ref()).ok()?;
    if transform_json_strings(&mut value, transform) {
        let body = serde_json::to_vec(&value).ok()?;
        return Some(Bytes::from(body));
    }

    None
}

fn transform_json_lines_body<F>(body: Bytes, transform: &F) -> Option<Bytes>
where
    F: Fn(Bytes) -> Bytes,
{
    let text = std::str::from_utf8(body.as_ref()).ok()?;
    let mut changed = false;
    let mut output = String::with_capacity(text.len());

    for segment in text.split_inclusive('\n') {
        let (line, newline) = split_line_ending(segment);
        if line.trim().is_empty() {
            output.push_str(segment);
            continue;
        }

        let Ok(mut value) = serde_json::from_str::<Value>(line) else {
            output.push_str(segment);
            continue;
        };

        if transform_json_strings(&mut value, transform) {
            let serialized = serde_json::to_string(&value).ok()?;
            output.push_str(&serialized);
            output.push_str(newline);
            changed = true;
        } else {
            output.push_str(segment);
        }
    }

    changed.then(|| Bytes::from(output))
}

fn split_line_ending(segment: &str) -> (&str, &str) {
    if let Some(line) = segment.strip_suffix("\r\n") {
        (line, "\r\n")
    } else if let Some(line) = segment.strip_suffix('\n') {
        (line, "\n")
    } else {
        (segment, "")
    }
}

pub(crate) fn transform_json_strings<F>(value: &mut Value, transform: &F) -> bool
where
    F: Fn(Bytes) -> Bytes,
{
    match value {
        Value::String(text) => {
            let original = text.clone();
            let transformed = transform(Bytes::from(original.clone()));
            let Ok(transformed_text) = String::from_utf8(transformed.to_vec()) else {
                return false;
            };
            if transformed_text == original {
                return false;
            }

            *text = transformed_text;
            true
        }
        Value::Array(values) => values.iter_mut().fold(false, |changed, value| {
            transform_json_strings(value, transform) || changed
        }),
        Value::Object(values) => values.values_mut().fold(false, |changed, value| {
            transform_json_strings(value, transform) || changed
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
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
        let body = Bytes::from_static(
            br#"{"items":["\\[email:abc123\\]",{"text":"\\[phone:def456\\]"}]}"#,
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
}
