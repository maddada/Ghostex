use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use futures::StreamExt as _;

use crate::app::helpers::{gpui_bundled_ghostex_cli_resource_dir, gpui_gxserver_rpc_result};
use crate::*;

use super::{
    GpuiExtensionBridgeResponder, GpuiExtensionCloseHandler, GpuiExtensionPermission,
    GpuiExtensionSurfaceContext, extension_permission_error_response, require_extension_permission,
};

const EXTENSION_COMMAND_OUTPUT_MAX_BYTES: usize = 4 * 1024 * 1024;

impl GhostexGpuiApp {
    pub(crate) fn handle_extension_bridge_event(
        &mut self,
        event: cef::ExtensionBridgeEvent,
        surface_context: GpuiExtensionSurfaceContext,
        responder: GpuiExtensionBridgeResponder,
        close_handler: Option<GpuiExtensionCloseHandler>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(request) = parse_bridge_request(&event.payload) else {
            responder(error_response(
                "",
                "invalidRequest",
                "The extension request is invalid.",
            ));
            return;
        };
        let Some(extension) = self
            .extensions_snapshot
            .installed
            .get(&event.extension_id)
            .cloned()
        else {
            responder(error_response(
                &request.request_id,
                "notFound",
                "The extension is not installed.",
            ));
            return;
        };
        if !extension.enabled {
            responder(error_response(
                &request.request_id,
                "operationFailed",
                "The extension is disabled.",
            ));
            return;
        }

        let required_permission = match request.method.as_str() {
            "cli" => Some(GpuiExtensionPermission::Cli),
            "exec" => Some(GpuiExtensionPermission::Exec),
            _ => None,
        };
        if let Some(permission) = required_permission
            && let Err(error) = require_extension_permission(&extension, permission)
        {
            responder(extension_permission_error_response(
                &request.request_id,
                &event.extension_id,
                &request.method,
                error,
            ));
            return;
        }

        match request.method.as_str() {
            "context" => responder(success_response(
                &request.request_id,
                self.extension_context_payload(&surface_context),
            )),
            "settings.get" => responder(success_response(
                &request.request_id,
                serde_json::json!(extension.preferences),
            )),
            "storage.get" => {
                let Some(key) = request_text(&request.params, "key", 128) else {
                    responder(error_response(
                        &request.request_id,
                        "invalidRequest",
                        "Storage requires a valid key.",
                    ));
                    return;
                };
                responder(success_response(
                    &request.request_id,
                    extension
                        .storage
                        .get(key)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                ));
            }
            "settings.set" => {
                let Some(values) = request
                    .params
                    .get("values")
                    .and_then(serde_json::Value::as_object)
                else {
                    responder(error_response(
                        &request.request_id,
                        "invalidRequest",
                        "Settings values must be an object.",
                    ));
                    return;
                };
                if values
                    .values()
                    .any(|value| !value.is_string() && !value.is_boolean() && !value.is_number())
                {
                    responder(error_response(
                        &request.request_id,
                        "invalidRequest",
                        "Setting values must be strings, booleans, or numbers.",
                    ));
                    return;
                }
                self.run_extension_state_rpc(
                    event.extension_id,
                    request.request_id,
                    serde_json::json!({ "preferences": values }),
                    "preferences",
                    responder,
                    cx,
                );
            }
            "storage.set" => {
                let Some(key) = request_text(&request.params, "key", 128) else {
                    responder(error_response(
                        &request.request_id,
                        "invalidRequest",
                        "Storage requires a valid key.",
                    ));
                    return;
                };
                let value = request
                    .params
                    .get("value")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let mut storage = serde_json::Map::new();
                storage.insert(key.to_string(), value);
                self.run_extension_state_rpc(
                    event.extension_id,
                    request.request_id,
                    serde_json::json!({ "storage": storage }),
                    "storage",
                    responder,
                    cx,
                );
            }
            "ui.setBadge" => {
                let Some(lines) = request
                    .params
                    .get("lines")
                    .and_then(serde_json::Value::as_array)
                else {
                    responder(error_response(
                        &request.request_id,
                        "invalidRequest",
                        "Badge lines must be an array.",
                    ));
                    return;
                };
                if lines.len() > 3
                    || lines
                        .iter()
                        .any(|line| line.as_str().is_none_or(|line| line.chars().count() > 32))
                {
                    responder(error_response(
                        &request.request_id,
                        "invalidRequest",
                        "Badge lines must contain at most three strings of 32 characters.",
                    ));
                    return;
                }
                self.run_extension_rpc(
                    "/api/extensionBadge",
                    serde_json::json!({ "id": event.extension_id, "lines": lines }),
                    request.request_id,
                    responder,
                    cx,
                );
            }
            "ui.toast" => {
                let Some(message) = request_text(&request.params, "message", 2_000) else {
                    responder(error_response(
                        &request.request_id,
                        "invalidRequest",
                        "Toast message is required.",
                    ));
                    return;
                };
                self.dispatch_gpui_workspace_action_toast("info", &extension.id, message, cx);
                responder(success_response(
                    &request.request_id,
                    serde_json::Value::Null,
                ));
            }
            "ui.close" => {
                responder(success_response(
                    &request.request_id,
                    serde_json::Value::Null,
                ));
                if let Some(close) = close_handler {
                    close();
                }
            }
            "cli" => self.run_extension_cli(request, responder, cx),
            "exec" => run_extension_exec(request, responder, cx),
            _ => responder(error_response(
                &request.request_id,
                "invalidRequest",
                "Unknown extension bridge method.",
            )),
        }
    }

    fn run_extension_state_rpc(
        &mut self,
        id: String,
        request_id: String,
        patch: serde_json::Value,
        result_field: &'static str,
        responder: GpuiExtensionBridgeResponder,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = serde_json::json!({ "id": id, "patch": patch });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    extension_rpc_result("/api/updateExtensionState", &params).and_then(|value| {
                        value
                            .get("extension")
                            .and_then(|value| value.get("state"))
                            .and_then(|value| value.get(result_field))
                            .cloned()
                            .ok_or_else(|| "gxserver returned invalid extension state.".to_string())
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                responder(result_response(&request_id, result));
                this.refresh_extensions_in_background(cx);
            });
        })
        .detach();
    }

    fn run_extension_rpc(
        &mut self,
        path: &'static str,
        params: serde_json::Value,
        request_id: String,
        responder: GpuiExtensionBridgeResponder,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { extension_rpc_result(path, &params) })
                .await;
            let _ = this.update(cx, |this, cx| {
                responder(result_response(&request_id, result));
                this.refresh_extensions_in_background(cx);
            });
        })
        .detach();
    }

    fn run_extension_cli(
        &mut self,
        request: ExtensionBridgeRequest,
        responder: GpuiExtensionBridgeResponder,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(verb) = request_text(&request.params, "verb", 128)
            .filter(|verb| !verb.chars().any(char::is_whitespace))
            .map(str::to_string)
        else {
            responder(error_response(
                &request.request_id,
                "invalidRequest",
                "CLI verb is invalid.",
            ));
            return;
        };
        let Some(args) = request
            .params
            .get("args")
            .and_then(serde_json::Value::as_array)
            .map(|args| {
                args.iter()
                    .map(|arg| arg.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()
            })
            .flatten()
        else {
            responder(error_response(
                &request.request_id,
                "invalidRequest",
                "CLI args must be strings.",
            ));
            return;
        };
        let request_id = request.request_id;
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, _cx| {
            let result = background
                .spawn(async move { run_ghostex_cli(&verb, &args) })
                .await;
            responder(result_response(&request_id, result));
        })
        .detach();
    }
}

#[derive(Clone)]
struct ExtensionBridgeRequest {
    request_id: String,
    method: String,
    params: serde_json::Value,
}

fn parse_bridge_request(payload: &str) -> Result<ExtensionBridgeRequest, ()> {
    let value: serde_json::Value = serde_json::from_str(payload).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    let request_id = request_text(&value, "requestId", 128)
        .ok_or(())?
        .to_string();
    let method = request_text(&value, "method", 64).ok_or(())?.to_string();
    let params = object
        .get("params")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if !params.is_object() {
        return Err(());
    }
    Ok(ExtensionBridgeRequest {
        request_id,
        method,
        params,
    })
}

fn request_text<'a>(value: &'a serde_json::Value, key: &str, max_chars: usize) -> Option<&'a str> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.chars().count() <= max_chars)
}

fn extension_rpc_result(
    path: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    gpui_gxserver_rpc_result(path, params, Duration::from_secs(30))
}

fn success_response(request_id: &str, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "requestId": request_id, "ok": true, "result": result })
}

fn error_response(request_id: &str, code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "requestId": request_id,
        "ok": false,
        "error": { "code": code, "message": message },
    })
}

fn result_response(
    request_id: &str,
    result: Result<serde_json::Value, String>,
) -> serde_json::Value {
    match result {
        Ok(value) => success_response(request_id, value),
        Err(_) => error_response(
            request_id,
            "operationFailed",
            "The extension operation failed.",
        ),
    }
}

fn ghostex_cli_executable() -> Result<PathBuf, String> {
    if let Ok(directory) = gpui_bundled_ghostex_cli_resource_dir() {
        return Ok(directory.join(if cfg!(target_os = "windows") {
            "ghostex.exe"
        } else {
            "ghostex"
        }));
    }
    let current = std::env::current_exe().map_err(|_| "Ghostex CLI is unavailable.".to_string())?;
    let sibling = current.with_file_name(if cfg!(target_os = "windows") {
        "ghostex.exe"
    } else {
        "ghostex"
    });
    sibling
        .is_file()
        .then_some(sibling)
        .ok_or_else(|| "Ghostex CLI is unavailable.".to_string())
}

fn run_ghostex_cli(verb: &str, args: &[String]) -> Result<serde_json::Value, String> {
    let output = Command::new(ghostex_cli_executable()?)
        .arg(verb)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "Could not run the Ghostex CLI.".to_string())?;
    Ok(serde_json::json!({
        "exitCode": output.status.code().unwrap_or(-1),
        "stdout": bounded_output(&output.stdout),
        "stderr": bounded_output(&output.stderr),
    }))
}

enum ExecMessage {
    Chunk(&'static str, String),
    Finished(i32),
    Failed,
}

fn run_extension_exec(
    request: ExtensionBridgeRequest,
    responder: GpuiExtensionBridgeResponder,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    let Some(command) = request_text(&request.params, "command", 128 * 1024).map(str::to_string)
    else {
        responder(error_response(
            &request.request_id,
            "invalidRequest",
            "Command is required.",
        ));
        return;
    };
    let cwd = request
        .params
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from);
    if cwd.as_deref().is_some_and(|path| !path.is_dir()) {
        responder(error_response(
            &request.request_id,
            "invalidRequest",
            "Working directory does not exist.",
        ));
        return;
    }
    let request_id = request.request_id;
    let (sender, mut receiver) = futures::channel::mpsc::unbounded();
    thread::spawn(move || run_streaming_command(&command, cwd.as_deref(), sender));
    cx.spawn(async move |_this, _cx| {
        let mut stdout = String::new();
        let mut stderr = String::new();
        while let Some(message) = receiver.next().await {
            match message {
                ExecMessage::Chunk(stream, text) => {
                    let target = if stream == "stdout" {
                        &mut stdout
                    } else {
                        &mut stderr
                    };
                    if target.len() < EXTENSION_COMMAND_OUTPUT_MAX_BYTES {
                        let remaining = EXTENSION_COMMAND_OUTPUT_MAX_BYTES - target.len();
                        target.extend(text.chars().take(remaining));
                    }
                    responder(serde_json::json!({
                        "requestId": request_id,
                        "chunk": { "stream": stream, "text": text },
                    }));
                }
                ExecMessage::Finished(exit_code) => {
                    responder(success_response(
                        &request_id,
                        serde_json::json!({
                            "exitCode": exit_code,
                            "stdout": stdout,
                            "stderr": stderr,
                        }),
                    ));
                    break;
                }
                ExecMessage::Failed => {
                    responder(error_response(
                        &request_id,
                        "operationFailed",
                        "Could not run the command.",
                    ));
                    break;
                }
            }
        }
    })
    .detach();
}

fn run_streaming_command(
    command: &str,
    cwd: Option<&Path>,
    sender: futures::channel::mpsc::UnboundedSender<ExecMessage>,
) {
    #[cfg(target_os = "windows")]
    let mut process = {
        let mut command_process = Command::new("powershell.exe");
        command_process.args(["-NoProfile", "-Command", command]);
        command_process
    };
    #[cfg(not(target_os = "windows"))]
    let mut process = {
        let mut command_process = Command::new("/bin/zsh");
        command_process.args(["-lc", command]);
        command_process
    };
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    let Ok(mut child) = process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        let _ = sender.unbounded_send(ExecMessage::Failed);
        return;
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_sender = sender.clone();
    let stdout_thread = thread::spawn(move || stream_pipe(stdout, "stdout", stdout_sender));
    let stderr_sender = sender.clone();
    let stderr_thread = thread::spawn(move || stream_pipe(stderr, "stderr", stderr_sender));
    let status = child.wait();
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    match status {
        Ok(status) => {
            let _ = sender.unbounded_send(ExecMessage::Finished(status.code().unwrap_or(-1)));
        }
        Err(_) => {
            let _ = sender.unbounded_send(ExecMessage::Failed);
        }
    }
}

fn stream_pipe(
    pipe: Option<impl std::io::Read>,
    stream: &'static str,
    sender: futures::channel::mpsc::UnboundedSender<ExecMessage>,
) {
    let Some(pipe) = pipe else {
        return;
    };
    use std::io::Read as _;
    let mut reader = BufReader::new(pipe);
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let Ok(count) = reader.read(&mut buffer) else {
            break;
        };
        if count == 0 {
            break;
        }
        let text = String::from_utf8_lossy(&buffer[..count]).into_owned();
        let _ = sender.unbounded_send(ExecMessage::Chunk(stream, text));
    }
}

fn bounded_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(EXTENSION_COMMAND_OUTPUT_MAX_BYTES)])
        .into_owned()
}
