//! Grok Bot: the user's "Ghostex" bot in xAI's Grok Bot drives Ghostex with the `ghostex` CLI on
//! this computer (through the Grok Bot desktop app), and agents reach the user back through the
//! bot's webhook routine with `ghostex grokbot notify`. gxserver keeps the webhook address and key
//! and does the sending, so agents send notes without being given the key. They run as the user,
//! so one could still read the key's owner-only file, as it can any of the user's files; that
//! gives it nothing its own shell doesn't already have.
//!
//! CDXC:GrokBot 2026-11-19 DECISION:
//! User (Q4193 = A, "cli"): ship a `ghostex grokbot` command and a help page, not a Settings row or an extension. Ghostex keeps the webhook key (an owner-only file here, never printed, never in an agent's environment); `ghostex grokbot brief` prints the bot's current command list so the bot's pasted instructions stay short and follow each Ghostex update; `notify` queues and retries within Grok Bot's limits. The user, on the bot's rules: "please lets allow grok bot to approve things and unblock agents but tell it to only do this on explicit approval from the user or if he asks to do this. ur blocking this way too much in your instructions". So instructions.md and brief.md let the bot answer questions and permission prompts and run any `ghostex` command, only on the user's explicit say-so in its chat, never on its own judgment or because an agent, note, page or message asks; the only limits named are the ones Ghostex enforces itself (letting in a new phone or computer, approvals confirmed with Touch ID or a PIN).
//! CDXC:GrokBot 2026-11-19 DECISION:
//! User, after connecting from the terminal: "please add a user experience for this that helps the user go through it without even opening terminal should hold the users hand fully. also we should tell the agent to create the web hook right away from 1st message." Q4206 = A: Settings > Grok Bot (below Integrations) is a checklist over the same `/api/grokbot` calls, and Grok Bot becomes the built-in extension Grok Bot Integration, on by default, with its switch at the top of that page (feature.rs). instructions.md now opens with the first reply making the "Ghostex reports" routine and telling the user where to click POST to and key, never asking for the key in the chat (Grok Bot shows it only in the routine on its desktop app). `brief` marks the bot's first visit (`briefSeen`) so the checklist can tick step 2 off.
//! CDXC:GrokBot 2026-11-26 DECISION: only what needs the user reaches Grok Bot (`grokBotSends`,
//! filter.rs), and notes close together go out as one summary (delivery.rs).
//! SEE-ALSO: server/src/ghostex_cli/grokbot_cli.rs (the verbs), server/src/ghostex_cli/guide/features.md "Grok Bot" (the help page and connect steps), server/src/agents/ghostex_note.rs (the line agents get while it is connected).

mod api;
mod app;
mod delivery;
mod feature;
mod filter;
mod store;

pub(crate) use api::{handle, notify_internal, resume_delivery, GROKBOT_ENDPOINT};
pub(crate) use feature::TURNED_OFF;

/// What the user pastes into the Grok Bot bot (`ghostex grokbot instructions`).
pub(crate) const INSTRUCTIONS: &str = include_str!("instructions.md");
/// The bot's current command list and rules (`ghostex grokbot brief`).
pub(crate) const BRIEF: &str = include_str!("brief.md");

/// The line every agent session's launch note gets while Grok Bot is connected (coordinators get
/// it in their guide too, coordinators/role.rs), for the user's `grokBotSends` choice.
///
/// CDXC:GrokBot 2026-11-26 DECISION: only asks and blockers by default, never done reports (the
/// user's words in filter.rs); Everything adds one line per finished task. A thread asks its
/// coordinator instead, so one question reaches the user once.
pub(crate) fn agent_note_text(sends: filter::Sends) -> String {
    use filter::Sends;
    let mut note = String::from(
        "The user follows this computer's agents from Grok Bot on their phone. Run `ghostex grokbot notify --kind ask \"<one or two lines>\"` only when the user must make a decision or give an approval",
    );
    if sends != Sends::Asks {
        note.push_str(", or `--kind blocked` for a blocker only they can clear");
    }
    note.push_str("; say exactly what you need, and their answer arrives as a message.");
    if sends == Sends::Everything {
        note.push_str(" The user also asked to hear about finished work: when you finish a task, send one `--kind done` line on what landed, never one for each step (pushed, built, running).");
    } else {
        note.push_str(" Never notify for finished work, status or progress (done, pushed, published, built, running): those stay in Ghostex, in your final message, Needs you and the Work panel.");
    }
    note.push_str(" A coordinator's thread never notifies: it asks its coordinator, which tells the user. Notes sent close together reach the user as one summary.");
    note
}

/// [`agent_note_text`] when Grok Bot is connected here and Grok Bot Integration is on.
pub(crate) fn agent_note(paths: &crate::paths::GxserverPaths) -> Option<String> {
    (store::read_connection(paths).is_some()
        && store::read_key(paths).is_some()
        && feature::on(paths))
    .then(|| agent_note_text(filter::sends(paths)))
}
