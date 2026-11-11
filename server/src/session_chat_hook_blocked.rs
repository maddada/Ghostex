//! A chat send that the agent's own prompt hook refused.
//!
//! CDXC:SessionChat 2026-11-11 WHY:
//! GitHub #543: when a Claude Code `UserPromptSubmit` hook blocks a prompt, Claude prints
//! "UserPromptSubmit operation blocked by hook:", the reason and "Original prompt: …" above its
//! input box and puts the prompt back in that box. The send's submit check (session_chat_send_submit.rs)
//! read that as a Return Claude had missed: it pressed Return again, which ran the hook a second
//! time, then cleared the box and failed the send with "Claude Code kept the message in its input
//! box", or with whatever else the screen showed. A refused prompt is an answer, not a lost key:
//! the send stops there, and a chat-box send's text goes back into the chat box through the
//! returned-prompt flow so the user can edit it and send again. The thread shows the prompt and the
//! hook's reason from Claude's own transcript row (gx-chat-core `transcript/hook_blocked.rs`).
//! Measured on Claude Code 2.1.319 with a hook printing `{"decision":"block","reason":…}` and one
//! exiting 2. Codex 0.167 empties its input box and records an empty turn with no prompt and no
//! reason, so its sends end the usual way.
//! SEE-ALSO: session_chat_returned_prompt.rs (`return_session_chat_send_blocked_by_hook`).

use crate::session_chat_send::{
    build_agent_tui_clear_input_for_text, normalize_session_chat_screen_text,
    session_chat_paste_needles, write_session_chat_payload,
};

/// The line Claude Code prints above its input box for a prompt a hook blocked.
const CLAUDE_HOOK_BLOCKED_LEAD: &str = "UserPromptSubmit operation blocked by hook";
/// The line that starts the prompt Claude echoes under the reason.
const CLAUDE_ORIGINAL_PROMPT_LEAD: &str = "Original prompt:";
/// Claude Code's collapsed large-paste placeholder, normalized and lowercased.
const CLAUDE_PASTED_PLACEHOLDER: &str = "[pastedtext";

/// Whether Claude's screen says its prompt hook blocked `text`: the newest block notice above the
/// input box echoes the start of the message.
pub(crate) fn claude_screen_shows_hook_block(agent: &str, screen: &str, text: &str) -> bool {
    if !matches!(agent, "claude" | "openclaude") {
        return false;
    }
    let lines: Vec<String> = screen
        .lines()
        .map(|line| crate::session_chat_options::strip_ansi_sgr(line))
        .collect();
    let Some(lead) = lines
        .iter()
        .rposition(|line| line.contains(CLAUDE_HOOK_BLOCKED_LEAD))
    else {
        return false;
    };
    let Some(prompt_at) = lines[lead..]
        .iter()
        .position(|line| line.trim_start().starts_with(CLAUDE_ORIGINAL_PROMPT_LEAD))
        .map(|offset| lead + offset)
    else {
        return false;
    };
    // The echoed prompt runs until the input box's top rule.
    let echoed: String = lines[prompt_at..]
        .iter()
        .take_while(|line| !line.trim_start().starts_with('\u{2500}'))
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    let echoed = normalize_session_chat_screen_text(
        echoed
            .trim_start()
            .trim_start_matches(CLAUDE_ORIGINAL_PROMPT_LEAD),
    );
    session_chat_paste_needles(text)
        .first()
        .is_some_and(|head| echoed.contains(head.as_str()))
        || echoed.to_lowercase().contains(CLAUDE_PASTED_PLACEHOLDER)
}

/// Ends a send whose prompt the hook blocked: a chat-box send's text goes back to the chat box,
/// and the copy Claude left in its own input box is cleared so it is not there twice. Any other
/// send (a queued prompt, a message from another agent) leaves Claude's input box as Claude left
/// it, with the prompt shown in the thread.
pub(crate) async fn finish_hook_blocked_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    text: &str,
    screen_tail: &[String],
) {
    let handed_back = crate::session_chat_returned_prompt::return_session_chat_send_blocked_by_hook(
        project_id, session_id, text,
    );
    if handed_back {
        let _ = write_session_chat_payload(
            project_id,
            session_id,
            zmx_name,
            source,
            &build_agent_tui_clear_input_for_text(text),
        )
        .await;
    }
    crate::session_chat_send_diagnostics::record_send_recovery_from_worker(
        "sessionChatSendBlockedByHook",
        project_id,
        session_id,
        if handed_back {
            "Claude Code's prompt hook blocked the message; it went back to the chat box."
        } else {
            "Claude Code's prompt hook blocked the message; it stays in Claude Code's input box."
        },
        screen_tail,
    );
}
