use std::collections::HashMap;
use std::process::Stdio;
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command;

use crate::config::{assert_configured, resolve_api_key};
use crate::error::{err, Result};
use crate::types::BridgeConfig;

#[derive(Debug)]
struct IncomingRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
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

pub async fn serve(config: BridgeConfig) -> Result<()> {
    assert_configured(&config)?;
    let key = resolve_api_key(&config);
    if key.api_key.is_none() {
        return Err(err(format!(
            "Missing API key. Run \"codex-provider-bridge setup\" or set {}.",
            config.api_key_env
        )));
    }
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
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, config).await {
                eprintln!("{error}");
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream, config: BridgeConfig) -> Result<()> {
    let start = Instant::now();
    let request = read_request(&mut stream).await?;
    if !is_v1_path(&request.path) {
        write_json(
            &mut stream,
            404,
            "Not Found",
            r#"{"error":"codex-provider-bridge only proxies /v1/* requests."}"#,
        )
        .await?;
        return Ok(());
    }
    let key = resolve_api_key(&config);
    let Some(api_key) = key.api_key else {
        write_json(
            &mut stream,
            500,
            "Internal Server Error",
            &format!(
                r#"{{"error":"Missing API key. Run \"codex-provider-bridge setup\" or set {}."}}"#,
                config.api_key_env
            ),
        )
        .await?;
        return Ok(());
    };
    let target = to_upstream_url(&request.path, &config.upstream_base_url)?;
    let status = forward_with_curl(&mut stream, &request, &target, &api_key).await?;
    println!(
        "{} {} -> {} {}ms",
        request.method,
        request.path,
        status,
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn read_request(stream: &mut TcpStream) -> Result<IncomingRequest> {
    let mut buffer = Vec::new();
    let header_end;
    loop {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(err("empty request"));
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(index) = find_header_end(&buffer) {
            header_end = index;
            break;
        }
        if buffer.len() > 1024 * 1024 {
            return Err(err("request headers too large"));
        }
    }

    let header_bytes = &buffer[..header_end];
    let header_text = String::from_utf8_lossy(header_bytes);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or_else(|| err("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| err("missing method"))?
        .to_string();
    let path = parts.next().ok_or_else(|| err("missing path"))?.to_string();
    let headers = lines
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect::<Vec<_>>();

    let content_length = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buffer[header_end + 4..].to_vec();
    while body.len() < content_length {
        let mut chunk = vec![0u8; content_length - body.len()];
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);

    Ok(IncomingRequest {
        method,
        path,
        headers,
        body,
    })
}

async fn forward_with_curl(
    stream: &mut TcpStream,
    request: &IncomingRequest,
    target: &str,
    api_key: &str,
) -> Result<u16> {
    let mut command = Command::new("curl");
    command.args(["-sS", "-N", "-i", "-X", &request.method]);
    command.arg(target);
    for (key, value) in forwarded_headers(&request.headers, api_key) {
        command.args(["-H", &format!("{key}: {value}")]);
    }
    if request.method != "GET" && request.method != "HEAD" {
        command.args(["--data-binary", "@-"]);
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&request.body).await?;
    }
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| err("failed to capture curl stdout"))?;
    let (status, reason, headers, buffered_body) = read_upstream_head(&mut stdout).await?;
    write_response_head(stream, status, &reason, &headers).await?;
    stream.write_all(&buffered_body).await?;
    tokio::io::copy(&mut stdout, stream).await?;
    let _ = child.wait().await;
    Ok(status)
}

fn forwarded_headers(headers: &[(String, String)], api_key: &str) -> HashMap<String, String> {
    let blocked = [
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
    ];
    let mut outgoing = HashMap::new();
    for (key, value) in headers {
        if !blocked.contains(&key.to_ascii_lowercase().as_str()) {
            outgoing.insert(key.clone(), value.clone());
        }
    }
    outgoing.insert("Authorization".to_string(), format!("Bearer {api_key}"));
    outgoing
}

fn parse_response_headers(raw: &str) -> (u16, String, Vec<(String, String)>) {
    let mut lines = raw.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.split_whitespace();
    let _ = parts.next();
    let status = parts
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(502);
    let reason = parts.collect::<Vec<_>>().join(" ");
    let blocked = [
        "content-encoding",
        "content-length",
        "transfer-encoding",
        "connection",
    ];
    let headers = lines
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            (!blocked.contains(&key.to_ascii_lowercase().as_str()))
                .then(|| (key.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    (status, reason, headers)
}

async fn write_response_head(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    headers: &[(String, String)],
) -> Result<()> {
    let reason = fallback_reason(status, reason);
    stream
        .write_all(format!("HTTP/1.1 {status} {reason}\r\n").as_bytes())
        .await?;
    for (key, value) in headers {
        stream
            .write_all(format!("{key}: {value}\r\n").as_bytes())
            .await?;
    }
    stream.write_all(b"Connection: close\r\n\r\n").await?;
    Ok(())
}

async fn write_json(stream: &mut TcpStream, status: u16, reason: &str, body: &str) -> Result<()> {
    stream
        .write_all(
            format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json; charset=utf-8\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{body}\n",
                body.len() + 1
            )
            .as_bytes(),
        )
        .await?;
    Ok(())
}

async fn read_upstream_head(
    stdout: &mut tokio::process::ChildStdout,
) -> Result<(u16, String, Vec<(String, String)>, Vec<u8>)> {
    let mut buffer = Vec::new();
    loop {
        while let Some(header_end) = find_header_end(&buffer) {
            let header_text = String::from_utf8_lossy(&buffer[..header_end]);
            let (status, reason, headers) = parse_response_headers(&header_text);
            let remaining = buffer[header_end + 4..].to_vec();
            if (100..200).contains(&status) && status != 101 {
                buffer = remaining;
                if buffer.is_empty() {
                    break;
                }
                continue;
            }
            return Ok((status, reason, headers, remaining));
        }

        let mut chunk = [0u8; 4096];
        let n = stdout.read(&mut chunk).await?;
        if n == 0 {
            return Err(err("upstream response missing final headers"));
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
}

fn fallback_reason<'a>(status: u16, upstream_reason: &'a str) -> &'a str {
    if !upstream_reason.trim().is_empty() {
        return upstream_reason;
    }
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        426 => "Upgrade Required",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "OK",
    }
}

fn is_v1_path(incoming_url: &str) -> bool {
    incoming_url == "/v1" || incoming_url.starts_with("/v1/") || incoming_url.starts_with("/v1?")
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn filters_authorization_header() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer old".to_string()),
            ("content-type".to_string(), "application/json".to_string()),
        ];
        let forwarded = forwarded_headers(&headers, "new");
        assert_eq!(forwarded.get("Authorization").unwrap(), "Bearer new");
        assert_eq!(forwarded.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn parses_reason_phrase() {
        let (status, reason, headers) = parse_response_headers(
            "HTTP/1.1 504 Gateway Timeout\r\ncontent-type: application/json\r\n\r\n",
        );
        assert_eq!(status, 504);
        assert_eq!(reason, "Gateway Timeout");
        assert_eq!(headers[0].0, "content-type");
    }

    #[test]
    fn falls_back_to_known_reason() {
        assert_eq!(fallback_reason(426, ""), "Upgrade Required");
    }
}
