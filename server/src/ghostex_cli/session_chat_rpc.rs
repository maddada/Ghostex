use base64::Engine as _;
use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::rpc::{
    request_gxserver_envelope, resolve_gxserver_server_target, CliError, CliResult,
};

/// `ghostex session-chat-rpc <method> --params-base64 <b64>`: one chat request, answered with the
/// daemon's own outcome.
///
/// CDXC:Mobile 2026-09-24 WHY:
/// The phone's Rust chat core asks for gxserver chat calls by method name and params, exactly as
/// the desktop core does (`Effect::SendRpc`), and the phone reaches the computer only by SSH exec.
/// One verb that performs any chat call and prints `{ok, result}` or `{ok: false, code, message,
/// endpoint}` gives the phone the same inputs the desktop core gets with one exec per effect,
/// instead of a hand-written CLI verb and a phone-side argument builder per method. Live frames
/// ride the same verb: the phone long-polls `readSessionChat` with `waitMs` and `fingerprint` and
/// hands each changed answer to the core as a snapshot frame. A streaming verb was not added
/// because the SSH module cannot stream an exec channel, and a per-poll WebSocket subscription
/// would start a new follower epoch and rebroadcast a snapshot to every other client of the
/// session (`subscribe_session_chat_follower`).
///
/// The method list is the chat core's (`ChatRpcMethod` in `packages/gx-chat-core/src/wire/rpc.rs`)
/// minus the two the host answers itself, plus `switchSessionAgent`, the call behind the chat's own
/// More actions > Switch Account rows for agents without the Accounts panel (the desktop host makes
/// it for its chat, the phone has no other way to), `readCoordinatorThreads`, the full list behind a
/// coordinator's Threads panel "N more" row, `openCoordinatorThread`, which resumes a closed thread
/// before one of that panel's rows opens it, and `claimSessionChatLaunchDraft`, which the
/// desktop host calls when a session opens straight in Chat and the phone calls for a Handoff's new
/// conversation so the staged handover link lands in its chat composer, and `runGitAction`, for
/// the phone's read-only Changes screen, which may only ask for `changesDiff` through this verb
/// (`READ_ONLY_GIT_ACTIONS`). It stays a chat bridge
/// rather than a door to every gxserver path, the same rule `saved_prompts.rs` keeps. Params travel
/// base64-encoded so no shell
/// (POSIX, PowerShell, WSL) can re-quote a JSON body on its way through.
/// SEE-ALSO: packages/gx-chat-core/src/wire/rpc.rs, apps/mobile/app/src/chat/rust/transport.ts
const SESSION_CHAT_RPC_METHODS: [&str; 39] = [
    "readSessionChat",
    "readSessionChatSkills",
    "readSessionChatFiles",
    "readSessionChatImage",
    "readVisualPageThumbnail",
    "sendSessionChatMessage",
    "interruptSessionChat",
    "answerSessionChatPrompt",
    "rewindSessionChat",
    "selectSessionChatModel",
    "queueSessionChatPrompt",
    "updateSessionChatQueuedPrompt",
    "removeSessionChatQueuedPrompt",
    "reorderSessionChatQueue",
    "sendSessionChatQueuedPrompt",
    "setSessionChatDraft",
    "acknowledgeSessionChatDraftHandoff",
    "readSessionTerminalTail",
    "readSessionChatReport",
    "readSessionChatResource",
    "sessionForkBranches",
    "switchDraftAgent",
    "draftRunLocation",
    "agentAccounts",
    "readSessionAgentNote",
    "saveSessionAgentNote",
    "listStashedPrompts",
    "saveStashedPrompt",
    "runProjectDocsAction",
    "readCoordinatorThreads",
    "openCoordinatorThread",
    "switchSessionAgent",
    "claimSessionChatLaunchDraft",
    "takeBackSessionChatQueuedMessage",
    "catchUpSession",
    "readSessionFilesLinks",
    "readSessionDetails",
    "stopSessionBackgroundTask",
    "runGitAction",
];

/// The Git actions `runGitAction` may run through this verb: reading a project's Changes diff.
///
/// CDXC:Changes 2026-10-25 DECISION:
/// The user chose a read-only Changes screen for the phone ("phone read-only"): it reads the same
/// `changesDiff` patch the desktop Changes tab shows and never reverts, stages or commits, so the
/// bridge refuses every other Git action instead of opening `runGitAction` whole.
/// SEE-ALSO: server/src/typed_operations/git_changes.rs, apps/mobile/app/src/changes/client.ts
const READ_ONLY_GIT_ACTIONS: [&str; 1] = ["changesDiff"];

/// The request timeout when the caller names none: the desktop chat's own (`native_chat/rpc.rs`).
const DEFAULT_TIMEOUT_MS: f64 = 60_000.0;
/// How much longer than a long-poll's `waitMs` the HTTP request may take.
const LONG_POLL_TIMEOUT_MARGIN_MS: f64 = 15_000.0;

fn usage() -> String {
    format!(
        "Usage: ghostex session-chat-rpc <method> (--params-base64 <base64 json> | --params <json>) [--timeout <ms>]\n\nPrints {{\"ok\":true,\"result\":...}} or {{\"ok\":false,\"code\":...,\"message\":...,\"endpoint\":...}}.\nMethods: {}",
        SESSION_CHAT_RPC_METHODS.join(", ")
    )
}

pub fn session_chat_rpc_command(args: &[String]) -> CliResult<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{}", usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let method = match parsed.rest.as_slice() {
        [method] => method.as_str(),
        _ => return Err(CliError::Other(usage())),
    };
    if !SESSION_CHAT_RPC_METHODS.contains(&method) {
        return Err(CliError::Other(format!(
            "Unknown session chat method: {method}\n\n{}",
            usage()
        )));
    }
    let params = read_params(&parsed.flags)?;
    if method == "runGitAction" {
        let action = params.get("action").and_then(Value::as_str).unwrap_or("");
        if !READ_ONLY_GIT_ACTIONS.contains(&action) {
            return Err(CliError::Other(format!(
                "session-chat-rpc runGitAction only reads: {} (got {action:?}).",
                READ_ONLY_GIT_ACTIONS.join(", ")
            )));
        }
    }
    let mut flags = parsed.flags.clone();
    if !flags.contains("timeout") && !flags.contains("timeoutMs") {
        let wait_ms = params.get("waitMs").and_then(Value::as_f64).unwrap_or(0.0);
        let timeout = if wait_ms > 0.0 {
            (wait_ms + LONG_POLL_TIMEOUT_MARGIN_MS).max(DEFAULT_TIMEOUT_MS)
        } else {
            DEFAULT_TIMEOUT_MS
        };
        flags.insert_text("timeout", &timeout.to_string());
    }
    let endpoint = format!("/api/{method}");
    let params = Value::Object(params);
    let target = resolve_gxserver_server_target(&flags, &params)?;
    let body = json!({
        "params": params,
        "protocolVersion": crate::ghostex_cli::rpc::GXSERVER_PROTOCOL_VERSION,
    });
    let outcome = match request_gxserver_envelope(&target, &endpoint, &body, &flags) {
        Ok(envelope) => json!({
            "ok": true,
            "result": envelope.get("result").cloned().unwrap_or(Value::Null),
        }),
        Err(CliError::Rpc { message, response }) => {
            crate::ghostex_cli::set_exit_code(1);
            refusal(response.get("error").cloned(), message, &endpoint)
        }
        Err(error) => {
            crate::ghostex_cli::set_exit_code(1);
            refusal(
                Some(Value::String("unreachable".to_string())),
                error.to_string(),
                &endpoint,
            )
        }
    };
    // One compact line: a transcript read can be megabytes, and the phone parses it whole.
    println!(
        "{}",
        serde_json::to_string(&outcome).unwrap_or_else(|_| "null".to_string())
    );
    Ok(())
}

/// `RpcOutcome::Err`'s fields, in the spelling the desktop view hands the core (`native_chat/rpc.rs`).
fn refusal(code: Option<Value>, message: String, endpoint: &str) -> Value {
    let mut outcome = Map::new();
    outcome.insert("ok".to_string(), Value::Bool(false));
    if let Some(code) = code.filter(Value::is_string) {
        outcome.insert("code".to_string(), code);
    }
    outcome.insert("message".to_string(), Value::String(message));
    outcome.insert("endpoint".to_string(), Value::String(endpoint.to_string()));
    Value::Object(outcome)
}

fn read_params(flags: &crate::ghostex_cli::args::Flags) -> CliResult<Map<String, Value>> {
    let text = if let Some(encoded) = flags.string_value("paramsBase64") {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|error| CliError::Other(format!("Invalid --params-base64: {error}")))?;
        String::from_utf8(bytes)
            .map_err(|error| CliError::Other(format!("Invalid --params-base64: {error}")))?
    } else {
        flags.string_value("params").unwrap_or("{}").to_string()
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(params)) => Ok(params),
        Ok(_) => Err(CliError::Other(
            "session-chat-rpc params must be a JSON object.".to_string(),
        )),
        Err(error) => Err(CliError::Other(format!(
            "Invalid session-chat-rpc params: {error}"
        ))),
    }
}
