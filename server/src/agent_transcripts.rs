use std::{fs, path::PathBuf, time::SystemTime};

use serde_json::Value;

use crate::resume_lookup::{
    codex_homes, codex_transcript_paths, expand_home, home_dir, parse_json_line, read_lines_lossy,
    text_from_message,
};

/*
CDXC:SessionHistoryTitleSource 2026-07-29:
Rename-modal "Generate Name" with an empty title needs the session's recent
user-sent messages as the generation source. Agent hooks already persist the
provider session identity (`agentSessionId`, and for Claude the transcript
path in `agentSessionPath`), so this module resolves the provider transcript
on disk and extracts the visible user prompts from it. Transcript locations
and per-agent record shapes mirror `resume_lookup`, `ghostex-history`, and
zehn: Claude project transcripts are `<id>.jsonl` role records under the
`.claude`/`.claude-profiles` projects dirs, Codex rollouts are
`response_item` payloads under the codex-home sessions dirs, and Cursor
transcripts live under `.cursor/projects` in `agent-transcripts/<id>`.
*/

pub fn recent_session_user_prompts(
    agent: &str,
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
) -> Vec<String> {
    let Some(transcript_path) =
        resolve_session_transcript_path(agent, agent_session_id, agent_session_path)
    else {
        return Vec::new();
    };
    let Some(lines) = read_lines_lossy(&transcript_path) else {
        return Vec::new();
    };
    match agent {
        "antigravity" => antigravity_user_prompts(&lines),
        "claude" => claude_user_prompts(&lines),
        "codex" => codex_user_prompts(&lines),
        "cursor" => cursor_user_prompts(&lines),
        _ => Vec::new(),
    }
}

pub fn agent_supports_session_history_title_source(agent: Option<&str>) -> bool {
    matches!(agent, Some("antigravity" | "claude" | "codex" | "cursor"))
}

/*
CDXC:SessionChatAntigravity 2026-09-03:
Antigravity's hooks carry the conversation id and the step log path but never
the prompt text, so the hook receiver reads the first prompt from the log
itself (the `USER_INPUT` step is appended the moment the user submits, before
`PreInvocation` fires). Same extractor as the title source below.
*/
pub(crate) fn first_antigravity_user_prompt(transcript_path: &str) -> Option<String> {
    let path = expand_home(transcript_path.trim());
    let lines = read_lines_lossy(&path)?;
    antigravity_user_prompts(&lines).into_iter().next()
}

pub(crate) fn resolve_session_transcript_path(
    agent: &str,
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = agent_session_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let expanded = expand_home(path);
        if expanded.is_file() {
            return Some(expanded);
        }
    }
    let session_id = agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    match agent {
        "antigravity" => {
            crate::session_chat_antigravity_mirror::find_antigravity_transcript(session_id)
        }
        "claude" => find_claude_transcript(session_id),
        "codex" => find_codex_transcript(session_id),
        "cursor" => find_cursor_transcript(session_id),
        _ => None,
    }
}

/// Claude transcript roots shared with the Session Chat resolver
/// (`session_chat.rs`), which also scans them for hook `session_id`s that
/// diverge from the file-name `sessionId` on resumed/forked sessions.
pub(crate) fn claude_project_roots() -> Vec<PathBuf> {
    let home = home_dir();
    let mut roots = vec![
        home.join(".claude").join("projects"),
        home.join(".claude").join("projects2"),
    ];
    if let Ok(profiles) = fs::read_dir(home.join(".claude-profiles")) {
        for profile in profiles.flatten() {
            roots.push(profile.path().join("projects"));
            roots.push(profile.path().join("projects2"));
        }
    }
    roots
}

pub(crate) fn find_claude_transcript(session_id: &str) -> Option<PathBuf> {
    let file_name = format!("{session_id}.jsonl");
    let mut candidates: Vec<PathBuf> = Vec::new();
    for root in claude_project_roots() {
        let Ok(project_dirs) = fs::read_dir(&root) else {
            continue;
        };
        for project_dir in project_dirs.flatten() {
            let candidate = project_dir.path().join(&file_name);
            if candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }
    newest_file(candidates)
}

pub(crate) fn find_codex_transcript(session_id: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for codex_home in codex_homes() {
        candidates.extend(
            codex_transcript_paths(&codex_home, session_id, None)
                .into_iter()
                .filter(|path| path.is_file()),
        );
    }
    newest_file(candidates)
}

fn find_cursor_transcript(session_id: &str) -> Option<PathBuf> {
    let projects = home_dir().join(".cursor").join("projects");
    let entries = fs::read_dir(&projects).ok()?;
    for project_dir in entries.flatten() {
        let candidate = project_dir
            .path()
            .join("agent-transcripts")
            .join(session_id)
            .join(format!("{session_id}.jsonl"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn newest_file(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().max_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    })
}

/// Antigravity step log: `USER_EXPLICIT` / `USER_INPUT` steps whose `content`
/// wraps the typed prompt in `<USER_REQUEST>` beside harness metadata tags.
fn antigravity_user_prompts(lines: &[String]) -> Vec<String> {
    let mut prompts: Vec<String> = Vec::new();
    for line in lines {
        let Some(item) = parse_json_line(line) else {
            continue;
        };
        if item.get("source").and_then(Value::as_str) != Some("USER_EXPLICIT")
            || item.get("type").and_then(Value::as_str) != Some("USER_INPUT")
        {
            continue;
        }
        let Some(content) = item.get("content").and_then(Value::as_str) else {
            continue;
        };
        let text = crate::session_chat_antigravity_mirror::antigravity_user_text(content);
        if !text.is_empty() {
            prompts.push(text);
        }
    }
    prompts
}

fn claude_user_prompts(lines: &[String]) -> Vec<String> {
    let mut prompts: Vec<String> = Vec::new();
    for line in lines {
        let Some(item) = parse_json_line(line) else {
            continue;
        };
        if item.get("isMeta").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if item.get("type").and_then(Value::as_str) != Some("user") {
            continue;
        }
        push_visible_user_prompt(&mut prompts, item.get("message"));
    }
    prompts
}

fn codex_user_prompts(lines: &[String]) -> Vec<String> {
    let mut prompts: Vec<String> = Vec::new();
    for line in lines {
        let Some(item) = parse_json_line(line) else {
            continue;
        };
        if item.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        let Some(payload) = item.get("payload") else {
            continue;
        };
        if payload.get("type").and_then(Value::as_str) != Some("message")
            || payload.get("role").and_then(Value::as_str) != Some("user")
        {
            continue;
        }
        push_visible_user_prompt(&mut prompts, Some(payload));
    }
    prompts
}

fn cursor_user_prompts(lines: &[String]) -> Vec<String> {
    let mut prompts: Vec<String> = Vec::new();
    for line in lines {
        let Some(item) = parse_json_line(line) else {
            continue;
        };
        if item.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        push_visible_user_prompt(&mut prompts, item.get("message"));
    }
    prompts
}

fn push_visible_user_prompt(prompts: &mut Vec<String>, message: Option<&Value>) {
    let Some(message) = message else {
        return;
    };
    if message
        .get("role")
        .and_then(Value::as_str)
        .map(|role| role != "user")
        .unwrap_or(false)
    {
        return;
    }
    if content_is_tool_result(message.get("content")) {
        return;
    }
    let text = text_from_message(Some(message));
    // Cursor records the real typed prompt wrapped in <user_query> tags.
    let text = tagged_text(&text, "user_query").unwrap_or(text);
    let trimmed = text.trim();
    if trimmed.is_empty() || !is_visible_user_prompt(trimmed) {
        return;
    }
    // Immediate retries of the same prompt add no naming signal.
    if prompts.last().map(String::as_str) == Some(trimmed) {
        return;
    }
    prompts.push(trimmed.to_string());
}

fn tagged_text(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(&close)?;
    Some(rest[..end].trim().to_string())
}

fn content_is_tool_result(content: Option<&Value>) -> bool {
    let Some(Value::Array(items)) = content else {
        return false;
    };
    !items.is_empty()
        && items.iter().all(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("tool_result" | "toolResult")
            ) || item.get("tool_use_id").is_some()
        })
}

/*
Agent CLIs wrap injected context in tag-prefixed pseudo-user records
(`<user_instructions>`, `<environment_context>`, Claude's `<command-name>`
wrappers, ...), record interruption markers as user turns, and Ghostex itself
stages `/rename <title>` commands into the composer. None of that is text the
user actually asked for, so it must not feed name generation.
*/
fn is_visible_user_prompt(text: &str) -> bool {
    if text.starts_with('<') {
        return false;
    }
    if text.starts_with("# AGENTS.md instructions")
        || text.starts_with("[Request interrupted")
        || text.starts_with("Caveat: The messages below")
    {
        return false;
    }
    if let Some(first_token) = text.split_whitespace().next() {
        let is_bare_slash_command = first_token.starts_with('/')
            && first_token
                .chars()
                .skip(1)
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            && !text.contains('\n');
        if is_bare_slash_command {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|line| line.to_string()).collect()
    }

    #[test]
    fn claude_prompts_keep_real_user_turns_only() {
        let transcript = lines(&[
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"Caveat: The messages below were generated"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"<command-name>/clear</command-name>"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"Fix the sidebar drag bug"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"On it"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok","tool_use_id":"t1"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Now add tests"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":"/rename Sidebar drag fix"}}"#,
        ]);
        assert_eq!(
            claude_user_prompts(&transcript),
            vec![
                "Fix the sidebar drag bug".to_string(),
                "Now add tests".to_string()
            ]
        );
    }

    #[test]
    fn codex_prompts_skip_injected_context_and_tool_records() {
        let transcript = lines(&[
            r#"{"type":"session_meta","payload":{"id":"abc","cwd":"/tmp/project"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<user_instructions>be terse</user_instructions>"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>cwd</environment_context>"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Port the click reflow logic"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{}"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Port the click reflow logic"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Ship it to the gpui app too"}]}}"#,
        ]);
        assert_eq!(
            codex_user_prompts(&transcript),
            vec![
                "Port the click reflow logic".to_string(),
                "Ship it to the gpui app too".to_string()
            ]
        );
    }

    #[test]
    fn cursor_prompts_read_role_records_and_unwrap_user_query() {
        let transcript = lines(&[
            r#"{"role":"user","message":{"content":"<user_query>\nWire the new settings pane\n</user_query>"}}"#,
            r#"{"role":"assistant","message":{"content":[{"type":"text","text":"sure"}]}}"#,
            r#"{"role":"user","message":{"content":[{"type":"tool_result","content":"ok","tool_use_id":"t1"}]}}"#,
            r#"{"role":"user","message":{"content":[{"type":"text","text":"Use the same layout as gpui"}]}}"#,
            r#"{"role":"user","message":{"content":"<user_query>/rename Settings pane</user_query>"}}"#,
        ]);
        assert_eq!(
            cursor_user_prompts(&transcript),
            vec![
                "Wire the new settings pane".to_string(),
                "Use the same layout as gpui".to_string()
            ]
        );
    }

    #[test]
    fn visible_user_prompt_filter_keeps_path_prompts_but_drops_slash_commands() {
        assert!(!is_visible_user_prompt("/rename Sidebar drag fix"));
        assert!(!is_visible_user_prompt("/compact"));
        assert!(is_visible_user_prompt("/Users/madda/dev/app.ts is broken"));
        assert!(is_visible_user_prompt("fix /rename handling in the modal"));
    }
}
