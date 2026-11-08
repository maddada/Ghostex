//! What a tool permission card shows: who asks, what for, the command, and why it was flagged.
//!
//! CDXC:SessionChat 2026-10-22 DECISION:
//! User: "please fix this, we're not showing the reason the permission request is appearing! please show this the best way for the user". A Claude Code permission card reads, from gxserver's `permissionRequest`: the description as its title (or the prompt's own question when the subject is a file path), the tool and the asking subagent under it in Claude's words ("Bash command · from the general-purpose agent"), the command, diff or input in a code block, and the reason Claude flagged it as a warning line (an MCP tool's labelled note as a plain one). A folded card keeps all of it with the code cut to a few lines; the chevron opens the rest of the code and every choice. A prompt with no warning stays a title, a source line, the code and the buttons.
//! SEE-ALSO: server/src/session_chat_claude_permission.rs, apps/desktop/src/app/native_chat/permission_card.rs, apps/mobile/app/src/chat/native/cards/PermissionDetails.tsx.

use serde_json::{json, Value};

/// Code lines a folded card shows.
const FOLDED_CODE_LINES: usize = 4;
/// The characters one row of a card's command box holds at its narrowest common width (the
/// Approvals menu, a phone), for counting the rows a long line wraps to.
pub const CODE_ROW_CHARS: usize = 48;

/// The rows `line` wraps to in a command box.
pub fn wrapped_rows(line: &str) -> usize {
    line.chars().count().div_ceil(CODE_ROW_CHARS).max(1)
}

/// CDXC:SessionChat 2026-11-08 WHY:
/// A folded command keeps its first `rows` wrapped rows, not its first lines: a one-line command
/// hundreds of characters long passed the line count whole, filled the Approvals menu and pushed
/// its Allow and Deny out of reach (2026-11-08, "i can't approve when i click on approvals").
/// `(preview, "N more lines")`, or `None` when the whole command fits.
pub fn fold_code(code: &str, rows: usize) -> Option<(String, String)> {
    let total: usize = code.lines().map(wrapped_rows).sum();
    if total <= rows {
        return None;
    }
    let mut preview = Vec::new();
    let mut left = rows;
    for line in code.lines() {
        if left == 0 {
            break;
        }
        let used = wrapped_rows(line);
        if used <= left {
            preview.push(line.to_string());
            left -= used;
        } else {
            let cut: String = line.chars().take(left * CODE_ROW_CHARS - 1).collect();
            preview.push(format!("{}\u{2026}", cut.trim_end()));
            left = 0;
        }
    }
    let more = total - rows;
    Some((
        preview.join("\n"),
        match more {
            1 => "1 more line".to_string(),
            more => format!("{more} more lines"),
        },
    ))
}

const GENERIC_QUESTION: &str = "Do you want to proceed?";

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// A subject that is one path (an Edit or Write prompt names its file), not a sentence.
fn is_path(subject: &str) -> bool {
    !subject.contains(char::is_whitespace) && subject.contains(['/', '.'])
}

/// The card's `approval` presentation for a notice carrying `permissionRequest`, or `None`.
pub fn presentation(request: &Value, notice_title: &str) -> Option<Value> {
    let source = text(request, "source")?;
    let subject = text(request, "subject");
    let question = text(request, "question").unwrap_or_else(|| GENERIC_QUESTION.to_string());
    let path = subject
        .as_deref()
        .filter(|subject| is_path(subject))
        .map(str::to_string);
    let title = match (&subject, &path) {
        (Some(subject), None) => subject.clone(),
        _ if question != GENERIC_QUESTION => question,
        _ => notice_title.to_string(),
    };
    let content = text(request, "content");
    let hidden_by_server = request
        .get("contentHiddenLines")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let folded = content
        .as_deref()
        .and_then(|content| fold_code(content, FOLDED_CODE_LINES));
    let note = text(request, "note");
    let reason = note.map(|note| {
        let label = text(request, "noteLabel");
        json!({
            "tone": if label.is_some() { "info" } else { "warning" },
            "label": label,
            "text": note,
        })
    });
    Some(json!({
        "title": title,
        "source": source,
        "path": path,
        "code": content,
        "codePreview": folded.as_ref().map(|(preview, _)| preview.clone()),
        "codeMore": folded.map(|(_, more)| more),
        "codeCut": (hidden_by_server > 0)
            .then(|| format!("{hidden_by_server} more lines in the terminal")),
        "reason": reason,
    }))
}
