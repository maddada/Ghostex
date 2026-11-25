//! The one-line labels simple mode puts on a folded group.
//!
//! Ported from `packages/shared/session-chat-presentation/simple.ts`.

/// "Edited 3 files", the label a folded turn's file changes carry.
pub fn simple_edit_label(count: usize) -> String {
    format!(
        "Edited {count} {}",
        if count == 1 { "file" } else { "files" }
    )
}

/// Whether a file-edit tool call has answered (`Done`), failed/been denied (`Failed`), or is still
/// waiting on approval or execution (`Running`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileCallStatus {
    Done,
    Running,
    Failed,
}

/// Reads a projected file row's `callStatus`, falling back to `failed` for rows without one.
pub fn file_call_status_of(file: &serde_json::Value) -> FileCallStatus {
    match file.get("callStatus").and_then(serde_json::Value::as_str) {
        Some("running") => FileCallStatus::Running,
        Some("failed") => FileCallStatus::Failed,
        Some("done") => FileCallStatus::Done,
        _ if file.get("failed") == Some(&serde_json::Value::Bool(true)) => FileCallStatus::Failed,
        _ => FileCallStatus::Done,
    }
}

/// CDXC:SessionChat 2026-10-17 WHY:
/// A Codex edit the user denied ("aborted by user", "patch rejected by user") still carries its patch, so the card counted it and read "Edited 1 file" while the file never changed. Only files with at least one applied change count as edited; files whose every change failed or was denied are counted apart, so the label says so ("Failed to edit 1 file", "Edited 2 files, 1 failed") and the row keeps its "Failed" tag and the agent's reason.
pub fn edit_label(edited: usize, failed: usize) -> String {
    let files = |count: usize| if count == 1 { "file" } else { "files" };
    match (edited, failed) {
        (_, 0) => simple_edit_label(edited),
        (0, _) => format!("Failed to edit {failed} {}", files(failed)),
        _ => format!("Edited {edited} {}, {failed} failed", files(edited)),
    }
}

/// Like [`edit_label`], using progressive tense ("Editing 1 file") while any edit in the group is
/// still waiting on approval or running (`any_running`).
pub fn file_edit_label(edited: usize, failed: usize, pending: usize, any_running: bool) -> String {
    if !any_running {
        return edit_label(edited, failed);
    }
    let files = |count: usize| if count == 1 { "file" } else { "files" };
    let active = edited + pending;
    if failed == 0 {
        format!("Editing {active} {}", files(active))
    } else {
        format!("Editing {active} {}, {failed} failed", files(active))
    }
}

/// Splits file rows into unique paths that completed (`edited`), paths whose every change failed
/// (`failed_only`), paths that are still waiting/running and not yet completed (`pending_only`),
/// and whether any row is still running (`any_running`).
pub fn file_edit_paths<'a>(
    rows: impl IntoIterator<Item = (&'a str, FileCallStatus)>,
) -> (Vec<&'a str>, Vec<&'a str>, Vec<&'a str>, bool) {
    let rows: Vec<(&str, FileCallStatus)> = rows.into_iter().collect();
    let mut edited: Vec<&str> = Vec::new();
    let mut pending_only: Vec<&str> = Vec::new();
    let mut failed_only: Vec<&str> = Vec::new();
    let mut any_running = false;
    for (path, status) in &rows {
        if *status == FileCallStatus::Running {
            any_running = true;
        }
        if *status == FileCallStatus::Done && !edited.contains(path) {
            edited.push(path);
        }
    }
    for (path, status) in &rows {
        if *status == FileCallStatus::Running
            && !edited.contains(path)
            && !pending_only.contains(path)
        {
            pending_only.push(path);
        }
    }
    for (path, status) in &rows {
        if *status == FileCallStatus::Failed
            && !edited.contains(path)
            && !pending_only.contains(path)
            && !failed_only.contains(path)
        {
            failed_only.push(path);
        }
    }
    (edited, failed_only, pending_only, any_running)
}

/// The paths a list of file rows edited, and the paths whose every change failed.
pub fn edited_and_failed_paths<'a>(
    rows: impl IntoIterator<Item = (&'a str, bool)>,
) -> (Vec<&'a str>, Vec<&'a str>) {
    let (edited, failed_only, _, _) = file_edit_paths(rows.into_iter().map(|(path, failed)| {
        (
            path,
            if failed {
                FileCallStatus::Failed
            } else {
                FileCallStatus::Done
            },
        )
    }));
    (edited, failed_only)
}

/// "3 tool calls", or "Tool output" when the group has no call of its own.
pub fn tool_count_label(count: usize) -> String {
    if count == 0 {
        "Tool output".to_string()
    } else {
        format!("{count} tool {}", if count == 1 { "call" } else { "calls" })
    }
}

/// The title of a folded group of tool calls (`simpleToolLabel`): the call's own words when the
/// group holds one ("Install emulator and Play Store system image", its description or the label
/// `tool_label` reads off it, as its row says), else "3 tool calls" ([`tool_count_label`]).
///
/// CDXC:SessionChat 2026-11-15 DECISION: User, of an opened Quiet View line drawn as a row of
/// cards each saying only "1 tool call": a card holding one call says what that call did, never a
/// bare "1 tool call". Every host draws this label for the group (desktop `tool_run.rs`, phone
/// `ToolRows.tsx`); a row with no label of its own keeps the count.
pub fn calls_label(calls: &[&serde_json::Value]) -> String {
    match calls {
        [only] => only["label"]
            .as_str()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map_or_else(|| tool_count_label(1), str::to_string),
        _ => tool_count_label(calls.len()),
    }
}

/// The title of an opened group of tool calls (`simpleToolOpenLabel`): what kind of call a group
/// of one was ("Read 1 file", "Ran 1 command", "Searched 1 time"), read off its row's `verb`, or
/// "1 tool call" when the kind has no count of its own; a longer group keeps [`calls_label`].
///
/// CDXC:SessionChat 2026-11-25 DECISION: User, of an opened one-call card whose header and only
/// row both read "Read …/Screenshots/2026-11-25_Ghostex_21-03-28@2x.png", picking B on
/// ~/.local/share/agent-work/Ghostex/tool-dup/page.html: once opened, the header says what kind of
/// call it is and the row under it says which, the same shape as a group of several calls. The
/// row keeps its verb, as a longer group's rows do. Shut, the card keeps the call's own words
/// ([`calls_label`], the 2026-11-15 decision above), since nothing under it repeats them. Every
/// host draws this while the group is open (desktop `tool_run.rs`, phone `ToolRows.tsx`).
pub fn calls_open_label(calls: &[&serde_json::Value]) -> String {
    let [only] = calls else {
        return calls_label(calls);
    };
    match only["verb"].as_str().unwrap_or_default() {
        verb @ ("Read" | "Viewed" | "Edited" | "Wrote" | "Created" | "Deleted" | "Moved"
        | "Copied" | "Downloaded") => format!("{verb} 1 file"),
        "Counted lines in" => "Read 1 file".to_string(),
        "Ran"
        | "Ran a script"
        | "Sent input to a running command"
        | "Waited for a running command" => "Ran 1 command".to_string(),
        "Searched for" | "Found files" | "Searched the page for" => "Searched 1 time".to_string(),
        "Searched the web for" => "Searched the web 1 time".to_string(),
        verb @ ("Fetched" | "Opened") => format!("{verb} 1 page"),
        "Listed" => "Listed 1 folder".to_string(),
        _ => tool_count_label(1),
    }
}
