//! wmx's one-shot session requests answered inside gxserver, over the session host's own loopback
//! connection, instead of by starting `wmx.exe` for each one.
//!
//! CDXC:PlatformSupport 2026-11-04 WHY:
//! gxserver read every native session's screen (`wmx history … --scrollback 512 [--vt]`) and grid by starting `wmx.exe`, and each start also brought up a `conhost.exe` and a Defender scan of the image: about 1.4 starts a second with eight sessions open (measured 2026-11-03 on a Ryzen 9 5900X, half of all Ghostex process starts). The request those commands make is a single authenticated JSON line to the session host's loopback port, read from the host's registry record, so gxserver makes it itself, the way it reads a zmx daemon's Unix socket directly on macOS and Linux (`screen_capture.rs`). The answers and exit codes are the ones the wmx command would give, so every caller keeps its contract; verbs that need more than one request (`send`, `kill`, `list`, `start`) still run wmx.
//! SEE-ALSO: .dependencies/wmx/src/protocol.rs (`Endpoint`, `path`, frames), .dependencies/wmx/src/client.rs (`request`), .dependencies/wmx/src/cli.rs (each verb's output).

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::Duration,
};

use super::types::{
    ZmxCommandOptions, ZmxCommandResult, GXSERVER_ZMX_COMMAND_STDOUT_LIMIT_BYTES,
    ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS,
};

/// `protocol::MAX_FRAME` in wmx.
const MAX_FRAME: usize = 2 * 1024 * 1024;
/// `client::connect` in wmx.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The fields of wmx's registry record (`protocol::Endpoint`) a request needs.
#[derive(Deserialize)]
struct Endpoint {
    protocol: u32,
    name: String,
    port: u16,
    token: String,
}

enum Failure {
    /// What wmx would print to stderr before exiting 1.
    Message(String),
    /// The host did not answer within the command's time box.
    TimedOut,
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        if matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ) {
            Failure::TimedOut
        } else {
            Failure::Message(error.to_string())
        }
    }
}

/// The result `wmx <args>` would give, for the verbs that are one request to the session host; `None` for every other command, which still runs wmx.
pub(super) fn answer(
    args: &[String],
    session_directory: &str,
    options: &ZmxCommandOptions,
) -> Option<ZmxCommandResult> {
    let verb = args.first()?.as_str();
    let name = args.get(1)?.as_str();
    let timeout = Duration::from_millis(
        options
            .timeout_ms
            .unwrap_or(ZMX_LIFECYCLE_COMMAND_TIMEOUT_MS),
    );
    let directory = Path::new(session_directory);
    let ask = |operation: &str, data: Value| request(directory, name, operation, data, timeout);
    let outcome: Result<Vec<u8>, Failure> = match verb {
        "history" | "hi" => {
            let vt = args.iter().any(|arg| arg == "--vt");
            let scrollback = match args
                .iter()
                .position(|arg| arg == "--scrollback")
                .and_then(|index| args.get(index + 1))
            {
                Some(value) => value.parse::<u32>().ok()?,
                None => 10_000,
            };
            ask("history", json!({"vt": vt, "scrollback": scrollback})).and_then(|reply| {
                if vt {
                    let output = reply["output"]
                        .as_str()
                        .ok_or_else(|| Failure::Message("Missing terminal snapshot".into()))?;
                    STANDARD
                        .decode(output)
                        .map_err(|error| Failure::Message(error.to_string()))
                } else {
                    reply["text"]
                        .as_str()
                        .map(|text| text.as_bytes().to_vec())
                        .ok_or_else(|| Failure::Message("Missing terminal text".into()))
                }
            })
        }
        "grid" => ask("grid", Value::Null).map(|reply| format!("{reply}\n").into_bytes()),
        "exists" => ask("ping", Value::Null).map(|_| Vec::new()),
        "refresh" => ask("refresh", Value::Null).map(|_| Vec::new()),
        "resize" => {
            let rows = args.get(2)?.parse::<u16>().ok()?;
            let cols = args.get(3)?.parse::<u16>().ok()?;
            ask("resize", json!({"rows": rows, "cols": cols})).map(|_| Vec::new())
        }
        "refresh-if-stale" => {
            let rows = args.get(2)?.parse::<u16>().ok()?;
            let cols = args.get(3)?.parse::<u16>().ok()?;
            ask("refresh-if-stale", json!({"rows": rows, "cols": cols})).and_then(|reply| {
                let applied = reply["applied"]
                    .as_bool()
                    .ok_or_else(|| Failure::Message("Missing refresh acknowledgement".into()))?;
                Ok(format!(
                    "refresh-if-stale {}\n",
                    if applied { "applied" } else { "skipped" }
                )
                .into_bytes())
            })
        }
        "prompt-editor-capability" => ask("prompt-editor-capability", Value::Null).map(|reply| {
            format!("{}\n", reply["capability"].as_str().unwrap_or("editor")).into_bytes()
        }),
        _ => return None,
    };
    Some(command_result(outcome, options, timeout))
}

/// Shapes an answer the way `run_zsh_script_blocking` shapes a finished wmx command: trimmed output, exit 124 on a timeout, and the head of an oversized stdout with exit 125.
fn command_result(
    outcome: Result<Vec<u8>, Failure>,
    options: &ZmxCommandOptions,
    timeout: Duration,
) -> ZmxCommandResult {
    match outcome {
        Ok(mut stdout) => {
            let limit = options
                .stdout_limit_bytes
                .unwrap_or(GXSERVER_ZMX_COMMAND_STDOUT_LIMIT_BYTES);
            let truncated = stdout.len() > limit;
            stdout.truncate(limit);
            ZmxCommandResult {
                exit_code: if truncated { 125 } else { 0 },
                stderr: if truncated {
                    format!("zmx command stdout exceeded {limit} bytes")
                } else {
                    String::new()
                },
                stdout: String::from_utf8_lossy(&stdout).trim().to_string(),
                stdout_truncated: truncated,
            }
        }
        Err(Failure::Message(message)) => ZmxCommandResult {
            exit_code: 1,
            stderr: message.trim().to_string(),
            stdout: String::new(),
            stdout_truncated: false,
        },
        Err(Failure::TimedOut) => ZmxCommandResult {
            exit_code: 124,
            stderr: format!(
                "zmx lifecycle command timed out after {}ms",
                timeout.as_millis()
            ),
            stdout: String::new(),
            stdout_truncated: false,
        },
    }
}

/// `client::request` in wmx: one frame out, one frame back, an `error` field as the failure.
fn request(
    directory: &Path,
    name: &str,
    operation: &str,
    data: Value,
    timeout: Duration,
) -> Result<Value, Failure> {
    let endpoint = endpoint(directory, name)?;
    let address = SocketAddr::from(([127, 0, 0, 1], endpoint.port));
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT.min(timeout))?;
    stream.set_read_timeout(Some(READ_TIMEOUT.min(timeout)))?;
    stream.set_write_timeout(Some(READ_TIMEOUT.min(timeout)))?;
    stream.set_nodelay(true)?;
    let mut frame = serde_json::to_vec(&json!({
        "token": endpoint.token,
        "operation": operation,
        "data": data,
    }))
    .map_err(|error| Failure::Message(error.to_string()))?;
    frame.push(b'\n');
    (&stream).write_all(&frame)?;
    (&stream).flush()?;
    let reply = read_frame(&mut BufReader::new(&stream))?;
    if let Some(message) = reply.get("error").and_then(Value::as_str) {
        return Err(Failure::Message(message.to_string()));
    }
    Ok(reply)
}

/// `protocol::endpoint` in wmx: the record is `<directory>\<sha256(name)>.json`.
fn endpoint(directory: &Path, name: &str) -> Result<Endpoint, Failure> {
    let key = hex::encode(Sha256::digest(name.as_bytes()));
    let bytes = std::fs::read(directory.join(format!("{key}.json")))
        .map_err(|error| Failure::Message(error.to_string()))?;
    let endpoint: Endpoint =
        serde_json::from_slice(&bytes).map_err(|error| Failure::Message(error.to_string()))?;
    if endpoint.protocol != 1 {
        return Err(Failure::Message(
            "This native session uses an unsupported protocol version".into(),
        ));
    }
    if endpoint.name != name {
        return Err(Failure::Message(
            "Session endpoint identity does not match".into(),
        ));
    }
    Ok(endpoint)
}

/// `protocol::read_frame` in wmx: one newline-terminated JSON value of at most `MAX_FRAME` bytes.
fn read_frame(reader: &mut impl BufRead) -> Result<Value, Failure> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Err(Failure::Message("Session connection closed".into()));
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(available.len());
        if bytes.len() + count > MAX_FRAME {
            return Err(Failure::Message(
                "Session message exceeds its size limit".into(),
            ));
        }
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if bytes.last() == Some(&b'\n') {
            break;
        }
    }
    serde_json::from_slice(&bytes).map_err(|_| Failure::Message("Invalid session message".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A stand-in session host: writes the registry record, answers one request with `reply`, and returns what it was asked.
    fn host(directory: &Path, name: &str, reply: Value) -> std::thread::JoinHandle<Value> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let key = hex::encode(Sha256::digest(name.as_bytes()));
        std::fs::write(
            directory.join(format!("{key}.json")),
            json!({"protocol": 1, "name": name, "port": port, "token": "secret", "pid": 1, "shell_pid": 2}).to_string(),
        )
        .unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let request = read_frame(&mut BufReader::new(&stream)).ok().unwrap();
            (&stream)
                .write_all(format!("{reply}\n").as_bytes())
                .unwrap();
            request
        })
    }

    fn args(text: &str) -> Vec<String> {
        text.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn history_asks_the_host_and_prints_its_text() {
        let directory = tempfile::tempdir().unwrap();
        let asked = host(
            directory.path(),
            "S1-P1-G1",
            json!({"text": "  line one\nline two\n"}),
        );
        let result = answer(
            &args("history S1-P1-G1 --scrollback 512"),
            &directory.path().to_string_lossy(),
            &ZmxCommandOptions::default(),
        )
        .unwrap();
        let request = asked.join().unwrap();
        assert_eq!(request["token"], "secret");
        assert_eq!(request["operation"], "history");
        assert_eq!(request["data"], json!({"vt": false, "scrollback": 512}));
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "line one\nline two");
    }

    #[test]
    fn vt_history_decodes_the_snapshot_and_keeps_the_head_at_the_limit() {
        let directory = tempfile::tempdir().unwrap();
        let output = STANDARD.encode(b"\x1b[1mbold\x1b[0m and more");
        let _asked = host(directory.path(), "S1-P1-G2", json!({"output": output}));
        let result = answer(
            &args("history S1-P1-G2 --scrollback 512 --vt"),
            &directory.path().to_string_lossy(),
            &ZmxCommandOptions {
                allow_stdout_truncation: true,
                stdout_limit_bytes: Some(12),
                ..ZmxCommandOptions::default()
            },
        )
        .unwrap();
        assert_eq!(result.exit_code, 125);
        assert!(result.stdout_truncated);
        assert_eq!(result.stdout, "\x1b[1mbold\x1b[0m");
    }

    #[test]
    fn a_host_error_and_a_missing_session_exit_one() {
        let directory = tempfile::tempdir().unwrap();
        let _asked = host(
            directory.path(),
            "S1-P1-G3",
            json!({"error": "Unknown session operation"}),
        );
        let failed = answer(
            &args("grid S1-P1-G3"),
            &directory.path().to_string_lossy(),
            &ZmxCommandOptions::default(),
        )
        .unwrap();
        assert_eq!(failed.exit_code, 1);
        assert_eq!(failed.stderr, "Unknown session operation");
        let missing = answer(
            &args("exists S1-P1-G4"),
            &directory.path().to_string_lossy(),
            &ZmxCommandOptions::default(),
        )
        .unwrap();
        assert_eq!(missing.exit_code, 1);
    }

    #[test]
    fn commands_that_are_not_one_request_still_run_wmx() {
        for command in [
            "send S1-P1-G1",
            "kill S1-P1-G1",
            "list --short",
            "start-encoded abc",
        ] {
            assert!(answer(&args(command), "unused", &ZmxCommandOptions::default()).is_none());
        }
    }
}
