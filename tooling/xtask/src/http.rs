//! A minimal HTTP/1.1 client for gxserver's loopback control API (health and stop). Only `http://127.0.0.1:<port>` is ever addressed, so no TLS or redirects.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// gxserver's control-plane protocol version header.
const PROTOCOL_VERSION: &str = "1";

/// Why a request got no HTTP answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoAnswer {
    /// Nothing accepted the connection (refused or reset): no server serves the port.
    Refused,
    /// The connection was not accepted in time. Windows takes about 2 s to report a refused loopback connection, so a closed port there usually reads as this, as does a listener whose backlog is full.
    ConnectTimedOut,
    /// The connection was made but no answer came in time: something holds the port and stays silent.
    TimedOut,
    /// Any other failure (a malformed address, a garbled answer).
    Failed,
}

/// An HTTP answer: its status code and its JSON body, when the body is JSON.
pub struct Answer {
    pub status: u16,
    pub json: Option<serde_json::Value>,
}

/// Sends a request and returns the parsed JSON body of a 2xx response; None for any failure, as the old `fetch` wrapper did.
pub fn request_json(
    base_url: &str,
    method: &str,
    path: &str,
    token: &str,
    timeout: Duration,
) -> Option<serde_json::Value> {
    let answer = request(base_url, method, path, token, timeout).ok()?;
    (200..300).contains(&answer.status).then_some(answer.json?)
}

/// Sends a request and returns whatever the server answered, whatever its status, or why nothing answered.
pub fn request(
    base_url: &str,
    method: &str,
    path: &str,
    token: &str,
    timeout: Duration,
) -> Result<Answer, NoAnswer> {
    let deadline = Instant::now() + timeout;
    let host = base_url
        .strip_prefix("http://")
        .ok_or(NoAnswer::Failed)?
        .trim_end_matches('/');
    let address: SocketAddr = host.parse().map_err(|_| NoAnswer::Failed)?;
    let mut stream =
        TcpStream::connect_timeout(&address, timeout).map_err(|error| match no_answer(error) {
            NoAnswer::TimedOut => NoAnswer::ConnectTimedOut,
            other => other,
        })?;
    stream
        .set_write_timeout(Some(remaining(deadline)?))
        .map_err(no_answer)?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nx-gxserver-protocol-version: {PROTOCOL_VERSION}\r\nAccept: application/json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).map_err(no_answer)?;
    let mut response = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(no_answer)?;
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => response.extend_from_slice(&buffer[..read]),
            Err(error) => return Err(no_answer(error)),
        }
        if let Some(answer) = complete_answer(&response) {
            return Ok(answer);
        }
    }
    // The server closed the connection: whatever arrived is the whole answer.
    let (head, rest) = split_head(&response).ok_or(NoAnswer::Refused)?;
    Ok(Answer {
        status: status_code(&head).ok_or(NoAnswer::Failed)?,
        json: complete_answer(&response)
            .and_then(|answer| answer.json)
            .or_else(|| parse(rest.to_vec())),
    })
}

fn no_answer(error: std::io::Error) -> NoAnswer {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => NoAnswer::TimedOut,
        ErrorKind::ConnectionRefused
        | ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::BrokenPipe
        | ErrorKind::NotConnected => NoAnswer::Refused,
        _ => NoAnswer::Failed,
    }
}

fn remaining(deadline: Instant) -> Result<Duration, NoAnswer> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or(NoAnswer::TimedOut)
}

fn parse(body: Vec<u8>) -> Option<serde_json::Value> {
    serde_json::from_slice(&body).ok()
}

fn split_head(response: &[u8]) -> Option<(String, &[u8])> {
    let end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    Some((
        String::from_utf8_lossy(&response[..end]).into_owned(),
        &response[end + 4..],
    ))
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn status_code(head: &str) -> Option<u16> {
    head.lines().next()?.split_whitespace().nth(1)?.parse().ok()
}

/// The answer once its head and whole body (by content-length or chunked framing) have arrived; None while incomplete.
fn complete_answer(response: &[u8]) -> Option<Answer> {
    let (head, rest) = split_head(response)?;
    let status = status_code(&head)?;
    let body = if header(&head, "transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
    {
        decode_chunked(rest)?
    } else {
        let length: usize = header(&head, "content-length")?.parse().ok()?;
        if rest.len() < length {
            return None;
        }
        rest[..length].to_vec()
    };
    Some(Answer {
        status,
        json: parse(body),
    })
}

fn decode_chunked(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let line_end = data.windows(2).position(|window| window == b"\r\n")?;
        let size_text = String::from_utf8_lossy(&data[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Some(body);
        }
        if data.len() < size + 2 {
            return None;
        }
        body.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}
