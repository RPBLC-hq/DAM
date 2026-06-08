use axum::body::Bytes;
use serde_json::Value;

use crate::json::transform_json_strings;

#[derive(Clone, Debug)]
enum EventLine {
    Data(String),
    Other(String),
}

#[derive(Clone, Debug)]
struct SseEvent {
    lines: Vec<EventLine>,
    replacement_data: Option<String>,
}

#[derive(Clone, Debug)]
enum TextDeltaPath {
    DeltaText,
    ChoiceDeltaContent(usize),
    ResponseDelta,
    TopLevelCompletion,
    TopLevelText,
    ContentText(usize),
    MessageContentText(usize),
}

#[derive(Clone, Debug)]
struct TextDeltaEvent {
    event_index: usize,
    value: Value,
    path: TextDeltaPath,
    text: String,
}

pub(crate) fn rewrite_event_stream_text<F>(body: Bytes, transform: F) -> Bytes
where
    F: Fn(Bytes) -> Bytes,
{
    let Ok(text) = std::str::from_utf8(body.as_ref()) else {
        return transform(body);
    };
    let normalized = normalize_line_endings(text);
    let trailing_separator = normalized.ends_with("\n\n");
    let mut events = parse_events(&normalized);
    let mut text_delta_events = collect_text_delta_events(&events);

    if !text_delta_events.is_empty() {
        let mut combined_text = String::new();
        for event in &text_delta_events {
            combined_text.push_str(&event.text);
        }

        let transformed = transform(Bytes::from(combined_text.clone()));
        if let Ok(transformed_text) = String::from_utf8(transformed.to_vec())
            && transformed_text != combined_text
        {
            for (index, event) in text_delta_events.iter_mut().enumerate() {
                let replacement = if index == 0 {
                    transformed_text.as_str()
                } else {
                    ""
                };
                if set_text_delta(&mut event.value, &event.path, replacement)
                    && let Ok(data) = serde_json::to_string(&event.value)
                    && let Some(sse_event) = events.get_mut(event.event_index)
                {
                    sse_event.replacement_data = Some(data);
                }
            }

            if let Some(output) =
                rewrite_json_string_values(&mut events, trailing_separator, &transform)
            {
                return output;
            }

            return Bytes::from(render_events(&events, trailing_separator));
        }
    }

    if let Some(output) = rewrite_json_string_values(&mut events, trailing_separator, &transform) {
        return output;
    }

    transform(body)
}

fn rewrite_json_string_values<F>(
    events: &mut [SseEvent],
    trailing_separator: bool,
    transform: &F,
) -> Option<Bytes>
where
    F: Fn(Bytes) -> Bytes,
{
    let mut changed = false;

    for event in events.iter_mut() {
        let Some(data) = event.data() else {
            continue;
        };
        let Ok(mut value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };

        if transform_json_strings(&mut value, transform)
            && let Ok(data) = serde_json::to_string(&value)
        {
            event.replacement_data = Some(data);
            changed = true;
        }
    }

    changed.then(|| Bytes::from(render_events(events, trailing_separator)))
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn parse_events(text: &str) -> Vec<SseEvent> {
    text.split("\n\n")
        .filter(|block| !block.is_empty())
        .map(parse_event)
        .collect()
}

fn parse_event(block: &str) -> SseEvent {
    let lines = block
        .lines()
        .map(|line| {
            if let Some(value) = line.strip_prefix("data:") {
                EventLine::Data(value.strip_prefix(' ').unwrap_or(value).to_string())
            } else {
                EventLine::Other(line.to_string())
            }
        })
        .collect();

    SseEvent {
        lines,
        replacement_data: None,
    }
}

fn collect_text_delta_events(events: &[SseEvent]) -> Vec<TextDeltaEvent> {
    events
        .iter()
        .enumerate()
        .filter_map(|(event_index, event)| {
            let data = event.data()?;
            let value = serde_json::from_str::<Value>(&data).ok()?;
            let (path, text) = text_delta(&value)?;
            let text = text.to_string();
            Some(TextDeltaEvent {
                event_index,
                value,
                path,
                text,
            })
        })
        .collect()
}

fn text_delta(value: &Value) -> Option<(TextDeltaPath, &str)> {
    if let Some(text) = value.pointer("/delta/text").and_then(Value::as_str) {
        return Some((TextDeltaPath::DeltaText, text));
    }
    if let Some(text) = value.get("delta").and_then(Value::as_str) {
        return Some((TextDeltaPath::ResponseDelta, text));
    }
    if let Some(choices) = value.get("choices").and_then(Value::as_array) {
        for (index, choice) in choices.iter().enumerate() {
            if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str) {
                return Some((TextDeltaPath::ChoiceDeltaContent(index), text));
            }
        }
    }
    if let Some(text) = value.get("completion").and_then(Value::as_str) {
        return Some((TextDeltaPath::TopLevelCompletion, text));
    }
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return Some((TextDeltaPath::TopLevelText, text));
    }
    if let Some((index, text)) = array_text_field(value.get("content")) {
        return Some((TextDeltaPath::ContentText(index), text));
    }
    if let Some((index, text)) = array_text_field(value.pointer("/message/content")) {
        return Some((TextDeltaPath::MessageContentText(index), text));
    }

    None
}

fn array_text_field(value: Option<&Value>) -> Option<(usize, &str)> {
    let values = value?.as_array()?;
    for (index, value) in values.iter().enumerate() {
        if let Some(text) = value.get("text").and_then(Value::as_str) {
            return Some((index, text));
        }
    }

    None
}

fn set_text_delta(value: &mut Value, path: &TextDeltaPath, replacement: &str) -> bool {
    match path {
        TextDeltaPath::DeltaText => set_pointer_string(value, "/delta/text", replacement),
        TextDeltaPath::ChoiceDeltaContent(index) => {
            let Some(choices) = value.get_mut("choices").and_then(Value::as_array_mut) else {
                return false;
            };
            let Some(choice) = choices.get_mut(*index) else {
                return false;
            };
            set_pointer_string(choice, "/delta/content", replacement)
        }
        TextDeltaPath::ResponseDelta => {
            let Some(delta) = value.get_mut("delta") else {
                return false;
            };
            *delta = Value::String(replacement.to_string());
            true
        }
        TextDeltaPath::TopLevelCompletion => set_top_level_string(value, "completion", replacement),
        TextDeltaPath::TopLevelText => set_top_level_string(value, "text", replacement),
        TextDeltaPath::ContentText(index) => {
            set_array_text_field(value.get_mut("content"), *index, replacement)
        }
        TextDeltaPath::MessageContentText(index) => {
            set_array_text_field(value.pointer_mut("/message/content"), *index, replacement)
        }
    }
}

fn set_array_text_field(value: Option<&mut Value>, index: usize, replacement: &str) -> bool {
    let Some(values) = value.and_then(Value::as_array_mut) else {
        return false;
    };
    let Some(value) = values.get_mut(index) else {
        return false;
    };
    set_top_level_string(value, "text", replacement)
}

fn set_top_level_string(value: &mut Value, key: &str, replacement: &str) -> bool {
    let Some(target) = value.get_mut(key) else {
        return false;
    };
    *target = Value::String(replacement.to_string());
    true
}

fn set_pointer_string(value: &mut Value, pointer: &str, replacement: &str) -> bool {
    let Some(target) = value.pointer_mut(pointer) else {
        return false;
    };
    *target = Value::String(replacement.to_string());
    true
}

fn render_events(events: &[SseEvent], trailing_separator: bool) -> String {
    let mut output = events
        .iter()
        .map(SseEvent::render)
        .collect::<Vec<_>>()
        .join("\n\n");
    if trailing_separator {
        output.push_str("\n\n");
    }
    output
}

impl SseEvent {
    fn data(&self) -> Option<String> {
        let values = self
            .lines
            .iter()
            .filter_map(|line| match line {
                EventLine::Data(value) => Some(value.as_str()),
                EventLine::Other(_) => None,
            })
            .collect::<Vec<_>>();
        if values.is_empty() {
            None
        } else {
            Some(values.join("\n"))
        }
    }

    fn render(&self) -> String {
        let mut replacement_written = false;
        self.lines
            .iter()
            .filter_map(|line| match (line, self.replacement_data.as_deref()) {
                (EventLine::Data(_), Some(_)) if replacement_written => None,
                (EventLine::Data(_), Some(replacement)) => {
                    replacement_written = true;
                    Some(format!("data: {replacement}"))
                }
                (EventLine::Data(value), None) => Some(format!("data: {value}")),
                (EventLine::Other(value), _) => Some(value.clone()),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod tests;
