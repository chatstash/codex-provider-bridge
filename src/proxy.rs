use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::header::{
    self, HeaderMap, HeaderName, HeaderValue, CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY,
    SEC_WEBSOCKET_PROTOCOL, UPGRADE,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use reqwest::Client;
use sha1::{Digest, Sha1};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};
use tokio_tungstenite::WebSocketStream;

use crate::config::{assert_configured, resolve_api_key};
use crate::error::{err, Result};
use crate::types::BridgeConfig;

type ResponseBody = BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

#[derive(Clone)]
struct AppState {
    config: BridgeConfig,
    client: Client,
}

pub fn to_upstream_url(incoming_url: &str, upstream_base_url: &str) -> Result<String> {
    let (path, query) = incoming_url
        .split_once('?')
        .map(|(path, query)| (path, Some(query)))
        .unwrap_or((incoming_url, None));
    let suffix = if path == "/v1" {
        ""
    } else if let Some(rest) = path.strip_prefix("/v1/") {
        return Ok(format!(
            "{}/{}{}",
            upstream_base_url.trim_end_matches('/'),
            rest,
            query.map(|query| format!("?{query}")).unwrap_or_default()
        ));
    } else {
        path
    };
    Ok(format!(
        "{}{}{}",
        upstream_base_url.trim_end_matches('/'),
        suffix,
        query.map(|query| format!("?{query}")).unwrap_or_default()
    ))
}

pub fn to_upstream_ws_url(incoming_url: &str, upstream_base_url: &str) -> Result<String> {
    let http_url = to_upstream_url(incoming_url, upstream_base_url)?;
    if let Some(rest) = http_url.strip_prefix("https://") {
        return Ok(format!("wss://{rest}"));
    }
    if let Some(rest) = http_url.strip_prefix("http://") {
        return Ok(format!("ws://{rest}"));
    }
    Err(err(format!(
        "上游地址必须以 http:// 或 https:// 开头，当前为 {upstream_base_url}"
    )))
}

pub async fn serve(config: BridgeConfig) -> Result<()> {
    assert_configured(&config)?;
    let key = resolve_api_key(&config);
    if key.api_key.is_none() {
        return Err(err(format!(
            "Missing API key. Run \"codex-provider-bridge setup\" or set {}.",
            config.api_key_env
        )));
    }

    let client = Client::builder()
        .http1_only()
        .build()
        .map_err(|error| err(format!("failed to build HTTP client: {error}")))?;
    let state = Arc::new(AppState {
        config: config.clone(),
        client,
    });
    let listener = TcpListener::bind(format!("{}:{}", config.host, config.port)).await?;
    println!(
        "codex-provider-bridge listening on http://{}:{}/v1",
        config.host, config.port
    );
    println!("upstream: {}", config.upstream_base_url);
    println!(
        "api key: {}",
        if key.source.as_deref() == Some("environment") {
            format!("environment {}", config.api_key_env)
        } else {
            "saved local config".to_string()
        }
    );

    loop {
        let (stream, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |request| {
                let state = state.clone();
                async move { handle_request(request, state).await }
            });
            if let Err(error) = http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await
            {
                eprintln!("{error}");
            }
        });
    }
}

async fn handle_request(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> std::result::Result<Response<ResponseBody>, Infallible> {
    let start = Instant::now();
    let method = request.method().clone();
    let path = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());

    let response = match handle_request_inner(request, state).await {
        Ok(response) => response,
        Err(error) => json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!(r#"{{"error":"{}"}}"#, escape_json(&error.to_string())),
        ),
    };
    println!(
        "{} {} -> {} {}ms",
        method,
        path,
        response.status().as_u16(),
        start.elapsed().as_millis()
    );
    Ok(response)
}

async fn handle_request_inner(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> Result<Response<ResponseBody>> {
    let path = request
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());
    if !is_v1_path(&path) {
        return Ok(json_response(
            StatusCode::NOT_FOUND,
            r#"{"error":"codex-provider-bridge only proxies /v1/* requests."}"#,
        ));
    }

    let key = resolve_api_key(&state.config);
    let Some(api_key) = key.api_key else {
        return Ok(json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!(
                r#"{{"error":"Missing API key. Run \"codex-provider-bridge setup\" or set {}."}}"#,
                state.config.api_key_env
            ),
        ));
    };

    if is_websocket_upgrade(request.headers()) {
        return handle_websocket_upgrade(request, &path, state, api_key).await;
    }
    if is_other_upgrade(request.headers()) {
        return Ok(json_response(
            StatusCode::UPGRADE_REQUIRED,
            r#"{"error":"Only websocket upgrade is supported."}"#,
        ));
    }

    proxy_http_request(request, &path, &state, &api_key).await
}

async fn proxy_http_request(
    request: Request<Incoming>,
    path: &str,
    state: &AppState,
    api_key: &str,
) -> Result<Response<ResponseBody>> {
    let target = to_upstream_url(path, &state.config.upstream_base_url)?;
    let uri: Uri = target
        .parse()
        .map_err(|error| err(format!("invalid upstream URI: {error}")))?;
    let (parts, body) = request.into_parts();
    let mut builder = state.client.request(parts.method.clone(), uri.to_string());
    let headers = forwarded_http_headers(&parts.headers, api_key);
    builder = builder.headers(headers);
    let body_stream = body
        .into_data_stream()
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) });
    let body = reqwest::Body::wrap_stream(body_stream);
    let upstream = builder
        .body(body)
        .send()
        .await
        .map_err(|error| err(format!("upstream request failed: {error}")))?;

    let status = upstream.status();
    let headers = filtered_response_headers(upstream.headers());
    let stream = upstream
        .bytes_stream()
        .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })
        .map_ok(Frame::data);
    let body = boxed_stream_body(stream);
    let mut response = Response::builder().status(status);
    for (name, value) in headers {
        response = response.header(name, value);
    }
    response
        .body(body)
        .map_err(|error| err(format!("failed to build response: {error}")))
}

async fn handle_websocket_upgrade(
    request: Request<Incoming>,
    path: &str,
    state: Arc<AppState>,
    api_key: String,
) -> Result<Response<ResponseBody>> {
    if request.method() != Method::GET {
        return Ok(json_response(
            StatusCode::METHOD_NOT_ALLOWED,
            r#"{"error":"WebSocket upgrade requires GET."}"#,
        ));
    }

    let key = request
        .headers()
        .get(SEC_WEBSOCKET_KEY)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| err("missing Sec-WebSocket-Key"))?
        .to_string();
    let accept_value = websocket_accept_value(&key);
    let protocol = request.headers().get(SEC_WEBSOCKET_PROTOCOL).cloned();
    let upstream_url = to_upstream_ws_url(path, &state.config.upstream_base_url)?;
    let upstream_request = build_upstream_ws_request(&upstream_url, request.headers(), &api_key)?;
    let upstream = connect_async(upstream_request).await;
    let (upstream_ws, upstream_response) = match upstream {
        Ok(result) => result,
        Err(tungstenite::Error::Http(response)) => return Ok(map_ws_error_response(response)),
        Err(error) => {
            return Ok(json_response(
                StatusCode::BAD_GATEWAY,
                &format!(
                    r#"{{"error":"upstream websocket connect failed: {}"}}"#,
                    escape_json(&error.to_string())
                ),
            ));
        }
    };
    let selected_protocol = upstream_response
        .headers()
        .get(SEC_WEBSOCKET_PROTOCOL)
        .cloned();

    let on_upgrade = hyper::upgrade::on(request);
    let response = Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(CONNECTION, "Upgrade")
        .header(UPGRADE, "websocket")
        .header(SEC_WEBSOCKET_ACCEPT, accept_value);
    let response = if let Some(protocol) = selected_protocol.as_ref() {
        response.header(SEC_WEBSOCKET_PROTOCOL, protocol)
    } else {
        response
    }
    .body(empty_body())
    .map_err(|error| err(format!("failed to build websocket response: {error}")))?;

    tokio::spawn(async move {
        let upgraded = match on_upgrade.await {
            Ok(upgraded) => upgraded,
            Err(error) => {
                eprintln!("websocket upgrade failed: {error}");
                return;
            }
        };
        let downstream =
            WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, None).await;
        if let Some(expected) = protocol.as_ref().and_then(|value| value.to_str().ok()) {
            if selected_protocol
                .as_ref()
                .and_then(|value| value.to_str().ok())
                != Some(expected)
            {
                eprintln!("upstream websocket protocol mismatch");
            }
        }
        if let Err(error) = bridge_websocket_streams(downstream, upstream_ws).await {
            eprintln!("websocket tunnel failed: {error}");
        }
    });

    Ok(response)
}

async fn bridge_websocket_streams(
    downstream: WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
    upstream: WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
) -> Result<()> {
    let (mut downstream_sink, mut downstream_stream) = downstream.split();
    let (mut upstream_sink, mut upstream_stream) = upstream.split();

    let downstream_to_upstream = async {
        while let Some(message) = downstream_stream.next().await {
            let message = message
                .map_err(|error| err(format!("downstream websocket read failed: {error}")))?;
            if message.is_close() {
                upstream_sink
                    .send(message)
                    .await
                    .map_err(|error| err(format!("upstream websocket close failed: {error}")))?;
                break;
            }
            upstream_sink
                .send(message)
                .await
                .map_err(|error| err(format!("upstream websocket send failed: {error}")))?;
        }
        upstream_sink
            .close()
            .await
            .map_err(|error| err(format!("upstream websocket final close failed: {error}")))
    };

    let upstream_to_downstream = async {
        while let Some(message) = upstream_stream.next().await {
            let message =
                message.map_err(|error| err(format!("upstream websocket read failed: {error}")))?;
            if message.is_close() {
                downstream_sink
                    .send(message)
                    .await
                    .map_err(|error| err(format!("downstream websocket close failed: {error}")))?;
                break;
            }
            downstream_sink
                .send(message)
                .await
                .map_err(|error| err(format!("downstream websocket send failed: {error}")))?;
        }
        downstream_sink
            .close()
            .await
            .map_err(|error| err(format!("downstream websocket final close failed: {error}")))
    };

    tokio::try_join!(downstream_to_upstream, upstream_to_downstream)?;
    Ok(())
}

fn build_upstream_ws_request(
    target: &str,
    headers: &HeaderMap<HeaderValue>,
    api_key: &str,
) -> Result<tungstenite::http::Request<()>> {
    let mut request = target.into_client_request().map_err(|error| {
        err(format!(
            "failed to create upstream websocket request: {error}"
        ))
    })?;
    let outgoing = request.headers_mut();
    for (name, value) in forwarded_ws_headers(headers, api_key) {
        outgoing.insert(name, value);
    }
    Ok(request)
}

fn forwarded_http_headers(headers: &HeaderMap<HeaderValue>, api_key: &str) -> HeaderMap {
    let blocked = blocked_http_headers();
    let mut outgoing = HeaderMap::new();
    for (name, value) in headers {
        if !blocked.contains(name.as_str()) {
            outgoing.insert(name.clone(), value.clone());
        }
    }
    outgoing.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}"))
            .unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    outgoing
}

fn forwarded_ws_headers(
    headers: &HeaderMap<HeaderValue>,
    api_key: &str,
) -> Vec<(
    tungstenite::http::HeaderName,
    tungstenite::http::HeaderValue,
)> {
    let mut outgoing = Vec::new();
    let blocked = blocked_ws_headers();
    for (name, value) in headers {
        if blocked.contains(name.as_str()) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            tungstenite::http::HeaderName::from_bytes(name.as_str().as_bytes()),
            tungstenite::http::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            outgoing.push((name, value));
        }
    }
    outgoing.push((
        tungstenite::http::header::AUTHORIZATION,
        tungstenite::http::HeaderValue::from_str(&format!("Bearer {api_key}"))
            .unwrap_or_else(|_| tungstenite::http::HeaderValue::from_static("")),
    ));
    outgoing
}

fn filtered_response_headers(headers: &HeaderMap<HeaderValue>) -> Vec<(HeaderName, HeaderValue)> {
    let blocked = blocked_response_headers();
    headers
        .iter()
        .filter_map(|(name, value)| {
            if blocked.contains(name.as_str()) {
                return None;
            }
            let name = HeaderName::from_bytes(name.as_str().as_bytes()).ok()?;
            let value = HeaderValue::from_bytes(value.as_bytes()).ok()?;
            Some((name, value))
        })
        .collect()
}

fn map_ws_error_response(
    response: tungstenite::http::Response<Option<Vec<u8>>>,
) -> Response<ResponseBody> {
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);
    for (name, value) in filtered_response_headers(response.headers()) {
        builder = builder.header(name, value);
    }
    let body = response.body().clone().unwrap_or_default();
    builder
        .body(full_body(Bytes::from(body)))
        .unwrap_or_else(|_| {
            json_response(
                StatusCode::BAD_GATEWAY,
                r#"{"error":"failed to map websocket error response"}"#,
            )
        })
}

fn blocked_http_headers() -> HashSet<&'static str> {
    HashSet::from([
        "authorization",
        "connection",
        "content-length",
        "host",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ])
}

fn blocked_ws_headers() -> HashSet<&'static str> {
    HashSet::from([
        "authorization",
        "connection",
        "host",
        "sec-websocket-accept",
        "sec-websocket-extensions",
        "sec-websocket-key",
        "sec-websocket-version",
        "upgrade",
    ])
}

fn blocked_response_headers() -> HashSet<&'static str> {
    HashSet::from([
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ])
}

fn websocket_accept_value(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(WS_GUID.as_bytes());
    STANDARD.encode(hasher.finalize())
}

fn is_websocket_upgrade(headers: &HeaderMap<HeaderValue>) -> bool {
    headers
        .get(CONNECTION)
        .and_then(|value| value.to_str().ok())
        .map(connection_has_upgrade)
        .unwrap_or(false)
        && headers
            .get(UPGRADE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.eq_ignore_ascii_case("websocket"))
            .unwrap_or(false)
}

fn is_other_upgrade(headers: &HeaderMap<HeaderValue>) -> bool {
    headers.contains_key(UPGRADE) && !is_websocket_upgrade(headers)
}

fn connection_has_upgrade(value: &str) -> bool {
    value
        .split(',')
        .map(|part| part.trim())
        .any(|part| part.eq_ignore_ascii_case("upgrade"))
}

fn is_v1_path(incoming_url: &str) -> bool {
    incoming_url == "/v1" || incoming_url.starts_with("/v1/") || incoming_url.starts_with("/v1?")
}

fn json_response(status: StatusCode, body: &str) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .body(full_body(Bytes::from(format!("{body}\n"))))
        .unwrap_or_else(|_| {
            Response::new(full_body(Bytes::from_static(
                b"{\"error\":\"internal response build failure\"}\n",
            )))
        })
}

fn empty_body() -> ResponseBody {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn full_body(body: Bytes) -> ResponseBody {
    Full::new(body).map_err(|never| match never {}).boxed()
}

fn boxed_stream_body<S>(stream: S) -> ResponseBody
where
    S: futures_util::Stream<
            Item = std::result::Result<Frame<Bytes>, Box<dyn std::error::Error + Send + Sync>>,
        > + Send
        + Sync
        + 'static,
{
    BodyExt::boxed(StreamBody::new(stream))
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::default_config;
    use hyper::header::{CONTENT_TYPE, SEC_WEBSOCKET_PROTOCOL};
    use reqwest::Client;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn maps_v1_paths_to_upstream() {
        assert_eq!(
            to_upstream_url("/v1/chat/completions?x=1", "https://api.example.com/v1").unwrap(),
            "https://api.example.com/v1/chat/completions?x=1"
        );
        assert_eq!(
            to_upstream_url("/v1", "https://api.example.com/v1/").unwrap(),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn maps_http_to_websocket_url() {
        assert_eq!(
            to_upstream_ws_url("/v1/responses?x=1", "https://api.example.com/v1").unwrap(),
            "wss://api.example.com/v1/responses?x=1"
        );
        assert_eq!(
            to_upstream_ws_url("/v1/responses", "http://127.0.0.1:8080/v1").unwrap(),
            "ws://127.0.0.1:8080/v1/responses"
        );
    }

    #[test]
    fn filters_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer old"),
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let forwarded = forwarded_http_headers(&headers, "new");
        assert_eq!(forwarded.get(header::AUTHORIZATION).unwrap(), "Bearer new");
        assert_eq!(
            forwarded.get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[test]
    fn filters_websocket_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer old"),
        );
        headers.insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static("realtime"));
        headers.insert(SEC_WEBSOCKET_KEY, HeaderValue::from_static("abc"));
        let forwarded = forwarded_ws_headers(&headers, "new");
        assert!(forwarded
            .iter()
            .any(|(name, value)| name == SEC_WEBSOCKET_PROTOCOL.as_str() && value == "realtime"));
        assert!(forwarded
            .iter()
            .any(|(name, value)| name == header::AUTHORIZATION.as_str() && value == "Bearer new"));
        assert!(!forwarded
            .iter()
            .any(|(name, _)| name == SEC_WEBSOCKET_KEY.as_str()));
    }

    #[test]
    fn keeps_content_encoding_in_response_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(
            header::TRANSFER_ENCODING,
            HeaderValue::from_static("chunked"),
        );

        let filtered = filtered_response_headers(&headers);

        assert!(filtered
            .iter()
            .any(|(name, value)| *name == CONTENT_TYPE && *value == "application/json"));
        assert!(filtered
            .iter()
            .any(|(name, value)| *name == header::CONTENT_ENCODING && *value == "gzip"));
        assert!(!filtered
            .iter()
            .any(|(name, _)| *name == header::TRANSFER_ENCODING));
    }

    #[test]
    fn detects_websocket_upgrade() {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("keep-alive, Upgrade"));
        headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
        assert!(is_websocket_upgrade(&headers));
    }

    #[test]
    fn computes_websocket_accept_value() {
        assert_eq!(
            websocket_accept_value("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[tokio::test]
    async fn proxies_http_response_stream() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = upstream.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
                )
                .await
                .unwrap();
        });

        let config = BridgeConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            upstream_base_url: format!("http://{upstream_addr}/v1"),
            api_key: Some("secret".to_string()),
            ..default_config()
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bridge_addr = listener.local_addr().unwrap();
        let state = Arc::new(AppState {
            config,
            client: Client::builder().http1_only().build().unwrap(),
        });
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let io = TokioIo::new(stream);
            let service_state = state.clone();
            let service = service_fn(move |request| {
                let service_state = service_state.clone();
                async move { handle_request(request, service_state).await }
            });
            http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await
                .unwrap();
        });

        let client = Client::builder().http1_only().build().unwrap();
        let response = client
            .post(format!("http://{bridge_addr}/v1/responses"))
            .header(CONTENT_TYPE, "application/json")
            .body(r#"{"x":1}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn maps_upstream_ws_error_response() {
        let response = tungstenite::http::Response::builder()
            .status(426)
            .header(CONTENT_TYPE, "application/json")
            .header(header::CONTENT_ENCODING, "gzip")
            .header(header::TRANSFER_ENCODING, "chunked")
            .body(Some(br#"{"error":"upgrade required"}"#.to_vec()))
            .unwrap();
        let mapped = map_ws_error_response(response);
        assert_eq!(mapped.status(), StatusCode::UPGRADE_REQUIRED);
        assert_eq!(
            mapped.headers().get(header::CONTENT_ENCODING).unwrap(),
            "gzip"
        );
        assert!(!mapped.headers().contains_key(header::TRANSFER_ENCODING));
    }

    #[tokio::test]
    async fn websocket_bridge_forwards_echo_frames() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = upstream.accept().await.unwrap();
            let callback =
                |req: &tungstenite::handshake::server::Request,
                 mut response: tungstenite::handshake::server::Response| {
                    if req.headers().get(SEC_WEBSOCKET_PROTOCOL).is_some() {
                        response
                            .headers_mut()
                            .insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static("realtime"));
                    }
                    Ok(response)
                };
            let ws = tokio_tungstenite::accept_hdr_async(stream, callback)
                .await
                .unwrap();
            let (mut sink, mut stream) = ws.split();
            while let Some(message) = stream.next().await {
                let message = message.unwrap();
                if message.is_close() {
                    sink.send(message).await.unwrap();
                    break;
                }
                sink.send(message).await.unwrap();
            }
        });

        let config = BridgeConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            upstream_base_url: format!("http://{upstream_addr}/v1"),
            api_key: Some("secret".to_string()),
            ..default_config()
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bridge_addr = listener.local_addr().unwrap();
        let state = Arc::new(AppState {
            config,
            client: Client::builder().http1_only().build().unwrap(),
        });
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let io = TokioIo::new(stream);
            let service_state = state.clone();
            let service = service_fn(move |request| {
                let service_state = service_state.clone();
                async move { handle_request(request, service_state).await }
            });
            http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await
                .unwrap();
        });

        let mut request = format!("ws://{bridge_addr}/v1/responses")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert(SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static("realtime"));
        let (mut client, response) = connect_async(request).await.unwrap();
        assert_eq!(
            response
                .headers()
                .get(SEC_WEBSOCKET_PROTOCOL)
                .and_then(|value| value.to_str().ok()),
            Some("realtime")
        );
        client
            .send(tungstenite::Message::Text("hello".into()))
            .await
            .unwrap();
        let response = client.next().await.unwrap().unwrap();
        assert_eq!(response.into_text().unwrap(), "hello");
        client.close(None).await.unwrap();
    }
}
