use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use super::*;

#[derive(Clone, Debug)]
pub(crate) struct ProcessRow {
    command: String,
    pid: i64,
    ppid: i64,
    terminal_name: Option<String>,
}

#[derive(Clone, Debug)]
struct ProcessIdentityCandidate {
    confidence: i64,
    depth: i64,
    identity: ZmxProcessIdentity,
}

#[derive(Clone, Debug)]
struct ProcessIdentityObservation {
    confidence: i64,
    identity: ZmxProcessIdentity,
}

const GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT: usize = 12;
const GXSERVER_DIRECT_AGENT_PROCESS_CONFIDENCE: i64 = 300;
const GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE: i64 = 275;
const GXSERVER_AGENT_SESSION_ID_CONFIDENCE_BONUS: i64 = 25;

pub(crate) fn parse_process_rows(ps_output: &str) -> Vec<ProcessRow> {
    let mut rows = Vec::new();
    for line in ps_output.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let Some(ppid) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let remaining = parts.collect::<Vec<_>>();
        let has_terminal_column = remaining
            .first()
            .copied()
            .is_some_and(looks_like_process_terminal_name);
        let terminal_name = remaining
            .first()
            .copied()
            .filter(|_| has_terminal_column)
            .and_then(normalize_process_terminal_name);
        let command_start = usize::from(has_terminal_column);
        rows.push(ProcessRow {
            command: remaining[command_start..].join(" "),
            pid,
            ppid,
            terminal_name,
        });
    }
    rows
}

/// PID of the daemon that owns `session_name`, read from the process table.
///
/// CDXC:ZmxWireGeneration 2026-08-23: `zmx run <name> -d` keeps its argv across
/// daemonization, so the owning process identifies itself without answering any
/// IPC. `zmx list` reports the same PID but pays a per-session probe that a
/// pre-wire-break daemon cannot complete, which is exactly the case this exists
/// for.
pub(crate) fn find_zmx_daemon_process_id(ps_output: &str, session_name: &str) -> Option<i64> {
    parse_process_rows(ps_output).into_iter().find_map(|row| {
        let mut arguments = row.command.split_whitespace();
        let executable = arguments.next()?;
        if Path::new(executable).file_stem()?.to_str()? != "zmx" {
            return None;
        }
        if arguments.next()? != "run" || arguments.next()? != session_name {
            return None;
        }
        (row.pid > 0).then_some(row.pid)
    })
}

fn looks_like_process_terminal_name(value: &str) -> bool {
    value == "??"
        || value == "-"
        || value.starts_with("tty")
        || value.starts_with("pts/")
        || value.starts_with("/dev/")
}

fn normalize_process_terminal_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || matches!(trimmed, "??" | "-") {
        return None;
    }
    let name = Path::new(trimmed).file_name()?.to_str()?;
    if name.is_empty()
        || name.len() > 128
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return None;
    }
    Some(name.to_string())
}

pub(crate) fn group_processes_by_parent_pid(
    processes: &[ProcessRow],
) -> HashMap<i64, Vec<ProcessRow>> {
    let mut grouped = HashMap::<i64, Vec<ProcessRow>>::new();
    for process_row in processes {
        grouped
            .entry(process_row.ppid)
            .or_default()
            .push(process_row.clone());
    }
    grouped
}

pub(crate) fn resolve_process_tree_agent_identity(
    root_pid: i64,
    processes: &[ProcessRow],
    children_by_parent_pid: &HashMap<i64, Vec<ProcessRow>>,
) -> Option<ZmxProcessIdentity> {
    let rows_by_pid = processes
        .iter()
        .map(|process_row| (process_row.pid, process_row))
        .collect::<HashMap<_, _>>();
    let mut candidates = Vec::<ProcessIdentityCandidate>::new();
    /*
    CDXC:SessionIdentity 2026-09-02:
    The first agent process found on a path down from the zmx root owns the
    terminal; every process below it was spawned BY that agent. A different
    agent CLI down there is one of its tool invocations (`grok models`,
    `cursor-agent --list-models` run from Claude's Bash tool, observed live
    2026-09-02), not the session's agent, and the deepest-wins ordering below
    let it replace the real identity: the row flipped to that agent, the
    transcript was then decoded with the wrong reader, and the chat view sat on
    "Loading conversation…" for good. Only same-agent descendants stay
    candidates, which keeps the launcher → binary wrapper chains (node codex →
    lib/codex) that deepest-wins exists for.
    */
    let mut queue = VecDeque::from([(0_i64, root_pid, None::<String>)]);
    let mut seen = HashSet::<i64>::new();
    while let Some((depth, pid, owner_agent_id)) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        let mut owner_agent_id = owner_agent_id;
        if let Some(row) = rows_by_pid.get(&pid) {
            if let Some(mut observation) = resolve_process_command_agent_identity(&row.command) {
                if let Some(agent_id) = observation.identity.agent_id.clone() {
                    let spawned_by_other_agent = owner_agent_id
                        .as_deref()
                        .is_some_and(|owner| owner != agent_id);
                    if !spawned_by_other_agent {
                        owner_agent_id = Some(agent_id);
                        observation.identity.process_id = Some(row.pid);
                        observation.identity.terminal_name = row.terminal_name.clone();
                        candidates.push(ProcessIdentityCandidate {
                            confidence: observation.confidence,
                            depth,
                            identity: observation.identity,
                        });
                    }
                }
            }
        }
        if let Some(children) = children_by_parent_pid.get(&pid) {
            for child in children {
                queue.push_back((depth + 1, child.pid, owner_agent_id.clone()));
            }
        }
    }
    candidates.sort_by(|left, right| {
        let confidence =
            score_process_identity_candidate(right).cmp(&score_process_identity_candidate(left));
        if confidence != std::cmp::Ordering::Equal {
            return confidence;
        }
        let id_score = right
            .identity
            .agent_session_id
            .is_some()
            .cmp(&left.identity.agent_session_id.is_some());
        if id_score != std::cmp::Ordering::Equal {
            return id_score;
        }
        right.depth.cmp(&left.depth)
    });
    candidates
        .into_iter()
        .next()
        .map(|candidate| candidate.identity)
}

fn score_process_identity_candidate(candidate: &ProcessIdentityCandidate) -> i64 {
    candidate.confidence
        + if candidate.identity.agent_session_id.is_some() {
            GXSERVER_AGENT_SESSION_ID_CONFIDENCE_BONUS
        } else {
            0
        }
}

fn resolve_process_command_agent_identity(command: &str) -> Option<ProcessIdentityObservation> {
    let tokens = split_process_command_prefix(command, GXSERVER_PROCESS_COMMAND_PREFIX_TOKEN_LIMIT);
    if tokens.is_empty() || should_ignore_process_command_for_agent_identity(command, &tokens) {
        return None;
    }
    resolve_agent_process_invocation(&tokens, 0, GXSERVER_DIRECT_AGENT_PROCESS_CONFIDENCE)
        .or_else(|| resolve_wrapped_agent_process_invocation(&tokens))
}

fn should_ignore_process_command_for_agent_identity(command: &str, tokens: &[String]) -> bool {
    let executable_name = normalize_process_executable_name(tokens.first().map(String::as_str));
    if executable_name.as_deref().is_some_and(|name| {
        matches!(
            name,
            "gte" | "node_repl" | "prompt-editor" | "skycomputeruseclient"
        )
    }) {
        return true;
    }
    let lower_command = command.to_ascii_lowercase();
    ["skycomputeruseclient", "/node_repl", " node_repl"]
        .iter()
        .any(|marker| lower_command.contains(marker))
}

fn resolve_wrapped_agent_process_invocation(
    tokens: &[String],
) -> Option<ProcessIdentityObservation> {
    let executable_name = normalize_process_executable_name(tokens.first().map(String::as_str));
    if executable_name.as_deref() == Some("env") {
        if let Some(invocation_index) = find_env_wrapped_command_index(tokens) {
            return resolve_agent_process_invocation(
                tokens,
                invocation_index,
                GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE,
            );
        }
    }
    if !executable_name
        .as_deref()
        .is_some_and(|name| matches!(name, "bun" | "node"))
    {
        return None;
    }
    for index in 1..tokens.len().min(8) {
        if let Some(observation) = resolve_agent_process_invocation(
            tokens,
            index,
            GXSERVER_WRAPPED_AGENT_PROCESS_CONFIDENCE,
        ) {
            return Some(observation);
        }
    }
    None
}

fn resolve_agent_process_invocation(
    tokens: &[String],
    executable_index: usize,
    confidence: i64,
) -> Option<ProcessIdentityObservation> {
    let agent_id = infer_agent_id_from_process_executable(tokens, executable_index)?;
    let agent_session_id = extract_agent_process_session_id(&agent_id, tokens, executable_index);
    Some(ProcessIdentityObservation {
        confidence,
        identity: ZmxProcessIdentity {
            agent_id: Some(agent_id),
            agent_session_id,
            agent_session_path: None,
            process_id: None,
            terminal_name: None,
        },
    })
}

pub(crate) fn read_codex_process_session_identity(
    process_id: Option<i64>,
) -> Option<(String, String)> {
    let process_id = process_id.filter(|process_id| *process_id > 0)?;
    let mut identities = HashMap::<String, PathBuf>::new();
    for target in process_open_file_paths(process_id) {
        let Some(agent_session_id) = codex_session_id_from_transcript_path(&target) else {
            continue;
        };
        identities.entry(agent_session_id).or_insert(target);
    }
    if identities.len() != 1 {
        return None;
    }
    identities
        .into_iter()
        .next()
        .map(|(agent_session_id, path)| (agent_session_id, path.to_string_lossy().into_owned()))
}

#[cfg(target_os = "linux")]
fn process_open_file_paths(process_id: i64) -> Vec<PathBuf> {
    let fd_dir = PathBuf::from(format!("/proc/{process_id}/fd"));
    fs::read_dir(fd_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .collect()
}

#[cfg(target_os = "macos")]
fn process_open_file_paths(process_id: i64) -> Vec<PathBuf> {
    let output = std::process::Command::new("/usr/sbin/lsof")
        .args(["-Fn", "-p", &process_id.to_string()])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix('n'))
        .filter(|path| Path::new(path).is_absolute())
        .map(PathBuf::from)
        .collect()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_open_file_paths(_process_id: i64) -> Vec<PathBuf> {
    Vec::new()
}

pub(crate) fn codex_session_id_from_transcript_path(path: &Path) -> Option<String> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl")
        || !path
            .ancestors()
            .filter_map(Path::file_name)
            .filter_map(|name| name.to_str())
            .any(|name| name == "sessions")
    {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if !stem.starts_with("rollout-") || stem.len() < 36 {
        return None;
    }
    normalize_codex_session_id(&stem[stem.len() - 36..])
}

/*
CDXC:SessionIdentity 2026-08-06:
OMP owns an exact terminal-to-transcript record under
`agent/terminal-sessions/<tty>`. A fresh OMP process has no `--session` argv,
so its TTY record is the authoritative provider identity source for the live
zmx process scan. Read only that exact record and require its existing JSONL
target; never guess from transcript recency or another session in the cwd.
*/
pub(crate) fn read_omp_terminal_session_identity(
    home_dir: &Path,
    terminal_name: Option<&str>,
) -> Option<(String, String)> {
    let terminal_name = normalize_process_terminal_name(terminal_name?)?;
    let agent_dir = omp_agent_directory(home_dir);
    read_omp_terminal_session_identity_from_agent_dir(&agent_dir, &terminal_name, home_dir)
}

pub(crate) fn read_omp_terminal_session_identity_from_agent_dir(
    agent_dir: &Path,
    terminal_name: &str,
    home_dir: &Path,
) -> Option<(String, String)> {
    let record_path = agent_dir.join("terminal-sessions").join(terminal_name);
    let metadata = fs::metadata(&record_path).ok()?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 {
        return None;
    }
    let record = fs::read_to_string(record_path).ok()?;
    let transcript_path = record
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| expand_home_path(line, home_dir))
        .find(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
                && path.is_file()
        })?;
    let stem = transcript_path.file_stem()?.to_str()?.trim();
    let agent_session_id = stem.rsplit_once('_').map(|(_, id)| id).unwrap_or(stem);
    if agent_session_id.is_empty() || agent_session_id.chars().count() > 256 {
        return None;
    }
    Some((
        agent_session_id.to_string(),
        transcript_path.to_string_lossy().into_owned(),
    ))
}

fn omp_agent_directory(home_dir: &Path) -> PathBuf {
    if let Some(agent_dir) = std::env::var("PI_CODING_AGENT_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return expand_home_path(&agent_dir, home_dir);
    }
    let config_dir = std::env::var("PI_CONFIG_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_home_path(&value, home_dir))
        .unwrap_or_else(|| home_dir.join(".omp"));
    config_dir.join("agent")
}

fn expand_home_path(value: &str, home_dir: &Path) -> PathBuf {
    let trimmed = value.trim();
    if trimmed == "~" {
        return home_dir.to_path_buf();
    }
    if let Some(relative) = trimmed.strip_prefix("~/") {
        return home_dir.join(relative);
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        path
    } else {
        home_dir.join(path)
    }
}

fn infer_agent_id_from_process_executable(
    tokens: &[String],
    executable_index: usize,
) -> Option<String> {
    let executable_name =
        normalize_process_executable_name(tokens.get(executable_index).map(String::as_str))?;
    if executable_name == "acli"
        && normalize_process_token(tokens.get(executable_index + 1).map(String::as_str)).as_deref()
            == Some("rovodev")
        && normalize_process_token(tokens.get(executable_index + 2).map(String::as_str)).as_deref()
            == Some("run")
    {
        return Some("rovodev".to_string());
    }
    Some(
        match executable_name.as_str() {
            "agy" => "antigravity",
            "amp" => "amp",
            "campfire" => "campfire",
            "claude" => "claude",
            "codebuddy" => "codebuddy",
            "codex" => "codex",
            "commandcode" => "command-code",
            "copilot" => "copilot",
            "cursor-agent" => "cursor",
            "devin" => "devin",
            "droid" => "droid",
            "gemini" => "gemini",
            "grok" => "grok",
            "hermes" => "hermes-agent",
            "kimi" => "kimi",
            "kiro-cli" => "kiro",
            "omp" => "omp",
            "openclaude" => "openclaude",
            "opencode" => "opencode",
            "pi" => "pi",
            "qodercli" => "qoder",
            "rovodev" => "rovodev",
            _ => return None,
        }
        .to_string(),
    )
}

fn extract_agent_process_session_id(
    agent_id: &str,
    tokens: &[String],
    executable_index: usize,
) -> Option<String> {
    let args = tokens.get(executable_index + 1..).unwrap_or(&[]);
    /*
    CDXC:SessionFork 2026-09-02:
    A fork launch (`codex fork <id>`, `claude --resume <id> --fork-session`)
    names the PARENT conversation in argv; the forked conversation gets a brand
    new id that only the agent's hook or transcript can report. Reading the
    parent id as this terminal's own identity made every list/snapshot poll
    overwrite the hook-reported fork id with the parent's, so the fork and its
    parent shared one id: the parent's chat follower then adopted the fork's
    transcript as its own "successor" and both rows showed the same chat and
    title (observed live 2026-09-01). A fork argv therefore yields no id at all.
    */
    if agent_id == "codex" {
        for index in 0..args.len().saturating_sub(1) {
            let token = normalize_process_token(args.get(index).map(String::as_str));
            if token.as_deref() == Some("fork") {
                return None;
            }
            if token.as_deref() == Some("resume") {
                return normalize_agent_process_session_id(
                    agent_id,
                    args.get(index + 1).map(String::as_str),
                );
            }
        }
        return None;
    }
    if matches!(agent_id, "claude" | "cursor") {
        if args.iter().any(|arg| {
            let token = arg.trim_matches(['"', '\'']);
            token == "--fork-session" || token.starts_with("--fork-session=")
        }) {
            return None;
        }
        return read_agent_process_flag_value(agent_id, args, "--resume");
    }
    if agent_id == "opencode" {
        return read_agent_process_flag_value(agent_id, args, "--session")
            .or_else(|| read_agent_process_flag_value(agent_id, args, "-s"));
    }
    if matches!(agent_id, "pi" | "omp") {
        return read_agent_process_flag_value(agent_id, args, "--session");
    }
    if agent_id == "kiro" {
        return read_agent_process_flag_value(agent_id, args, "--resume-id");
    }
    None
}

fn read_agent_process_flag_value(agent_id: &str, args: &[String], flag: &str) -> Option<String> {
    for index in 0..args.len() {
        let arg = args.get(index)?;
        if arg == flag {
            return normalize_agent_process_session_id(
                agent_id,
                args.get(index + 1).map(String::as_str),
            );
        }
        if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            return normalize_agent_process_session_id(agent_id, Some(value));
        }
    }
    None
}

fn normalize_agent_process_session_id(agent_id: &str, value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().trim_end_matches([';', '&', '|']).to_string();
    if normalized.is_empty() {
        return None;
    }
    if agent_id == "codex" {
        normalize_codex_session_id(&normalized).or(Some(normalized))
    } else {
        Some(normalized)
    }
}

fn find_env_wrapped_command_index(tokens: &[String]) -> Option<usize> {
    for index in 1..tokens.len() {
        let token = tokens.get(index)?;
        if token.starts_with('-') || is_environment_assignment(token) {
            continue;
        }
        return Some(index);
    }
    None
}

fn split_process_command_prefix(command: &str, max_tokens: usize) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for char in command.chars() {
        if escaped {
            current.push(char);
            escaped = false;
            continue;
        }
        if char == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if char == active_quote {
                quote = None;
            } else {
                current.push(char);
            }
            continue;
        }
        if matches!(char, '"' | '\'') {
            quote = Some(char);
            continue;
        }
        if char.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
                if tokens.len() >= max_tokens {
                    return tokens;
                }
            }
            continue;
        }
        current.push(char);
    }
    if !current.is_empty() && tokens.len() < max_tokens {
        tokens.push(current);
    }
    tokens
}

fn normalize_process_executable_name(token: Option<&str>) -> Option<String> {
    let normalized = normalize_process_token(token)?;
    let basename = normalized
        .rsplit('/')
        .next()
        .unwrap_or(normalized.as_str())
        .to_string();
    for extension in [".cjs", ".cmd", ".exe", ".js", ".mjs", ".ts"] {
        if let Some(stripped) = basename.strip_suffix(extension) {
            return (!stripped.is_empty()).then(|| stripped.to_string());
        }
    }
    Some(basename)
}

fn normalize_process_token(token: Option<&str>) -> Option<String> {
    let text = token?.trim().to_ascii_lowercase();
    let text = text.trim_matches(['"', '\'', '`']).to_string();
    (!text.is_empty()).then_some(text)
}

fn is_environment_assignment(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let mut seen_equals = false;
    for char in chars {
        if char == '=' {
            seen_equals = true;
            break;
        }
        if !(char.is_ascii_alphanumeric() || char == '_') {
            return false;
        }
    }
    seen_equals
}

fn normalize_codex_session_id(value: &str) -> Option<String> {
    is_uuid(value).then(|| value.to_ascii_lowercase())
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}
