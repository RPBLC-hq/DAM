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
        Value::Array(values) => {
            let mut changed = false;
            for value in values {
                changed = transform_json_strings(value, transform) || changed;
            }
            changed
        }
        Value::Object(values) => {
            let mut changed = false;
            for value in values.values_mut() {
                changed = transform_json_strings(value, transform) || changed;
            }
            changed
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
