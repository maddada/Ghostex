//! `ghostex acp-host`: runs inside a session's `zmx` PTY, owns the agent's stdio
//! ACP subprocess (`agy acp --stdio`, etc.), spools every ACP event to an
//! append-only per-session file, bridges commands/approvals/state over
//! `/api/agentChatChannel`, survives `gxserver` reloads without dropping the
//! agent, and can switch in-place into the interactive CLI terminal on request.

use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde_json::{json, Value};

use crate::constants::{
    GXSERVER_LOCAL_API_HOST, GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION,
};

use super::approval::{
    canonicalize_writable_roots, evaluate_permission, find_advertised_auto_mode_id,
    find_advertised_default_mode_id, is_auto_or_bypass_mode_id, AcpApprovalLevel,
    AcpPermissionOption, PermissionDecision,
};
use super::spec::{
    build_acp_server_argv, build_cli_switch_argv_with_options, spec_for_family,
    transcript_path_for_session, AcpAccountWrapper, AcpAgentSpec, AcpLaunchProfile,
};

const ACP_LOAD_TIMEOUT: Duration = Duration::from_secs(90);
const ACP_RPC_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const POLL_RETRY_DELAY: Duration = Duration::from_millis(300);
const SPOOL_MAX_BYTES: u64 = 2 * 1024 * 1024;
const SPOOL_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const SPOOL_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const SPOOL_ACTIVE_GRACE: Duration = Duration::from_secs(60);
const SPOOL_MAX_LINE_BYTES: usize = 16 * 1024;

#[cfg(unix)]
static SIGNAL_TERMINATE: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
pub(super) fn termination_requested() -> bool {
    SIGNAL_TERMINATE.load(Ordering::Acquire)
}

#[cfg(unix)]
extern "C" fn handle_termination_signal(_signal: libc::c_int) {
    SIGNAL_TERMINATE.store(true, Ordering::SeqCst);
}

/// Which installed guard the unhandled-termination watchdog belongs to; a dropped guard bumps it.
#[cfg(unix)]
static SIGNAL_GUARD_GENERATION: AtomicU64 = AtomicU64::new(0);

/// How long a termination signal may go unanswered before the host exits on its own. A driver's
/// signal thread acts within 50 ms and spends at most about a second stopping the agent.
#[cfg(unix)]
const UNHANDLED_TERMINATION_EXIT_AFTER: Duration = Duration::from_secs(3);

pub(super) struct TerminationSignalGuard;

impl TerminationSignalGuard {
    pub(super) fn install() -> Self {
        #[cfg(unix)]
        unsafe {
            SIGNAL_TERMINATE.store(false, Ordering::SeqCst);
            let handler = handle_termination_signal as extern "C" fn(libc::c_int);
            libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
            libc::signal(libc::SIGHUP, handler as libc::sighandler_t);
            libc::signal(libc::SIGINT, handler as libc::sighandler_t);
        }
        #[cfg(unix)]
        Self::watch_for_unhandled_termination();
        Self
    }

    /// CDXC:AgentProviders 2026-11-27 WHY (GitHub #786 follow-up):
    /// Every driver installs this handler before it starts the agent, but only starts the thread
    /// that acts on it once the agent has answered its startup handshake. A host whose agent hung
    /// there (a stuck `claude` on a busy server, an agent that never answers `initialize`) took
    /// SIGTERM, SIGHUP and SIGINT as a flag nobody read and kept running; only SIGKILL ended it.
    /// This watchdog exits the host when a termination request is still unanswered after
    /// `UNHANDLED_TERMINATION_EXIT_AFTER`. The agent it may have started is stopped by the
    /// `acp-child-watch` process (and `PR_SET_PDEATHSIG` on Linux) that already covers a host
    /// that dies hard (child_lifetime.rs).
    #[cfg(unix)]
    fn watch_for_unhandled_termination() {
        let generation = SIGNAL_GUARD_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
        let _ = thread::Builder::new()
            .name("acp-host-termination-watchdog".to_string())
            .spawn(move || {
                let mut requested_at: Option<std::time::Instant> = None;
                loop {
                    thread::sleep(Duration::from_millis(100));
                    if SIGNAL_GUARD_GENERATION.load(Ordering::Acquire) != generation {
                        return;
                    }
                    if !termination_requested() {
                        requested_at = None;
                        continue;
                    }
                    let since = *requested_at.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() >= UNHANDLED_TERMINATION_EXIT_AFTER {
                        eprintln!(
                            "[Native] Stopping: a termination signal arrived while the agent was still starting."
                        );
                        std::process::exit(143);
                    }
                }
            });
    }
}

impl Drop for TerminationSignalGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            SIGNAL_GUARD_GENERATION.fetch_add(1, Ordering::AcqRel);
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGHUP, libc::SIG_DFL);
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            SIGNAL_TERMINATE.store(false, Ordering::SeqCst);
        }
    }
}

pub(super) struct HostArgs {
    pub(super) family: String,
    pub(super) cli_command: Option<String>,
    pub(super) resume_session_id: Option<String>,
    pub(super) fork_session: bool,
    pub(super) read_only: bool,
    pub(super) approval_level: Option<AcpApprovalLevel>,
    pub(super) writable_roots: Vec<PathBuf>,
    pub(super) model: Option<String>,
    pub(super) effort: Option<String>,
    pub(super) channel_secret_file: Option<String>,
    pub(super) profile: AcpLaunchProfile,
}

fn parse_args(args: &[String]) -> Result<HostArgs, String> {
    let mut family = String::new();
    let mut cli_command = None;
    let mut resume_session_id = None;
    let mut fork_session = false;
    let mut read_only = false;
    let mut approval_level: Option<AcpApprovalLevel> = None;
    let mut writable_roots = Vec::new();
    let mut model = None;
    let mut effort = None;
    let mut channel_secret_file = None;
    let mut wrapper_bin: Option<String> = None;
    let mut wrapper_slot: Option<String> = None;
    let mut profile = AcpLaunchProfile::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--agent" => {
                i += 1;
                family = args.get(i).cloned().unwrap_or_default();
            }
            "--cli-command" => {
                i += 1;
                cli_command = args.get(i).cloned();
            }
            "--resume" => {
                i += 1;
                resume_session_id = args.get(i).cloned();
            }
            "--fork-session" => fork_session = true,
            "--read-only" => read_only = true,
            "--accept-all" => approval_level = Some(AcpApprovalLevel::FullAccess),
            "--approval-level" => {
                i += 1;
                let raw = args.get(i).map(String::as_str).unwrap_or_default();
                approval_level = Some(
                    AcpApprovalLevel::parse(raw)
                        .ok_or_else(|| format!("Unknown --approval-level value: {raw}"))?,
                );
            }
            "--writable-root" => {
                i += 1;
                if let Some(root) = args.get(i).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    writable_roots.push(PathBuf::from(root));
                }
            }
            "--model" => {
                i += 1;
                model = args.get(i).cloned();
            }
            "--effort" => {
                i += 1;
                effort = args.get(i).cloned();
            }
            "--account-wrapper-bin" => {
                i += 1;
                wrapper_bin = args.get(i).cloned();
            }
            "--account-wrapper-slot" => {
                i += 1;
                wrapper_slot = args.get(i).cloned();
            }
            "--hub-id" => {
                i += 1;
                profile.hub_id = args.get(i).cloned();
            }
            "--connector-env" => {
                i += 1;
                if let Some(pair) = args.get(i) {
                    if let Some((k, v)) = pair.split_once('=') {
                        profile.connector_envs.push((k.to_string(), v.to_string()));
                    }
                }
            }
            "--append-system-prompt-file" => {
                i += 1;
                profile.append_system_prompt_file = args.get(i).cloned();
            }
            "--developer-instructions" => {
                i += 1;
                profile.developer_instructions = args.get(i).cloned();
            }
            "--developer-instructions-json" => {
                i += 1;
                if let Some(raw) = args.get(i) {
                    let decoded: String = serde_json::from_str(raw).map_err(|err| {
                        format!("Invalid --developer-instructions-json value: {err}")
                    })?;
                    profile.developer_instructions = Some(decoded);
                }
            }
            "--gbot-chat" => profile.gbot_chat = true,
            "--gbot-api-chat" => profile.gbot_api_chat = true,
            "--gbot-computer-use-off" => profile.gbot_computer_use_off = true,
            "--lean-worker" => profile.lean_worker = true,
            "--chrome" => profile.claude_chrome = true,
            "--no-chrome" => profile.claude_chrome_off = true,
            "--hermes-profile" => {
                i += 1;
                if let Some(raw) = args
                    .get(i)
                    .map(|s| s.trim())
                    .filter(|s| super::hermes::is_valid_hermes_profile(s))
                {
                    profile.hermes_profile = Some(raw.to_string());
                }
            }
            "--channel-secret-file" => {
                i += 1;
                channel_secret_file = args.get(i).cloned();
            }
            other => return Err(format!("Unknown acp-host option: {other}")),
        }
        i += 1;
    }
    if family.is_empty() {
        return Err("acp-host requires --agent <family>.".to_string());
    }
    if let Some(slot) = wrapper_slot.filter(|s| super::spec::is_valid_account_wrapper_slot(s)) {
        // Ignore any caller-supplied `--account-wrapper-bin` path; always resolve the installed
        // `cswap` / `xswap` executable through Ghostex's own accounts helper.
        let _ = wrapper_bin;
        if let Some(bin) = super::spec::resolve_account_wrapper_executable(&family) {
            profile.account_wrapper = Some(AcpAccountWrapper { bin, slot });
        }
    }
    Ok(HostArgs {
        family,
        cli_command,
        resume_session_id,
        fork_session,
        read_only,
        approval_level,
        writable_roots,
        model,
        effort,
        channel_secret_file,
        profile,
    })
}

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Clone)]
pub(super) struct GxserverEndpoint {
    pub(super) base_url: String,
    pub(super) token: String,
    pub(super) channel_secret: Option<String>,
    pub(super) session_ref: String,
    pub(super) project_id: String,
    pub(super) session_id: String,
    pub(super) instance: String,
    saw_working: Arc<AtomicBool>,
}

impl GxserverEndpoint {
    pub(super) fn from_env_or_secret_file(secret_file: Option<&str>) -> Option<Self> {
        // Consume and unlink any one-shot secret file immediately, even if session_ref is absent.
        let file_secret = secret_file
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|p| super::channel::read_and_remove_channel_secret_file(Path::new(p)));
        let session_ref = env::var("GHOSTEX_GLOBAL_SESSION_REF")
            .ok()?
            .trim()
            .to_string();
        if session_ref.is_empty() {
            return None;
        }
        let (project_id, session_id) =
            match crate::agent_hooks::probing::parse_global_session_ref(&session_ref) {
                (Some(p), Some(s)) => (p, s),
                _ => return None,
            };
        let paths = crate::paths::get_gxserver_paths(None);
        let base_url = env::var("GHOSTEX_GXSERVER_BASE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| {
                let port = crate::config::read_selected_local_api_port().unwrap_or(58744);
                format!("http://{GXSERVER_LOCAL_API_HOST}:{port}")
            })
            .trim_end_matches('/')
            .to_string();
        let token = env::var("GHOSTEX_SESSION_TOKEN")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                fs::read_to_string(&paths.auth_token_file)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_default();
        let channel_secret = file_secret.or_else(|| {
            env::var(super::channel::ACP_CHANNEL_SECRET_ENV)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        });
        let instance = format!("acp-{}-{}", std::process::id(), now_ms());
        Some(Self {
            base_url,
            token,
            channel_secret,
            session_ref,
            project_id,
            session_id,
            instance,
            saw_working: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) fn post_json(
        &self,
        path: &str,
        body: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let url = format!("{}{path}", self.base_url);
        let agent: ureq::Agent = crate::http_agent::config()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .build()
            .into();
        let mut req = agent
            .post(&url)
            .header("Content-Type", "application/json")
            .header(
                GXSERVER_PROTOCOL_HEADER,
                &GXSERVER_PROTOCOL_VERSION.to_string(),
            )
            .header("X-Ghostex-Session-Ref", &self.session_ref);
        if !self.token.is_empty() {
            req = req.header("Authorization", &format!("Bearer {}", self.token));
        }
        let response = req.send_json(&body).map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let parsed: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}: {parsed}"));
        }
        Ok(parsed)
    }

    pub(super) fn report_activity(
        &self,
        family: &str,
        activity: &str,
        native_session_id: Option<&str>,
        pending_permission: Option<&PendingPermission>,
        interrupted: bool,
    ) -> bool {
        self.report_activity_with_path(
            family,
            activity,
            native_session_id,
            None,
            pending_permission,
            interrupted,
        )
    }

    pub(super) fn report_activity_with_path(
        &self,
        family: &str,
        activity: &str,
        native_session_id: Option<&str>,
        explicit_transcript_path: Option<&str>,
        pending_permission: Option<&PendingPermission>,
        interrupted: bool,
    ) -> bool {
        self.report_activity_with_error(
            family,
            activity,
            native_session_id,
            explicit_transcript_path,
            pending_permission,
            interrupted,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn report_activity_with_error(
        &self,
        family: &str,
        activity: &str,
        native_session_id: Option<&str>,
        explicit_transcript_path: Option<&str>,
        pending_permission: Option<&PendingPermission>,
        interrupted: bool,
        stop_error: Option<&str>,
    ) -> bool {
        let event_name = match activity {
            "working" => {
                self.saw_working.store(true, Ordering::Relaxed);
                "UserPromptSubmit"
            }
            "attention" => {
                self.saw_working.store(true, Ordering::Relaxed);
                "PermissionRequest"
            }
            // A failed turn settles as `StopFailure` (idle, no blue dot, wakes account recovery
            // and failure notifications) rather than `Stop`.
            _ if stop_error.is_some() => "StopFailure",
            // An interrupted/cancelled turn settles as `Interrupt` (idle), never `Stop`, so
            // gxserver never treats a user-stopped Non-CLI turn as finished.
            _ if interrupted => "Interrupt",
            // Initial startup idle before any turn has run must report `SessionStart` (not `Stop`),
            // so brand-new Non-CLI drafts are not promoted out of draft status on boot.
            _ if self.saw_working.load(Ordering::Relaxed) => "Stop",
            _ => "SessionStart",
        };
        let mut params = json!({
            "projectId": self.project_id,
            "sessionId": self.session_id,
            "agentName": family,
            "eventName": event_name,
            "status": activity,
            "timestamp": now_ms(),
        });
        if event_name == "StopFailure" {
            if let Some(code) = stop_error.filter(|s| !s.trim().is_empty()) {
                params["stopError"] = json!(code);
            }
        }
        if let Some(sid) = native_session_id.filter(|s| !s.is_empty()) {
            params["agentSessionId"] = json!(sid);
            if let Some(path) = explicit_transcript_path
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    transcript_path_for_session(family, sid)
                        .map(|p| p.to_string_lossy().to_string())
                })
            {
                params["agentSessionPath"] = json!(path);
            }
        }
        if let Some(perm) = pending_permission {
            let prompt = perm.to_ghostex_prompt();
            if perm.is_question || prompt.get("kind").and_then(Value::as_str) == Some("question") {
                // `/api/ingestAgentHookEvent` derives the stored question card from
                // `(toolName, toolInput)` via `derive_session_chat_prompt`; sending
                // `"AskUserQuestion"` + `{ "questions": [...] }` ensures every agent family
                // (Claude `AskUserQuestion`, Codex `request_user_input`, Antigravity) stores a
                // real `kind: "question"` card rather than falling back to `kind: "approval"`.
                params["toolName"] = json!("AskUserQuestion");
                params["toolInput"] = json!({
                    "questions": prompt.get("questions").cloned().unwrap_or_else(|| json!([])),
                });
            } else {
                params["toolName"] = json!(perm.tool_name);
                params["toolInput"] = json!({ "command": perm.summary });
            }
            if let Some(tool_use_id) = perm.tool_use_id.as_deref().filter(|s| !s.is_empty()) {
                params["toolUseId"] = json!(tool_use_id);
            }
            if let Some(reason) = perm.reason.as_deref().filter(|s| !s.trim().is_empty()) {
                params["approvalReason"] = json!(reason);
            }
            params["sessionChatPrompt"] = prompt;
        }
        let body = json!({
            "requestId": format!("acp-act-{}", now_ms()),
            "params": params,
        });
        self.post_json("/api/ingestAgentHookEvent", body, Duration::from_secs(5))
            .is_ok()
    }

    pub(super) fn ack(&self, id: &str, ok: bool, result: Value, error: Option<&str>) {
        let mut params = json!({
            "action": "ack",
            "sessionRef": self.session_ref,
            "id": id,
            "ok": ok,
            "result": result,
        });
        if let Some(secret) = self.channel_secret.as_deref() {
            params["channelSecret"] = json!(secret);
        }
        if let Some(err) = error {
            params["error"] = json!(err);
        }
        let body = json!({
            "requestId": format!("acp-ack-{}", now_ms()),
            "params": params,
        });
        let _ = self.post_json("/api/agentChatChannel", body, Duration::from_secs(5));
    }

    pub(super) fn close_channel(&self) {
        let mut params = json!({
            "action": "close",
            "sessionRef": self.session_ref,
            "instance": self.instance,
        });
        if let Some(secret) = self.channel_secret.as_deref() {
            params["channelSecret"] = json!(secret);
        }
        let body = json!({
            "requestId": format!("acp-close-{}", now_ms()),
            "params": params,
        });
        let _ = self.post_json("/api/agentChatChannel", body, Duration::from_secs(3));
    }

    pub(super) fn report_startup_error(&self, family: &str, message: &str, is_auth: bool) {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return;
        }
        let mut params = json!({
            "action": "startupError",
            "sessionRef": self.session_ref,
            "family": family,
            "message": trimmed,
            "isAuth": is_auth,
        });
        if let Some(secret) = self.channel_secret.as_deref() {
            params["channelSecret"] = json!(secret);
        }
        let body = json!({
            "requestId": format!("acp-startup-err-{}", now_ms()),
            "params": params,
        });
        let _ = self.post_json("/api/agentChatChannel", body, Duration::from_secs(3));
    }
}

pub(super) fn extract_jsonrpc_error_message(error: &Value, fallback: &str) -> String {
    error
        .pointer("/data/message")
        .and_then(Value::as_str)
        .or_else(|| error.get("data").and_then(Value::as_str))
        .or_else(|| error.get("message").and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(super) fn report_driver_startup_error(
    endpoint: Option<&GxserverEndpoint>,
    family: &str,
    message: &str,
    is_auth: bool,
) {
    if let Some(ep) = endpoint {
        ep.report_startup_error(family, message, is_auth);
    }
}

pub(super) struct SpoolWriter {
    path: PathBuf,
    file: Option<File>,
    bytes_written: u64,
    seq: u64,
}

/// Deletes ACP spool files past [`SPOOL_RETENTION`], then trims the oldest inactive files so the
/// whole `acp-spool` folder stays within [`SPOOL_MAX_TOTAL_BYTES`].
pub(crate) fn prune_acp_spool(
    gxserver_state_dir: &Path,
    now: SystemTime,
) -> crate::storage_cleanup::Tally {
    prune_acp_spool_dir(&gxserver_state_dir.join("acp-spool"), now)
}

fn prune_acp_spool_dir(dir: &Path, now: SystemTime) -> crate::storage_cleanup::Tally {
    use crate::storage_cleanup::{remove_file, Tally};

    let mut tally = Tally::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return tally;
    };
    let mut kept: Vec<(PathBuf, fs::Metadata, SystemTime)> = Vec::new();
    let mut total_bytes: u64 = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !(name.ends_with(".jsonl") || name.ends_with(".jsonl.1")) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        if now.duration_since(modified).unwrap_or_default() > SPOOL_RETENTION {
            remove_file(&path, &meta, &mut tally);
        } else {
            total_bytes = total_bytes.saturating_add(meta.len());
            kept.push((path, meta, modified));
        }
    }

    if total_bytes > SPOOL_MAX_TOTAL_BYTES {
        kept.sort_by_key(|(_, _, modified)| *modified);
        for (path, meta, modified) in kept {
            if total_bytes <= SPOOL_MAX_TOTAL_BYTES {
                break;
            }
            if now.duration_since(modified).unwrap_or_default() < SPOOL_ACTIVE_GRACE {
                continue;
            }
            let len = meta.len();
            let before = tally.files;
            remove_file(&path, &meta, &mut tally);
            if tally.files > before {
                total_bytes = total_bytes.saturating_sub(len);
            }
        }
    }

    tally
}

impl SpoolWriter {
    fn open_private_file(path: &PathBuf, truncate: bool) -> Option<File> {
        let mut options = OpenOptions::new();
        options.create(true).write(true);
        if truncate {
            options.truncate(true);
        } else {
            options.append(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        }
        Some(file)
    }

    fn sweep_stale_spools(dir: &Path) {
        let _ = prune_acp_spool_dir(dir, SystemTime::now());
    }

    pub(super) fn open(session_id: &str) -> Self {
        let dir = crate::paths::get_gxserver_paths(None)
            .root_dir
            .join("acp-spool");
        let _ = fs::create_dir_all(&dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
        }
        Self::sweep_stale_spools(&dir);
        let safe_id: String = session_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
            .collect();
        let name = if safe_id.is_empty() {
            format!("pid-{}.jsonl", std::process::id())
        } else {
            format!("{safe_id}.jsonl")
        };
        let path = dir.join(name);
        let file = Self::open_private_file(&path, false);
        let bytes_written = file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map(|m| m.len())
            .unwrap_or(0);
        Self {
            path,
            file,
            bytes_written,
            seq: 0,
        }
    }

    fn rotate(&mut self) {
        self.file = None;
        let rotated = PathBuf::from(format!("{}.1", self.path.display()));
        let _ = fs::remove_file(&rotated);
        let _ = fs::rename(&self.path, &rotated);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&rotated, fs::Permissions::from_mode(0o600));
        }
        self.file = Self::open_private_file(&self.path, true);
        self.bytes_written = 0;
    }

    pub(super) fn append(&mut self, event: &Value) {
        self.seq += 1;
        let ts_ms = now_ms();
        let line = json!({
            "seq": self.seq,
            "tsMs": ts_ms,
            "event": event,
        });
        let Ok(mut encoded) = serde_json::to_string(&line) else {
            return;
        };
        if encoded.len() > SPOOL_MAX_LINE_BYTES {
            let preview_end = encoded.floor_char_boundary(512.min(encoded.len()));
            let summary = json!({
                "seq": self.seq,
                "tsMs": ts_ms,
                "truncatedBytes": encoded.len(),
                "preview": &encoded[..preview_end],
            });
            let Ok(compact) = serde_json::to_string(&summary) else {
                return;
            };
            encoded = compact;
        }
        encoded.push('\n');
        let write_len = encoded.len() as u64;
        if self.bytes_written.saturating_add(write_len) > SPOOL_MAX_BYTES {
            self.rotate();
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if file.write_all(encoded.as_bytes()).is_ok() {
            self.bytes_written = self.bytes_written.saturating_add(write_len);
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PendingPermission {
    pub(super) rpc_id: Value,
    pub(super) tool_name: String,
    pub(super) summary: String,
    pub(super) tool_use_id: Option<String>,
    pub(super) reason: Option<String>,
    pub(super) is_question: bool,
    pub(super) options: Vec<AcpPermissionOption>,
    pub(super) custom_prompt_override: Option<Value>,
}

impl PendingPermission {
    pub(super) fn to_ghostex_prompt(&self) -> Value {
        if let Some(custom) = &self.custom_prompt_override {
            return custom.clone();
        }
        if self.is_question {
            return json!({
                "kind": "question",
                "toolUseId": self.tool_use_id,
                "questions": [{
                    "question": if self.tool_name.trim().is_empty() { "Choose an option." } else { self.tool_name.as_str() },
                    "header": "Question",
                    "multiSelect": false,
                    "allowCustom": false,
                    "options": self.options.iter().map(|opt| json!({
                        "label": if opt.name.trim().is_empty() { opt.option_id.as_str() } else { opt.name.as_str() },
                    })).collect::<Vec<_>>(),
                }],
            });
        }
        let mut prompt = json!({
            "kind": "approval",
            "tool": self.tool_name,
            "summary": self.summary,
            "toolUseId": self.tool_use_id,
            "options": self.options.iter().map(|opt| json!({
                "optionId": opt.option_id,
                "label": opt.name,
                "kind": opt.kind,
            })).collect::<Vec<_>>(),
        });
        if let Some(reason) = self.reason.as_deref().filter(|s| !s.trim().is_empty()) {
            prompt["reason"] = json!(reason);
        }
        prompt
    }

    fn resolve_question_option_id(&self, selections: &[Value]) -> Option<String> {
        let first = selections.first()?;
        if let Some(indices) = first.get("indices").and_then(Value::as_array) {
            if let Some(idx) = indices.first().and_then(Value::as_u64) {
                if let Some(opt) = self.options.get(idx as usize) {
                    return Some(opt.option_id.clone());
                }
            }
        }
        if let Some(other) = first
            .get("other")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(opt) = self
                .options
                .iter()
                .find(|opt| opt.option_id == other || opt.name.eq_ignore_ascii_case(other))
            {
                return Some(opt.option_id.clone());
            }
        }
        None
    }

    fn resolve_option_id(
        &self,
        explicit_option_id: Option<&str>,
        decision: &str,
    ) -> Option<String> {
        if let Some(id) = explicit_option_id.filter(|s| !s.is_empty()) {
            if self.options.iter().any(|opt| opt.option_id == id) {
                return Some(id.to_string());
            }
        }
        let normalized = decision.trim().to_ascii_lowercase();
        let (is_deny, wanted_kinds): (bool, &[&str]) = match normalized.as_str() {
            "allow" | "allow_once" | "1" | "y" | "yes" => (false, &["allow_once", "allow_always"]),
            "always" | "allow_always" | "2" => (false, &["allow_always", "allow_once"]),
            "deny" | "reject" | "reject_once" | "escape" | "cancel" | "\x1b" | "n" | "no" => {
                (true, &["reject_once", "reject_always"])
            }
            "reject_always" => (true, &["reject_always", "reject_once"]),
            _ => (false, &["allow_once"]),
        };
        for kind in wanted_kinds {
            if let Some(opt) = self.options.iter().find(|opt| opt.kind == *kind) {
                return Some(opt.option_id.clone());
            }
        }
        if is_deny {
            // Never fall back to an allow option on deny/reject; caller responds with `cancelled`.
            return None;
        }
        self.options
            .iter()
            .find(|opt| !opt.kind.starts_with("reject"))
            .map(|opt| opt.option_id.clone())
    }
}

pub(super) fn now_iso_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(super) const LIVE_STREAM_MAX_CHARS: usize = 32_768;

pub(super) fn append_bounded(buf: &mut String, chunk: &str, max_bytes: usize) {
    if chunk.is_empty() {
        return;
    }
    buf.push_str(chunk);
    if buf.len() > max_bytes {
        let mut cut = buf.len() - max_bytes;
        while cut < buf.len() && !buf.is_char_boundary(cut) {
            cut += 1;
        }
        buf.drain(..cut);
    }
}

/// Ends the live segment a Native driver is streaming: its text and thought, and the times they
/// started, which the chat uses as the segment's id. Takes the driver's state (or its lock guard).
///
/// CDXC:NativeChat 2026-11-28 WHY: issue #791, a Claude Code turn 47 minutes in opened as "one
/// long assistant paragraph" holding all 66 text segments run together, then redrew into the
/// normal view once the transcript loaded. Every driver appended each text delta of the turn to
/// one buffer and only cleared it when the turn ended, so the live message was the whole turn, and
/// a non-empty buffer also outranked the running tool in the live activity. The live message is
/// the segment being written now: each driver ends it where the agent starts a new message or a
/// tool call, and the transcript holds everything before it.
macro_rules! end_live_segment {
    ($state:expr) => {{
        let state = &mut *$state;
        state.live_stream_text.clear();
        state.live_stream_started_at = None;
        state.live_thought_text.clear();
        state.live_thought_started_at = None;
    }};
}
pub(crate) use end_live_segment;

#[derive(Clone, Debug)]
struct HostPendingPermission {
    perm: PendingPermission,
    elicitation: Option<super::elicitation::PendingElicitation>,
    omp_elicitation: Option<super::omp_elicitation::ElicitationMeta>,
}

impl HostPendingPermission {
    fn from_permission(perm: PendingPermission) -> Self {
        Self {
            perm,
            elicitation: None,
            omp_elicitation: None,
        }
    }

    fn from_elicitation(
        rpc_id: Value,
        tool_use_id: String,
        elicitation: super::elicitation::PendingElicitation,
    ) -> Self {
        let prompt = elicitation.to_ghostex_prompt(Some(&tool_use_id));
        let title = elicitation.title().to_string();
        Self {
            perm: PendingPermission {
                rpc_id,
                tool_name: title.clone(),
                summary: title,
                tool_use_id: Some(tool_use_id),
                reason: None,
                is_question: true,
                options: Vec::new(),
                custom_prompt_override: Some(prompt),
            },
            elicitation: Some(elicitation),
            omp_elicitation: None,
        }
    }

    fn from_omp_elicitation(
        perm: PendingPermission,
        meta: super::omp_elicitation::ElicitationMeta,
    ) -> Self {
        Self {
            perm,
            elicitation: None,
            omp_elicitation: Some(meta),
        }
    }

    fn cancel_response(&self) -> Value {
        if self.elicitation.is_some() || self.omp_elicitation.is_some() {
            json!({ "action": "cancel" })
        } else {
            json!({ "outcome": { "outcome": "cancelled" } })
        }
    }
}

struct HostState {
    family: String,
    native_session_id: Option<String>,
    working: bool,
    last_turn_interrupted: bool,
    turn_generation: u64,
    approval_level: AcpApprovalLevel,
    read_only: bool,
    in_flight_target_level: Option<AcpApprovalLevel>,
    default_native_mode: Option<String>,
    native_auto_active: bool,
    cwd: PathBuf,
    writable_roots: Vec<PathBuf>,
    current_model: Option<String>,
    current_effort: Option<String>,
    current_mode: Option<String>,
    available_models: Value,
    available_modes: Value,
    config_options: Value,
    pending_permissions: VecDeque<HostPendingPermission>,
    live_stream_text: String,
    live_stream_started_at: Option<String>,
    /// Every text delta of the turn, for the slash-command reply Hermes and OMP record; the live
    /// message is only the current segment (`end_live_segment!`).
    turn_reply_text: String,
    live_thought_text: String,
    live_thought_started_at: Option<String>,
    live_tool_label: Option<String>,
    live_tool_detail: Option<String>,
    live_tool_started_at: Option<String>,
    usage_used_tokens: Option<u64>,
    usage_window_size: Option<u64>,
    session_title: Option<String>,
    last_event_at_ms: i64,
    child_exited: bool,
}

impl HostState {
    fn new(
        family: &str,
        approval_level: AcpApprovalLevel,
        read_only: bool,
        cwd: PathBuf,
        writable_roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            family: family.to_string(),
            native_session_id: None,
            working: false,
            last_turn_interrupted: false,
            turn_generation: 0,
            approval_level,
            read_only,
            in_flight_target_level: None,
            default_native_mode: None,
            native_auto_active: false,
            cwd,
            writable_roots,
            current_model: None,
            current_effort: None,
            current_mode: None,
            available_models: Value::Array(Vec::new()),
            available_modes: Value::Array(Vec::new()),
            config_options: Value::Array(Vec::new()),
            pending_permissions: VecDeque::new(),
            live_stream_text: String::new(),
            live_stream_started_at: None,
            turn_reply_text: String::new(),
            live_thought_text: String::new(),
            live_thought_started_at: None,
            live_tool_label: None,
            live_tool_detail: None,
            live_tool_started_at: None,
            usage_used_tokens: None,
            usage_window_size: None,
            session_title: None,
            last_event_at_ms: now_ms(),
            child_exited: false,
        }
    }

    fn clear_live_turn_output(&mut self) {
        self.live_stream_text.clear();
        self.live_stream_started_at = None;
        self.turn_reply_text.clear();
        self.live_thought_text.clear();
        self.live_thought_started_at = None;
        self.live_tool_label = None;
        self.live_tool_detail = None;
        self.live_tool_started_at = None;
    }

    fn live_activity_json(&self) -> Value {
        if !self.working {
            return Value::Null;
        }
        if !self.live_stream_text.trim().is_empty() {
            return json!({
                "kind": "stream",
                "text": self.live_stream_text,
                "detectedAt": self.live_stream_started_at.as_deref().unwrap_or_default(),
            });
        }
        if let Some(label) = self
            .live_tool_label
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            return json!({
                "kind": "tool",
                "label": label,
                "detail": self.live_tool_detail,
                "detectedAt": self.live_tool_started_at.as_deref().unwrap_or_default(),
            });
        }
        if !self.live_thought_text.trim().is_empty() {
            return json!({
                "kind": "thinking",
                "text": self.live_thought_text,
                "detectedAt": self.live_thought_started_at.as_deref().unwrap_or_default(),
            });
        }
        Value::Null
    }

    fn context_usage_json(&self) -> Value {
        let Some(used) = self.usage_used_tokens.filter(|u| *u > 0) else {
            return Value::Null;
        };
        let mut obj = serde_json::Map::new();
        obj.insert("usedTokens".to_string(), json!(used));
        if let Some(size) = self.usage_window_size.filter(|s| *s > 0) {
            obj.insert("windowSize".to_string(), json!(size));
            let pct = ((used as f64 / size as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u32;
            obj.insert("usedPercentage".to_string(), json!(pct));
        }
        Value::Object(obj)
    }

    fn snapshot_json(&self) -> Value {
        let transcript_path = self
            .native_session_id
            .as_deref()
            .and_then(|id| transcript_path_for_session(&self.family, id))
            .map(|p| p.to_string_lossy().to_string());
        json!({
            "family": self.family,
            "nativeSessionId": self.native_session_id,
            "working": self.working,
            "model": self.current_model,
            "effort": self.current_effort,
            "mode": self.approval_level.as_str(),
            "modeLabel": self.approval_level.label(),
            "approvalLevel": self.approval_level.as_str(),
            "nativeMode": self.current_mode,
            "nativeAutoActive": self.native_auto_active,
            "nativeAutoAvailable": find_advertised_auto_mode_id(&self.available_modes).is_some(),
            "availableModels": self.available_models,
            "availableModes": self.available_modes,
            "configOptions": self.config_options,
            "pendingPermission": self.pending_permissions.front().map(|p| p.perm.to_ghostex_prompt()),
            "pendingPermissionCount": self.pending_permissions.len(),
            "liveActivity": self.live_activity_json(),
            "contextUsage": self.context_usage_json(),
            "sessionTitle": self.session_title,
            "lastEventAtMs": self.last_event_at_ms,
            "transcriptPath": transcript_path,
            "childExited": self.child_exited,
        })
    }
}

struct AcpTransport {
    stdin: Mutex<Option<ChildStdin>>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    spool: Mutex<SpoolWriter>,
}

impl AcpTransport {
    fn new(stdin: ChildStdin, session_id: &str) -> Self {
        Self {
            stdin: Mutex::new(Some(stdin)),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            spool: Mutex::new(SpoolWriter::open(session_id)),
        }
    }

    fn write_json(&self, message: &Value) -> Result<(), String> {
        let mut guard = self
            .stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(stdin) = guard.as_mut() else {
            return Err("Native host stdin is closed.".to_string());
        };
        let encoded = serde_json::to_string(message).map_err(|e| e.to_string())?;
        stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("Failed writing to Native host: {e}"))
    }

    fn close_stdin(&self) {
        let mut guard = self
            .stdin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = None;
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_json(&msg)
    }

    fn start_call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(u64, mpsc::Receiver<Value>), String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(err) = self.write_json(&msg) {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            return Err(err);
        }
        Ok((id, rx))
    }

    fn finish_call(
        &self,
        id: u64,
        method: &str,
        rx: mpsc::Receiver<Value>,
        timeout: Duration,
    ) -> Result<Value, String> {
        match rx.recv_timeout(timeout) {
            Ok(resp) => {
                if let Some(error) = resp.get("error") {
                    Err(extract_jsonrpc_error_message(
                        error,
                        "Native host request failed",
                    ))
                } else {
                    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
                }
            }
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                Err(format!("Native host request `{method}` timed out."))
            }
        }
    }

    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let (id, rx) = self.start_call(method, params)?;
        self.finish_call(id, method, rx, timeout)
    }

    fn respond_to_agent(&self, rpc_id: Value, result: Value) -> Result<(), String> {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "result": result,
        });
        self.write_json(&msg)
    }

    fn respond_error_to_agent(&self, rpc_id: Value, code: i64, message: &str) {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": rpc_id,
            "error": { "code": code, "message": message },
        });
        let _ = self.write_json(&msg);
    }

    fn spool_event(&self, event: &Value) {
        if let Ok(mut spool) = self.spool.lock() {
            spool.append(event);
        }
    }
}

fn apply_session_metadata(state: &mut HostState, result: &Value) {
    use ghostex_gx_chat_core::menus::catalog::{
        is_placeholder_model_label, is_placeholder_model_value,
    };
    if let Some(sid) = result
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        state.native_session_id = Some(sid.to_string());
    }
    if let Some(models) = result.get("models") {
        if let Some(cur) = models
            .get("currentModelId")
            .or_else(|| models.get("currentModel"))
            .and_then(Value::as_str)
            .filter(|m| !is_placeholder_model_value(m))
        {
            state.current_model = Some(cur.to_string());
            if state.family == "antigravity" && state.current_effort.is_none() {
                if let Some((_, effort)) =
                    crate::agent_model_catalog::split_antigravity_model_id(cur)
                {
                    state.current_effort = Some(effort);
                }
            }
        }
        if let Some(avail) = models.get("availableModels").and_then(Value::as_array) {
            let filtered: Vec<Value> = avail
                .iter()
                .filter(|entry| {
                    let id = entry
                        .get("modelId")
                        .or_else(|| entry.get("id"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let name = entry
                        .get("name")
                        .or_else(|| entry.get("displayName"))
                        .and_then(Value::as_str)
                        .unwrap_or(id);
                    !is_placeholder_model_value(id) && !is_placeholder_model_label(name)
                })
                .cloned()
                .collect();
            state.available_models = Value::Array(filtered);
        }
    }
    if let Some(modes) = result.get("modes") {
        if let Some(cur) = modes
            .get("currentModeId")
            .or_else(|| modes.get("currentMode"))
            .and_then(Value::as_str)
        {
            state.current_mode = Some(cur.to_string());
        }
        if let Some(avail) = modes.get("availableModes") {
            state.available_modes = avail.clone();
        }
    }
    if let Some(configs) = result.get("configOptions") {
        apply_config_options(state, configs, false);
    }
}

fn apply_config_options(state: &mut HostState, configs: &Value, overwrite_model: bool) {
    state.config_options = configs.clone();
    if state.family == "omp" {
        let parsed = super::omp_elicitation::parse_config_options(configs);
        if let Some(model) = parsed.current_model {
            state.current_model = Some(model);
        }
        if !parsed.available_models.is_empty() {
            state.available_models = Value::Array(parsed.available_models);
        }
        if let Some(effort) = parsed.current_effort {
            state.current_effort = Some(effort);
        } else if super::omp_elicitation::find_effort_config_id(configs).is_some() {
            state.current_effort = None;
        }
        if let Some(mode) = parsed.current_mode {
            state.current_mode = Some(mode);
        }
        if !parsed.available_modes.is_empty()
            && state.available_modes.as_array().is_none_or(Vec::is_empty)
        {
            state.available_modes = Value::Array(parsed.available_modes);
        }
        return;
    }
    let (effort, model) = super::hermes::extract_config_option_state(configs);
    if let Some(eff) = effort {
        state.current_effort = Some(eff);
    } else if find_effort_config_option_id(configs).is_some() {
        state.current_effort = None;
    }
    if let Some(m) = model {
        if overwrite_model || state.current_model.is_none() {
            state.current_model = Some(m);
        }
    }
}

fn find_effort_config_option_id(config_options: &Value) -> Option<String> {
    config_options.as_array().and_then(|arr| {
        arr.iter().find_map(|item| {
            let cid = item.get("id").and_then(Value::as_str)?;
            let cat = item.get("category").and_then(Value::as_str).unwrap_or("");
            (cat.contains("thought")
                || cat.contains("effort")
                || cid.contains("effort")
                || cid.contains("thinking"))
            .then(|| cid.to_string())
        })
    })
}

/// Resolves `(model, effort)` to the exact ACP `modelId` that `session/set_model` expects.
/// For Antigravity, `agy acp` exposes flattened `<base>-<effort>` ids (e.g. `gemini-4-neon-high`),
/// whereas Ghostex's model picker keys rows by `<base>` (`gemini-4-neon`) + `effort` (`high`).
fn resolve_acp_target_model_id(
    family: &str,
    available_models: &Value,
    current_model: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> Option<(String, Option<String>)> {
    use ghostex_gx_chat_core::menus::catalog::{family_and_version, is_placeholder_model_value};
    let avail = available_models.as_array();
    let has_id = |candidate: &str| {
        avail.is_some_and(|list| {
            list.iter().any(|entry| {
                entry
                    .get("modelId")
                    .or_else(|| entry.get("id"))
                    .and_then(Value::as_str)
                    == Some(candidate)
            })
        })
    };
    let model = model
        .map(str::trim)
        .filter(|m| !is_placeholder_model_value(m));
    let effort = effort
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(|e| e.to_ascii_lowercase());

    if let Some(model) = model {
        if family == "hermes" {
            let resolved = super::hermes::resolve_hermes_target_model_id(
                available_models,
                current_model,
                model,
            );
            return Some((resolved, effort));
        }
        if family == "antigravity" {
            if has_id(model) {
                let parsed_effort = crate::agent_model_catalog::split_antigravity_model_id(model)
                    .map(|(_, e)| e)
                    .or(effort);
                return Some((model.to_string(), parsed_effort));
            }
            if let Some(ref eff) = effort {
                let combined = format!("{model}-{eff}");
                if has_id(&combined) {
                    return Some((combined, Some(eff.clone())));
                }
            }
            for fallback_eff in ["high", "medium", "low", "xhigh"] {
                let combined = format!("{model}-{fallback_eff}");
                if has_id(&combined) {
                    return Some((combined, Some(fallback_eff.to_string())));
                }
            }
        }
        return Some((model.to_string(), effort));
    }

    // No explicit model: pick the newest Neon for Antigravity, else the first concrete model.
    if family == "antigravity" {
        if let Some(list) = avail {
            let newest_neon = list
                .iter()
                .filter_map(|entry| {
                    let id = entry
                        .get("modelId")
                        .or_else(|| entry.get("id"))
                        .and_then(Value::as_str)?;
                    let name = entry.get("name").and_then(Value::as_str).unwrap_or(id);
                    let (fam, ver) = family_and_version(name).or_else(|| family_and_version(id))?;
                    let words: Vec<&str> = fam.split_whitespace().collect();
                    (words.contains(&"neon") && words.contains(&"gemini")).then_some((id, ver))
                })
                .max_by(|a, b| a.1.cmp(&b.1))
                .map(|(id, _)| id.to_string());
            if let Some(id) = newest_neon {
                let parsed_effort = crate::agent_model_catalog::split_antigravity_model_id(&id)
                    .map(|(_, e)| e)
                    .or(effort);
                return Some((id, parsed_effort));
            }
        }
    }
    avail
        .and_then(|list| list.first())
        .and_then(|entry| {
            entry
                .get("modelId")
                .or_else(|| entry.get("id"))
                .and_then(Value::as_str)
        })
        .filter(|id| !is_placeholder_model_value(id))
        .map(|id| (id.to_string(), effort))
}

fn parse_permission_request(rpc_id: Value, params: &Value) -> PendingPermission {
    let tool_call = params.get("toolCall").unwrap_or(&Value::Null);
    let tool_use_id = tool_call
        .get("toolCallId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let is_question = tool_use_id
        .as_deref()
        .is_some_and(|id| id.starts_with("interaction_") || id.starts_with("clarify-"));
    let tool_name = if is_question {
        tool_call
            .get("title")
            .or_else(|| tool_call.pointer("/rawInput/question"))
            .or_else(|| tool_call.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("Question")
            .to_string()
    } else {
        super::hermes::extract_permission_tool_name(tool_call).unwrap_or_else(|| {
            tool_call
                .get("name")
                .or_else(|| tool_call.get("kind"))
                .or_else(|| tool_call.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("Tool")
                .to_string()
        })
    };
    let summary = tool_call
        .get("rawInput")
        .and_then(|v| {
            [
                "command",
                "CommandLine",
                "file_path",
                "filePath",
                "path",
                "Path",
                "target_path",
                "targetPath",
                "TargetFile",
                "AbsolutePath",
            ]
            .iter()
            .find_map(|key| v.get(*key).and_then(Value::as_str))
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .or_else(|| super::hermes::extract_permission_summary(tool_call))
            .or_else(|| {
                tool_call
                    .pointer("/locations/0/path")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string)
            })
            .or_else(|| serde_json::to_string(v).ok())
        })
        .or_else(|| super::hermes::extract_permission_summary(tool_call))
        .or_else(|| {
            tool_call
                .pointer("/locations/0/path")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| tool_name.clone());
    let options = params
        .get("options")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let option_id = item.get("optionId").and_then(Value::as_str)?.to_string();
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&option_id)
                        .to_string();
                    // Never assume an omitted `kind` is `allow_once`: auto-approval only picks
                    // options explicitly marked `allow_once`.
                    let kind = item
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    Some(AcpPermissionOption {
                        option_id,
                        name,
                        kind,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    PendingPermission {
        rpc_id,
        tool_name,
        summary,
        tool_use_id,
        reason: None,
        is_question,
        options,
        custom_prompt_override: None,
    }
}

fn build_prompt_blocks(
    paths: &crate::paths::GxserverPaths,
    text: &str,
    image_paths: &[String],
) -> Result<Vec<Value>, String> {
    let mut blocks = Vec::new();
    for raw in image_paths {
        let (bytes, mime) = super::read_validated_acp_image(paths, raw)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        blocks.push(json!({
            "type": "image",
            "data": encoded,
            "mimeType": mime,
        }));
    }
    if !text.trim().is_empty() || blocks.is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": text,
        }));
    }
    Ok(blocks)
}

/// Single ordered background worker for `/api/ingestAgentHookEvent` activity updates.
/// Every state transition sends a non-blocking wake signal; the worker reads the current
/// authoritative `pending_permissions.front()` / `working` / `idle` state before posting so a
/// stale `"working"` report can never overwrite a newly arrived `"attention"` card and the stdout
/// JSON-RPC reader never blocks on HTTP.
#[derive(Clone)]
struct ActivityReporter {
    tx: Option<mpsc::Sender<()>>,
}

impl ActivityReporter {
    fn start(
        endpoint: Option<GxserverEndpoint>,
        state: Arc<Mutex<HostState>>,
        family: String,
    ) -> Self {
        let Some(ep) = endpoint else {
            return Self { tx: None };
        };
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            while rx.recv().is_ok() {
                // Coalesce burst updates so we always publish the latest state.
                while rx.try_recv().is_ok() {}
                let (act, sid, perm, interrupted) = {
                    let s = state.lock().unwrap_or_else(|p| p.into_inner());
                    let front = s.pending_permissions.front().map(|p| p.perm.clone());
                    let act = if front.is_some() {
                        "attention"
                    } else if s.working {
                        "working"
                    } else {
                        "idle"
                    };
                    (
                        act,
                        s.native_session_id.clone(),
                        front,
                        s.last_turn_interrupted,
                    )
                };
                let mut ok =
                    ep.report_activity(&family, act, sid.as_deref(), perm.as_ref(), interrupted);
                for delay_ms in [500u64, 1_000, 2_000] {
                    if ok {
                        break;
                    }
                    match rx.recv_timeout(Duration::from_millis(delay_ms)) {
                        Ok(()) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            let (act, sid, perm, interrupted) = {
                                let s = state.lock().unwrap_or_else(|p| p.into_inner());
                                let front = s.pending_permissions.front().map(|p| p.perm.clone());
                                let act = if front.is_some() {
                                    "attention"
                                } else if s.working {
                                    "working"
                                } else {
                                    "idle"
                                };
                                (
                                    act,
                                    s.native_session_id.clone(),
                                    front,
                                    s.last_turn_interrupted,
                                )
                            };
                            ok = ep.report_activity(
                                &family,
                                act,
                                sid.as_deref(),
                                perm.as_ref(),
                                interrupted,
                            );
                        }
                    }
                }
            }
        });
        Self { tx: Some(tx) }
    }

    fn sync(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(());
        }
    }
}

/// Ensures `child_exited` is always set if the stdout JSON-RPC reader thread exits unexpectedly.
struct ReaderExitGuard(Arc<Mutex<HostState>>);

impl Drop for ReaderExitGuard {
    fn drop(&mut self) {
        let mut s = self.0.lock().unwrap_or_else(|p| p.into_inner());
        s.child_exited = true;
        s.working = false;
    }
}

/// Keep the underlying ACP server in its advertised `auto` mode only while `target_level` is
/// `Auto` (and the agent advertises both an auto mode and a safe non-auto mode to return to), and
/// in its default/ask mode for `Ask`, `Auto-edits`, and `Full access` so `acp-host` evaluates
/// every `session/request_permission` live with `allow_once`.
///
/// Serialized by `mode_sync_lock` and records `in_flight_target_level` so an agent's own
/// `current_mode_update` notification during an intentional transition into `Auto` is never
/// misread as drift. Fails visibly (`Err`) whenever leaving an active native auto/bypass mode
/// cannot be confirmed on the subprocess.
fn apply_approval_level_and_sync_native_mode(
    transport: &AcpTransport,
    state: &Mutex<HostState>,
    mode_sync_lock: &Mutex<()>,
    target_level: AcpApprovalLevel,
) -> Result<(), String> {
    let _mode_guard = mode_sync_lock.lock().unwrap_or_else(|p| p.into_inner());
    apply_approval_level_and_sync_native_mode_locked(transport, state, target_level)
}

/// Acquires `mode_sync_lock` FIRST, then re-reads the current `approval_level` and native mode
/// under the lock before re-syncing, preventing a stale drift thread from reverting a newer user
/// switch into `Auto`.
fn resync_native_mode_if_drifted(
    transport: &AcpTransport,
    state: &Mutex<HostState>,
    mode_sync_lock: &Mutex<()>,
    drifted_mode: &str,
) {
    let _mode_guard = mode_sync_lock.lock().unwrap_or_else(|p| p.into_inner());
    let target_level = {
        let s = state.lock().unwrap_or_else(|p| p.into_inner());
        let expected = s.in_flight_target_level.unwrap_or(s.approval_level);
        let still_in_auto = s.native_auto_active
            || s.current_mode
                .as_deref()
                .is_some_and(is_auto_or_bypass_mode_id);
        (expected != AcpApprovalLevel::Auto && still_in_auto).then_some(s.approval_level)
    };
    if let Some(level) = target_level {
        if let Err(err) = apply_approval_level_and_sync_native_mode_locked(transport, state, level)
        {
            eprintln!(
                "[Native] Warning: agent switched to native mode `{drifted_mode}` while level is {}: {err}",
                level.label()
            );
        }
    }
}

fn apply_approval_level_and_sync_native_mode_locked(
    transport: &AcpTransport,
    state: &Mutex<HostState>,
    target_level: AcpApprovalLevel,
) -> Result<(), String> {
    let (sid, current_mode, was_auto_or_bypass, target_mode, is_auto_target) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let auto_id = find_advertised_auto_mode_id(&s.available_modes);
        let default_id = s.default_native_mode.clone().or_else(|| {
            find_advertised_default_mode_id(&s.available_modes, s.current_mode.as_deref())
        });
        let was_auto = s.native_auto_active
            || s.current_mode
                .as_deref()
                .is_some_and(is_auto_or_bypass_mode_id);
        let (target, is_auto) = match target_level {
            AcpApprovalLevel::Auto => match (auto_id, default_id.clone()) {
                // Only enter native auto if we also know a safe non-auto mode to switch back to.
                (Some(id), Some(_)) => (Some(id), true),
                (_, default) => (default, false),
            },
            AcpApprovalLevel::Ask | AcpApprovalLevel::AutoEdits | AcpApprovalLevel::FullAccess => {
                (default_id, false)
            }
        };
        s.in_flight_target_level = Some(target_level);
        (
            s.native_session_id.clone(),
            s.current_mode.clone(),
            was_auto,
            target,
            is_auto,
        )
    };

    let Some(sid) = sid else {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.in_flight_target_level = None;
        s.approval_level = target_level;
        return Ok(());
    };

    let Some(target_mode) = target_mode else {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.in_flight_target_level = None;
        if was_auto_or_bypass && !is_auto_target {
            return Err(
                "Could not leave the agent's automatic mode because the agent did not advertise a safe non-automatic mode.".to_string(),
            );
        }
        s.approval_level = target_level;
        s.native_auto_active = false;
        return Ok(());
    };

    if current_mode.as_deref() == Some(target_mode.as_str()) {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.in_flight_target_level = None;
        s.approval_level = target_level;
        s.native_auto_active = target_level == AcpApprovalLevel::Auto && is_auto_target;
        return Ok(());
    }

    match transport.call(
        "session/set_mode",
        json!({ "sessionId": sid, "modeId": target_mode }),
        ACP_RPC_TIMEOUT,
    ) {
        Ok(_) => {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.in_flight_target_level = None;
            s.approval_level = target_level;
            s.current_mode = Some(target_mode);
            s.native_auto_active = target_level == AcpApprovalLevel::Auto && is_auto_target;
            Ok(())
        }
        Err(err) => {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            s.in_flight_target_level = None;
            if was_auto_or_bypass && !is_auto_target {
                s.native_auto_active = true;
                return Err(format!("Could not leave the agent's automatic mode: {err}"));
            }
            // Entering native Auto failed: stay safely on the non-auto native mode (behaving like
            // Ask) while recording the requested level.
            s.approval_level = target_level;
            s.native_auto_active = false;
            Ok(())
        }
    }
}

fn cancel_all_pending_permissions(transport: &AcpTransport, state: &Mutex<HostState>) {
    let drained: Vec<HostPendingPermission> = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        s.pending_permissions.drain(..).collect()
    };
    for entry in drained {
        let cancel_body = entry.cancel_response();
        let _ = transport.respond_to_agent(entry.perm.rpc_id, cancel_body);
    }
}

fn stop_child_gracefully(
    transport: &AcpTransport,
    child: &mut Child,
    lifetime: &mut super::child_lifetime::ChildLifetime,
    child_pid: &AtomicU32,
) {
    transport.close_stdin();
    let deadline = SystemTime::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                child_pid.store(0, Ordering::Release);
                lifetime.cleanup();
                return;
            }
            Ok(None) => {
                if SystemTime::now() >= deadline {
                    super::child_lifetime::force_kill_child(child);
                    child_pid.store(0, Ordering::Release);
                    lifetime.cleanup();
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                child_pid.store(0, Ordering::Release);
                lifetime.cleanup();
                return;
            }
        }
    }
}

pub(super) fn exec_interactive_cli(
    spec: &AcpAgentSpec,
    cli_command: Option<&str>,
    native_session_id: Option<&str>,
    approval_level: AcpApprovalLevel,
    read_only: bool,
    profile: &AcpLaunchProfile,
    cwd: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> i32 {
    let (envs, program, args) = build_cli_switch_argv_with_options(
        spec,
        cli_command,
        native_session_id,
        approval_level,
        read_only,
        profile,
        cwd,
        model,
        effort,
    );
    println!(
        "\r\nSwitching to interactive {} terminal...",
        spec.display_name
    );
    let _ = std::io::stdout().flush();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(&program)
            .env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
            .envs(envs)
            .args(&args)
            .exec();
        eprintln!("Failed to launch {program}: {err}");
        1
    }

    #[cfg(not(unix))]
    {
        match Command::new(&program)
            .env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
            .envs(envs)
            .args(&args)
            .status()
        {
            Ok(status) => status.code().unwrap_or(0),
            Err(err) => {
                eprintln!("Failed to launch {program}: {err}");
                1
            }
        }
    }
}

/// Entry point for `ghostex acp-host`.
pub fn run(args: &[String]) -> i32 {
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };
    let Some(spec) = spec_for_family(&parsed.family) else {
        eprintln!(
            "Agent `{}` does not support Native mode yet.",
            parsed.family
        );
        return 2;
    };
    let gx_paths = crate::paths::get_gxserver_paths(None);
    let _host_lock = match super::host_lock::acquire_for_current_session(&gx_paths) {
        super::host_lock::AcquireResult::NotApplicable => None,
        super::host_lock::AcquireResult::Acquired(lock) => Some(lock),
        super::host_lock::AcquireResult::AlreadyHeld {
            session_ref,
            owner_pid,
        } => {
            if let Some(secret_file) = parsed
                .channel_secret_file
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let _ = super::channel::read_and_remove_channel_secret_file(Path::new(secret_file));
            }
            if let Some(pid) = owner_pid.filter(|p| *p > 0) {
                eprintln!(
                    "Ghostex Native Host · {} · Another host (pid {pid}) is already running for {session_ref}; exiting.",
                    spec.display_name
                );
            } else {
                eprintln!(
                    "Ghostex Native Host · {} · Another host is already running for {session_ref}; exiting.",
                    spec.display_name
                );
            }
            return 1;
        }
        super::host_lock::AcquireResult::Unavailable { session_ref, error } => {
            eprintln!("[Native] Warning: could not acquire host lock for {session_ref}: {error}");
            None
        }
    };
    crate::agent_model_catalog::adopt_cached_copy(&gx_paths);
    if spec.family == "claude" {
        return super::driver_claude::run(spec, parsed);
    }
    if spec.family == "codex" {
        return super::driver_codex::run(spec, parsed);
    }
    if spec.family == "opencode" {
        return super::driver_opencode::run(spec, parsed);
    }
    if spec.family == "grok" {
        return super::driver_grok::run(spec, parsed);
    }
    if spec.family == "cursor" {
        return super::driver_cursor::run(spec, parsed);
    }
    if spec.family == "pi" {
        return super::driver_pi::run(spec, parsed);
    }
    let cwd_path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd = cwd_path.to_string_lossy().to_string();
    let writable_roots = canonicalize_writable_roots(&cwd_path, &parsed.writable_roots);
    let endpoint = GxserverEndpoint::from_env_or_secret_file(parsed.channel_secret_file.as_deref());
    let spool_session_id = endpoint
        .as_ref()
        .map(|ep| ep.session_id.clone())
        .unwrap_or_else(|| format!("local-{}", std::process::id()));

    println!("Ghostex Native Host · {} · Starting...", spec.display_name);
    let _ = std::io::stdout().flush();

    let gx_paths = crate::paths::get_gxserver_paths(None);
    let signal_guard = TerminationSignalGuard::install();

    let Some((acp_envs, prog, argv)) =
        build_acp_server_argv(spec, parsed.cli_command.as_deref(), &parsed.profile)
    else {
        let msg = format!("Invalid Native host command for {}", spec.family);
        eprintln!("{msg}");
        report_driver_startup_error(endpoint.as_ref(), spec.family, &msg, false);
        return 1;
    };
    let record_target = endpoint
        .as_ref()
        .map(super::child_lifetime::ChildRecordTarget::from_endpoint);
    let mut cmd = Command::new(&prog);
    super::child_lifetime::end_with_host(&mut cmd)
        .env_remove(super::channel::ACP_CHANNEL_SECRET_ENV)
        .envs(acp_envs)
        .args(&argv)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if spec.family == "omp" {
        // Disable Terminal-based Ghostex OMP hooks and chat channel inside the Native ACP child
        // so only `ghostex acp-host` owns `/api/agentChatChannel` and hook reporting.
        cmd.env("GHOSTEX_OMP_HOOKS_DISABLED", "1")
            .env("GHOSTEX_CHAT_CHANNEL_DISABLED", "1");
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            let msg = format!("Failed to start `{prog} {}`: {err}", argv.join(" "));
            eprintln!("{msg}");
            report_driver_startup_error(endpoint.as_ref(), spec.family, &msg, false);
            return 1;
        }
    };
    let mut child_lifetime =
        super::child_lifetime::ChildLifetime::attach(&child, record_target.as_ref());
    let child_pid = Arc::new(AtomicU32::new(child.id()));
    let Some(stdin) = child.stdin.take() else {
        let msg = "Failed to open stdin pipe to Native host.";
        eprintln!("{msg}");
        report_driver_startup_error(endpoint.as_ref(), spec.family, msg, false);
        super::child_lifetime::force_kill_child(&mut child);
        child_lifetime.cleanup();
        return 1;
    };
    let Some(stdout) = child.stdout.take() else {
        let msg = "Failed to open stdout pipe from Native host.";
        eprintln!("{msg}");
        report_driver_startup_error(endpoint.as_ref(), spec.family, msg, false);
        super::child_lifetime::force_kill_child(&mut child);
        child_lifetime.cleanup();
        return 1;
    };
    let stderr = child.stderr.take();

    let transport = Arc::new(AcpTransport::new(stdin, &spool_session_id));
    // Keep default (no `--approval-level`): Antigravity's own `toolPermission` names the level
    // its `--cli_dir_name ghostex-acp` host would otherwise hide (CDXC:NativeApprovals 2026-11-25
    // in `acp/cli_default.rs`); other agents start in the mode they report.
    let initial_explicit_level = parsed
        .approval_level
        .or_else(|| parsed.read_only.then_some(AcpApprovalLevel::Ask))
        .or_else(|| super::cli_default::cli_default_level(spec.family));
    let state = Arc::new(Mutex::new(HostState::new(
        spec.family,
        initial_explicit_level.unwrap_or(AcpApprovalLevel::Ask),
        parsed.read_only,
        cwd_path.clone(),
        writable_roots,
    )));
    let mode_sync_lock = Arc::new(Mutex::new(()));
    let activity =
        ActivityReporter::start(endpoint.clone(), state.clone(), spec.family.to_string());
    let shutting_down = Arc::new(AtomicBool::new(false));

    // Watch for SIGTERM/SIGHUP/SIGINT so the ACP subprocess is stopped promptly even while the
    // main thread is blocked in `/api/agentChatChannel` long-poll.
    #[cfg(unix)]
    {
        let t_sig = transport.clone();
        let pid_sig = child_pid.clone();
        let down_sig = shutting_down.clone();
        let ep_sig = endpoint.clone();
        let target_sig = record_target.clone();
        thread::spawn(move || {
            while !down_sig.load(Ordering::Acquire) {
                if SIGNAL_TERMINATE.load(Ordering::Acquire) {
                    down_sig.store(true, Ordering::Release);
                    t_sig.close_stdin();
                    let pid = pid_sig.swap(0, Ordering::AcqRel);
                    super::child_lifetime::terminate_child_on_host_signal(pid, target_sig.as_ref());
                    if let Some(ep) = ep_sig.as_ref() {
                        ep.close_channel();
                    }
                    std::process::exit(143);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    // Drain stderr into spool so pipe never blocks.
    let stderr_head: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    if let Some(stderr) = stderr {
        let t_err = transport.clone();
        let head = stderr_head.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    t_err.spool_event(&json!({ "stderr": trimmed }));
                    super::startup_error::record_stderr_head_line(&head, trimmed);
                }
            }
        });
    }

    // Stdout JSON-RPC reader loop (panic-safe so an unexpected message can never wedge `acp-host`).
    {
        let t_out = transport.clone();
        let st_out = state.clone();
        let mode_lock_out = mode_sync_lock.clone();
        let act_out = activity.clone();
        thread::spawn(move || {
            let _exit_guard = ReaderExitGuard(st_out.clone());
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let handle_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                        return;
                    };
                    t_out.spool_event(&msg);
                    {
                        let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                        s.last_event_at_ms = now_ms();
                    }
                    let has_method = msg.get("method").and_then(Value::as_str).is_some();
                    let has_id = msg.get("id").is_some();
                    if !has_method && has_id {
                        if let Some(id) = msg.get("id").and_then(Value::as_u64) {
                            if let Some(tx) = t_out
                                .pending
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .remove(&id)
                            {
                                let _ = tx.send(msg);
                            }
                        }
                        return;
                    }
                    if has_method && has_id {
                        let method = msg
                            .get("method")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let rpc_id = msg.get("id").cloned().unwrap_or(Value::Null);
                        let params = msg.get("params").cloned().unwrap_or(Value::Null);
                        if method == "session/request_permission" {
                            let mut perm = parse_permission_request(rpc_id, &params);
                            if !perm.is_question {
                                let (level, read_only, roots, session_cwd) = {
                                    let s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    (
                                        s.approval_level,
                                        s.read_only,
                                        s.writable_roots.clone(),
                                        s.cwd.clone(),
                                    )
                                };
                                let eval_level = if read_only {
                                    AcpApprovalLevel::Ask
                                } else {
                                    level
                                };
                                match evaluate_permission(
                                    eval_level,
                                    &params,
                                    &perm.options,
                                    &roots,
                                    &session_cwd,
                                ) {
                                    PermissionDecision::AutoApprove { option_id } => {
                                        println!(
                                            "\r\n[Native] Auto-approved ({}): {} ({})",
                                            eval_level.label(),
                                            perm.tool_name,
                                            perm.summary
                                        );
                                        let _ = std::io::stdout().flush();
                                        let _ = t_out.respond_to_agent(
                                            perm.rpc_id,
                                            json!({
                                                "outcome": {
                                                    "outcome": "selected",
                                                    "optionId": option_id,
                                                }
                                            }),
                                        );
                                        return;
                                    }
                                    PermissionDecision::AskUser { .. } if read_only => {
                                        println!(
                                            "\r\n[Native] Denied in read-only mode: {} ({})",
                                            perm.tool_name, perm.summary
                                        );
                                        let _ = std::io::stdout().flush();
                                        let outcome = if let Some(opt_id) =
                                            perm.resolve_option_id(None, "reject_once")
                                        {
                                            json!({ "outcome": "selected", "optionId": opt_id })
                                        } else {
                                            json!({ "outcome": "cancelled" })
                                        };
                                        let _ = t_out.respond_to_agent(
                                            perm.rpc_id,
                                            json!({ "outcome": outcome }),
                                        );
                                        return;
                                    }
                                    PermissionDecision::AskUser { reason } => {
                                        perm.reason = reason;
                                    }
                                }
                            }
                            println!(
                                "\r\n[Native] Waiting for {}: {} ({})",
                                if perm.is_question {
                                    "question answer"
                                } else {
                                    "approval"
                                },
                                perm.tool_name,
                                perm.summary
                            );
                            let _ = std::io::stdout().flush();
                            let was_empty = {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                let empty = s.pending_permissions.is_empty();
                                s.pending_permissions
                                    .push_back(HostPendingPermission::from_permission(perm));
                                empty
                            };
                            if was_empty {
                                act_out.sync();
                            }
                        } else if method == "elicitation/create" {
                            let is_omp =
                                st_out.lock().unwrap_or_else(|p| p.into_inner()).family == "omp";
                            let omp_parsed = if is_omp {
                                super::omp_elicitation::parse_elicitation_request(
                                    rpc_id.clone(),
                                    &params,
                                )
                            } else {
                                None
                            };
                            if let Some((perm, meta)) = omp_parsed {
                                println!(
                                    "\r\n[Native] Waiting for {}: {}",
                                    if perm.is_question {
                                        "question answer"
                                    } else {
                                        "approval"
                                    },
                                    perm.summary
                                );
                                let _ = std::io::stdout().flush();
                                let entry = HostPendingPermission::from_omp_elicitation(perm, meta);
                                let was_empty = {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    let empty = s.pending_permissions.is_empty();
                                    s.pending_permissions.push_back(entry);
                                    empty
                                };
                                if was_empty {
                                    act_out.sync();
                                }
                            } else if let Some(form) =
                                super::elicitation::parse_elicitation_request(&params)
                            {
                                let tool_use_id = format!("elicitation-{}", now_ms());
                                println!(
                                    "\r\n[Native] Waiting for question answer: {}",
                                    form.title()
                                );
                                let _ = std::io::stdout().flush();
                                let entry = HostPendingPermission::from_elicitation(
                                    rpc_id,
                                    tool_use_id,
                                    form,
                                );
                                let was_empty = {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    let empty = s.pending_permissions.is_empty();
                                    s.pending_permissions.push_back(entry);
                                    empty
                                };
                                if was_empty {
                                    act_out.sync();
                                }
                            } else {
                                let _ =
                                    t_out.respond_to_agent(rpc_id, json!({ "action": "decline" }));
                            }
                        } else {
                            t_out.respond_error_to_agent(
                                rpc_id,
                                -32601,
                                &format!("Unsupported client method: {method}"),
                            );
                        }
                        return;
                    }
                    // JSON-RPC notification from agent.
                    if msg.get("method").and_then(Value::as_str) == Some("$/cancel_request") {
                        if let Some(cancel_id) = msg.pointer("/params/requestId") {
                            let removed = {
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if let Some(idx) = s
                                    .pending_permissions
                                    .iter()
                                    .position(|p| p.perm.rpc_id == *cancel_id)
                                {
                                    s.pending_permissions.remove(idx);
                                    true
                                } else {
                                    false
                                }
                            };
                            if removed {
                                act_out.sync();
                            }
                        }
                        return;
                    }
                    if let Some(update) = msg.pointer("/params/update") {
                        let kind = update
                            .get("sessionUpdate")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        match kind {
                            "current_mode_update" => {
                                if let Some(mode_id) = update
                                    .get("currentModeId")
                                    .or_else(|| update.get("modeId"))
                                    .and_then(Value::as_str)
                                {
                                    let needs_drift_resync = {
                                        let mut s =
                                            st_out.lock().unwrap_or_else(|p| p.into_inner());
                                        s.current_mode = Some(mode_id.to_string());
                                        let expected =
                                            s.in_flight_target_level.unwrap_or(s.approval_level);
                                        if is_auto_or_bypass_mode_id(mode_id) {
                                            s.native_auto_active = true;
                                            expected != AcpApprovalLevel::Auto
                                        } else {
                                            s.native_auto_active = false;
                                            false
                                        }
                                    };
                                    if needs_drift_resync {
                                        // Re-sync off the stdout reader thread, acquiring
                                        // `mode_sync_lock` BEFORE reading the latest `approval_level`.
                                        let t_sync = t_out.clone();
                                        let st_sync = st_out.clone();
                                        let lock_sync = mode_lock_out.clone();
                                        let drifted = mode_id.to_string();
                                        thread::spawn(move || {
                                            resync_native_mode_if_drifted(
                                                &t_sync, &st_sync, &lock_sync, &drifted,
                                            );
                                        });
                                    }
                                }
                            }
                            "config_option_update" => {
                                if let Some(opts) = update.get("configOptions") {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    apply_config_options(&mut s, opts, true);
                                }
                            }
                            "agent_message_chunk" => {
                                if let Some(text) = update
                                    .pointer("/content/text")
                                    .or_else(|| update.get("text"))
                                    .and_then(Value::as_str)
                                    .filter(|t| !t.is_empty())
                                {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    if s.live_stream_started_at.is_none() {
                                        s.live_stream_started_at = Some(now_iso_rfc3339());
                                    }
                                    append_bounded(
                                        &mut s.live_stream_text,
                                        text,
                                        LIVE_STREAM_MAX_CHARS,
                                    );
                                    append_bounded(
                                        &mut s.turn_reply_text,
                                        text,
                                        LIVE_STREAM_MAX_CHARS,
                                    );
                                    s.live_tool_label = None;
                                    s.live_tool_detail = None;
                                    s.live_tool_started_at = None;
                                }
                            }
                            "agent_thought_chunk" => {
                                if let Some(text) = update
                                    .pointer("/content/text")
                                    .or_else(|| update.get("text"))
                                    .and_then(Value::as_str)
                                    .filter(|t| !t.is_empty())
                                {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    if s.live_thought_started_at.is_none() {
                                        s.live_thought_started_at = Some(now_iso_rfc3339());
                                    }
                                    append_bounded(
                                        &mut s.live_thought_text,
                                        text,
                                        LIVE_STREAM_MAX_CHARS,
                                    );
                                }
                            }
                            "tool_call" | "tool_call_update" => {
                                let status = update
                                    .get("status")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default();
                                let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                if kind == "tool_call" {
                                    end_live_segment!(s);
                                }
                                if matches!(status, "completed" | "failed" | "cancelled") {
                                    s.live_tool_label = None;
                                    s.live_tool_detail = None;
                                    s.live_tool_started_at = None;
                                } else {
                                    let label = update
                                        .get("title")
                                        .or_else(|| update.get("name"))
                                        .or_else(|| update.get("kind"))
                                        .and_then(Value::as_str)
                                        .map(str::trim)
                                        .filter(|t| !t.is_empty())
                                        .map(str::to_string);
                                    if let Some(label) = label {
                                        s.live_tool_label = Some(label);
                                    }
                                    if s.live_tool_started_at.is_none() {
                                        s.live_tool_started_at = Some(now_iso_rfc3339());
                                    }
                                    let detail = update
                                        .pointer("/locations/0/path")
                                        .or_else(|| update.pointer("/rawInput/command"))
                                        .or_else(|| update.pointer("/rawInput/CommandLine"))
                                        .or_else(|| update.pointer("/rawInput/file_path"))
                                        .or_else(|| update.pointer("/rawInput/path"))
                                        .or_else(|| update.pointer("/rawInput/TargetFile"))
                                        .and_then(Value::as_str)
                                        .map(str::trim)
                                        .filter(|t| !t.is_empty())
                                        .map(str::to_string);
                                    if detail.is_some() {
                                        s.live_tool_detail = detail;
                                    }
                                }
                            }
                            "usage_update" => {
                                let used = update.get("used").and_then(Value::as_u64);
                                let size = update.get("size").and_then(Value::as_u64);
                                if used.is_some() || size.is_some() {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    if let Some(u) = used {
                                        s.usage_used_tokens = Some(u);
                                    }
                                    if let Some(sz) = size {
                                        s.usage_window_size = Some(sz);
                                    }
                                }
                            }
                            "session_info_update" => {
                                if let Some(title) = update
                                    .get("title")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|t| !t.is_empty())
                                {
                                    let mut s = st_out.lock().unwrap_or_else(|p| p.into_inner());
                                    s.session_title = Some(title.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }));
                if handle_res.is_err() {
                    eprintln!(
                        "[Native] Warning: recovered from panic while handling host stdout message."
                    );
                }
            }
        });
    }

    // 1. Initialize ACP protocol.
    let init_params = json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false,
            "elicitation": { "form": {} },
        },
        "clientInfo": crate::acp::client_info(),
    });
    let init_result = match transport.call("initialize", init_params, ACP_RPC_TIMEOUT) {
        Ok(res) => res,
        Err(err) => {
            let exited = child.try_wait().ok().flatten();
            let agent_name = super::startup_error::agent_label(spec.family);
            let message = super::startup_error::startup_failure_message(
                agent_name,
                &err,
                exited,
                &stderr_head,
            );
            eprintln!("Native host initialize failed: {message}");
            report_driver_startup_error(
                endpoint.as_ref(),
                spec.family,
                &message,
                super::startup_error::looks_like_auth_error(&message),
            );
            stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
            return 1;
        }
    };
    if spec.family == "antigravity" {
        if let Some(refusal) = super::startup_error::antigravity_too_old(&init_result) {
            eprintln!("Native host refused agy: {refusal}");
            report_driver_startup_error(endpoint.as_ref(), spec.family, &refusal, false);
            stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
            return 1;
        }
    }
    let supports_steering = init_result
        .pointer("/_meta/steering/supported")
        .and_then(Value::as_bool)
        == Some(true);
    let supports_session_resume = init_result
        .pointer("/agentCapabilities/sessionCapabilities/resume")
        .is_some_and(|v| !v.is_null() && v.as_bool() != Some(false));

    // 2. Open or resume conversation.
    let session_result = if let Some(resume_id) = parsed.resume_session_id.as_deref() {
        let load_params = json!({
            "sessionId": resume_id,
            "cwd": cwd,
            "mcpServers": [],
        });
        let attach_id = |mut res: Value| {
            if res.get("sessionId").is_none() {
                if let Some(obj) = res.as_object_mut() {
                    obj.insert("sessionId".to_string(), json!(resume_id));
                }
            }
            res
        };
        if supports_session_resume {
            match transport.call("session/resume", load_params.clone(), ACP_RPC_TIMEOUT) {
                Ok(res) => Ok(attach_id(res)),
                Err(_) => transport
                    .call("session/load", load_params, ACP_LOAD_TIMEOUT)
                    .map(attach_id),
            }
        } else {
            match transport.call("session/load", load_params.clone(), ACP_LOAD_TIMEOUT) {
                Ok(res) => Ok(attach_id(res)),
                Err(_) => transport
                    .call("session/resume", load_params, ACP_RPC_TIMEOUT)
                    .map(attach_id),
            }
        }
    } else {
        transport.call(
            "session/new",
            json!({ "cwd": cwd, "mcpServers": [] }),
            ACP_RPC_TIMEOUT,
        )
    };

    let session_result = match session_result {
        Ok(res) => res,
        Err(err) => {
            eprintln!("Native session open failed: {err}");
            report_driver_startup_error(
                endpoint.as_ref(),
                spec.family,
                &err,
                super::startup_error::looks_like_auth_error(&err),
            );
            stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
            return 1;
        }
    };

    let (native_session_id, has_plan_mode) = {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        apply_session_metadata(&mut s, &session_result);
        // Clear any history chunks replayed during `session/load` so liveActivity stays clean.
        s.clear_live_turn_output();
        s.default_native_mode =
            find_advertised_default_mode_id(&s.available_modes, s.current_mode.as_deref());
        let has_plan = s.available_modes.as_array().is_some_and(|modes| {
            modes.iter().any(|m| {
                m.get("id")
                    .or_else(|| m.get("modeId"))
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.eq_ignore_ascii_case("plan"))
            })
        });
        (s.native_session_id.clone(), has_plan)
    };

    if parsed.read_only && has_plan_mode {
        if let Some(sid) = native_session_id.as_deref() {
            if transport
                .call(
                    "session/set_mode",
                    json!({ "sessionId": sid, "modeId": "plan" }),
                    ACP_RPC_TIMEOUT,
                )
                .is_ok()
            {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                s.current_mode = Some("plan".to_string());
                s.approval_level = AcpApprovalLevel::Ask;
                s.native_auto_active = false;
            }
        }
    } else if let Some(target_level) = initial_explicit_level {
        if let Err(err) = apply_approval_level_and_sync_native_mode(
            &transport,
            &state,
            &mode_sync_lock,
            target_level,
        ) {
            eprintln!("[Native] Warning: could not sync initial native mode: {err}");
        }
    } else {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        let detected = s
            .current_mode
            .as_deref()
            .and_then(AcpApprovalLevel::parse)
            .unwrap_or(AcpApprovalLevel::Ask);
        s.approval_level = detected;
        s.native_auto_active = detected == AcpApprovalLevel::Auto;
    }

    {
        let (avail, existing_model, configs, current_is_concrete) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            let concrete = s.current_model.as_deref().is_some_and(|m| {
                !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m)
            });
            (
                s.available_models.clone(),
                s.current_model.clone(),
                s.config_options.clone(),
                concrete,
            )
        };
        let requested_model = parsed
            .model
            .as_deref()
            .filter(|m| !ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(m));
        let target = if requested_model.is_some() || !current_is_concrete {
            resolve_acp_target_model_id(
                spec.family,
                &avail,
                existing_model.as_deref(),
                requested_model,
                parsed.effort.as_deref(),
            )
        } else {
            None
        };
        if let Some((target_id, resolved_effort)) = target {
            if let Some(sid) = native_session_id.as_deref() {
                let already_same = existing_model
                    .as_deref()
                    .is_some_and(|cur| cur == target_id);
                let model_cfg_id = super::omp_elicitation::find_model_config_id(&configs);
                let set_res = if already_same {
                    Ok(Value::Null)
                } else if let Some(cfg_id) = model_cfg_id {
                    transport.call(
                        "session/set_config_option",
                        json!({ "sessionId": sid, "configId": cfg_id, "value": target_id }),
                        ACP_RPC_TIMEOUT,
                    )
                } else {
                    transport.call(
                        "session/set_model",
                        json!({ "sessionId": sid, "modelId": target_id }),
                        ACP_RPC_TIMEOUT,
                    )
                };
                if let Ok(res) = set_res {
                    let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                    if let Some(updated_configs) = res.get("configOptions") {
                        apply_config_options(&mut s, updated_configs, true);
                    }
                    s.current_model = Some(target_id);
                    if let Some(eff) = resolved_effort {
                        s.current_effort = Some(eff);
                    }
                }
            }
        }
    }
    if let Some(effort) = parsed
        .effort
        .as_deref()
        .filter(|e| !ghostex_gx_chat_core::menus::catalog::is_placeholder_effort_value(e))
    {
        let effort_cfg_id = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            super::omp_elicitation::find_effort_config_id(&s.config_options)
                .or_else(|| find_effort_config_option_id(&s.config_options))
        };
        if let (Some(sid), Some(cfg_id)) = (native_session_id.as_deref(), effort_cfg_id) {
            if let Ok(resp) = transport.call(
                "session/set_config_option",
                json!({
                    "sessionId": sid,
                    "configId": cfg_id,
                    "value": effort,
                }),
                ACP_RPC_TIMEOUT,
            ) {
                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(opts) = resp.get("configOptions") {
                    apply_config_options(&mut s, opts, false);
                }
                s.current_effort = Some(effort.to_string());
            }
        } else {
            let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
            if s.current_effort.is_none() {
                s.current_effort = Some(effort.to_string());
            }
        }
    }
    {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        if s.current_effort.is_none() {
            if let Some((_, _, Some(default_effort))) =
                crate::agent_model_catalog::resolve_acp_model(
                    spec.family,
                    s.current_model.as_deref(),
                    None,
                    s.available_models.as_array().map(Vec::as_slice),
                )
            {
                s.current_effort = Some(default_effort);
            }
        }
    }

    println!(
        "Ghostex Native Host · {} · Ready (conversation: {})",
        spec.display_name,
        native_session_id.as_deref().unwrap_or("pending")
    );
    println!("Use Chat View to talk with this agent, or switch to Terminal to open Terminal mode.");
    let _ = std::io::stdout().flush();

    activity.sync();

    // 3. Main command poll loop against `/api/agentChatChannel`.
    // Survives every `gxserver` reload cleanly without restarting the ACP subprocess.
    let mut switch_to_cli_requested = false;
    let mut caps = vec![
        "acp",
        "send",
        "abort",
        "approve",
        "answer_question",
        "model",
        "mode",
        "state",
        "switch_to_cli",
    ];
    if spec.supports_mid_turn_prompt {
        caps.push("mid_turn_send");
    }
    if supports_steering {
        caps.push("steer");
    }
    let capabilities = json!(caps);
    let channel_agent = format!("acp:{}", spec.family);
    let mut reconnecting = false;

    loop {
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        if state.lock().unwrap_or_else(|p| p.into_inner()).child_exited {
            println!("\r\n[Native] Agent process exited.");
            break;
        }
        let Some(ep) = endpoint.as_ref() else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let mut poll_params = json!({
            "action": "poll",
            "sessionRef": ep.session_ref,
            "agent": channel_agent,
            "pid": std::process::id(),
            "instance": ep.instance,
            "version": 1,
            "capabilities": capabilities,
        });
        if let Some(secret) = ep.channel_secret.as_deref() {
            poll_params["channelSecret"] = json!(secret);
        }
        let poll_body = json!({
            "requestId": format!("acp-poll-{}", now_ms()),
            "params": poll_params,
        });
        let resp = match ep.post_json("/api/agentChatChannel", poll_body, Duration::from_secs(28)) {
            Ok(resp) => {
                let newly_registered = resp
                    .pointer("/result/newlyRegistered")
                    .or_else(|| resp.get("newlyRegistered"))
                    .and_then(Value::as_bool)
                    == Some(true);
                let mut had_desired_approval = false;
                // Only pushed once by gxserver on channel open/reconnect (`newly_registered`).
                if let Some(desired) = resp
                    .pointer("/result/desiredApprovalLevel")
                    .or_else(|| resp.get("desiredApprovalLevel"))
                    .and_then(Value::as_str)
                    .and_then(AcpApprovalLevel::parse)
                {
                    had_desired_approval = true;
                    let current = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .approval_level;
                    if desired != current {
                        if let Err(err) = apply_approval_level_and_sync_native_mode(
                            &transport,
                            &state,
                            &mode_sync_lock,
                            desired,
                        ) {
                            eprintln!(
                                "[Native] Warning: failed to sync persisted approval level {}: {err}",
                                desired.label()
                            );
                        }
                    }
                }
                if reconnecting || newly_registered || had_desired_approval {
                    reconnecting = false;
                    // Re-sync live state after a gxserver reload or fresh channel open.
                    activity.sync();
                }
                resp
            }
            Err(_) => {
                reconnecting = true;
                thread::sleep(POLL_RETRY_DELAY);
                continue;
            }
        };
        let commands = resp
            .pointer("/result/commands")
            .or_else(|| resp.get("commands"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for cmd in commands {
            let id = cmd.get("id").and_then(Value::as_str).unwrap_or_default();
            let kind = cmd.get("kind").and_then(Value::as_str).unwrap_or_default();
            let params = cmd.get("params").cloned().unwrap_or(Value::Null);
            match kind {
                "state" => {
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "abort" => {
                    let sid = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if s.working {
                            s.last_turn_interrupted = true;
                        }
                        s.native_session_id.clone()
                    };
                    if let Some(sid) = sid {
                        let _ = transport.notify("session/cancel", json!({ "sessionId": sid }));
                    }
                    cancel_all_pending_permissions(&transport, &state);
                    let wait_until = SystemTime::now() + Duration::from_secs(2);
                    while SystemTime::now() < wait_until {
                        if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                    if state.lock().unwrap_or_else(|p| p.into_inner()).working {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = false;
                        s.clear_live_turn_output();
                    }
                    activity.sync();
                    ep.ack(id, true, json!({ "aborted": true }), None);
                }
                "approve" => {
                    let decision = params
                        .get("decision")
                        .and_then(Value::as_str)
                        .unwrap_or("allow");
                    let explicit = params.get("optionId").and_then(Value::as_str);
                    let target_tool_use_id = params
                        .get("toolUseId")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let entry = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_permissions
                                .iter()
                                .position(|p| {
                                    !p.perm.is_question
                                        && p.perm.tool_use_id.as_deref() == Some(wanted_id)
                                })
                                .and_then(|idx| s.pending_permissions.remove(idx))
                        } else {
                            s.pending_permissions
                                .iter()
                                .position(|p| !p.perm.is_question)
                                .and_then(|idx| s.pending_permissions.remove(idx))
                        }
                    };
                    let Some(entry) = entry else {
                        let msg = if target_tool_use_id.is_some() {
                            "That Native approval request is no longer pending."
                        } else {
                            "No pending Native approval."
                        };
                        ep.ack(id, false, Value::Null, Some(msg));
                        continue;
                    };
                    let response_payload = if entry.omp_elicitation.is_some() {
                        super::omp_elicitation::build_confirm_elicitation_response(
                            explicit.unwrap_or(decision),
                        )
                    } else {
                        let outcome = if decision.eq_ignore_ascii_case("escape")
                            || decision.eq_ignore_ascii_case("cancel")
                        {
                            json!({ "outcome": "cancelled" })
                        } else if let Some(opt_id) =
                            entry.perm.resolve_option_id(explicit, decision)
                        {
                            json!({ "outcome": "selected", "optionId": opt_id })
                        } else {
                            json!({ "outcome": "cancelled" })
                        };
                        json!({ "outcome": outcome })
                    };
                    match transport.respond_to_agent(entry.perm.rpc_id, response_payload) {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "answer_question" => {
                    let selections = params
                        .get("selections")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let target_tool_use_id = params
                        .get("toolUseId")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    let entry = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        if let Some(wanted_id) = target_tool_use_id {
                            s.pending_permissions
                                .iter()
                                .position(|p| {
                                    p.perm.is_question
                                        && p.perm.tool_use_id.as_deref() == Some(wanted_id)
                                })
                                .and_then(|idx| s.pending_permissions.remove(idx))
                        } else {
                            s.pending_permissions
                                .iter()
                                .position(|p| p.perm.is_question)
                                .and_then(|idx| s.pending_permissions.remove(idx))
                        }
                    };
                    let Some(entry) = entry else {
                        ep.ack(id, false, Value::Null, Some("NO_PENDING_QUESTION"));
                        continue;
                    };
                    let response_body = if let Some(meta) = entry.omp_elicitation {
                        super::omp_elicitation::build_question_elicitation_response(
                            &meta,
                            &selections,
                        )
                    } else if let Some(form) = entry.elicitation {
                        form.build_response(&selections)
                    } else {
                        let outcome = if let Some(opt_id) =
                            entry.perm.resolve_question_option_id(&selections)
                        {
                            json!({ "outcome": "selected", "optionId": opt_id })
                        } else {
                            json!({ "outcome": "cancelled" })
                        };
                        json!({ "outcome": outcome })
                    };
                    match transport.respond_to_agent(entry.perm.rpc_id, response_body) {
                        Ok(()) => {
                            activity.sync();
                            ep.ack(id, true, json!({ "answered": true }), None);
                        }
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "model" => {
                    let model = params
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let effort = params
                        .get("effort")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let (sid, avail, configs, existing_model) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.native_session_id.clone(),
                            s.available_models.clone(),
                            s.config_options.clone(),
                            s.current_model.clone(),
                        )
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Native session not initialized yet."),
                        );
                        continue;
                    };
                    let base_for_antigravity = if model.is_empty() && spec.family == "antigravity" {
                        existing_model.as_deref().map(|cur| {
                            crate::agent_model_catalog::split_antigravity_model_id(cur)
                                .map(|(base, _)| base)
                                .unwrap_or_else(|| cur.to_string())
                        })
                    } else {
                        None
                    };
                    let effective_model = if !model.is_empty() {
                        Some(model.clone())
                    } else {
                        base_for_antigravity
                    };
                    let target_model = effective_model.as_deref().and_then(|m| {
                        resolve_acp_target_model_id(
                            spec.family,
                            &avail,
                            existing_model.as_deref(),
                            Some(m),
                            (!effort.is_empty()).then_some(effort.as_str()),
                        )
                    });
                    // Run `session/set_model` + `session/set_config_option` off the poll loop
                    // thread so an agent that requests confirmation via `session/request_permission`
                    // (such as Hermes guarded model switches) cannot deadlock the poll loop.
                    let t_model = transport.clone();
                    let st_model = state.clone();
                    let ep_model = ep.clone();
                    let cmd_id = id.to_string();
                    thread::spawn(move || {
                        if let Some((target_id, resolved_effort)) = target_model {
                            let same_model = existing_model
                                .as_deref()
                                .is_some_and(|cur| cur == target_id);
                            if !same_model {
                                let model_cfg_id =
                                    super::omp_elicitation::find_model_config_id(&configs);
                                let call_res = if let Some(cfg_id) = model_cfg_id {
                                    t_model.call(
                                        "session/set_config_option",
                                        json!({
                                            "sessionId": sid,
                                            "configId": cfg_id,
                                            "value": target_id,
                                        }),
                                        ACP_RPC_TIMEOUT,
                                    )
                                } else {
                                    t_model.call(
                                        "session/set_model",
                                        json!({ "sessionId": sid, "modelId": target_id }),
                                        ACP_RPC_TIMEOUT,
                                    )
                                };
                                match call_res {
                                    Ok(resp) => {
                                        if let Some(opts) = resp.get("configOptions") {
                                            let mut s =
                                                st_model.lock().unwrap_or_else(|p| p.into_inner());
                                            apply_config_options(&mut s, opts, true);
                                        }
                                    }
                                    Err(err) => {
                                        ep_model.ack(&cmd_id, false, Value::Null, Some(&err));
                                        return;
                                    }
                                }
                            }
                            let mut s = st_model.lock().unwrap_or_else(|p| p.into_inner());
                            s.current_model = Some(target_id);
                            if let Some(eff) = resolved_effort {
                                s.current_effort = Some(eff);
                            }
                        }
                        if !effort.is_empty() {
                            let effort_cfg_id = {
                                let s = st_model.lock().unwrap_or_else(|p| p.into_inner());
                                super::omp_elicitation::find_effort_config_id(&s.config_options)
                                    .or_else(|| find_effort_config_option_id(&s.config_options))
                            };
                            if let Some(cfg_id) = effort_cfg_id {
                                match t_model.call(
                                    "session/set_config_option",
                                    json!({
                                        "sessionId": sid,
                                        "configId": cfg_id,
                                        "value": effort,
                                    }),
                                    ACP_RPC_TIMEOUT,
                                ) {
                                    Ok(resp) => {
                                        let mut s =
                                            st_model.lock().unwrap_or_else(|p| p.into_inner());
                                        if let Some(opts) = resp.get("configOptions") {
                                            apply_config_options(&mut s, opts, false);
                                        }
                                        s.current_effort = Some(effort);
                                    }
                                    Err(err) => {
                                        ep_model.ack(&cmd_id, false, Value::Null, Some(&err));
                                        return;
                                    }
                                }
                            } else {
                                st_model
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .current_effort = Some(effort);
                            }
                        }
                        let snap = st_model
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .snapshot_json();
                        ep_model.ack(&cmd_id, true, snap, None);
                    });
                }
                "mode" => {
                    let mode = params
                        .get("mode")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let Some(level) = AcpApprovalLevel::parse(mode) else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some(&format!("Unsupported approval level: {mode}")),
                        );
                        continue;
                    };
                    if let Err(err) = apply_approval_level_and_sync_native_mode(
                        &transport,
                        &state,
                        &mode_sync_lock,
                        level,
                    ) {
                        ep.ack(id, false, Value::Null, Some(&err));
                        continue;
                    }
                    let snap = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .snapshot_json();
                    ep.ack(id, true, snap, None);
                }
                "steer" => {
                    let text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let sid = state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .native_session_id
                        .clone();
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Native session not initialized yet."),
                        );
                        continue;
                    };
                    if !supports_steering {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Steering is not supported by this ACP agent."),
                        );
                        continue;
                    }
                    let blocks = vec![json!({ "type": "text", "text": text })];
                    match transport.call(
                        "_session/steering",
                        json!({ "sessionId": sid, "prompt": blocks }),
                        ACP_RPC_TIMEOUT,
                    ) {
                        Ok(_) => ep.ack(
                            id,
                            true,
                            json!({ "accepted": true, "midTurn": true, "sessionId": sid }),
                            None,
                        ),
                        Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                    }
                }
                "send" => {
                    let text = params
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let images: Vec<String> = params
                        .get("images")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    let interrupt = params
                        .get("interrupt")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let (is_working, has_pending_perm, sid) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        (
                            s.working,
                            !s.pending_permissions.is_empty(),
                            s.native_session_id.clone(),
                        )
                    };
                    let Some(sid) = sid else {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("Native session not initialized yet."),
                        );
                        continue;
                    };
                    if !interrupt && has_pending_perm {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_WAITING_INPUT"));
                        continue;
                    }
                    if is_working && !interrupt && !spec.supports_mid_turn_prompt {
                        ep.ack(id, false, Value::Null, Some("ACP_AGENT_BUSY"));
                        continue;
                    }
                    let blocks = match build_prompt_blocks(&gx_paths, &text, &images) {
                        Ok(blocks) => blocks,
                        Err(err) => {
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };
                    let is_mid_turn = is_working && !interrupt;
                    if is_mid_turn && supports_steering {
                        // Agents advertising `_meta.steering.supported` (such as OMP) cancel the
                        // active turn if a second `session/prompt` arrives mid-turn; route mid-turn
                        // sends through `_session/steering` instead.
                        match transport.call(
                            "_session/steering",
                            json!({ "sessionId": sid, "prompt": blocks }),
                            ACP_RPC_TIMEOUT,
                        ) {
                            Ok(_) => {
                                println!("\r\n[Native] Delivered mid-turn steering message...");
                                let _ = std::io::stdout().flush();
                                ep.ack(
                                    id,
                                    true,
                                    json!({
                                        "accepted": true,
                                        "midTurn": true,
                                        "sessionId": sid,
                                    }),
                                    None,
                                );
                            }
                            Err(err) => ep.ack(id, false, Value::Null, Some(&err)),
                        }
                        continue;
                    }
                    if is_working && interrupt {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .last_turn_interrupted = true;
                        cancel_all_pending_permissions(&transport, &state);
                        let _ = transport.notify("session/cancel", json!({ "sessionId": sid }));
                        let wait_until = SystemTime::now() + Duration::from_secs(4);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    if !is_mid_turn {
                        cancel_all_pending_permissions(&transport, &state);
                    }
                    // Bump `turn_generation` AND write `session/prompt` to stdin synchronously on
                    // the poll loop thread before acking, so rapid mid-turn sends preserve strict
                    // FIFO wire order and any superseded `session/prompt` response from the agent
                    // sees a stale generation instead of clearing `working`.
                    let (prev_working, prev_gen, turn_gen) = {
                        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let prev_working = s.working;
                        let prev_gen = s.turn_generation;
                        s.turn_generation = s.turn_generation.wrapping_add(1);
                        s.working = true;
                        s.last_turn_interrupted = false;
                        if !is_mid_turn {
                            s.clear_live_turn_output();
                        }
                        (prev_working, prev_gen, s.turn_generation)
                    };
                    let (rpc_id, rx) = match transport.start_call(
                        "session/prompt",
                        json!({
                            "sessionId": sid,
                            "prompt": blocks,
                        }),
                    ) {
                        Ok(pair) => pair,
                        Err(err) => {
                            {
                                let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
                                if s.turn_generation == turn_gen {
                                    s.turn_generation = prev_gen;
                                    s.working = prev_working;
                                }
                            }
                            activity.sync();
                            ep.ack(id, false, Value::Null, Some(&err));
                            continue;
                        }
                    };
                    if is_mid_turn {
                        println!("\r\n[Native] Delivered mid-turn message...");
                    } else {
                        println!("\r\n[Native] Turn started...");
                        activity.sync();
                    }
                    let _ = std::io::stdout().flush();
                    ep.ack(
                        id,
                        true,
                        json!({
                            "accepted": true,
                            "midTurn": is_mid_turn,
                            "sessionId": sid,
                        }),
                        None,
                    );

                    let t_turn = transport.clone();
                    let st_turn = state.clone();
                    let act_turn = activity.clone();
                    let turn_family = spec.family;
                    let turn_sid = sid.clone();
                    let turn_prompt_text = text.clone();
                    thread::spawn(move || {
                        let res = t_turn.finish_call(
                            rpc_id,
                            "session/prompt",
                            rx,
                            Duration::from_secs(60 * 60),
                        );
                        let stop_reason = match &res {
                            Ok(val) => {
                                if let Some(total) = val
                                    .pointer("/usage/totalTokens")
                                    .or_else(|| val.pointer("/usage/inputTokens"))
                                    .and_then(Value::as_u64)
                                    .filter(|u| *u > 0)
                                {
                                    let mut s = st_turn.lock().unwrap_or_else(|p| p.into_inner());
                                    if s.usage_used_tokens.is_none() {
                                        s.usage_used_tokens = Some(total);
                                    }
                                }
                                val.get("stopReason")
                                    .and_then(Value::as_str)
                                    .unwrap_or("end_turn")
                                    .to_string()
                            }
                            Err(err) => format!("error: {err}"),
                        };
                        let was_cancelled = matches!(
                            stop_reason.to_ascii_lowercase().as_str(),
                            "cancelled" | "canceled" | "interrupted" | "aborted"
                        );
                        let (is_current_turn, orphaned_perms, streamed_reply) = {
                            let mut s = st_turn.lock().unwrap_or_else(|p| p.into_inner());
                            if s.turn_generation == turn_gen {
                                let reply = s.turn_reply_text.clone();
                                s.working = false;
                                if was_cancelled {
                                    s.last_turn_interrupted = true;
                                }
                                s.clear_live_turn_output();
                                let drained: Vec<HostPendingPermission> =
                                    s.pending_permissions.drain(..).collect();
                                (true, drained, reply)
                            } else {
                                (false, Vec::new(), String::new())
                            }
                        };
                        for entry in orphaned_perms {
                            let cancel_body = entry.cancel_response();
                            let _ = t_turn.respond_to_agent(entry.perm.rpc_id, cancel_body);
                        }
                        if is_current_turn
                            && !was_cancelled
                            && turn_prompt_text.trim_start().starts_with('/')
                            && !streamed_reply.trim().is_empty()
                        {
                            match turn_family {
                                "hermes" => {
                                    crate::session_chat_hermes::append_native_slash_exchange(
                                        &turn_sid,
                                        &turn_prompt_text,
                                        &streamed_reply,
                                    );
                                }
                                "omp" => {
                                    crate::session_chat_omp_local::append_native_slash_exchange(
                                        &turn_sid,
                                        &turn_prompt_text,
                                        &streamed_reply,
                                    );
                                }
                                _ => {}
                            }
                        }
                        if is_current_turn {
                            println!("[Native] Turn finished ({stop_reason}).");
                            let _ = std::io::stdout().flush();
                            act_turn.sync();
                        }
                    });
                }
                "switch_to_cli" => {
                    let interrupt = params
                        .get("interrupt")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    let (is_working, has_pending_perm, sid, carry_model, carry_effort) = {
                        let s = state.lock().unwrap_or_else(|p| p.into_inner());
                        let (m, e) = match s.current_model.as_deref() {
                            Some(raw) if spec.family == "antigravity" => {
                                if let Some((base, eff)) =
                                    crate::agent_model_catalog::split_antigravity_model_id(raw)
                                {
                                    (Some(base), s.current_effort.clone().or(Some(eff)))
                                } else {
                                    (Some(raw.to_string()), s.current_effort.clone())
                                }
                            }
                            Some(raw) if spec.family == "hermes" => (
                                Some(super::hermes::strip_hermes_provider_prefix(raw).to_string()),
                                s.current_effort
                                    .clone()
                                    .filter(|e| !e.eq_ignore_ascii_case("default")),
                            ),
                            Some(raw) => (Some(raw.to_string()), s.current_effort.clone()),
                            None => (None, s.current_effort.clone()),
                        };
                        (
                            s.working,
                            !s.pending_permissions.is_empty(),
                            s.native_session_id.clone(),
                            m,
                            e,
                        )
                    };
                    if (is_working || has_pending_perm) && !interrupt {
                        ep.ack(
                            id,
                            false,
                            Value::Null,
                            Some("The agent is currently working on a turn. Interrupt it or wait for it to finish before switching to Terminal."),
                        );
                        continue;
                    }
                    cancel_all_pending_permissions(&transport, &state);
                    if is_working {
                        state
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .last_turn_interrupted = true;
                        if let Some(sid_ref) = sid.as_deref() {
                            let _ =
                                transport.notify("session/cancel", json!({ "sessionId": sid_ref }));
                        }
                        let wait_until = SystemTime::now() + Duration::from_millis(1500);
                        while SystemTime::now() < wait_until {
                            if !state.lock().unwrap_or_else(|p| p.into_inner()).working {
                                break;
                            }
                            thread::sleep(Duration::from_millis(50));
                        }
                    }
                    shutting_down.store(true, Ordering::Release);
                    stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
                    ep.ack(
                        id,
                        true,
                        json!({
                            "switched": true,
                            "nativeSessionId": sid,
                            "model": carry_model,
                            "effort": carry_effort,
                        }),
                        None,
                    );
                    ep.close_channel();
                    switch_to_cli_requested = true;
                    break;
                }
                other => {
                    ep.ack(
                        id,
                        false,
                        Value::Null,
                        Some(&format!("Unsupported Native host command: {other}")),
                    );
                }
            }
        }
    }

    if switch_to_cli_requested {
        drop(signal_guard);
        let (sid, approval_level, carry_model, carry_effort) = {
            let s = state.lock().unwrap_or_else(|p| p.into_inner());
            let (m, e) = match s.current_model.as_deref() {
                Some(raw) if spec.family == "antigravity" => {
                    if let Some((base, eff)) =
                        crate::agent_model_catalog::split_antigravity_model_id(raw)
                    {
                        (Some(base), s.current_effort.clone().or(Some(eff)))
                    } else {
                        (Some(raw.to_string()), s.current_effort.clone())
                    }
                }
                Some(raw) if spec.family == "hermes" => (
                    Some(super::hermes::strip_hermes_provider_prefix(raw).to_string()),
                    s.current_effort
                        .clone()
                        .filter(|e| !e.eq_ignore_ascii_case("default")),
                ),
                Some(raw) => (Some(raw.to_string()), s.current_effort.clone()),
                None => (None, s.current_effort.clone()),
            };
            (s.native_session_id.clone(), s.approval_level, m, e)
        };
        return exec_interactive_cli(
            spec,
            parsed.cli_command.as_deref(),
            sid.as_deref(),
            approval_level,
            parsed.read_only,
            &parsed.profile,
            Some(&cwd),
            carry_model.as_deref(),
            carry_effort.as_deref(),
        );
    }

    shutting_down.store(true, Ordering::Release);
    stop_child_gracefully(&transport, &mut child, &mut child_lifetime, &child_pid);
    if let Some(ep) = endpoint.as_ref() {
        ep.close_channel();
    }
    drop(signal_guard);
    0
}
