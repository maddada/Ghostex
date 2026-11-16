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
            row("autopilotEnabled", "Autopilot", "Off, Careful, Full or Custom. It keeps sessions going while you are away: carries on a session that stopped mid-task or while its background job finished, interrupts one stuck on a hung command, reloads one that stays frozen, moves one at a usage limit to an account with room, restarts one whose terminal ended. Careful also answers questions after 15 minutes and approves safe steps; Full answers questions after 10 minutes and approves anything that is not a hard limit. Stays on until you turn it off. Also from a coordinator's chat (its Autopilot pill) and ghostex autopilot on|off."),
            row("autopilotScope", "Which sessions", "Every session, only the projects you pick (and their worktrees), or only sessions you pick one by one. A session's Autopilot pill in its chat takes it out or puts it back; a project's right-click menu, Advanced > Autopilot, does the same for the project and its worktrees. The projects and sessions left out are listed here, each with a switch to put it back."),
        ],
    )
}

fn allowed() -> Section {
    section(
        "allowed",
        "What it does",
        vec![
            row("autopilotAnswerQuestions", "When it asks a question", "Wait for me, or after 5, 10 or 15 minutes without your answer pick each question's recommended choice (a question with none goes to the supervisor)."),
            row("autopilotApprovals", "When it asks permission", "Permission prompts, Connectors cards and folder trust: Wait for me, Safe steps (after a minute, approve reads, builds, tests, edits and local commits inside the project; anything else waits for you) or All but limits (after a minute, approve anything that is not a hard limit). A read-only thread's cards always wait for you."),
            row("autopilotHardLimits", "Hard limits it may cross", "With All but limits only. Each off until you turn it on: push a branch, finish a release you asked for, push main, merge pull requests, delete outside the project, post publicly. Sign-ins, 2FA, identity checks and payments are never done."),
        ],
    )
}

/// The supervisor and the sweep sit in the page's collapsed Advanced fold.
///
/// CDXC:Autopilot 2026-11-14 DECISION: user (Q3037, "advanced"): the supervisor and the sweep move out of the middle of the page into its Advanced fold at the bottom.
fn supervisor() -> Section {
    let advanced = |row: crate::rows::SettingRow| crate::rows::SettingRow {
        advanced: true,
        ..row
    };
    section(
        "supervisor",
        "Advanced",
        vec![
            advanced(row("autopilotSupervisor", "Supervisor model", "The agent that wakes for judgment calls: Claude Opus 5.5 at medium, or Codex GPT 6.1 Sol at high. It starts on an account with room.")),
            advanced(row("autopilotSweep", "Look at every session every", "How often Autopilot sweeps every session it covers, from 1 to 10 minutes (2 by default). A session whose turn ends is looked at right away.")),
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
