use std::{
    collections::HashSet,
    env, fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use anyhow::{anyhow, Result};
use md5::{Digest, Md5};
use serde_json::Value;

/*
CDXC:RemotePairing 2026-07-13:
Agent resume commands used to embed `node -e <script>` lookups that parsed
provider transcript/index files, which forced every host (including remote
Linux) to carry a Node runtime for session restore. The same lookups now run
as `gxserver resume-lookup <provider> ...`, so the resume command built into
the terminal calls back into this binary. Output contract matches the old
scripts exactly: the matched id on stdout with no trailing newline and exit
code 0, or empty stdout with exit code 1 when nothing matches, because the
generated shell wrappers test `-n "$ID"` on the captured substitution.
*/

pub fn run_resume_lookup(args: Vec<String>) -> Result<()> {
    let provider = args.first().map(String::as_str).unwrap_or_default();
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    let outcome = match provider {
        "claude" => lookup_claude(
            rest.first().copied().unwrap_or_default(),
            rest.get(1).copied().unwrap_or_default(),
            rest.get(2).copied().unwrap_or_default(),
        ),
        "codex" => lookup_codex(
            rest.first().copied().unwrap_or_default(),
            rest.get(1).copied().unwrap_or_default(),
        ),
        "cursor" => lookup_cursor(
            rest.first().copied().unwrap_or_default(),
            rest.get(1).copied().unwrap_or_default(),
        ),
        "opencode" => lookup_opencode_from_stdin(rest.first().copied().unwrap_or_default()),
        other => {
            return Err(anyhow!("Unknown gxserver resume-lookup provider: {other}"));
        }
    };
    match outcome {
        Some(value) => {
            print!("{value}");
            Ok(())
        }
        None => std::process::exit(1),
    }
}

/// CDXC:AgentProviders 2026-09-18 WHY:
/// HOME is unset on Windows, so this returned "." and the account usage-history scan looked for transcripts under the process working directory.
pub(crate) fn home_dir() -> PathBuf {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .find(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(value)
}

/* Lexical absolute normalization matching Node's path.resolve: make the path
absolute against the current directory and fold `.`/`..` components without
touching the filesystem. */
fn normalized_path(value: &str) -> PathBuf {
    let expanded = expand_home(value.trim());
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(expanded)
    };
    let mut prefix: Option<std::ffi::OsString> = None;
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(part) => parts.push(part.to_os_string()),
            // The drive or UNC prefix is part of the identity: dropping it made `C:\project` and `D:\project` the same folder.
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            Component::RootDir => {}
        }
    }
    let mut result = match prefix {
        Some(prefix) => {
            let mut root = prefix;
            root.push(std::path::MAIN_SEPARATOR_STR);
            PathBuf::from(root)
        }
        None => PathBuf::from("/"),
    };
    for part in parts {
        result.push(part);
    }
    result
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn read_lines_lossy(path: &Path) -> Option<Vec<String>> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    Some(
        text.split(['\n'])
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect(),
    )
}

pub(crate) fn parse_json_line(line: &str) -> Option<Value> {
    let parsed: Value = serde_json::from_str(line).ok()?;
    parsed.is_object().then_some(parsed)
}

fn text_field(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/* ---------------------------------------------------------------- claude */

/// CDXC:AgentProviders 2026-10-21 WHY:
/// The title lookup read every Claude transcript on the computer from start to end (13 GB in 1,840 files on one Mac) although it uses only each file's first 2,001 lines, so a wake's fallback sat at full CPU for minutes (session G2nir) and held the shell. It now reads only the folders Claude stores this project's conversations in, stops each file after those lines, and gives up after this limit, newest conversations first, so the restore script always gets its shell back.
const CLAUDE_LOOKUP_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Claude keeps a conversation under `projects/<folder>`, where the folder is the launch directory with every character other than an ASCII letter or digit (each UTF-16 unit) replaced by `-`, cut to 200 characters plus `-<hash>` when longer. Both the given and the resolved path are tried, because Claude records the resolved one (`/tmp` is stored as `-private-tmp`).
fn claude_project_folder_names(project_path: &str) -> Vec<String> {
    let encode = |path: &Path| -> String {
        path.to_string_lossy()
            .encode_utf16()
            .map(|unit| match char::from_u32(u32::from(unit)) {
                Some(c) if c.is_ascii_alphanumeric() => c,
                _ => '-',
            })
            .collect()
    };
    let project = normalized_path(project_path);
    let mut names = vec![encode(&project)];
    if let Ok(resolved) = fs::canonicalize(&project) {
        let resolved = encode(&resolved);
        if !names.contains(&resolved) {
            names.push(resolved);
        }
    }
    names
}

fn claude_folder_matches(folder: &str, name: &str) -> bool {
    const CLAUDE_FOLDER_NAME_LIMIT: usize = 200;
    if name.len() <= CLAUDE_FOLDER_NAME_LIMIT {
        return folder == name;
    }
    folder
        .strip_prefix(&name[..CLAUDE_FOLDER_NAME_LIMIT])
        .is_some_and(|rest| rest.starts_with('-'))
}

/// The first `limit` lines of a file, read lazily so a large transcript is never loaded whole.
fn read_first_lines_lossy(path: &Path, limit: usize) -> Option<Vec<String>> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(fs::File::open(path).ok()?);
    let mut lines = Vec::new();
    let mut buffer = Vec::new();
    while lines.len() < limit {
        buffer.clear();
        if reader.read_until(b'\n', &mut buffer).ok()? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&buffer);
        let line = line.strip_suffix('\n').unwrap_or(&line);
        lines.push(line.strip_suffix('\r').unwrap_or(line).to_string());
    }
    Some(lines)
}

struct ClaudeMatch {
    latest: String,
    score: i64,
    session_id: String,
}

fn lookup_claude(project_path: &str, title: &str, first_prompt: &str) -> Option<String> {
    let project_path = project_path.trim();
    let title = normalize_whitespace(title);
    let first_prompt = normalize_whitespace(first_prompt);
    if title.is_empty() && first_prompt.is_empty() {
        return None;
    }

    let home = home_dir();
    let mut roots = vec![home.join(".claude").join("projects")];
    if let Ok(entries) = fs::read_dir(home.join(".claude-profiles")) {
        for entry in entries.flatten() {
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                roots.push(entry.path().join("projects"));
            }
        }
    }

    // The folder names Claude can store this project's conversations under; `None` scans every folder (no project given).
    let project_folders =
        (!project_path.is_empty()).then(|| claude_project_folder_names(project_path));
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for root in roots {
        let Ok(project_dirs) = fs::read_dir(&root) else {
            continue;
        };
        for project_dir in project_dirs.flatten() {
            if !project_dir
                .file_type()
                .map(|kind| kind.is_dir())
                .unwrap_or(false)
                || project_dir.file_name() == "subagents"
            {
                continue;
            }
            let folder = project_dir.file_name().to_string_lossy().to_string();
            if project_folders.as_ref().is_some_and(|names| {
                !names
                    .iter()
                    .any(|name| claude_folder_matches(&folder, name))
            }) {
                continue;
            }
            let Ok(entries) = fs::read_dir(project_dir.path()) else {
                continue;
            };
            for file in entries.flatten() {
                let is_file = file.file_type().map(|kind| kind.is_file()).unwrap_or(false);
                let name = file.file_name().to_string_lossy().to_string();
                if !is_file || !name.ends_with(".jsonl") {
                    continue;
                }
                let modified = file
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                files.push((modified, file.path()));
            }
        }
    }
    // Newest first, so the time limit leaves out the oldest conversations.
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));

    let started = std::time::Instant::now();
    let mut matches: Vec<ClaudeMatch> = Vec::new();
    for (_, path) in files {
        if started.elapsed() > CLAUDE_LOOKUP_TIME_LIMIT {
            break;
        }
        if let Some(result) = scan_claude_transcript(&path, project_path, &title, &first_prompt) {
            matches.push(result);
        }
    }

    matches.sort_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| left.latest.cmp(&right.latest))
    });
    matches.pop().map(|best| best.session_id)
}

fn scan_claude_transcript(
    file_path: &Path,
    project_path: &str,
    title: &str,
    first_prompt: &str,
) -> Option<ClaudeMatch> {
    let file_name = file_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut session_id = file_name
        .strip_suffix(".jsonl")
        .unwrap_or(file_name.as_str())
        .to_string();
    let mut cwd_values: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut summaries: Vec<String> = Vec::new();
    let mut first_user = String::new();
    let mut latest = String::new();
    let lines = read_first_lines_lossy(file_path, 2001)?;
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let Some(item) = parse_json_line(line) else {
            continue;
        };
        let item_session_id = text_field(&item, "sessionId");
        if !item_session_id.is_empty() {
            session_id = item_session_id;
        }
        // Every record repeats the cwd, and each distinct one is resolved on disk once below.
        for key in ["cwd", "projectPath"] {
            if let Some(cwd) = item.get(key).and_then(Value::as_str) {
                if !cwd_values.iter().any(|seen| seen == cwd) {
                    cwd_values.push(cwd.to_string());
                }
            }
        }
        if let Some(timestamp) = item.get("timestamp").and_then(Value::as_str) {
            if timestamp > latest.as_str() {
                latest = timestamp.to_string();
            }
        }
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
        if item_type == "custom-title" {
            if let Some(custom_title) = item.get("customTitle").and_then(Value::as_str) {
                names.push(custom_title.to_string());
            }
        }
        if item_type == "ai-title" {
            if let Some(ai_title) = item.get("aiTitle").and_then(Value::as_str) {
                names.push(ai_title.to_string());
            }
        }
        if item_type == "agent-name" {
            if let Some(agent_name) = item.get("agentName").and_then(Value::as_str) {
                names.push(agent_name.to_string());
            }
        }
        if let Some(slug) = item.get("slug").and_then(Value::as_str) {
            names.push(slug.to_string());
        }
        if let Some(summary) = item.get("summary").and_then(Value::as_str) {
            summaries.push(summary.to_string());
        }
        if item_type == "user"
            && first_user.is_empty()
            && item.get("isMeta").and_then(Value::as_bool) != Some(true)
        {
            let text = text_from_message(item.get("message"));
            if !is_claude_local_command_text(&text) {
                first_user = text;
            }
        }
    }
    let project_score = if cwd_values
        .iter()
        .any(|cwd| claude_project_matches(project_path, cwd))
    {
        2
    } else {
        0
    };
    if !project_path.is_empty() && project_score == 0 {
        return None;
    }
    let normalized_names: Vec<String> = names
        .iter()
        .chain(summaries.iter())
        .map(|value| normalize_whitespace(value))
        .filter(|value| !value.is_empty())
        .collect();
    let normalized_first_user = normalize_whitespace(&first_user);
    let title_score = if title.is_empty() {
        0
    } else if normalized_names.iter().any(|value| value == title) {
        8
    } else if normalized_names
        .iter()
        .any(|value| substring_is_evidence(value, title))
    {
        4
    } else {
        0
    };
    let first_prompt_score = if first_prompt.is_empty() || normalized_first_user.is_empty() {
        0
    } else if normalized_first_user == first_prompt {
        10
    } else if substring_is_evidence(&normalized_first_user, first_prompt) {
        5
    } else {
        0
    };
    /*
    CDXC:AgentProviders 2026-09-20 WHY:
    The project score alone used to be enough to return a transcript, so "find the conversation with this title" became "hand back the newest conversation near this folder" whenever the title matched nothing, and a wake whose saved conversation id no longer existed silently opened an unrelated chat instead of failing. Only a name or a first prompt identifies a conversation; the project is a filter, never the evidence.
    */
    (title_score > 0 || first_prompt_score > 0).then(|| ClaudeMatch {
        latest,
        score: project_score + title_score + first_prompt_score,
        session_id,
    })
}

/// CDXC:AgentProviders 2026-09-24 WHY:
/// Claude records slash commands and their output as `user` lines (`<command-name>`, `<local-command-stdout>`, the `isMeta` caveat) before the first typed prompt, so taking the first `user` line compared the saved first prompt against `/effort` output and never matched. Claude's automatic conversation title is stored only as `ai-title`, which is the title Ghostex shows for untitled sessions, so the title lookup reads it too.
fn is_claude_local_command_text(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("<command-name>")
        || text.starts_with("<command-message>")
        || text.starts_with("<local-command-")
}

/// CDXC:AgentProviders 2026-09-20 WHY:
/// A substring match with no length floor let noise win: a session named "y" is contained in almost every title and a first message of "hi" in almost every prompt, so those conversations outscored real matches machine-wide. Require the shorter side to be long enough to identify a conversation on its own; exact equality still matches at any length.
fn substring_is_evidence(stored: &str, requested: &str) -> bool {
    const MINIMUM_SUBSTRING_MATCH_CHARACTERS: usize = 12;
    stored.chars().count().min(requested.chars().count()) >= MINIMUM_SUBSTRING_MATCH_CHARACTERS
        && (stored.contains(requested) || requested.contains(stored))
}

/// CDXC:AgentProviders 2026-09-20 WHY:
/// This used to accept a transcript whose cwd merely contained the project path or sat inside it, so a session opened at the home directory matched every Claude conversation on the machine and a session in any subfolder matched every home-directory conversation. A conversation belongs to the pane it was launched in, so the recorded cwd has to be that directory.
fn claude_project_matches(project_path: &str, cwd: &str) -> bool {
    if project_path.is_empty() {
        return true;
    }
    let cwd_text = cwd.trim();
    if cwd_text.is_empty() {
        return false;
    }
    let project = normalized_path(project_path);
    let candidate = normalized_path(cwd_text);
    if candidate == project {
        return true;
    }
    match (fs::canonicalize(&candidate), fs::canonicalize(&project)) {
        (Ok(candidate), Ok(project)) => candidate == project,
        _ => false,
    }
}

pub(crate) fn text_from_message(message: Option<&Value>) -> String {
    let Some(message) = message else {
        return String::new();
    };
    if let Some(text) = message.as_str() {
        return text.to_string();
    }
    let Some(content) = message.get("content") else {
        return String::new();
    };
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    if let Some(items) = content.as_array() {
        let mut parts: Vec<String> = Vec::new();
        for item in items {
            if let Some(text) = item.as_str() {
                parts.push(text.to_string());
            } else if let Some(text) = item.get("text").and_then(Value::as_str) {
                parts.push(text.to_string());
            }
        }
        return parts.join("\n");
    }
    String::new()
}

/* ----------------------------------------------------------------- codex */

const CODEX_INTERNAL_TITLE_PROMPT_MARKERS: [&str; 3] = [
    "Write a concise session title that summarizes the user's text.",
    "Output handling:",
    "Print only the final result to stdout.",
];

fn lookup_codex(mode: &str, query: &str) -> Option<String> {
    let mode = mode.trim();
    let query = query.trim();
    if !matches!(mode, "--exact" | "--title") || query.is_empty() {
        return None;
    }
    let codex_homes = codex_homes();
    if mode == "--exact" {
        let Some(session_id) = find_uuid(query) else {
            /* Non-UUID references pass through untouched, matching the JS
            script's exit-0 passthrough. */
            return Some(query.to_string());
        };
        if codex_homes
            .iter()
            .any(|codex_home| is_internal_codex_session(codex_home, &session_id, None))
        {
            return None;
        }
        return Some(session_id);
    }

    let mut matches: Vec<(String, String)> = Vec::new();
    for codex_home in &codex_homes {
        let Some(lines) = read_lines_lossy(&codex_home.join("session_index.jsonl")) else {
            continue;
        };
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let Some(item) = parse_json_line(&line) else {
                continue;
            };
            if text_field(&item, "thread_name") != query {
                continue;
            }
            let session_id = text_field(&item, "id");
            if session_id.is_empty()
                || is_internal_codex_session(codex_home, &session_id, Some(&item))
            {
                continue;
            }
            let updated_at = text_field(&item, "updated_at");
            matches.push((session_id, updated_at));
        }
    }
    matches.sort_by(|left, right| left.1.cmp(&right.1));
    matches.pop().map(|(session_id, _)| session_id)
}

pub(crate) fn codex_homes() -> Vec<PathBuf> {
    let home = home_dir();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(codex_home) = env::var_os("CODEX_HOME") {
        candidates.push(expand_home(codex_home.to_string_lossy().as_ref()));
    }
    candidates.push(home.join(".codex-profiles").join("personal"));
    candidates.push(home.join(".codex-profiles").join("work"));
    candidates.push(home.join(".codex"));

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut homes: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        let normalized = fs::canonicalize(&candidate).unwrap_or(candidate);
        if seen.insert(normalized.clone()) {
            homes.push(normalized);
        }
    }
    homes
}

fn find_uuid(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.len() < 36 {
        return None;
    }
    for start in 0..=(bytes.len() - 36) {
        let candidate = &bytes[start..start + 36];
        if is_uuid_bytes(candidate) {
            return Some(String::from_utf8_lossy(candidate).to_ascii_lowercase());
        }
    }
    None
}

fn is_uuid_bytes(candidate: &[u8]) -> bool {
    if candidate.len() != 36 {
        return false;
    }
    for (index, byte) in candidate.iter().enumerate() {
        let expected_dash = matches!(index, 8 | 13 | 18 | 23);
        if expected_dash {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn is_internal_codex_session(codex_home: &Path, session_id: &str, item: Option<&Value>) -> bool {
    if let Some(item) = item {
        if text_field(item, "originator") == "codex_exec" {
            return true;
        }
        if text_field(item, "source") == "exec" {
            return true;
        }
    }
    codex_transcript_paths(codex_home, session_id, item)
        .iter()
        .any(|path| codex_transcript_is_internal_exec(path))
}

pub(crate) fn codex_transcript_paths(
    codex_home: &Path,
    session_id: &str,
    item: Option<&Value>,
) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    if let Some(item) = item {
        for key in [
            "path",
            "session_path",
            "sessionPath",
            "transcript_path",
            "transcriptPath",
        ] {
            let value = text_field(item, key);
            if value.is_empty() {
                continue;
            }
            let expanded = expand_home(&value);
            let transcript_path = if expanded.is_absolute() {
                expanded
            } else {
                codex_home.join(expanded)
            };
            let transcript_path = crate::codex_rollout_archive::readable_rollout(&transcript_path)
                .unwrap_or(transcript_path);
            if seen.insert(transcript_path.clone()) {
                paths.push(transcript_path);
            }
        }
    }
    let mut stack = vec![codex_home.join("sessions")];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry_path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !kind.is_file()
                || !name.contains(session_id)
                || !(name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"))
            {
                continue;
            }
            // A compressed rollout is offered as its plain twin or mirror, once.
            let Some(readable) = crate::codex_rollout_archive::readable_rollout(&entry_path) else {
                continue;
            };
            if seen.insert(readable.clone()) {
                paths.push(readable);
            }
        }
    }
    paths
}

fn codex_transcript_is_internal_exec(transcript_path: &Path) -> bool {
    let Some(lines) = read_lines_lossy(transcript_path) else {
        return false;
    };
    for line in lines.iter().take(81) {
        if CODEX_INTERNAL_TITLE_PROMPT_MARKERS
            .iter()
            .all(|marker| line.contains(marker))
        {
            return true;
        }
        let Some(entry) = parse_json_line(line) else {
            continue;
        };
        let Some(payload) = entry.get("payload").filter(|value| value.is_object()) else {
            continue;
        };
        if text_field(payload, "originator") == "codex_exec" {
            return true;
        }
        if text_field(payload, "source") == "exec" {
            return true;
        }
    }
    false
}

/* ---------------------------------------------------------------- cursor */

fn lookup_cursor(project_path: &str, title: &str) -> Option<String> {
    let project_path = project_path.trim();
    let title = title.trim();
    if project_path.is_empty() || title.is_empty() {
        return None;
    }
    let mut hasher = Md5::new();
    hasher.update(project_path.as_bytes());
    let project_hash = hex::encode(hasher.finalize());
    let chats_dir = home_dir().join(".cursor").join("chats").join(project_hash);
    let chat_dirs = fs::read_dir(&chats_dir).ok()?;

    let mut matches: Vec<(String, f64)> = Vec::new();
    for chat_dir in chat_dirs.flatten() {
        if !chat_dir
            .file_type()
            .map(|kind| kind.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let db_path = chat_dir.path().join("store.db");
        if !db_path.is_file() {
            continue;
        }
        let rows = read_cursor_meta_values(&db_path);
        for raw in rows {
            let Some(meta) = parse_cursor_meta_value(&raw) else {
                continue;
            };
            if text_field(&meta, "name") != title {
                continue;
            }
            let mut chat_id = text_field(&meta, "agentId");
            if chat_id.is_empty() {
                chat_id = chat_dir.file_name().to_string_lossy().trim().to_string();
            }
            if chat_id.is_empty() {
                continue;
            }
            let created_at = match meta.get("createdAt") {
                Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
                Some(Value::String(text)) => text.trim().parse::<f64>().unwrap_or(0.0),
                _ => 0.0,
            };
            let created_at = if created_at.is_finite() {
                created_at
            } else {
                0.0
            };
            matches.push((chat_id, created_at));
        }
    }
    matches.sort_by(|left, right| {
        left.1
            .partial_cmp(&right.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches.pop().map(|(chat_id, _)| chat_id)
}

fn read_cursor_meta_values(db_path: &Path) -> Vec<String> {
    let Ok(db) = ghostex_find::agent_store::open_agent_store(db_path) else {
        return Vec::new();
    };
    let Ok(mut statement) = db.prepare("select value from meta") else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(0)) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

pub(crate) fn parse_cursor_meta_value(raw: &str) -> Option<Value> {
    let value = raw.trim();
    if value.starts_with('{') {
        return serde_json::from_str(value).ok();
    }
    let bytes = decode_hex(value)?;
    serde_json::from_slice(&bytes).ok()
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let raw = value.as_bytes();
    for pair in raw.chunks(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes.push((high << 4) | low);
    }
    Some(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/* -------------------------------------------------------------- opencode */

fn lookup_opencode_from_stdin(title: &str) -> Option<String> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok()?;
    lookup_opencode(title, &input)
}

/// `opencode session list --format json` prints one JSON array of `{id, title, …}` objects.
fn lookup_opencode(title: &str, input: &str) -> Option<String> {
    let sessions: Vec<Value> = serde_json::from_str(input.trim()).ok()?;
    sessions.iter().find_map(|item| {
        let session_id = text_field(item, "id");
        (text_field(item, "title") == title && !session_id.is_empty()).then_some(session_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_uuid_extracts_embedded_ids() {
        assert_eq!(
            find_uuid("resume 12345678-ABCD-1234-abcd-123456789abc now"),
            Some("12345678-abcd-1234-abcd-123456789abc".to_string())
        );
        assert_eq!(find_uuid("no id here"), None);
    }

    #[test]
    fn normalize_whitespace_collapses_runs() {
        assert_eq!(normalize_whitespace("  a\t b\n  c "), "a b c");
    }

    #[test]
    fn opencode_lookup_matches_title() {
        let input = r#"[
  { "id": "s1", "title": "other" },
  { "id": "s2", "title": "target" }
]"#;
        assert_eq!(lookup_opencode("target", input), Some("s2".to_string()));
        assert_eq!(lookup_opencode("missing", input), None);
    }

    #[test]
    fn cursor_meta_values_parse_json_and_hex() {
        let json_meta = parse_cursor_meta_value("{\"name\":\"chat\"}").expect("json meta");
        assert_eq!(json_meta.get("name").and_then(Value::as_str), Some("chat"));
        let hex = "7b226e616d65223a2263686174227d";
        let hex_meta = parse_cursor_meta_value(hex).expect("hex meta");
        assert_eq!(hex_meta.get("name").and_then(Value::as_str), Some("chat"));
    }

    #[test]
    fn claude_project_matching_requires_the_same_directory() {
        assert!(claude_project_matches("/tmp/project", "/tmp/project"));
        assert!(claude_project_matches("/tmp/project", "/tmp/project/./"));
        assert!(!claude_project_matches("/tmp/project", "/tmp/project/sub"));
        assert!(!claude_project_matches("/tmp/project/sub", "/tmp/project"));
        assert!(!claude_project_matches("/tmp/project", "/tmp/other"));
        assert!(claude_project_matches("", "anything"));
    }
}
