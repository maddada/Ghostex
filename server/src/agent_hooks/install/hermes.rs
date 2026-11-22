use std::{fs, path::Path};

use serde_json::{json, Value};

use crate::domain::DomainStateError;

use crate::agent_hooks::plugin_sources::{shell_quote, yaml_double_quote};
use crate::agent_hooks::probing::{io_error, json_error, temp_path_for};
use crate::agent_hooks::resolution::text_contains_ghostex_owned_hook_command;

/*
The agent consents to each configured shell hook per (event, command) pair: an
unapproved pair triggers a TTY prompt on every interactive launch, and is
silently skipped on non-interactive ones — which would mean ten prompts per
fresh machine, and no session-state events at all from gateway or automation
runs. Approvals live in `<config dir>/shell-hooks-allowlist.json` keyed by the
exact event name and command string, so installing the hooks and approving
them are one act of the same consent: the user already authorized Ghostex's
hook integration.
*/
const HERMES_HOOK_EVENTS: [(&str, u32); 10] = [
    ("on_session_start", 5),
    ("pre_llm_call", 5),
    ("post_llm_call", 5),
    ("pre_approval_request", 5),
    ("post_approval_response", 5),
    ("on_session_end", 5),
    ("on_session_finalize", 5),
    ("on_session_reset", 5),
    ("pre_tool_call", 120),
    ("post_tool_call", 120),
];

const HERMES_ALLOWLIST_FILE: &str = "shell-hooks-allowlist.json";

/*
The config's `hooks:` section may already belong to the user. YAML keeps only
the last duplicate of a mapping key, so appending a second top-level `hooks:`
block would silently drop either the user's hooks or ours depending on file
order. Instead the entries merge into the existing section: events the user
already configures get our list items inserted inside a marked block under
their event line, and the rest land as new event sections in one marked block
directly under `hooks:`. Marked blocks are removed by `without_marked_block`
wherever they sit, so uninstall stays a pure block deletion.
*/
pub(super) fn merge_hermes_hook_block(
    lines: &mut Vec<String>,
    begin_marker: &str,
    end_marker: &str,
    shell_command: &str,
) {
    let hook_entry_lines = |item_indent: &str, timeout: u32| {
        vec![
            format!(
                "{item_indent}- command: {}",
                yaml_double_quote(shell_command)
            ),
            format!("{item_indent}  timeout: {timeout}"),
        ]
    };

    let Some(hooks_index) = hermes_hooks_line_index(lines) else {
        if lines.last().map(|line| !line.trim().is_empty()) == Some(true) {
            lines.push(String::new());
        }
        lines.push(begin_marker.to_string());
        lines.push("hooks:".to_string());
        for (event, timeout) in HERMES_HOOK_EVENTS {
            lines.push(format!("  {event}:"));
            lines.extend(hook_entry_lines("    ", timeout));
        }
        lines.push(end_marker.to_string());
        return;
    };

    // `hooks: {}` / `hooks: []` mean "no hooks", which is also what the bare
    // key means once the block entries below it exist.
    lines[hooks_index] = "hooks:".to_string();
    let child_indent = hermes_hooks_child_indent(lines, hooks_index);
    /*
    An entry of ours that is no longer inside a marked block — written by an
    older installer, or left behind when a marker was edited away — would
    otherwise be joined by the marked copy added below and run the hook twice
    per event. Ownership is decided by the command, so a hook the user wrote
    is never touched.
    */
    remove_unmarked_hermes_hook_entries(lines, hooks_index, &child_indent);
    let existing_events = hermes_direct_event_line_indexes(lines, hooks_index, &child_indent);

    let mut missing_events: Vec<(&str, u32)> = Vec::new();
    let mut matched_events: Vec<(u32, usize)> = Vec::new();
    for (event, timeout) in HERMES_HOOK_EVENTS {
        match existing_events.get(event) {
            Some(&event_index) => matched_events.push((timeout, event_index)),
            None => missing_events.push((event, timeout)),
        }
    }

    // Bottom-up so the earlier indexes stay valid across insertions.
    matched_events.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (timeout, event_index) in matched_events {
        if let Some(colon) = lines[event_index].find(':') {
            lines[event_index].truncate(colon + 1);
        }
        let item_indent = format!("{child_indent}  ");
        let mut block = vec![format!("{item_indent}{begin_marker}")];
        block.extend(hook_entry_lines(&item_indent, timeout));
        block.push(format!("{item_indent}{end_marker}"));
        lines.splice(event_index + 1..event_index + 1, block);
    }

    if !missing_events.is_empty() {
        let mut block = vec![format!("{child_indent}{begin_marker}")];
        for (event, timeout) in missing_events {
            block.push(format!("{child_indent}{event}:"));
            block.extend(hook_entry_lines(&format!("{child_indent}  "), timeout));
        }
        block.push(format!("{child_indent}{end_marker}"));
        lines.splice(hooks_index + 1..hooks_index + 1, block);
    }
}

/// Drops every list item under `hooks:` that runs one of our hook commands,
/// leaving the user's own entries and the event keys themselves in place. An
/// item is its `- ` line plus every deeper-indented line that follows it.
fn remove_unmarked_hermes_hook_entries(
    lines: &mut Vec<String>,
    hooks_index: usize,
    child_indent: &str,
) {
    let mut index = hooks_index + 1;
    while index < lines.len() {
        let line = &lines[index];
        if !line.trim().is_empty() && !line.starts_with(child_indent) {
            break;
        }
        let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
        if !line.trim_start().starts_with("- ") || indent.len() <= child_indent.len() {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < lines.len() {
            let next = &lines[end];
            let next_indent: String = next.chars().take_while(|ch| ch.is_whitespace()).collect();
            if next.trim().is_empty()
                || next.trim_start().starts_with("- ")
                || next_indent.len() <= indent.len()
            {
                break;
            }
            end += 1;
        }
        if text_contains_ghostex_owned_hook_command(&lines[index..end].join("\n")) {
            lines.drain(index..end);
            continue;
        }
        index = end;
    }
}

/// The top-level `hooks:` key: unindented, with nothing after the colon except
/// an inline empty value or a comment.
fn hermes_hooks_line_index(lines: &[String]) -> Option<usize> {
    lines.iter().position(|line| {
        let Some(suffix) = line.strip_prefix("hooks:") else {
            return false;
        };
        hermes_suffix_is_inline_empty(suffix)
    })
}

fn hermes_suffix_is_inline_empty(suffix: &str) -> bool {
    let uncommented = suffix.split('#').next().unwrap_or("").trim();
    uncommented.is_empty() || uncommented == "{}" || uncommented == "[]"
}

/// Indentation of the section's direct children, read from the first child
/// rather than assumed, so a config indented with four spaces keeps one
/// consistent sibling indent after the merge.
fn hermes_hooks_child_indent(lines: &[String], hooks_index: usize) -> String {
    for line in lines.iter().skip(hooks_index + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
        if indent.is_empty() {
            break;
        }
        return indent;
    }
    "  ".to_string()
}

/// Direct children of `hooks:` whose value is a block (or inline empty), keyed
/// by event name. An event holding a non-empty inline value cannot take merged
/// list items, so it is deliberately absent here.
fn hermes_direct_event_line_indexes(
    lines: &[String],
    hooks_index: usize,
    child_indent: &str,
) -> std::collections::HashMap<String, usize> {
    let mut indexes = std::collections::HashMap::new();
    for (offset, line) in lines.iter().enumerate().skip(hooks_index + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !line.starts_with(child_indent) {
            break;
        }
        let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
        if indent != child_indent {
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        if hermes_suffix_is_inline_empty(&trimmed[colon + 1..]) {
            indexes.insert(trimmed[..colon].to_string(), offset);
        }
    }
    indexes
}

/// The exact command string the agent parses back out of the YAML entry; the
/// allowlist matches on it byte-for-byte. On macOS and Linux the notify
/// command's `GHOSTEX_AGENT=` prefix needs a shell; the Windows command is
/// already the plain argv Hermes starts (`windows::hermes_command`).
pub(super) fn hermes_hook_shell_command(command: &str) -> String {
    if cfg!(windows) {
        return command.to_string();
    }
    format!("sh -c {}", shell_quote(command))
}

/// Reconcile the approvals file with what is installed. `installed_command` is
/// the one command every event entry runs; `None` means uninstall. Foreign
/// approvals are preserved verbatim; stale Ghostex-owned approvals (an older
/// hook path, a removed event) are pruned so the file tracks exactly the
/// installed set. The read-modify-write is serialized against the agent's own
/// writers through the sibling `.lock` file, and the rewrite lands via
/// temp-file + rename so a concurrent reader never sees a torn file.
pub(super) fn update_hermes_shell_hook_allowlist(
    config_path: &Path,
    installed_command: Option<&str>,
) -> Result<(), DomainStateError> {
    let Some(config_dir) = config_path.parent() else {
        return Ok(());
    };
    let allowlist_path = config_dir.join(HERMES_ALLOWLIST_FILE);
    if installed_command.is_none() && !allowlist_path.is_file() {
        return Ok(());
    }
    fs::create_dir_all(config_dir).map_err(io_error)?;

    let lock_path = config_dir.join(format!("{HERMES_ALLOWLIST_FILE}.lock"));
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&lock_path)
        .map_err(io_error)?;
    let _lock = HermesAllowlistLock::acquire(&lock_file)?;

    let mut root = fs::read(&allowlist_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    let approvals = match root.get("approvals") {
        Some(Value::Array(entries)) => entries.clone(),
        _ => Vec::new(),
    };

    let installed: Vec<(&str, &str)> = installed_command
        .map(|command| {
            HERMES_HOOK_EVENTS
                .iter()
                .map(|(event, _)| (*event, command))
                .collect()
        })
        .unwrap_or_default();
    let mut next: Vec<Value> = approvals
        .into_iter()
        .filter(|entry| {
            let event = entry.get("event").and_then(Value::as_str);
            let command = entry.get("command").and_then(Value::as_str);
            let (Some(event), Some(command)) = (event, command) else {
                return true;
            };
            if installed
                .iter()
                .any(|(installed_event, installed_command)| {
                    *installed_event == event && *installed_command == command
                })
            {
                return true;
            }
            !text_contains_ghostex_owned_hook_command(command)
        })
        .collect();
    let approved_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for (event, command) in &installed {
        let already_present = next.iter().any(|entry| {
            entry.get("event").and_then(Value::as_str) == Some(*event)
                && entry.get("command").and_then(Value::as_str) == Some(*command)
        });
        if !already_present {
            next.push(json!({
                "event": event,
                "command": command,
                "approved_at": approved_at,
            }));
        }
    }
    root.insert("approvals".to_string(), Value::Array(next));

    let serialized = serde_json::to_string_pretty(&Value::Object(root)).map_err(json_error)?;
    let temp_path = temp_path_for(&allowlist_path);
    fs::write(&temp_path, format!("{serialized}\n")).map_err(io_error)?;
    fs::rename(&temp_path, &allowlist_path).map_err(io_error)
}

/// Exclusive advisory lock on the allowlist's sibling lock file, released on
/// drop. On non-Unix targets the atomic rename alone carries the write.
struct HermesAllowlistLock<'a> {
    #[cfg_attr(not(unix), allow(dead_code))]
    file: &'a fs::File,
}

impl<'a> HermesAllowlistLock<'a> {
    fn acquire(file: &'a fs::File) -> Result<Self, DomainStateError> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result != 0 {
                return Err(io_error(std::io::Error::last_os_error()));
            }
        }
        Ok(Self { file })
    }
}

#[cfg(unix)]
impl Drop for HermesAllowlistLock<'_> {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}
