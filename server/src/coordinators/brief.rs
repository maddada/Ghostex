//! The text a thread starts from and the reports a coordinator receives.

/// What a brief carries from its coordinator.
pub struct BriefContext<'a> {
    pub goal: &'a str,
    pub instructions: &'a str,
    pub notes: &'a [String],
    /// The thread works in its own Ghostex worktree.
    pub worktree: bool,
}

/// A report longer than this is cut, with a pointer to the full reply.
pub const COORDINATOR_REPORT_MAX_CHARS: usize = 12_000;

/// Who a message is from, in the fields of the agent-message header.
#[derive(Clone, Debug, Default)]
pub struct MessageSender {
    pub agent_name: String,
    pub title: String,
    pub session_id: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub global_ref: String,
}

fn header_value(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return "unavailable".to_string();
    }
    value
        .chars()
        .map(|c| {
            if c.is_control() || c == '\u{2028}' || c == '\u{2029}' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// CDXC:Coordinators 2026-09-30 WHY:
/// Thread briefs and thread reports use the block `ghostex agents send` writes, so every chat (desktop, web, phone) already draws them as "Message from" cards and the coordinator already knows to answer the `Reply to` reference.
/// CDXC:Cli 2026-10-05 DECISION:
/// User: the sender block goes BELOW the message body, so Claude and Codex title a session from the task text instead of the sender's `Session:` title; identity stays in every message. The block itself is unchanged (opener, six labelled lines, `Reply to` last) and follows the body after a blank line. This supersedes the 2026-09-17 decision that put it first; transcripts recorded with the block first still parse.
/// CDXC:Cli 2026-10-05 DECISION:
/// User: a body that starts with `/` or `!` (after leading whitespace) keeps the sender block FIRST, as before, so the receiving agent never runs the body's first line as a slash or shell command. Every other body gets the block last.
/// SEE-ALSO: server/src/ghostex_cli/agents/identity.rs `message` (same block), packages/gx-chat-core/src/transcript/agent_message.rs (parses both positions), `split_agent_message` below.
pub fn agent_message(sender: &MessageSender, body: &str) -> String {
    let block = format!(
        "{AGENT_MESSAGE_OPENER}\nAgent: {}\nSession: {}\nSession ID: {}\nAgent ID: {}\nAgent Session ID: {}\nReply to: {}",
        header_value(&sender.agent_name),
        header_value(&sender.title),
        header_value(&sender.session_id),
        header_value(&sender.agent_id),
        header_value(&sender.agent_session_id),
        header_value(&sender.global_ref),
    );
    place_agent_message_block(&block, body)
}

/// The block after the body, or before it when the body would otherwise open with a slash or shell command.
pub fn place_agent_message_block(block: &str, body: &str) -> String {
    if body.trim_start().starts_with(['/', '!']) {
        format!("{block}\n\n{body}")
    } else {
        format!("{body}\n\n{block}")
    }
}

pub const AGENT_MESSAGE_OPENER: &str = "Message from another agent";

/// The labels of the lines `agent_message` writes after its opener, in order.
const AGENT_MESSAGE_HEADER_LABELS: [&str; 6] = [
    "Agent: ",
    "Session: ",
    "Session ID: ",
    "Agent ID: ",
    "Agent Session ID: ",
    "Reply to: ",
];

/// An agent message taken apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentMessageParts<'a> {
    pub body: &'a str,
    /// The sender's global session reference (`Reply to:`).
    pub reply_to: &'a str,
}

/// `Reply to` of `block` when it is exactly the opener and the six labelled lines in order, and nothing else.
fn agent_message_block_reply_to(block: &str) -> Option<&str> {
    let mut lines = block.trim_end().split('\n');
    if lines.next()? != AGENT_MESSAGE_OPENER {
        return None;
    }
    let mut reply_to = "";
    for label in AGENT_MESSAGE_HEADER_LABELS {
        reply_to = lines.next()?.strip_prefix(label)?;
    }
    lines.next().is_none().then(|| reply_to.trim())
}

/// Splits `text` into body and sender when it carries the exact block `agent_message` writes, below the body (current) or above it (messages sent before 2026-10-05); any other text is `None`.
/// CDXC:SessionTitles 2026-10-05 WHY:
/// The first-message auto-title read the whole prompt, so a session started by another agent was named after the SENDER's `Session:` title ("Coordinator promote operation"). Titles come from the body only. The match is the exact block (opener, the six labelled lines in order, nothing after them or a blank line before the body), not a heuristic, so a user's own message that merely mentions similar words is never cut.
/// SEE-ALSO: server/src/ghostex_cli/agents/identity.rs `message` writes the same block; readers: the title paths (server/src/server/title_generation/first_prompt_decision.rs, server/src/agents/activity.rs, `build_session_history_title_source`) and server/src/session_chat_queue_undelivered.rs.
pub fn split_agent_message(text: &str) -> Option<AgentMessageParts<'_>> {
    if let Some(rest) = text.strip_prefix(AGENT_MESSAGE_OPENER) {
        if let Some(at) = rest.find("\n\n") {
            let block = &text[..AGENT_MESSAGE_OPENER.len() + at];
            if let Some(reply_to) = agent_message_block_reply_to(block) {
                return Some(AgentMessageParts {
                    body: &rest[at + 2..],
                    reply_to,
                });
            }
        }
    }
    let at = text.rfind(&format!("\n\n{AGENT_MESSAGE_OPENER}\n"))?;
    let reply_to = agent_message_block_reply_to(&text[at + 2..])?;
    Some(AgentMessageParts {
        body: &text[..at],
        reply_to,
    })
}

/// The body of an agent message, without its sender block (below or above it); any other text comes back unchanged.
pub fn strip_agent_message_header(text: &str) -> &str {
    split_agent_message(text).map_or(text, |parts| parts.body)
}

/// CDXC:Coordinators 2026-09-30 WHY:
/// Claude's projects send "the project's instructions" to every new thread so a rule stated once reaches all of them. Here the goal, the standing instructions and the memory notes ride under the coordinator's task, followed by the reporting rules the supervisor depends on: the thread's final message is its report, so it must not message the coordinator itself.
/// CDXC:Coordinators 2026-10-05 DECISION:
/// User chose to drop the coordinator's name from the brief's "You are a thread started by…" line: with a short task it was the only name in the first message, and Claude named the thread after its coordinator (thread G9c61, 2026-10-05). The sender block below the brief still names the coordinator.
/// CDXC:Coordinators 2026-10-24 DECISION:
/// User, after a thread handed over a finished part of a two-part task by messaging its orchestrator mid-turn (plain user bubbles, no sender block, no handover markers): "we agreed on a format and he didn't follow it pls fix this in the root cause so it doesn't happen again for any user using agent messaging/orchestrator sessions". The rules name the verbs a thread must not use on its orchestrator (gxserver refuses them, coordinators/thread_messages.rs) and say what to do with several tasks in one message: finish all, then one report, or report after the first and wait when asked to.
/// CDXC:MarkedHandovers 2026-10-11 WHY:
/// A thread's final message goes to its orchestrator, not to the user, and threads always mark their handovers, so the brief says that report is a handover too and gets the markers, which the orchestrator can keep when it passes the report on.
pub fn thread_brief(coordinator: &BriefContext<'_>, task: &str) -> String {
    let mut brief = task.trim().to_string();
    brief.push_str("\n\n---\n");
    brief.push_str("You are a thread started by a Ghostex orchestrator.");
    if !coordinator.goal.trim().is_empty() {
        brief.push_str(&format!("\nThe overall goal: {}", coordinator.goal.trim()));
    }
    if !coordinator.instructions.trim().is_empty() {
        brief.push_str("\n\nStanding instructions:\n");
        brief.push_str(coordinator.instructions.trim());
    }
    if !coordinator.notes.is_empty() {
        brief.push_str("\n\nNotes to keep in mind:");
        for (index, note) in coordinator.notes.iter().enumerate() {
            brief.push_str(&format!("\n{}. {}", index + 1, note.trim()));
        }
    }
    if coordinator.worktree {
        brief.push_str(
            "\n\nYou work in your own git worktree. Ghostex renames its temporary ghostex/<id> branch after this thread's title once you start; that is expected, so keep working on whichever branch is checked out.",
        );
    }
    brief.push_str(
        "\n\nYour session title names your latest task in 3 to 6 words (`ghostex agents whoami` shows it). Whenever a message from your orchestrator starts a task your title does not describe, rename yourself before you start: `ghostex orchestrator rename-self \"<3 to 6 words>\"` (each of its messages ends with a reminder naming your current title). If it answers that the user renamed you, keep that name.",
    );
    brief.push_str(
        "\n\nWhen you are done:\n\
- End your turn with a final report: what you did, where (files, branch, pull request), how you verified it, and anything left or blocked. Ghostex forwards your final message to the orchestrator automatically. Your final message is the only way to report: never message or type into the orchestrator's session yourself (`ghostex agents send`, `send-session-chat-message`, `send-text` and the like), not even for a progress update or a patch that is ready; Ghostex refuses it.\n\
- When a message gives you several tasks, finish them all and then end your turn with one report on all of them, unless it asks you to report after each one; then end your turn after the first with its report, and wait for the orchestrator to tell you to go on.\n\
- If you need a decision you cannot make, ask it plainly in your final message and stop. The orchestrator answers in this chat.\n\
- The user talks only to the orchestrator. Never ask the user directly or publish a page for them (`ghostex show`): put questions, mockups and the path of any page file in your final report, and the orchestrator shows them to the user and sends you the answers.\n\
- Do not merge, push, or delete anything unless this brief asks for it.",
    );
    brief.push_str(
        "\n- Your final report is a handover: start each relevant section of it with one of the markers ❓ Needs you, ✅ Done, ⚠️ Read this, 🛫 Kicked off, 🔄 In progress, 🧪 Untested, 🚀 Shipped or 🛑 Blocked. Start every bullet and sub-bullet with a capital letter, even when it continues a lead-in sentence.",
    );
    brief
}

pub enum ThreadReport<'a> {
    Finished {
        message: Option<&'a str>,
    },
    Interrupted,
    Waiting {
        prompt: &'a str,
    },
    Closed,
    /// A message its coordinator sent it never showed up in its transcript, and it went idle.
    Undelivered {
        excerpt: &'a str,
        /// What its chat or screen shows about the send, when anything does.
        evidence: Option<&'a str>,
    },
}

/// The body of one report; the sender header names the thread.
pub fn report_body(report: &ThreadReport<'_>, thread_ref: &str) -> String {
    match report {
        ThreadReport::Finished { message: Some(message) } => {
            let message = message.trim();
            let mut body = String::from("Ghostex thread report: finished its turn. Its final message:\n\n");
            if message.chars().count() > COORDINATOR_REPORT_MAX_CHARS {
                body.extend(message.chars().take(COORDINATOR_REPORT_MAX_CHARS));
                body.push_str(&format!(
                    "\n\n[Cut here. Read the whole reply with: ghostex read-session-chat {thread_ref} --last 1 --format text]"
                ));
            } else {
                body.push_str(message);
            }
            body
        }
        ThreadReport::Finished { message: None } => format!(
            "Ghostex thread report: finished its turn, but its final message could not be read. Read it with: ghostex read-session-chat {thread_ref} --last 2 --format text"
        ),
        ThreadReport::Interrupted => format!(
            "Ghostex thread report: its turn was interrupted. The requested work may be unfinished. Read its chat with: ghostex read-session-chat {thread_ref} --last 2 --format text"
        ),
        ThreadReport::Waiting { prompt } => format!(
            "Ghostex thread report: waiting for an answer.\n\n{}",
            prompt.trim()
        ),
        ThreadReport::Closed => format!(
            "Ghostex thread report: its session was closed, so it is marked done. `ghostex orchestrator reopen {thread_ref}` or a message to it resumes the same conversation."
        ),
        ThreadReport::Undelivered { excerpt, evidence } => {
            let mut body = format!(
                "Ghostex thread report: your message did not reach it. Its transcript does not show the message you sent, and it is idle, so nothing is working on it.

Your message began: {}",
                excerpt.trim()
            );
            if let Some(evidence) = evidence.map(str::trim).filter(|evidence| !evidence.is_empty()) {
                body.push_str(&format!("

Its chat shows: {evidence}"));
            }
            body.push_str(&format!(
                "

Read its chat (ghostex read-session-chat {thread_ref} --last 2 --format text), then send the message again with ghostex agents send {thread_ref}. If it fails again, tell the user."
            ));
            body
        }
    }
}

/// The first line worth reading: a report's "## Final Report" or "**Summary:**" heading says
/// nothing, so the first plain sentence wins, with the heading as the fallback.
pub fn report_headline(text: &str, max: usize) -> String {
    let clean = |line: &str| {
        line.trim()
            .trim_start_matches(['#', '-', '*', '>', ' '])
            .replace("**", "")
            .replace('`', "")
            .trim()
            .to_string()
    };
    let is_heading = |line: &str| {
        let trimmed = line.trim();
        trimmed.starts_with('#')
            || (trimmed.starts_with("**") && trimmed.ends_with("**"))
            || trimmed.ends_with(':')
    };
    let lines = text.lines().filter(|line| !clean(line).is_empty());
    let line = lines
        .clone()
        .find(|line| !is_heading(line))
        .or_else(|| lines.clone().next())
        .map(clean)
        .unwrap_or_default();
    if line.chars().count() > max {
        format!("{}…", line.chars().take(max).collect::<String>())
    } else {
        line.to_string()
    }
}
