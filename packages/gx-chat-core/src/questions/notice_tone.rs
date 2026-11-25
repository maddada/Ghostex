//! The tone a notice or approval card is drawn in, and the plain-words title and detail line of an
//! approval card. Each renderer maps the tone name to its hue and glyph through
//! `packages/gx-chat-core/visual/notice-tone.json`.

use crate::extras::welcome::welcome_agent_name;
use crate::questions::model::TerminalNotice;

/// CDXC:SessionChat 2026-10-11 DECISION:
/// User picked the "proposed" notices and approvals design (docs/2026-10-10/chat-cards-questions/05-notices.html): each notice and approval card carries a faint wash of its tone (amber for waits, orange for approvals, rose for errors), a tinted icon tile beside a title and a muted detail line, one primary action with keyboard hints on approvals, and the other actions as secondary buttons. User: "please tone down the bg colors", so the wash and tile fills are quieter than the mockup's while text keeps its contrast. User: "please dont do the stuff about named this session and context compacted, keep those as they are", so the transcript's auto-named and context-compacted rows keep their old look.
/// SEE-ALSO: packages/gx-chat-core/visual/notice-tone.json, apps/desktop/src/app/native_chat/tone_card.rs, apps/mobile/app/src/chat/native/cards/ToneCard.tsx.
pub fn notice_tone(notice: &TerminalNotice) -> &'static str {
    // A terminal panel the user opened (a `/` command's dialog) is not a warning of any kind.
    let standalone_dialog = notice
        .dialog
        .as_ref()
        .is_some_and(|dialog| dialog.rows.is_empty());
    if standalone_dialog {
        return "terminal";
    }
    match notice.kind.as_str() {
        "permissionPrompt" | "trustPrompt" | "permissionsWarning" | "switchConfirmPrompt" => {
            return "approval";
        }
        "usageLimit" | "resumePrompt" | "sessionPausedPrompt" | "queuedInput" => return "wait",
        // A sign-in the user is finishing ("Finish signing in", Q1103 and Q1107) is a wait; gxserver
        // sends it as a warning, and an expired login stays an error.
        "loginExpired" if notice.severity == "warning" => return "wait",
        _ => {}
    }
    match notice.severity.as_str() {
        "error" => "error",
        "warning" => "warning",
        _ => "info",
    }
}

/// Whether a projected `terminalNotice` is an error, warning, or wait/issue banner (send failure,
/// agent error, terminal issue, usage limit, sign-in required, memory guard, etc.) rather than an
/// interactive dialog, picker, passthrough, side question, or approval prompt.
///
/// CDXC:SessionChat 2026-11-24 DECISION:
/// Error and issue banners stack above the Inbox dock (and temporarily collapse the dock to its
/// header bar while shown), while interactive dialogs and approvals keep their place below it.
/// SEE-ALSO: apps/desktop/src/app/native_chat/composer.rs,
/// apps/mobile/app/src/chat/native/cards/NativeChatCards.tsx.
pub fn is_error_terminal_notice(notice: &serde_json::Value) -> bool {
    if !notice.is_object() {
        return false;
    }
    if notice["passthrough"].is_object() {
        return false;
    }
    if notice["dialog"]["presentation"]["picker"].is_object()
        || notice["dialog"]["presentation"]["sideQuestion"].is_object()
    {
        return false;
    }
    let tone = notice["tone"].as_str().unwrap_or_default();
    if matches!(tone, "error" | "warning" | "wait") {
        return true;
    }
    if matches!(tone, "terminal" | "approval") {
        return false;
    }
    // Interactive terminal dialogs with rows (`/plugin`, `/model`, plan review) carry `dialog` with
    // `tone == "info"` and stay below the Inbox dock; dialog-free `info` notices (`loginExpired`,
    // `agentExited`, `empryoSignIn`, `updatePrompt`, `memoryGuard`, `nonCliIgnoredCommand`) are
    // terminal/agent issues and stack above it.
    !notice["dialog"].is_object()
}

/// What an approval card's title says the agent wants, by the kind of tool waiting.
pub fn approval_title(agent_label: Option<&str>, tool: &str) -> String {
    let agent = welcome_agent_name(agent_label).unwrap_or_else(|| "The agent".to_string());
    let wants = match crate::transcript::tool_rows::tool_glyph(tool) {
        "edit" => "wants to edit a file",
        "terminal" => "wants to run a command",
        "web" => "wants to reach the web",
        "file" => "wants to read files",
        _ => "wants to use a tool",
    };
    format!("{agent} {wants}")
}

/// The muted line under an approval card's title: the tool, and the folder it runs in when known.
pub fn approval_detail(tool: &str, working_directory: Option<&str>) -> String {
    let tool = tool.trim();
    let folder = working_directory
        .map(str::trim)
        .filter(|folder| !folder.is_empty());
    match (tool.is_empty(), folder) {
        (false, Some(folder)) => format!("{tool} · in {folder}"),
        (false, None) => tool.to_string(),
        (true, Some(folder)) => format!("in {folder}"),
        (true, None) => String::new(),
    }
}
