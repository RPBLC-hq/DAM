use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, Method, Response, Uri, header},
};
use dam_provider_common::{
    ProviderByteStream, transform_event_stream_text_body, transform_json_string_body,
};
use futures_util::TryStreamExt;
use reqwest::Url;
use std::time::Duration;

const UPSTREAM_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum HttpAdapterError {
    #[error("failed to initialize HTTP upstream adapter: {0}")]
    Client(String),

    #[error("failed to build upstream URL: {0}")]
    UpstreamUrl(String),

    #[error("upstream request failed: {0}")]
    Request(String),

    #[error("upstream response failed: {0}")]
    Response(String),
}

#[derive(Clone)]
pub struct HttpAdapter {
    client: reqwest::Client,
}

pub struct ForwardRequest<'a> {
    pub upstream: &'a str,
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub target_api_key: Option<&'a str>,
    pub target_api_key_injection: Option<AuthInjection<'a>>,
    pub transform_streaming_response: bool,
}

#[derive(Clone, Copy)]
pub struct AuthInjection<'a> {
    pub header: &'a str,
    pub scheme: Option<&'a str>,
    pub strip_headers: &'a [String],
}

impl HttpAdapter {
    pub fn new() -> Result<Self, HttpAdapterError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(UPSTREAM_REQUEST_TIMEOUT)
            .build()
            .map_err(|error| HttpAdapterError::Client(error.to_string()))?;

        Ok(Self { client })
    }

    pub async fn forward<F>(
        &self,
        request: ForwardRequest<'_>,
        transform_response_body: F,
    ) -> Result<Response<Body>, HttpAdapterError>
    where
        F: Fn(Bytes) -> Bytes + Clone + Send + Sync + 'static,
    {
        let url = upstream_url(request.upstream, &request.uri)?;
        let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
            .map_err(|error| HttpAdapterError::Request(error.to_string()))?;
        let request_connection_headers = connection_header_tokens(&request.headers);
        let mut upstream_request = self.client.request(method, url).body(request.body);

        for (name, value) in request.headers.iter() {
            if should_skip_request_header(
                name.as_str(),
                if request.target_api_key.is_some() {
                    request.target_api_key_injection
                } else {
                    None
                },
                &request_connection_headers,
            ) {
                continue;
            }
            upstream_request = upstream_request.header(name, value);
        }
        upstream_request = upstream_request.header(header::ACCEPT_ENCODING, "identity");

        if let (Some(api_key), Some(injection)) =
            (request.target_api_key, request.target_api_key_injection)
        {
            let value = match injection.scheme {
                Some(scheme) if !scheme.trim().is_empty() => {
                    format!("{} {}", scheme.trim(), api_key)
                }
                _ => api_key.to_string(),
            };
            upstream_request = upstream_request.header(injection.header, value);
        }

        let response = upstream_request
            .send()
            .await
            .map_err(|error| HttpAdapterError::Request(error.without_url().to_string()))?;
        let status = response.status();
        let response_headers = response.headers().clone();
        let response_connection_headers = connection_header_tokens(&response_headers);
        let streaming_response = is_streaming_response(&response_headers);

        if streaming_response {
            let mut builder = Response::builder().status(status);
            for (name, value) in response_headers.iter() {
                if should_skip_response_header(name.as_str(), &response_connection_headers) {
                    continue;
                }
                builder = builder.header(name, value);
            }

            let stream = response
                .bytes_stream()
                .map_err(|error| std::io::Error::other(error.without_url().to_string()));
            let stream: ProviderByteStream = if request.transform_streaming_response {
                let transform = transform_response_body.clone();
                transform_event_stream_text_body(stream, transform)
            } else {
                Box::pin(stream)
            };

            return builder
                .body(Body::from_stream(stream))
                .map_err(|error| HttpAdapterError::Response(error.to_string()));
        }

        let response_body = response
            .bytes()
            .await
            .map_err(|error| HttpAdapterError::Response(error.without_url().to_string()))?;
        let response_body = transform_non_streaming_response_body(
            &response_headers,
            response_body,
            transform_response_body,
        );

        let mut builder = Response::builder().status(status);
        for (name, value) in response_headers.iter() {
            if should_skip_response_header(name.as_str(), &response_connection_headers) {
                continue;
            }
            builder = builder.header(name, value);
        }

        builder
            .body(Body::from(response_body))
            .map_err(|error| HttpAdapterError::Response(error.to_string()))
    }
}

fn upstream_url(base: &str, uri: &Uri) -> Result<String, HttpAdapterError> {
    let mut url =
        Url::parse(base).map_err(|error| HttpAdapterError::UpstreamUrl(error.to_string()))?;
    let base_path = url.path().trim_end_matches('/');
    let request_path = uri.path().trim_start_matches('/');
    let path = match (
        base_path.is_empty() || base_path == "/",
        request_path.is_empty(),
    ) {
        (true, true) => "/".to_string(),
        (true, false) => format!("/{request_path}"),
        (false, true) => base_path.to_string(),
        (false, false) => format!("{base_path}/{request_path}"),
    };
    url.set_path(&path);
    url.set_query(uri.query());
    Ok(url.to_string())
}

fn should_skip_request_header(
    name: &str,
    target_api_key_injection: Option<AuthInjection<'_>>,
    connection_headers: &[String],
) -> bool {
    let normalized = name.to_ascii_lowercase();
    connection_headers
        .iter()
        .any(|header| header == &normalized)
        || matches!(
            normalized.as_str(),
            "host"
                | "content-length"
                | "connection"
                | "transfer-encoding"
                | "te"
                | "trailer"
                | "upgrade"
                | "keep-alive"
                | "proxy-authorization"
                | "proxy-authenticate"
                | "accept-encoding"
        )
        || target_api_key_injection.is_some_and(|injection| {
            normalized == injection.header.trim().to_ascii_lowercase()
                || injection
                    .strip_headers
                    .iter()
                    .any(|header| normalized == header.trim().to_ascii_lowercase())
        })
}

fn should_skip_response_header(name: &str, connection_headers: &[String]) -> bool {
    let normalized = name.to_ascii_lowercase();
    connection_headers
        .iter()
        .any(|header| header == &normalized)
        || matches!(
            normalized.as_str(),
            "content-length"
                | "connection"
                | "content-digest"
                | "content-md5"
                | "digest"
                | "repr-digest"
                | "signature"
                | "signature-input"
                | "transfer-encoding"
                | "te"
                | "trailer"
                | "upgrade"
                | "keep-alive"
                | "proxy-authenticate"
                | "x-body-digest"
                | "x-body-sha256"
                | "x-content-digest"
                | "x-content-md5"
                | "x-payload-digest"
                | "x-payload-sha256"
                | "x-signature"
        )
}

fn connection_header_tokens(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

fn is_streaming_response(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .any(|part| part.trim().eq_ignore_ascii_case("text/event-stream"))
        })
}

fn transform_non_streaming_response_body<F>(
    _headers: &HeaderMap,
    body: Bytes,
    transform: F,
) -> Bytes
where
    F: Fn(Bytes) -> Bytes + Clone,
{
    if let Some(transformed) = transform_json_string_body(body.clone(), transform.clone()) {
        return transformed;
    }

    transform(body)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
