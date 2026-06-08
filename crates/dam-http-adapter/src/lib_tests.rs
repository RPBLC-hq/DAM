use super::*;
use axum::{
    Router,
    body::to_bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use futures_util::stream;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::time::sleep;

type CapturedHeaders = Arc<Mutex<Vec<(String, String)>>>;

async fn spawn_app(app: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn spawn_capture_echo_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn echo(State(seen_body): State<Arc<Mutex<Option<String>>>>, body: Bytes) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        (StatusCode::OK, body_text).into_response()
    }

    spawn_app(
        Router::new()
            .route("/base/v1/chat/completions", post(echo))
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

async fn spawn_json_response_upstream() -> String {
    async fn json_response() -> Response {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"choices":[{"message":{"content":"\\[email:abc123\\]"}}],"metadata":{"safe":"ok"}}"#,
            ))
            .unwrap()
    }

    spawn_app(Router::new().route("/v1/chat/completions", post(json_response))).await
}

async fn spawn_sse_upstream(seen_body: Arc<Mutex<Option<String>>>) -> String {
    async fn sse(State(seen_body): State<Arc<Mutex<Option<String>>>>, body: Bytes) -> Response {
        let body_text = String::from_utf8(body.to_vec()).expect("upstream body should be utf-8");
        *seen_body.lock().unwrap() = Some(body_text.clone());
        let event = format!("event: response.output_text.delta\ndata: {body_text}\n\n");
        let split_at = event.find("token").unwrap_or(event.len());
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

async fn spawn_delayed_response_upstream(delay: Duration) -> String {
    async fn delayed(State(delay): State<Duration>) -> Response {
        sleep(delay).await;
        (StatusCode::OK, "delayed response").into_response()
    }

    spawn_app(
        Router::new()
            .route("/v1/responses", post(delayed))
            .with_state(delay),
    )
    .await
}

async fn response_body(response: Response<Body>) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn upstream_url_preserves_base_path_request_path_and_query() {
    let uri = Uri::from_static("/v1/chat/completions?stream=false");

    let url = upstream_url("https://api.example.test/base", &uri).unwrap();

    assert_eq!(
        url,
        "https://api.example.test/base/v1/chat/completions?stream=false"
    );
}

#[test]
fn response_integrity_headers_are_not_forwarded_after_body_transform() {
    assert!(should_skip_response_header("Content-Digest", &[]));
    assert!(should_skip_response_header("Repr-Digest", &[]));
    assert!(should_skip_response_header("Signature-Input", &[]));
}

#[tokio::test]
async fn connect_timeout_is_not_a_total_request_deadline() {
    let upstream = spawn_delayed_response_upstream(Duration::from_millis(150)).await;
    let provider = HttpAdapter::with_timeouts(HttpAdapterTimeouts {
        connect_timeout: Duration::from_millis(50),
        read_timeout: Duration::from_secs(1),
    })
    .unwrap();

    let response = provider
        .forward(
            ForwardRequest {
                upstream: &upstream,
                method: Method::POST,
                uri: Uri::from_static("/v1/responses"),
                headers: HeaderMap::new(),
                body: Bytes::from_static(b"{}"),
                target_api_key: None,
                target_api_key_injection: None,
                transform_streaming_response: false,
            },
            |body| body,
        )
        .await
        .unwrap();

    assert_eq!(response_body(response).await, "delayed response");
}

#[tokio::test]
async fn non_streaming_response_uses_body_transform() {
    let seen_body = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_capture_echo_upstream(seen_body.clone()).await;
    let provider = HttpAdapter::new().unwrap();

    let response = provider
        .forward(
            ForwardRequest {
                upstream: &format!("{upstream}/base"),
                method: Method::POST,
                uri: Uri::from_static("/v1/chat/completions"),
                headers: HeaderMap::new(),
                body: Bytes::from_static(b"raw [email:abc]"),
                target_api_key: None,
                target_api_key_injection: None,
                transform_streaming_response: false,
            },
            |_| Bytes::from_static(b"resolved body"),
        )
        .await
        .unwrap();

    assert_eq!(
        seen_body.lock().unwrap().as_deref(),
        Some("raw [email:abc]")
    );
    assert_eq!(response_body(response).await, "resolved body");
}

#[tokio::test]
async fn non_streaming_json_response_transforms_string_values() {
    let upstream = spawn_json_response_upstream().await;
    let provider = HttpAdapter::new().unwrap();

    let response = provider
        .forward(
            ForwardRequest {
                upstream: &upstream,
                method: Method::POST,
                uri: Uri::from_static("/v1/chat/completions"),
                headers: HeaderMap::new(),
                body: Bytes::from_static(b"{}"),
                target_api_key: None,
                target_api_key_injection: None,
                transform_streaming_response: false,
            },
            |chunk| {
                let text = String::from_utf8(chunk.to_vec()).unwrap();
                Bytes::from(text.replace(r"\[email:abc123\]", "banana@example.test"))
            },
        )
        .await
        .unwrap();

    let body = response_body(response).await;
    assert!(body.contains("banana@example.test"));
    assert!(!body.contains(r"\\[email:abc123\\]"));
    assert!(body.contains(r#""safe":"ok""#));
}

#[tokio::test]
async fn target_api_key_replaces_inbound_authorization() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let upstream = spawn_capture_headers_upstream(seen_headers.clone()).await;
    let provider = HttpAdapter::new().unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        "Bearer local-agent-secret".parse().unwrap(),
    );

    provider
        .forward(
            ForwardRequest {
                upstream: &upstream,
                method: Method::POST,
                uri: Uri::from_static("/v1/chat/completions"),
                headers,
                body: Bytes::from_static(b"{}"),
                target_api_key: Some("upstream-secret"),
                target_api_key_injection: Some(AuthInjection {
                    header: "authorization",
                    scheme: Some("Bearer"),
                    strip_headers: &[],
                }),
                transform_streaming_response: false,
            },
            |body| body,
        )
        .await
        .unwrap();

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
async fn hop_by_hop_and_connection_listed_headers_are_not_forwarded() {
    let seen_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let upstream = spawn_capture_headers_upstream(seen_headers.clone()).await;
    let provider = HttpAdapter::new().unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONNECTION, "x-drop-me, keep-alive".parse().unwrap());
    headers.insert("x-drop-me", "secret".parse().unwrap());
    headers.insert("te", "trailers".parse().unwrap());
    headers.insert("trailer", "x-trailer".parse().unwrap());
    headers.insert("upgrade", "websocket".parse().unwrap());
    headers.insert("proxy-authorization", "Basic local".parse().unwrap());
    headers.insert(header::ACCEPT_ENCODING, "gzip".parse().unwrap());
    headers.insert("x-keep-me", "ok".parse().unwrap());

    provider
        .forward(
            ForwardRequest {
                upstream: &upstream,
                method: Method::POST,
                uri: Uri::from_static("/v1/chat/completions"),
                headers,
                body: Bytes::from_static(b"{}"),
                target_api_key: None,
                target_api_key_injection: None,
                transform_streaming_response: false,
            },
            |body| body,
        )
        .await
        .unwrap();

    let headers = seen_headers.lock().unwrap();
    assert!(
        headers
            .iter()
            .any(|(name, value)| { name.eq_ignore_ascii_case("x-keep-me") && value == "ok" })
    );
    let accept_encoding_values = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("accept-encoding"))
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    assert_eq!(accept_encoding_values, ["identity"]);
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
async fn event_stream_response_passes_through_without_body_transform() {
    let seen_body = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_sse_upstream(seen_body.clone()).await;
    let provider = HttpAdapter::new().unwrap();

    let response = provider
        .forward(
            ForwardRequest {
                upstream: &upstream,
                method: Method::POST,
                uri: Uri::from_static("/v1/responses"),
                headers: HeaderMap::new(),
                body: Bytes::from_static(b"stream token"),
                target_api_key: None,
                target_api_key_injection: None,
                transform_streaming_response: false,
            },
            |_| panic!("streaming response body should not be transformed"),
        )
        .await
        .unwrap();

    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    assert_eq!(seen_body.lock().unwrap().as_deref(), Some("stream token"));
    assert!(response_body(response).await.contains("stream token"));
}

#[tokio::test]
async fn event_stream_response_uses_chunk_transform_when_enabled() {
    let seen_body = Arc::new(Mutex::new(None::<String>));
    let upstream = spawn_sse_upstream(seen_body.clone()).await;
    let provider = HttpAdapter::new().unwrap();

    let response = provider
        .forward(
            ForwardRequest {
                upstream: &upstream,
                method: Method::POST,
                uri: Uri::from_static("/v1/responses"),
                headers: HeaderMap::new(),
                body: Bytes::from_static(b"stream token"),
                target_api_key: None,
                target_api_key_injection: None,
                transform_streaming_response: true,
            },
            |chunk| {
                let text = String::from_utf8(chunk.to_vec()).unwrap();
                Bytes::from(text.replace("stream token", "resolved token"))
            },
        )
        .await
        .unwrap();

    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    assert_eq!(seen_body.lock().unwrap().as_deref(), Some("stream token"));
    let body = response_body(response).await;
    assert!(body.contains("resolved token"));
    assert!(!body.contains("stream token"));
}
