//! What an agent session writes into another session through the raw verbs
//! (`send-session-chat-message`, `queue-session-chat-prompt`, `send-message`, `send-text`,
//! `send-enter`, `send-key`), which a person also runs from their own terminal.

use super::identity::{self, text};
use crate::ghostex_cli::{
    args::Flags,
    rpc::{call_gxserver_rpc, CliError, CliResult},
    selector,
};
use serde_json::{json, Value};

/// The agent session running this command and the session it writes to, when the two differ.
pub(crate) struct AgentWrite {
    pub sender: Value,
    pub target: Value,
    flags: Flags,
}

/// CDXC:Coordinators 2026-10-24 DECISION:
/// User, about a thread whose `ghostex send-session-chat-message` to its orchestrator arrived as a plain user bubble: "we agreed on a format and he didn't follow it pls fix this in the root cause so it doesn't happen again for any user using agent messaging/orchestrator sessions". Every message an agent session sends another session carries the sender block `ghostex agents send` writes, whichever verb it used, so the chat always shows it as a message from that agent, never as the user's own words. A person's own terminal, a plain shell pane and the phone over SSH have no agent identity and send as the user, as before.
/// WHY: the CLI is where the sender is known (its environment names the calling session), so the block is added here; gxserver decides whether the write is allowed at all (`/api/checkAgentSessionWrite`, coordinators/thread_messages.rs), which refuses a thread writing to its own orchestrator.
/// SEE-ALSO: server/src/ghostex_cli/agents/identity.rs `message` (the block), server/src/coordinators/thread_messages.rs.
pub(crate) fn agent_write(selector_text: &str, flags: &Flags) -> CliResult<Option<AgentWrite>> {
    let Some(sender) = identity::agent_caller()? else {
        return Ok(None);
    };
    let flags = identity::inventory_flags(flags, selector_text)?;
    let target = selector::resolve_cli_session_selector(selector_text, &flags)?;
    let same_session = text(&target, "projectId") == text(&sender, "projectId")
        && text(&target, "sessionId") == text(&sender, "sessionId");
    if same_session {
        return Ok(None);
    }
    let write = AgentWrite {
        sender,
        target,
        flags,
    };
    write.check()?;
    Ok(Some(write))
}

impl AgentWrite {
    /// Fails with gxserver's answer when it refuses this write. When gxserver can't be asked, the
    /// write goes out with a warning on stderr.
    pub fn check(&self) -> CliResult<()> {
        check_agent_session_write(&self.sender, &self.target, &self.flags)
    }

    /// `body` with the sender block below it (above it for a `/` or `!` body), plus the rename
    /// reminder an orchestrator's message to its own thread carries.
    pub fn signed(&self, body: &str) -> String {
        signed_text(
            &self.sender,
            body,
            super::delivery::thread_title_reminder(&self.sender, &self.target, &self.flags),
        )
    }
}

fn signed_text(sender: &Value, body: &str, reminder: Option<String>) -> String {
    let body = match reminder {
        Some(reminder) => format!("{}\n\n{reminder}", body.trim_end()),
        None => body.to_string(),
    };
    identity::message(sender, &body)
}

/// Signs the `text` of a chat-send payload. A raw key (`--key`) is no message, so it stays as
/// it is; an image-only message still says who sent it.
pub(crate) fn sign_message_payload(payload: &mut Value, sign: impl FnOnce(&str) -> String) {
    let body = payload["text"].as_str().unwrap_or_default();
    if !body.trim().is_empty() || payload.get("imagePaths").is_some() {
        payload["text"] = json!(sign(body));
    }
}

/// CDXC:Coordinators 2026-11-10 WHY: when gxserver can't be asked (it is restarting, or too old to know the check), the write still goes out, because a check that cannot run must not stop every agent's messages. It used to go out silently, so nobody could tell the check was skipped; it now says so on stderr.
pub(crate) fn check_agent_session_write(
    sender: &Value,
    target: &Value,
    flags: &Flags,
) -> CliResult<()> {
    let answer = match call_gxserver_rpc(
        "/api/checkAgentSessionWrite",
        &json!({
            "senderProjectId": sender["projectId"], "senderSessionId": sender["sessionId"],
            "projectId": target["projectId"], "sessionId": target["sessionId"],
        }),
        flags,
    ) {
        Ok(answer) => answer,
        Err(error) => {
            eprintln!("{}", unchecked_write_warning(&error));
            return Ok(());
        }
    };
    match answer["refusal"].as_str() {
        Some(refusal) if !refusal.trim().is_empty() => Err(CliError::Other(refusal.to_string())),
        _ => Ok(()),
    }
}

fn unchecked_write_warning(error: &CliError) -> String {
    format!(
        "Warning: Ghostex could not check whether this message is allowed ({}), so it is sending it anyway.",
        error.to_string().trim().trim_end_matches('.')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread_sender() -> Value {
        json!({
            "globalRef": "S90:P3lv0:G38os", "sessionId": "G38os", "projectId": "P3lv0",
            "title": "Session links to PRs and issues", "agentName": "Claude",
            "agentId": "claude", "agentSessionId": "b2423e44",
        })
    }

    #[test]
    fn a_raw_send_from_an_agent_carries_the_sender_block() {
        let mut payload = json!({"sessionId": "S90:P3lv0:G4ldy", "text": "From the Session links thread, UI patch ready"});
        sign_message_payload(&mut payload, |body| {
            signed_text(&thread_sender(), body, None)
        });
        let text = payload["text"].as_str().unwrap();
        // The exact block gxserver and every chat parse into a "Message from" card.
        let parts = crate::coordinators::split_agent_message(text).expect("sender block");
        assert_eq!(parts.body, "From the Session links thread, UI patch ready");
        assert_eq!(parts.reply_to, "S90:P3lv0:G38os");
        assert!(text.contains("\nSession: Session links to PRs and issues\n"));
    }

    #[test]
    fn an_orchestrator_message_to_its_thread_keeps_the_rename_reminder_above_the_block() {
        let text = signed_text(
            &thread_sender(),
            "Next task",
            Some("Your title is \"X\".".into()),
        );
        let parts = crate::coordinators::split_agent_message(&text).expect("sender block");
        assert_eq!(parts.body, "Next task\n\nYour title is \"X\".");
    }

    #[test]
    fn a_slash_command_body_keeps_the_block_first() {
        let text = signed_text(&thread_sender(), "/compact", None);
        assert!(text.starts_with(crate::coordinators::AGENT_MESSAGE_OPENER));
        assert_eq!(
            crate::coordinators::split_agent_message(&text)
                .unwrap()
                .body,
            "/compact"
        );
    }

    #[test]
    fn a_raw_key_is_not_signed_but_an_image_only_message_is() {
        let mut key = json!({"sessionId": "G4ldy", "key": "escape", "text": ""});
        sign_message_payload(&mut key, |_| unreachable!("a key is no message"));
        assert_eq!(key["text"], "");
        let mut image = json!({"sessionId": "G4ldy", "text": "", "imagePaths": ["/a.png"]});
        sign_message_payload(&mut image, |body| signed_text(&thread_sender(), body, None));
        assert!(image["text"]
            .as_str()
            .unwrap()
            .contains("Reply to: S90:P3lv0:G38os"));
    }
}
