use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const HEADER_LIMIT: usize = 8_192;
const IO_DEADLINE: Duration = Duration::from_secs(2);

pub(super) struct RequestHead {
    pub path: String,
    pub head: bool,
}

pub(super) struct HttpError {
    pub status: u16,
    pub head: bool,
}

pub(super) fn read_head(stream: &mut TcpStream, port: u16) -> Result<RequestHead, HttpError> {
    let deadline = Instant::now() + IO_DEADLINE;
    let mut buffer = [0_u8; HEADER_LIMIT];
    let mut length = 0;
    loop {
        let head = buffer[..length].starts_with(b"HEAD ");
        match parse(&buffer[..length], port) {
            Ok(Some(request)) => return Ok(request),
            Err(status) => return Err(HttpError { status, head }),
            _ => {}
        }
        if length == HEADER_LIMIT {
            return Err(HttpError { status: 431, head });
        }
        let count =
            read_before(stream, &mut buffer[length..], deadline).map_err(|error| HttpError {
                status: if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) {
                    408
                } else {
                    400
                },
                head,
            })?;
        if count == 0 {
            return Err(HttpError { status: 400, head });
        }
        length += count;
    }
}

fn parse(buffer: &[u8], port: u16) -> Result<Option<RequestHead>, u16> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    match request.parse(buffer) {
        Ok(httparse::Status::Partial) => return Ok(None),
        Err(httparse::Error::TooManyHeaders) => return Err(431),
        Err(_) => return Err(400),
        Ok(httparse::Status::Complete(_)) => {}
    }
    validate_origin(request.headers, port)?;
    let head = match request.method {
        Some("GET") => false,
        Some("HEAD") => true,
        _ => return Err(405),
    };
    validate_body(request.headers)?;
    let path = request.path.ok_or(400_u16)?.to_owned();
    Ok(Some(RequestHead { path, head }))
}

fn validate_origin(headers: &[httparse::Header<'_>], port: u16) -> Result<(), u16> {
    let hosts = header_values(headers, "Host");
    if hosts.len() != 1 {
        return Err(403);
    }
    let host = validate_host(hosts[0], port)?;
    let origins = header_values(headers, "Origin");
    if origins.len() > 1 {
        return Err(403);
    }
    if let Some(origin) = origins.first() {
        let origin = std::str::from_utf8(origin).map_err(|_| 403_u16)?;
        if omit_default_port(origin, port) != format!("http://{host}") {
            return Err(403);
        }
    }
    Ok(())
}

fn validate_host(value: &[u8], port: u16) -> Result<&str, u16> {
    let host = omit_default_port(std::str::from_utf8(value).map_err(|_| 403_u16)?, port);
    let allowed = if port == 80 {
        matches!(host, "127.0.0.1" | "localhost")
    } else {
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    };
    if allowed { Ok(host) } else { Err(403) }
}

fn omit_default_port(value: &str, port: u16) -> &str {
    if port == 80 {
        value.strip_suffix(":80").unwrap_or(value)
    } else {
        value
    }
}

fn validate_body(headers: &[httparse::Header<'_>]) -> Result<(), u16> {
    if !header_values(headers, "Transfer-Encoding").is_empty() {
        return Err(400);
    }
    let lengths = header_values(headers, "Content-Length");
    if lengths.len() > 1 {
        return Err(400);
    }
    if let Some(length) = lengths.first()
        && (length.is_empty() || !length.iter().all(|byte| *byte == b'0'))
    {
        return Err(400);
    }
    Ok(())
}

fn header_values<'a>(headers: &'a [httparse::Header<'a>], name: &str) -> Vec<&'a [u8]> {
    headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value)
        .collect()
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "Dashboard request deadline exceeded",
            )
        })
}

fn read_before(stream: &mut TcpStream, buffer: &mut [u8], deadline: Instant) -> io::Result<usize> {
    loop {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

pub(super) fn write_response(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(&str, &str)],
    body: &[u8],
    head: bool,
) -> io::Result<()> {
    let mut response = format!(
        "HTTP/1.1 {status} {}\r\nConnection: close\r\nContent-Length: {}\r\n",
        status_text(status),
        body.len()
    );
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    let deadline = Instant::now() + IO_DEADLINE;
    write_before(stream, response.as_bytes(), deadline)?;
    if !head {
        write_before(stream, body, deadline)?;
    }
    Ok(())
}

fn write_before(stream: &mut TcpStream, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
    while !bytes.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Dashboard connection closed",
                ));
            }
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub(super) fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_80_accepts_browser_normalized_host_and_origin() {
        for host in ["127.0.0.1", "localhost", "127.0.0.1:80", "localhost:80"] {
            let hostname = host.strip_suffix(":80").unwrap_or(host);
            for origin in [
                format!("http://{hostname}"),
                format!("http://{hostname}:80"),
            ] {
                let request = format!(
                    "GET /api/snapshot HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\n\r\n"
                );
                assert!(
                    matches!(parse(request.as_bytes(), 80), Ok(Some(_))),
                    "{host}, {origin}"
                );
            }
        }
    }

    #[test]
    fn default_port_normalization_does_not_allow_other_ports_or_hosts() {
        for (port, host, origin) in [
            (81, "localhost", "http://localhost"),
            (80, "localhost:81", "http://localhost:81"),
            (80, "localhost", "http://localhost:81"),
            (80, "localhost", "http://127.0.0.1"),
        ] {
            let request =
                format!("GET /api/snapshot HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\n\r\n");
            assert!(matches!(parse(request.as_bytes(), port), Err(403)));
        }
    }
}
