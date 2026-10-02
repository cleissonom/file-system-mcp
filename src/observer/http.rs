use super::Observer;
use std::io;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[path = "http_request.rs"]
mod http_request;

use http_request::{RequestHead, read_head, write_response};

const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; font-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";

pub struct Dashboard {
    port: u16,
    observer: Arc<Observer>,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Dashboard {
    #[cfg(test)]
    pub fn start(port: u16) -> io::Result<Self> {
        Self::start_with_observer(port, Observer::new())
    }

    pub fn start_persistent(port: u16, path: &Path) -> io::Result<Self> {
        Self::start_with_observer(port, Observer::persistent(path)?)
    }

    fn start_with_observer(port: u16, observer: Observer) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let observer = Arc::new(observer);
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_observer = Arc::clone(&observer);
        let worker_stopping = Arc::clone(&stopping);
        let worker = thread::Builder::new()
            .name("mcp-observer".to_owned())
            .spawn(move || {
                serve(listener, port, &worker_observer, &worker_stopping);
            })?;
        Ok(Self {
            port,
            observer,
            stopping,
            worker: Some(worker),
        })
    }

    pub fn observer(&self) -> Arc<Observer> {
        Arc::clone(&self.observer)
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port())
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

fn serve(listener: TcpListener, port: u16, observer: &Observer, stopping: &AtomicBool) {
    while !stopping.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => respond(&mut stream, port, observer),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::park_timeout(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }
}

fn respond(stream: &mut TcpStream, port: u16, observer: &Observer) {
    // macOS can inherit the listener's nonblocking flag on accepted sockets.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let (reply, head) = match read_head(stream, port) {
        Ok(request) => (route(&request, observer), request.head),
        Err(error) => (Reply::error(error.status), error.head),
    };
    let headers = reply.headers();
    let _ = write_response(stream, reply.status, &headers, &reply.body, head);
    let _ = stream.shutdown(Shutdown::Both);
}

struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    export: bool,
}

impl Reply {
    fn ok(content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            content_type,
            body: body.into(),
            export: false,
        }
    }

    fn error(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: http_request::status_text(status).as_bytes().to_vec(),
            export: false,
        }
    }

    fn headers(&self) -> Vec<(&'static str, &'static str)> {
        let mut headers = vec![
            ("Content-Type", self.content_type),
            ("Cache-Control", "no-store"),
            ("Content-Security-Policy", CSP),
            ("X-Content-Type-Options", "nosniff"),
            ("Cross-Origin-Resource-Policy", "same-origin"),
            ("X-Frame-Options", "DENY"),
            ("Referrer-Policy", "no-referrer"),
        ];
        if self.export {
            headers.push((
                "Content-Disposition",
                "attachment; filename=\"mcp-observer-session.json\"",
            ));
        }
        if self.status == 405 {
            headers.push(("Allow", "GET, HEAD"));
        }
        headers
    }
}

fn route(request: &RequestHead, observer: &Observer) -> Reply {
    match request.path.as_str() {
        "/" => Reply::ok(
            "text/html; charset=utf-8",
            include_bytes!("../../observer-ui/index.html").as_slice(),
        ),
        "/styles.css" => Reply::ok(
            "text/css; charset=utf-8",
            include_bytes!("../../observer-ui/styles.css").as_slice(),
        ),
        "/app.js" => Reply::ok(
            "text/javascript; charset=utf-8",
            include_bytes!("../../observer-ui/app.js").as_slice(),
        ),
        "/charts.js" => Reply::ok(
            "text/javascript; charset=utf-8",
            include_bytes!("../../observer-ui/charts.js").as_slice(),
        ),
        "/api/snapshot" | "/api/export" => {
            let mut reply = Reply::ok(
                "application/json; charset=utf-8",
                observer.snapshot().to_string().into_bytes(),
            );
            reply.export = request.path == "/api/export";
            reply
        }
        _ => Reply::error(404),
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod http_tests;
