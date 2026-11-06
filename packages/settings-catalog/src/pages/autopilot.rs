use crate::rows::{row, section, Page, Section};

/// Settings > Autopilot. Its choices are gxserver's (`/api/autopilot`, `ghostex autopilot`), not
/// keys of the settings file, so every row here is a search row without a settings key.
pub(crate) fn page() -> Page {
    Page {
        id: "autopilot",
        title: "Autopilot",
        sections: vec![switch(), allowed(), supervisor(), activity()],
    }
}

fn switch() -> Section {
    section(
        "switch",
        "Autopilot",
        vec![
            row("autopilotEnabled", "Autopilot", "Keep sessions going while you are away: carry on a session that stopped while its background job finished or mid-task, interrupt one stuck on a hung command, reload one that stays frozen, move one at a usage limit to an account with room, restart one whose terminal ended. Stays on until you turn it off. Also from a coordinator's chat (its Autopilot pill) and ghostex autopilot on|off."),
            row("autopilotScope", "Which sessions", "Every session, only the projects you pick (and their worktrees), or only sessions you pick one by one. A session's Autopilot pill in its chat takes it out or puts it back; a project's right-click menu, Advanced > Autopilot, does the same for the project and its worktrees."),
        ],
    )
}

fn allowed() -> Section {
    section(
        "allowed",
        "What it may do",
        vec![
            row("autopilotTier", "What it may do on its own", "Heal (carry on, interrupt, reload, switch account), Heal and decide (also answer with the recommended choice), or Everything (also approve any prompt that is not a hard limit)."),
            row("autopilotHardLimits", "Hard limits it may cross", "Each off until you turn it on: push a branch, finish a release you asked for, push main, merge pull requests, delete outside the project, post publicly. Sign-ins, 2FA, identity checks and payments are never done."),
            row("autopilotAnswerQuestions", "Answer questions for me", "After waiting 5, 10 or 15 minutes for you, pick each question's recommended choice. Off by default."),
            row("autopilotApprovals", "Answer approval cards", "Answer approval cards (permission prompts, Connectors cards, folder trust) within what it may do. Off by default: only your click answers them."),
        ],
    )
}

fn supervisor() -> Section {
    section(
        "supervisor",
        "Supervisor",
        vec![
            row("autopilotSupervisor", "Supervisor model", "The agent that wakes for judgment calls: Claude Opus 5.5 at medium, or Codex GPT 6.1 Sol at high. It starts on an account with room."),
            row("autopilotSweep", "Look at every session every", "How often Autopilot sweeps every session it covers, from 1 to 10 minutes (2 by default). A session whose turn ends is looked at right away."),
        ],
    )
}

fn activity() -> Section {
    section(
        "activity",
        "What it did",
        vec![row("autopilotLog", "Summary and log", "Every decision Autopilot made, newest first: the session, what it saw, what it did, and what happened. Kept 30 days. Also ghostex autopilot log.")],
    )
}
