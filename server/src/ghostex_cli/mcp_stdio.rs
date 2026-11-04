//! The stdio framing shared by Ghostex's MCP servers (`ghostex connectors mcp`, `ghostex browser
//! mcp`).
//!
//! CDXC:Cli 2026-10-27 WHY:
//! Claude Code and Codex speak newline-delimited JSON-RPC over stdio while other clients use LSP-style `Content-Length` frames. A server that reads only one of them never answers the other kind of client (`ghostex browser mcp` read only `Content-Length` and was silent for Claude and Codex). Both servers share this reader and writer: it learns the framing from each incoming message and answers in the same one.

use std::io::Write;
use std::sync::{Mutex, PoisonError};

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Framing {
    ContentLength,
    Lines,
}

/// The framing the client used last (lines until it says otherwise), shared by the reader and
/// every thread that writes a reply.
pub(super) struct McpStdio {
    framing: Mutex<Framing>,
}

impl McpStdio {
    pub(super) fn new() -> Self {
        McpStdio {
            framing: Mutex::new(Framing::Lines),
        }
    }

    fn set(&self, framing: Framing) {
        *self.framing.lock().unwrap_or_else(PoisonError::into_inner) = framing;
    }

    pub(super) fn current(&self) -> Framing {
        *self.framing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes one message to stdout in the client's framing.
    pub(super) fn send(&self, message: &Value) {
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        let _ = self.write_to(&mut stdout, message);
        let _ = stdout.flush();
    }

    pub(super) fn write_to(&self, out: &mut impl Write, message: &Value) -> std::io::Result<()> {
        let body = serde_json::to_string(message).unwrap_or_else(|_| "null".to_string());
        match self.current() {
            Framing::ContentLength => {
                write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
                out.write_all(body.as_bytes())
            }
            Framing::Lines => {
                out.write_all(body.as_bytes())?;
                out.write_all(b"\n")
            }
        }
    }

    /// Pulls every complete message out of `buffer`, learning the framing from each one. A
    /// partial line or a partial frame stays in `buffer` until the rest arrives.
    pub(super) fn take_messages(&self, buffer: &mut Vec<u8>) -> Vec<Value> {
        let mut messages = Vec::new();
        loop {
            let start = buffer
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .unwrap_or(buffer.len());
            if start > 0 {
                buffer.drain(..start);
            }
            if buffer.is_empty() {
                return messages;
            }
            // A line-framed message is an object or a batch (an array); anything else is a header.
            if buffer[0] == b'{' || buffer[0] == b'[' {
                let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
                    return messages;
                };
                let line: Vec<u8> = buffer.drain(..=end).collect();
                self.set(Framing::Lines);
                if let Ok(frame) = serde_json::from_slice::<Value>(&line) {
                    push_frame(&mut messages, frame);
                }
                continue;
            }
            let Some(header_end) = find(buffer, b"\r\n\r\n") else {
                return messages;
            };
            let header = String::from_utf8_lossy(&buffer[..header_end]).to_string();
            let Some(length) = content_length(&header) else {
                buffer.drain(..header_end + 4);
                continue;
            };
            let body_start = header_end + 4;
            if buffer.len() < body_start + length {
                return messages;
            }
            let body: Vec<u8> = buffer[body_start..body_start + length].to_vec();
            buffer.drain(..body_start + length);
            self.set(Framing::ContentLength);
            if let Ok(frame) = serde_json::from_slice::<Value>(&body) {
                push_frame(&mut messages, frame);
            }
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(header: &str) -> Option<usize> {
    header.split("\r\n").find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })
}

/// A parsed frame's messages: a JSON-RPC batch (an array) is its members, each handled on its own.
fn push_frame(messages: &mut Vec<Value>, frame: Value) {
    match frame {
        Value::Array(batch) => messages.extend(batch),
        message => messages.push(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BODY: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;

    fn written(stdio: &McpStdio, message: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        stdio.write_to(&mut out, message).unwrap();
        out
    }

    #[test]
    fn newline_requests_get_newline_replies() {
        let stdio = McpStdio::new();
        let mut buffer = format!("{BODY}\n").into_bytes();
        let messages = stdio.take_messages(&mut buffer);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].get("id"), Some(&json!(1)));
        assert!(buffer.is_empty());
        assert_eq!(stdio.current(), Framing::Lines);
        let reply = written(&stdio, &json!({"id": 1}));
        assert_eq!(reply, b"{\"id\":1}\n");
    }

    #[test]
    fn content_length_requests_get_content_length_replies() {
        let stdio = McpStdio::new();
        let mut buffer = format!("Content-Length: {}\r\n\r\n{}", BODY.len(), BODY).into_bytes();
        let messages = stdio.take_messages(&mut buffer);
        assert_eq!(messages.len(), 1);
        assert!(buffer.is_empty());
        assert_eq!(stdio.current(), Framing::ContentLength);
        let reply = written(&stdio, &json!({"id": 1}));
        assert_eq!(reply, b"Content-Length: 8\r\n\r\n{\"id\":1}");
    }

    #[test]
    fn a_partial_line_waits_for_the_rest() {
        let stdio = McpStdio::new();
        let mut buffer = BODY[..20].as_bytes().to_vec();
        assert!(stdio.take_messages(&mut buffer).is_empty());
        assert_eq!(buffer.len(), 20);
        buffer.extend_from_slice(BODY[20..].as_bytes());
        // Still no newline: the line is not complete yet.
        assert!(stdio.take_messages(&mut buffer).is_empty());
        buffer.push(b'\n');
        assert_eq!(stdio.take_messages(&mut buffer).len(), 1);
        assert!(buffer.is_empty());
    }

    #[test]
    fn a_partial_frame_waits_for_the_rest() {
        let stdio = McpStdio::new();
        let mut buffer = b"Content-Len".to_vec();
        assert!(stdio.take_messages(&mut buffer).is_empty());
        buffer.extend_from_slice(format!("gth: {}\r\n\r\n{}", BODY.len(), &BODY[..10]).as_bytes());
        assert!(stdio.take_messages(&mut buffer).is_empty());
        buffer.extend_from_slice(BODY[10..].as_bytes());
        assert_eq!(stdio.take_messages(&mut buffer).len(), 1);
        assert!(buffer.is_empty());
    }

    #[test]
    fn two_lines_and_a_batch_in_one_chunk() {
        let stdio = McpStdio::new();
        let mut buffer = format!("{BODY}\r\n{BODY}\n[{BODY},{BODY}]\n").into_bytes();
        assert_eq!(stdio.take_messages(&mut buffer).len(), 4);
        assert!(buffer.is_empty());
    }

    #[test]
    fn headers_are_case_insensitive_and_frames_can_follow_each_other() {
        let stdio = McpStdio::new();
        let mut buffer = format!(
            "X-Other: 1\r\ncontent-length: {n}\r\n\r\n{BODY}CONTENT-LENGTH: {n}\r\n\r\n{BODY}",
            n = BODY.len()
        )
        .into_bytes();
        assert_eq!(stdio.take_messages(&mut buffer).len(), 2);
        assert!(buffer.is_empty());
    }

    #[test]
    fn the_reply_follows_the_framing_of_the_latest_request() {
        let stdio = McpStdio::new();
        let mut buffer = format!("Content-Length: {}\r\n\r\n{}", BODY.len(), BODY).into_bytes();
        stdio.take_messages(&mut buffer);
        let mut buffer = format!("{BODY}\n").into_bytes();
        stdio.take_messages(&mut buffer);
        assert_eq!(stdio.current(), Framing::Lines);
    }
}
