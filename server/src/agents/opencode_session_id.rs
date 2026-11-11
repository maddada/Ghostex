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
/// conversation or runs a subcommand, and an AgentBox launch, keep launching as before; a
/// `--continue` launch is pinned to the conversation it continues (newest_root_conversation).
/// SEE-ALSO: create_agent_session_params_for_project in server/src/agents/launch_plan.rs, server/src/session_chat_opencode/client.rs.
pub(crate) fn mint_opencode_session_id(command: &str, directory: Option<&str>) -> Option<String> {
    // Unit tests assert exact commands, which must not depend on the CLIs installed on the machine.
    if cfg!(test) {
        return None;
    }
    match opencode_session_choice(command) {
        SessionChoice::Chosen => None,
        SessionChoice::Continue => newest_root_conversation(directory?),
        SessionChoice::New => {
            let home = crate::resume_lookup::home_dir();
            crate::agent_cli::opencode::launch_executable(&home).ok()?;
            Some(new_opencode_session_id())
        }
    }
}

/// CDXC:SessionIdentity 2026-11-11 WHY:
/// The user wants Ghostex to stop installing OpenCode's plugin, which reported the conversation a `-c` / `--continue` launch opened. OpenCode 2.2.7's TUI opens the newest top-level, unarchived conversation in its folder (`session.list` with `limit=1`, `order=desc`, `parentID=null`, `archived=false`, `directory`), so Ghostex asks the service the same question at launch and runs `--session <id>` in place of the flag: the session knows its conversation, and a conversation another launch creates in the folder meanwhile cannot take its place. With `--fork` (a new conversation OpenCode names), no conversation yet, or the service down, the command runs as written.
fn newest_root_conversation(directory: &str) -> Option<String> {
    let client = crate::session_chat_opencode::Client::discover().ok()?;
    // The TUI's folder is its working directory, as the operating system spells it.
    let canonical = std::fs::canonicalize(directory)
        .ok()
        .map(|path| path.to_string_lossy().into_owned());
    for directory in [canonical.as_deref(), Some(directory)]
        .into_iter()
        .flatten()
    {
        let encoded: String = url::form_urlencoded::byte_serialize(directory.as_bytes()).collect();
        let path = format!(
            "/api/session?limit=1&order=desc&parentID=null&archived=false&directory={encoded}"
        );
        let Ok(response) = client.request("GET", &path, None) else {
            continue;
        };
        if let Some(id) = response["data"][0]["id"]
            .as_str()
            .filter(|id| crate::session_chat_opencode::safe_id(id))
        {
            return Some(id.to_string());
        }
    }
    None
}

/// `command` with OpenCode told which conversation to open, in place of a `-c` / `--continue`.
pub(crate) fn with_opencode_session_id(command: &str, id: &str) -> String {
    let mut kept = String::new();
    let mut offset = 0;
    let mut program_seen = false;
    while let Some((start, end, word)) = super::command_word(command, offset) {
        let literal = &command[start..end];
        let continue_flag = program_seen
            && super::is_option_word(literal, &word)
            && matches!(word.as_str(), "-c" | "--continue");
        if !word.contains('=') || word.starts_with('-') {
            program_seen = true;
        }
        if !continue_flag {
            kept.push_str(&command[offset..end]);
        }
        offset = end;
    }
    kept.push_str(&command[offset..]);
    format!("{} --session {id}", kept.trim_end())
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

enum SessionChoice {
    /// A new conversation with an id Ghostex gives it.
    New,
    /// `-c` / `--continue`: the folder's newest conversation.
    Continue,
    /// The command picks its own (`--session`, `-s`, `--fork`), runs a subcommand, or is unclear.
    Chosen,
}

/// Whether the command picks a conversation (`--session`, `-s`, `--fork`), continues the folder's
/// newest (`--continue`, `-c`, in the folder it starts in), or runs a subcommand (`opencode run …`,
/// `opencode serve`), which a launch id must not change.
fn opencode_session_choice(command: &str) -> SessionChoice {
    let mut offset = 0;
    let mut skip_value = false;
    let mut program_seen = false;
    let mut continues = false;
    let mut directory_argument = false;
    while !command[offset..].trim().is_empty() {
        let Some((start, end, word)) = super::command_word(command, offset) else {
            // Unfinished quoting: leave the command alone.
            return SessionChoice::Chosen;
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
                return SessionChoice::Chosen;
            }
            // `opencode <directory>` starts the TUI in another folder.
            directory_argument = true;
            continue;
        }
        let flag = word.split_once('=').map_or(word.as_str(), |(flag, _)| flag);
        if matches!(flag, "--session" | "-s" | "--fork") {
            return SessionChoice::Chosen;
        }
        if matches!(flag, "--continue" | "-c") {
            continues = true;
        }
        skip_value = !word.contains('=')
            && matches!(
                flag,
                "--server" | "--prompt" | "--log-level" | "--completions"
            );
    }
    match (program_seen, continues, directory_argument) {
        (false, _, _) | (true, true, true) => SessionChoice::Chosen,
        (true, true, false) => SessionChoice::Continue,
        (true, false, _) => SessionChoice::New,
    }
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
