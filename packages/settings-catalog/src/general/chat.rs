use crate::data::*;
use crate::json::{num_opt, opt};
use crate::rows::{row, section, RuntimeOptions, Section};

pub(crate) fn chat() -> Section {
    section(
        "chat",
        "Chat",
        vec![
            row("preferredAgentInterface", "Default view for compatible agents", "Automatically switch to chat as soon as Ghostex detects that an agent session supports it.").options(PREFERRED_AGENT_INTERFACE_OPTIONS),
            row("closeEmptySessionsOnNew", "Close empty sessions when starting a new one", "When you start a new session, close this project's other sessions that have nothing typed, drafted or queued. Only applies when Chat is the default view for that agent; with Terminal it never runs."),
            row("sessionChatCustomTranscriptWidthEnabled", "Custom transcript width (set with Transcript width)", "On once Transcript width is dragged to a percentage; off keeps Auto, where the transcript matches the prompt composer's width."),
            row("sessionChatTranscriptWidthPercent", "Transcript width", "The centered transcript's width on wide panes. Auto matches the prompt composer; drag to set a percentage of the pane instead. The prompt composer keeps its standard width either way."),
            row("sessionChatFileEditPreviews", "Show file edit previews", "Show the first seven code lines in each file edit. Turn off to show only the path and change counts."),
            row("chatToolCallCards", "Tool call cards (preview)", "Show each run of tool calls as one card: it counts the calls, says what each one is doing and then did, with its result and time, explains a failure, and folds to one line once the agent writes again. Off keeps the current look."),
            row("sessionChatDetailedView", "Default to Detailed view", "New chats open in Detailed view, with the work under each agent reply shown. Off, they open in Normal view. More actions > View in a chat picks Summary, Normal or Detailed for that chat."),
            row("sessionChatKeepComposerExpanded", "Keep chat box expanded while scrolling", "Keep the desktop chat box at full size while you scroll the transcript instead of shrinking it as you scroll up and growing it back at the end."),
            row("sessionChatConfirmEscapeInterrupt", "Press Escape twice to interrupt", "While the agent is working, the first Escape asks you to press Escape again within 2 seconds before it interrupts. Turn off to interrupt on the first Escape."),
            row("chatCatchMeUp", "Catch me up button", "Show a Catch me up button at the top right of the chat box while it is empty. It has your Title Generation Agent (a small, cheap model) read the session's last 30 agent messages and shows where the session stands: what needs you, what is done, in progress or untested."),
            row("chatSendOnPick", "Send when you pick an answer", "When a question card asks one question, picking an answer sends it right away, without Send answers. Cards with several questions, multiple-choice questions, Other and text answers still wait for Send."),
            row("chatAnswersWaitForIdle", "Wait for turn end before sending form answers", "Send answers from Ghostex forms and Ghostex pages only after the agent finishes its turn. Off sends them like a normal chat message, which the agent can read after its current tool call finishes."),
            row("chatPageOpenTarget", "Open Ghostex pages in", "Where a Ghostex page (gx page) an agent shows in chat, such as a mockup or a comparison, opens when you click its card: a popup over the chat, the Files view, or your default web browser. Answers you send from the page reach the agent the same way in all three. The phone always opens pages in its own viewer.").options(CHAT_PAGE_OPEN_TARGET_OPTIONS),
            row("coordinatorBoardView", "Coordinator board opens as", "What the Board pill above a coordinator's chat box opens: its lines of work as a list beside the chat, or as columns over the chat, one per group. The switch at the top of either view changes this too.").options(COORDINATOR_BOARD_VIEW_OPTIONS),
            row("agentGhostexNote", "Tell agents about Ghostex", "Agent sessions Ghostex starts get a short note that they run in Ghostex: it can show Ghostex pages, forms and visuals in the chat, install Ghostex skills, and `ghostex agent-guide` explains the rest. Agents use it only when you ask for one of those things. Applies to sessions started or resumed after you change it."),
            row("orchestratorAllEfforts", "Every effort level in coordinator pickers", "Also lists the levels above XHigh (Max, Ultra and the like) in the coordinator effort pickers, where the agent has them. Coordinators work best on High or XHigh."),
            row("orchestratorPlaybook", "Coordinator playbook", "How every coordinator works: the playbook it follows. Edit it for all projects here, or for one project and its worktrees in Settings > Projects. Coordinators, existing ones too, follow the edited playbook from their next guide read, and a switch turns an edited playbook off without losing it."),
            /*
            CDXC:VoiceMode 2026-10-21 DECISION:
            User (Q26): Read aloud's Voice and Speed are two rows on the existing Chat page, not a page of their own. (Q25) Hidden on Linux, which has no system voice: there the page shows `readAloudUnavailable` instead, pointing to the phone and the browser (`availability.rs`). Speed stays on Linux because Ghostex in a browser uses this computer's Speed.
            CDXC:VoiceMode 2026-10-21 WHY: the rows show only while Read aloud is on (they are the feature's `settingKeys` in official_extensions.rs), like every other built-in feature's rows; with it off they would set something nothing uses. This is the orchestrator's call, not the user's.
            */
            row("readAloudVoice", "Read aloud voice", "The voice Read aloud speaks the agent's replies with: System voice, or one of the voices installed on this computer. Add voices in your computer's speech or accessibility settings. The phone has its own Voice in its Settings > Chat.").runtime_options(RuntimeOptions::SystemVoices),
            row("readAloudRate", "Read aloud speed", "How fast Read aloud speaks, from 0.75× to twice as fast. Ghostex in a browser uses it too; the phone has its own Speed.").num_options(&[
                num_opt("0.75×", 0.75), num_opt("1×", 1.0), num_opt("1.25×", 1.25), num_opt("1.5×", 1.5), num_opt("2×", 2.0),
            ]),
            row("readAloudUnavailable", "Read aloud", "Read aloud can't speak on this computer, so it has no controls here. It works in the Ghostex phone app and in Ghostex in a browser connected to this computer."),
            row("sessionDetailsClaudeTaskTools", "Claude task list", "Turn on Claude's task list tools, so Claude can keep a checklist that the Session details panel shows. Recent Claude models only get them when Ghostex asks. Applies to Claude sessions started or woken after you change it. Shown while the Session details panel extension is on."),
        ],
    )
}

/// CDXC:Settings 2026-11-02 DECISION: User: "in settings i think we have setting for verbose, make that about defaulting to detailed view but make sure it's in advanced". The chat's More actions > View menu picks Summary, Normal or Detailed per chat, and the one Advanced row Default to Detailed view (`sessionChatDetailedView`) decides how new chats open; the retired Simple and Verbose default keys migrate into it (`migrate_chat_view_default`). Supersedes 2026-10-29's cleanup Q11 that left no row.
/// Q18: Show notification on terminal bell moved here from the Terminal section.
/// CDXC:Settings 2026-10-29 DECISION (wording Q25): the five marked-handover rows are their own
/// section of the Chat page instead of sitting loose in the Chat list.
pub(crate) fn marked_handovers() -> Section {
    section(
        "markedHandovers",
        "Marked handovers",
        vec![
            row("markedHandovers", "Marked handovers in every session", "Coordinators and their threads start each part of the message they hand back with a marker such as ✅ Done or 🧪 Untested, and end it with what needs you (⚠️ Read this, 🛑 Blocked, 💡 Suggestions, ❓ Needs you), so you never miss it. Turn this on to have every other agent session do the same. Applies to sessions started or resumed after you change it."),
            row("markedHandoversEmoji", "Show marker emojis", "Not recommended. Show the marker emoji (❓ ✅ ⚠️ and the rest) at the start of each coloured section, in every style. Off: the colour and the label show the kind. Shown while Marked handovers is on."),
            row("markedHandoversStyle", "Marked sections style", "How the chat colours the sections an agent marks: coloured labels, a thin left rule, or a coloured badge in place of the emoji. Shown while Marked handovers is on.").options(MARKED_HANDOVERS_STYLE_OPTIONS),
            row("markedHandoversHeadingStrength", "Heading colour strength", "How strongly the coloured section headings in agents' handovers (Done, Needs you, Untested and the rest) stand out. 100% is full colour; lower values blend the headings toward the normal text colour, and 0% leaves them in plain bold text."),
            row("orchestratorPlainReplies", "Coordinators answer without markers", "Not recommended. Coordinators answer normally, without marking the sections of their handovers. Their threads still mark the reports they send to the coordinator. Applies to coordinators started or resumed after you change it."),
        ],
    )
}

pub(crate) fn sounds() -> Section {
    section(
        "sounds",
        "Sounds",
        vec![
            row("showNotificationOnTerminalBell", "Show notification on terminal bell", "Treat terminal bell events as session attention."),
            row("completionSound", "Agent finished sound", "The sound when an agent finishes a task or needs you, or Off.").options(&[opt("Off", "off")]).options_of(COMPLETION_SOUND_OPTIONS, "label", "value"),
            row("showMacOSAttentionNotifications", "System notification when a session needs you", "Show a system notification when a session needs attention."),
            row("showFinishedWorkBorder", "Finished-work border", "Outline a session's pane in a colour when its agent finishes work or needs you, until you look at it. The sidebar dot, sounds and notifications stay either way."),
            row("finishedWorkBorderColor", "Finished-work border colour", "The colour of the border a session's pane gets when its agent finishes work or needs you. Shown while Finished-work border is on."),
            row("resetExpirySystemNotifications", "Warn before a banked usage reset expires", "Show a system notification when a banked Claude or Codex usage reset expires within 3 days and again within 24 hours. The red notification in the bell stays either way."),
            row("attentionNotificationActions", "Test alerts", "Play the agent finished sound and show the notification as they are set now, or open the system's notification settings for Ghostex."),
            row("actionCompletionSound", "Command finished sound", "The sound when a command run from the Commands pane finishes.").options_of(COMPLETION_SOUND_OPTIONS, "label", "value"),
            row("copySound", "Copy sound", "Play a short sound when copying to the clipboard, including text from the chat composer."),
        ],
    )
}
