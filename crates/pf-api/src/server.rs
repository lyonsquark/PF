//! The API's background thread (T12; R-API-1, R-API-9, R-API-10). One request at a time, like the
//! Swift app's main-queue handler. The engine lock is never held while the socket is written.

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::thread::JoinHandle;

use pf_engine::SharedEngine;
use tiny_http::{Request, StatusCode};

use crate::router::{ApiResponse, bad_request, handle};

/// R-API-10: request bodies up to this many bytes (10 MB) are read; a larger one is a 400.
pub const MAX_BODY_BYTES: usize = 10_485_760;

/// A running API server. Dropping it, or calling `stop`, unblocks the thread and joins it.
pub struct ApiHandle {
    /// The port actually bound (the OS choice when 0 was asked for).
    pub port: u16,
    server: Arc<tiny_http::Server>,
    thread: Option<JoinHandle<()>>,
}

impl ApiHandle {
    /// Stops the server and waits for its thread to finish.
    pub fn stop(mut self) {
        self.shutdown();
    }

    /// Ends the request loop and joins the thread. Safe to call more than once.
    fn shutdown(&mut self) {
        self.server.unblock();
        let joined = self.thread.take().map(JoinHandle::join);
        if let Some(Err(_)) = joined {
            log::warn!("API server thread panicked");
        }
    }
}

impl Drop for ApiHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// R-API-1: binds `127.0.0.1:{port}` only (port 0 asks the OS for a free port). A bind failure is
/// returned, and the caller decides what to log. `on_change` runs after the reply to any request
/// that changed data (R-API-9); the app passes its repaint request.
pub fn start(
    engine: SharedEngine,
    port: u16,
    on_change: Arc<dyn Fn() + Send + Sync>,
) -> io::Result<ApiHandle> {
    let server = tiny_http::Server::http(format!("127.0.0.1:{port}"))
        .map_err(|e| io::Error::other(e.to_string()))?;
    let bound = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| io::Error::other("API listener has no IP address"))?;
    let server = Arc::new(server);
    let worker = Arc::clone(&server);
    let thread = std::thread::Builder::new()
        .name("pf-api".to_string())
        .spawn(move || {
            for request in worker.incoming_requests() {
                serve(&engine, &*on_change, request);
            }
        })?;
    Ok(ApiHandle {
        port: bound.port(),
        server,
        thread: Some(thread),
    })
}

/// Answers one request: reads its body (R-API-10), routes it, writes the reply, and then repaints
/// when the request changed data.
fn serve(engine: &SharedEngine, on_change: &(dyn Fn() + Send + Sync), mut request: Request) {
    let method = request.method().as_str().to_string();
    let url = request.url().to_string();
    let body = match read_body(&mut request) {
        Ok(body) => body,
        Err(reply) => {
            respond_unread(request, &reply);
            return;
        }
    };
    let mut changed = false;
    let reply = handle(engine, &method, &url, &body, &mut changed);
    if let Err(e) = write_reply(request, &reply) {
        log::warn!("API reply not delivered: {e}");
    }
    if changed {
        on_change();
    }
}

/// Replies to a request whose body may be unread (over the limit, or not UTF-8). tiny_http's raw
/// writer waits for an unread body and never sends, so this goes through `Request::respond`,
/// which sends at once and closes the connection. Its headers come from `Response::add_header`,
/// which drops `Connection`, so this reply has no `Connection: close` line.
fn respond_unread(request: Request, reply: &ApiResponse) {
    let mut response = tiny_http::Response::from_data(reply.body.clone().into_bytes())
        .with_status_code(reply.status);
    let content_type = format!("{}; charset=utf-8", reply.content_type);
    for (name, value) in [
        ("Content-Type", content_type.as_str()),
        ("Access-Control-Allow-Origin", "*"),
    ] {
        if let Ok(header) = tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            response.add_header(header);
        }
    }
    if let Err(e) = request.respond(response) {
        log::warn!("API reply not delivered: {e}");
    }
}

/// Reads the body as UTF-8. A `Content-Length` over the limit is refused before reading, and a
/// chunked body is cut off at the limit (R-API-10).
fn read_body(request: &mut Request) -> Result<String, ApiResponse> {
    if request.body_length().is_some_and(|n| n > MAX_BODY_BYTES) {
        return Err(bad_request());
    }
    let mut bytes = Vec::new();
    let reader = request.as_reader();
    reader
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| bad_request())?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(bad_request());
    }
    String::from_utf8(bytes).map_err(|_| bad_request())
}

/// Writes the reply as raw HTTP/1.1 (R-API-3). The bytes are written by hand because tiny_http
/// drops a `Connection` header set through its `Response` API, and the reply must carry
/// `Connection: close`.
fn write_reply(request: Request, reply: &ApiResponse) -> io::Result<()> {
    let reason = StatusCode(reply.status).default_reason_phrase();
    let head = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    let mut writer = request.into_writer();
    writer.write_all(head.as_bytes())?;
    writer.write_all(reply.body.as_bytes())?;
    writer.flush()
}
