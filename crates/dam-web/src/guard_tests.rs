use super::*;
use axum::http::HeaderValue;

fn header_map(values: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (k, v) in values {
        headers.insert(*k, HeaderValue::from_static(v));
    }
    headers
}

#[test]
fn loopback_host_accepts_127() {
    let headers = header_map(&[("host", "127.0.0.1:2896")]);
    let uri: Uri = "/api/v1/wallet".parse().unwrap();
    assert!(host_is_loopback(&headers, &uri));
}

#[test]
fn loopback_host_rejects_remote() {
    let headers = header_map(&[("host", "example.com")]);
    let uri: Uri = "/api/v1/wallet".parse().unwrap();
    assert!(!host_is_loopback(&headers, &uri));
}

#[test]
fn local_origin_passes() {
    let headers = header_map(&[("origin", "http://127.0.0.1:2896")]);
    assert!(origin_is_local(&headers));
}

#[test]
fn remote_origin_fails() {
    let headers = header_map(&[("origin", "https://example.com")]);
    assert!(!origin_is_local(&headers));
}

#[test]
fn tray_token_matches_when_equal() {
    let headers = header_map(&[("x-dam-web-tray-token", "secret")]);
    assert!(tray_token_matches(&headers, Some("secret")));
}

#[test]
fn tray_token_rejects_when_different() {
    let headers = header_map(&[("x-dam-web-tray-token", "wrong")]);
    assert!(!tray_token_matches(&headers, Some("secret")));
}

#[test]
fn tray_token_rejects_when_missing() {
    let headers = HeaderMap::new();
    assert!(!tray_token_matches(&headers, Some("secret")));
}
