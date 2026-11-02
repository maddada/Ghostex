//! How a side chat starts: its marker on the session row, its title, and the agent command
//! without accept-all.

use serde_json::{json, Value};

/// The runtime setting that makes a session a side chat, holding its main session's id.
pub(crate) const PARENT_KEY: &str = "sideChatParentSessionId";

/// A side chat's own name, its first question. The forked agent reports its main session's name
/// (Claude's fork copies it), so the name a side chat shows is kept apart from the agent's.
const TITLE_KEY: &str = "sideChatTitle";

/// The name a side chat shows: its question, when the session is a side chat that has one.
pub(crate) fn side_chat_title(session: &Value) -> Option<String> {
    parent_of(session)?;
    session
        .pointer(&format!("/runtimeSettings/{TITLE_KEY}"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The main session a session is a side chat of.
pub(crate) fn parent_of(session: &Value) -> Option<String> {
    session
        .pointer(&format!("/runtimeSettings/{PARENT_KEY}"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// A side chat's name: its first question, cut to fit a tab.
pub(crate) fn title_for(question: &str) -> String {
    let line = question
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Side chat");
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    title
}

/// Marks fork params as a side chat of `parent_session_id`: its own title, which no fork rename or
/// first-prompt title replaces, and the parent link the session lists and the pill read.
pub(crate) fn mark_fork_params(params: &mut Value, parent_session_id: &str, question: &str) {
    params["title"] = json!(title_for(question));
    if let Some(runtime) = params
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
    {
        runtime.insert(PARENT_KEY.to_string(), json!(parent_session_id));
        runtime.insert(TITLE_KEY.to_string(), json!(title_for(question)));
        runtime.insert("forkFirstPromptAutoTitlePending".to_string(), json!(false));
        runtime.insert(
            "gxserverForkInitialRenameStatus".to_string(),
            json!("applied"),
        );
        runtime.insert("titleSource".to_string(), json!("user"));
    }
    if let Some(launch) = params
        .get_mut("launchSettings")
        .and_then(Value::as_object_mut)
    {
        launch.insert(PARENT_KEY.to_string(), json!(parent_session_id));
    }
}

/// Flags that make an agent act without asking, by agent: ones that stand alone, and ones that take
/// a value.
fn auto_approve_flags(agent_id: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match agent_id {
        "claude" | "openclaude" => (
            &[
                "--dangerously-skip-permissions",
                "--allow-dangerously-skip-permissions",
            ],
            &["--permission-mode"],
        ),
        "codex" => (
            &[
                "--yolo",
                "--full-auto",
                "--dangerously-bypass-approvals-and-sandbox",
            ],
            &["-a", "--ask-for-approval", "-s", "--sandbox"],
        ),
        "grok" => (&["--always-approve"], &["--permission-mode"]),
        "copilot" => (&["--allow-all", "--allow-all-tools", "--yolo"], &[]),
        _ => (&[], &[]),
    }
}

/// What makes an agent ask before it edits, added after the flags above are gone. Agents without
/// a permission mode (Pi, Empryo) start as they are; Grok Build and Copilot ask by default.
fn ask_before_edits_flags(agent_id: &str) -> &'static str {
    match agent_id {
        "claude" | "openclaude" => "--permission-mode default",
        "codex" => "-s workspace-write -a on-request",
        _ => "",
    }
}

/// CDXC:SideChats 2026-11-02 DECISION:
/// User: a side chat starts in ask-before-edits (Claude Manual, Codex Ask for approval), whatever the main session runs in, because it works in the same folder while the main session may still be editing there. Accept-all is taken off the fork's command, then the agent's ask mode is added.
pub(crate) fn ask_before_edits_command(command: &str, agent_id: &str) -> String {
    let (single, paired) = auto_approve_flags(agent_id);
    // Words with their byte ranges, so what is kept stays exactly as written (quoted paths with
    // spaces included).
    let mut words = Vec::new();
    let mut word_start = None;
    for (index, character) in command.char_indices() {
        match (character.is_whitespace(), word_start) {
            (true, Some(start)) => {
                words.push((start, index));
                word_start = None;
            }
            (false, None) => word_start = Some(index),
            _ => {}
        }
    }
    if let Some(start) = word_start {
        words.push((start, command.len()));
    }
    let mut dropped = vec![false; words.len()];
    let mut index = 0;
    while index < words.len() {
        let word = &command[words[index].0..words[index].1];
        let inline_value = |flag: &&str| {
            word.strip_prefix(*flag)
                .is_some_and(|rest| rest.starts_with('='))
        };
        if single.contains(&word) || single.iter().chain(paired).any(inline_value) {
            dropped[index] = true;
        } else if paired.contains(&word) {
            dropped[index] = true;
            if index + 1 < words.len() {
                dropped[index + 1] = true;
                index += 1;
            }
        }
        index += 1;
    }
    let mut output = String::with_capacity(command.len() + 40);
    let mut cursor = 0;
    for ((start, end), drop) in words.iter().zip(&dropped) {
        if *drop {
            output.push_str(command[cursor..*start].trim_end());
            cursor = *end;
        }
    }
    output.push_str(&command[cursor..]);
    let mut output = output.trim().to_string();
    let ask = ask_before_edits_flags(agent_id);
    if !ask.is_empty() {
        output.push(' ');
        output.push_str(ask);
    }
    output
}
