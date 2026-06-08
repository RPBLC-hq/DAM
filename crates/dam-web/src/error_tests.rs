use super::*;

#[test]
fn error_code_serializes_snake_case() {
    let body = serde_json::to_string(&WebError::new(WebErrorCode::DaemonUnreachable)).unwrap();
    assert!(body.contains("\"code\":\"daemon_unreachable\""));
    assert!(body.contains("\"ok\":false"));
    assert!(body.contains("\"retriable\":true"));
}

#[test]
fn invalid_request_is_not_retriable() {
    let body = serde_json::to_string(&WebError::new(WebErrorCode::InvalidRequest)).unwrap();
    assert!(body.contains("\"retriable\":false"));
}

#[test]
fn ok_envelope_serializes_correctly() {
    let body = serde_json::to_string(&Ok::new(serde_json::json!({"x": 1}))).unwrap();
    assert!(body.contains("\"ok\":true"));
    assert!(body.contains("\"data\""));
}
