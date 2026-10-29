//! Minimal raw HTTP/1.1 client for the API tests. Talks to `127.0.0.1` only, with `Connection:
//! close`, so every reply is read to EOF. Each test binary uses a subset of these helpers.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// One parsed HTTP reply.
pub struct Raw {
    /// Status code from the status line.
    pub status: u16,
    /// Header (name, value) pairs in wire order.
    pub headers: Vec<(String, String)>,
    /// Body text (UTF-8).
    pub body: String,
}

impl Raw {
    /// The first header named `name`, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The `Content-Type` header, or "" when absent.
    pub fn content_type(&self) -> &str {
        self.header("content-type").unwrap_or("")
    }
}

/// Writes `bytes` to the API on `port` and reads the whole reply.
pub fn send_bytes(port: u16, bytes: &[u8]) -> Raw {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the API");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    stream.write_all(bytes).expect("write request");
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).expect("read reply to EOF");
    parse(&reply)
}

/// Writes a raw request text and reads the whole reply.
pub fn send_raw(port: u16, request: &str) -> Raw {
    send_bytes(port, request.as_bytes())
}

/// `GET path`, returning (status, content type, body).
pub fn get(port: u16, path: &str) -> (u16, String, String) {
    let raw = send_raw(
        port,
        &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"),
    );
    (raw.status, raw.content_type().to_string(), raw.body)
}

/// `POST path` with `body`, returning (status, content type, body).
pub fn post(port: u16, path: &str, body: &str) -> (u16, String, String) {
    let raw = send_raw(
        port,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    );
    (raw.status, raw.content_type().to_string(), raw.body)
}

/// `DELETE path`, returning (status, content type, body).
pub fn delete(port: u16, path: &str) -> (u16, String, String) {
    let raw = send_raw(
        port,
        &format!("DELETE {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"),
    );
    (raw.status, raw.content_type().to_string(), raw.body)
}

/// Splits a reply into status, headers and body at the first blank line.
fn parse(reply: &[u8]) -> Raw {
    let split = reply
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("reply has a header/body separator");
    let head = std::str::from_utf8(&reply[..split]).expect("headers are UTF-8");
    let mut lines = head.split("\r\n");
    let status_line = lines.next().expect("status line");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("numeric status code");
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let body = String::from_utf8(reply[split + 4..].to_vec()).expect("body is UTF-8");
    Raw {
        status,
        headers,
        body,
    }
}
