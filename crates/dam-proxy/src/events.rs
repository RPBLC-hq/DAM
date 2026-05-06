use axum::{
    body::Body,
    http::{HeaderMap, HeaderName, Response, header},
};
use dam_core::{LogEvent, LogEventType, LogLevel};

use crate::{ProxyState, response_is_streaming};

pub(crate) fn record_proxy_event(
    state: &ProxyState,
    operation_id: &str,
    level: LogLevel,
    event_type: LogEventType,
    action: impl Into<String>,
    message: impl Into<String>,
) {
    let Some(sink) = &state.log_sink else {
        return;
    };

    let event = LogEvent::new(operation_id, level, event_type, message).with_action(action);
    let _ = sink.record(&event);
}

pub(crate) fn log_provider_response(
    state: &ProxyState,
    operation_id: &str,
    response: &Response<Body>,
) {
    record_proxy_event(
        state,
        operation_id,
        LogLevel::Info,
        LogEventType::ProxyForward,
        "provider_response",
        format!(
            "provider response status={} content_type={} content_encoding={} streaming={}",
            response.status().as_u16(),
            header_value(response.headers(), header::CONTENT_TYPE),
            header_value(response.headers(), header::CONTENT_ENCODING),
            response_is_streaming(response)
        ),
    );
}

pub(crate) fn log_intercepted_response_write(
    state: &ProxyState,
    operation_id: &str,
    response: &Response<Body>,
) {
    record_proxy_event(
        state,
        operation_id,
        LogLevel::Info,
        LogEventType::ProxyForward,
        "intercepted_response_write",
        format!(
            "intercepted response write status={} content_type={} streaming={}",
            response.status().as_u16(),
            header_value(response.headers(), header::CONTENT_TYPE),
            response_is_streaming(response)
        ),
    );
}

fn header_value(headers: &HeaderMap, name: HeaderName) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(80).collect())
        .unwrap_or_else(|| "none".to_string())
}
