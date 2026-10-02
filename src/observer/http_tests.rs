use super::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[test]
fn fragmented_headers_wait_for_the_remaining_bytes() {
    let dashboard = Dashboard::start(0).unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", dashboard.port())).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    stream
        .write_all(
            format!(
                "GET /api/snapshot HTTP/1.1\r\nHost: localhost:{}\r\n",
                dashboard.port()
            )
            .as_bytes(),
        )
        .unwrap();
    let error = stream.peek(&mut [0_u8; 1]).unwrap_err();
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
    stream.write_all(b"\r\n").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
}

#[test]
fn incomplete_request_headers_time_out_without_blocking_shutdown() {
    let dashboard = Dashboard::start(0).unwrap();
    let port = dashboard.port();
    let response = request(
        port,
        &format!("GET /api/snapshot HTTP/1.1\r\nHost: localhost:{port}\r\n"),
    );
    assert!(response.starts_with("HTTP/1.1 408"));
    let mut idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
    idle.write_all(b"GET / HTTP/1.1\r\n").unwrap();
    let start = std::time::Instant::now();
    drop(dashboard);
    assert!(start.elapsed() < Duration::from_secs(5));
}

fn request(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn get(dashboard: &Dashboard, path: &str) -> String {
    request(
        dashboard.port(),
        &format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            dashboard.port()
        ),
    )
}

#[test]
fn snapshot_and_export_only_expose_metrics() {
    let dashboard = Dashboard::start(0).unwrap();
    let observer = dashboard.observer();
    observer.finish(
        observer.begin("sk-secret-/Users/private/.env", 42),
        super::super::Outcome::ToolError,
        24,
    );
    let response = get(&dashboard, "/api/snapshot");
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(
        response
            .to_ascii_lowercase()
            .contains("content-type: application/json")
    );
    assert!(
        response
            .to_ascii_lowercase()
            .contains("cache-control: no-store")
    );
    assert!(response.contains("unknown_tool"));
    assert!(!response.contains("sk-secret"));
    assert!(!response.contains("/Users"));
    assert!(
        get(&dashboard, "/api/export")
            .to_ascii_lowercase()
            .contains("content-disposition: attachment")
    );
}

#[test]
fn host_origin_and_mutating_methods_are_rejected() {
    let dashboard = Dashboard::start(0).unwrap();
    let port = dashboard.port();
    for header in [
        "Host: evil.example".to_owned(),
        format!("Host: localhost:{port}\r\nOrigin: https://evil.example"),
    ] {
        let response = request(
            port,
            &format!("GET /api/snapshot HTTP/1.1\r\n{header}\r\nConnection: close\r\n\r\n"),
        );
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    }
    let response = request(
        port,
        &format!(
            "POST /api/snapshot HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(response.starts_with("HTTP/1.1 405"));
    let response = request(
        port,
        &format!(
            "GET /api/snapshot HTTP/1.1\r\nHost: localhost:{port}\r\nOrigin: http://localhost:{port}\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(response.starts_with("HTTP/1.1 200"));
    for extra in [
        format!("Host: localhost:{port}"),
        format!("Origin: http://localhost:{port}\r\nOrigin: http://localhost:{port}"),
    ] {
        let response = request(
            port,
            &format!(
                "GET /api/snapshot HTTP/1.1\r\nHost: localhost:{port}\r\n{extra}\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(response.starts_with("HTTP/1.1 403"));
    }
}

#[test]
fn fixed_assets_have_security_headers_and_unknown_paths_are_not_served() {
    let dashboard = Dashboard::start(0).unwrap();
    for (path, content_type) in [
        ("/", "text/html"),
        ("/styles.css", "text/css"),
        ("/app.js", "text/javascript"),
        ("/charts.js", "text/javascript"),
    ] {
        let response = get(&dashboard, path).to_ascii_lowercase();
        assert!(response.starts_with("http/1.1 200"));
        assert!(response.contains(&format!("content-type: {content_type}")));
        assert!(response.contains("content-security-policy:"));
        assert!(response.contains("x-content-type-options: nosniff"));
        assert!(!response.contains("access-control-allow-origin"));
    }
    assert!(get(&dashboard, "/../../.env").starts_with("HTTP/1.1 404"));
}

#[test]
fn head_has_no_body_and_drop_releases_the_port() {
    let dashboard = Dashboard::start(0).unwrap();
    let port = dashboard.port();
    assert!(Dashboard::start(port).is_err());
    let response = request(
        port,
        &format!(
            "HEAD /api/snapshot HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
        ),
    );
    assert!(response.starts_with("HTTP/1.1 200"));
    assert_eq!(response.split_once("\r\n\r\n").unwrap().1, "");
    drop(dashboard);
    let replacement = Dashboard::start(port).unwrap();
    assert_eq!(replacement.port(), port);
}

#[test]
fn oversized_headers_and_request_bodies_are_rejected() {
    let dashboard = Dashboard::start(0).unwrap();
    let port = dashboard.port();
    let oversized = format!(
        "GET / HTTP/1.1\r\nHost: localhost:{port}\r\nX-Large: {}",
        "x".repeat(8_192)
    );
    assert!(request(port, &oversized).starts_with("HTTP/1.1 431"));
    for body_header in ["Content-Length: 1", "Transfer-Encoding: chunked"] {
        let response = request(
            port,
            &format!(
                "GET /api/snapshot HTTP/1.1\r\nHost: localhost:{port}\r\n{body_header}\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(response.starts_with("HTTP/1.1 400"));
    }
}
