use std::sync::atomic::{AtomicU64, Ordering};

/// CDXC:SessionIdentity 2026-11-05 WHY:
/// Issue #418: an OpenCode chat could not send until the TUI plugin had created a conversation and
/// reported it, so a session nobody opened in a terminal answered every message with "Open an
/// OpenCode conversation in the terminal first". OpenCode 2's `--session <id>` continues that
/// conversation or creates it, and its service creates one with a chosen id, so a new local
/// OpenCode session gets its id from Ghostex, as Pi does (pi_session_id.rs): it is stored as the
/// session's `agentSessionId` when the row is created, the launch runs `opencode --session <id>`,
/// and the chat's first send creates the same conversation through the service when the terminal
/// has not (`session_chat_opencode::ensure_conversation`). A command that already chooses a
/// conversation or runs a subcommand, and an AgentBox launch, keep launching as before.
/// SEE-ALSO: create_agent_session_params_for_project in server/src/agents/launch_plan.rs, server/src/session_chat_opencode/client.rs.
pub(crate) fn mint_opencode_session_id(command: &str) -> Option<String> {
    // Unit tests assert exact commands, which must not depend on the CLIs installed on the machine.
    if cfg!(test) || command_chooses_opencode_session(command) {
        return None;
    }
    let home = crate::resume_lookup::home_dir();
    crate::agent_cli::opencode::launch_executable(&home).ok()?;
    Some(new_opencode_session_id())
}

/// `command` with OpenCode told which conversation to open.
pub(crate) fn with_opencode_session_id(command: &str, id: &str) -> String {
    format!("{} --session {id}", command.trim_end())
}

/// An id in OpenCode's own shape (`ses_`, 12 hex digits of the inverted creation time, 14 base62
/// characters), so OpenCode lists the conversation in its place among the others: the hex part is
/// `!(milliseconds * 4096 + counter)`, which makes newer conversations sort first.
pub(crate) fn new_opencode_session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let millis = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed) % 0x1000;
    let time = !(millis.wrapping_mul(0x1000).wrapping_add(counter)) & 0xffff_ffff_ffff;
    let random: String = uuid::Uuid::new_v4().as_bytes()[..14]
        .iter()
        .map(|byte| BASE62[usize::from(*byte) % BASE62.len()] as char)
        .collect();
    format!("ses_{time:012x}{random}")
}

/// Whether the command already picks a conversation (`--session`, `-s`, `--continue`, `-c`) or
/// runs a subcommand (`opencode run …`, `opencode serve`), which a launch id must not change.
fn command_chooses_opencode_session(command: &str) -> bool {
    let mut offset = 0;
    let mut skip_value = false;
    let mut program_seen = false;
    while !command[offset..].trim().is_empty() {
        let Some((start, end, word)) = super::command_word(command, offset) else {
            // Unfinished quoting: leave the command alone.
            return true;
        };
        offset = end;
        if skip_value {
            skip_value = false;
            continue;
        }
        if !program_seen {
            // Environment assignments before the program (`VAR=value opencode`).
            if !word.contains('=') || word.starts_with('-') {
                program_seen = true;
            }
            continue;
        }
        if !super::is_option_word(&command[start..end], &word) {
            if OPENCODE_SUBCOMMANDS.contains(&word.as_str()) {
                return true;
            }
            continue;
        }
        let flag = word.split_once('=').map_or(word.as_str(), |(flag, _)| flag);
        if matches!(flag, "--session" | "-s" | "--continue" | "-c") {
            return true;
        }
        skip_value = !word.contains('=')
            && matches!(
                flag,
                "--server" | "--prompt" | "--log-level" | "--completions"
            );
    }
    !program_seen
}

/// `opencode --help` (2.1.4): every subcommand, none of which opens the interactive app.
const OPENCODE_SUBCOMMANDS: &[&str] = &[
    "upgrade",
    "update",
    "uninstall",
    "acp",
    "api",
    "debug",
    "auth",
    "mcp",
    "plugin",
    "models",
    "stats",
    "mini",
    "run",
    "session",
    "service",
    "reload",
    "pair",
    "serve",
    "web",
    "export",
    "import",
    "github",
    "agent",
];
