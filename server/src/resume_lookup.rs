use std::{
    collections::HashSet,
    env, fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use anyhow::{anyhow, Result};
use md5::{Digest, Md5};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

/*
CDXC:RemoteMinimalDeps 2026-07-13:
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

pub(crate) fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
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
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(part) => parts.push(part.to_os_string()),
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    let mut result = PathBuf::from("/");
    for part in parts {
        result.push(part);
    }
    result
}

fn path_contains(parent: &Path, child: &Path) -> bool {
    child != parent && child.starts_with(parent)
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

    let mut matches: Vec<ClaudeMatch> = Vec::new();
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
            let Ok(files) = fs::read_dir(project_dir.path()) else {
                continue;
            };
            for file in files.flatten() {
                let is_file = file.file_type().map(|kind| kind.is_file()).unwrap_or(false);
                let name = file.file_name().to_string_lossy().to_string();
                if !is_file || !name.ends_with(".jsonl") {
                    continue;
                }
                if let Some(result) =
                    scan_claude_transcript(&file.path(), project_path, &title, &first_prompt)
                {
                    matches.push(result);
                }
            }
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
    let lines = read_lines_lossy(file_path)?;
    for line in lines.iter().take(2001) {
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
        if let Some(cwd) = item.get("cwd").and_then(Value::as_str) {
            cwd_values.push(cwd.to_string());
        }
        if let Some(item_project) = item.get("projectPath").and_then(Value::as_str) {
            cwd_values.push(item_project.to_string());
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
        if item_type == "user" && first_user.is_empty() {
            first_user = text_from_message(item.get("message"));
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
    let mut score = project_score;
    if !title.is_empty() {
        if normalized_names.iter().any(|value| value == title) {
            score += 8;
        } else if normalized_names
            .iter()
            .any(|value| value.contains(title) || title.contains(value.as_str()))
        {
            score += 4;
        }
    }
    if !first_prompt.is_empty() && !normalized_first_user.is_empty() {
        if normalized_first_user == first_prompt {
            score += 10;
        } else if first_prompt.contains(&normalized_first_user)
            || normalized_first_user.contains(first_prompt)
        {
            score += 5;
        }
    }
    (score > 0).then_some(ClaudeMatch {
        latest,
        score,
        session_id,
    })
}

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
    candidate == project
        || path_contains(&project, &candidate)
        || path_contains(&candidate, &project)
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
            if kind.is_file()
                && name.contains(session_id)
                && name.ends_with(".jsonl")
                && seen.insert(entry_path.clone())
            {
                paths.push(entry_path);
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
    let project_hash = format!("{:x}", hasher.finalize());
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
    let Ok(db) = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
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
    if value.len() % 2 != 0 {
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

fn lookup_opencode(title: &str, input: &str) -> Option<String> {
    for line in input.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        let Some(item) = parse_json_line(line) else {
            continue;
        };
        let mut name = text_field(&item, "title");
        if name.is_empty() {
            name = text_field(&item, "name");
        }
        let session_id = text_field(&item, "id");
        if name == title && !session_id.is_empty() {
            return Some(session_id);
        }
    }
    None
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
    fn opencode_lookup_matches_title_or_name() {
        let input = concat!(
            "{\"id\":\"s1\",\"title\":\"other\"}\n",
            "not json\n",
            "{\"id\":\"s2\",\"name\":\"target\"}\n",
        );
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
    fn claude_project_matching_accepts_nested_paths() {
        assert!(claude_project_matches("/tmp/project", "/tmp/project"));
        assert!(claude_project_matches("/tmp/project", "/tmp/project/sub"));
        assert!(claude_project_matches("/tmp/project/sub", "/tmp/project"));
        assert!(!claude_project_matches("/tmp/project", "/tmp/other"));
        assert!(claude_project_matches("", "anything"));
    }
}
