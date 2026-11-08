//! The one line under every Settings page's title that says what the page is for, by the page's
//! rail id (the desktop's `SettingsTabId::id`, the catalog's page ids, `theme` for Interface).
//!
//! CDXC:Settings 2026-11-08 DECISION:
//! User (Q1712 "Page title and line", Q1713 skipped so the proposed lines stand as written): every Settings page shows its title with a one-line intro under it, kept here per page so a new page can't ship without one (`every_rail_page_has_an_intro` in the desktop's settings_modal/model.rs and this crate's tests).
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/page.rs (the header), apps/mobile/app/src/screens/settings/pages.ts (the phone's lines).

/// `(page id, intro)`, in rail order.
pub const PAGE_INTROS: &[(&str, &str)] = &[
    ("updates", "Your Ghostex version, newer releases, and the tools Ghostex installs for you, such as Hex."),
    ("chat", "How agent chats look and behave: the default view, transcript width, question cards and Ghostex pages."),
    ("sidebar", "How the sidebar lists your sessions, what each card shows, and parking and sleeping sessions."),
    ("terminal", "The terminal's look, scrolling and tab behavior."),
    ("theme", "Theme, colours, fonts and glass for the whole app."),
    ("notifications", "Sounds and alerts when an agent finishes or needs you."),
    ("tools", "Where files, links and dev servers open: in Ghostex, your browser or your editor."),
    ("system", "What Ghostex does when your computer starts, when idle views sleep, and keeping the computer awake."),
    ("dictation", "Talk instead of typing: the engine that turns speech into text, your words, the hotkey and past recordings."),
    ("voiceAgent", "A spoken conversation about your sessions: its provider, voice, speed and memory."),
    ("hotkeys", "Every keyboard shortcut in Ghostex, to look up or change."),
    ("agents", "The coding agents Ghostex runs: which are on, their defaults, approvals and resume hooks."),
    ("accounts", "Your agent accounts, their usage, and moving on when one hits a limit."),
    ("autopilot", "Keeps your sessions going while you're away, within limits you set, and logs what it did."),
    ("collab", "Share a chat through a link that opens in a browser, and what guests may do in it."),
    ("secrets", "Where Ghostex reads keys from (1Password), and how agents ask you for a secret privately."),
    ("security", "The log of sensitive actions on this computer: who asked, what you approved, and whether anyone changed the log."),
    ("integrations", "What Ghostex adds to your agents and computer: the ghostex CLI, skills, Floating Capture and permissions."),
    ("remote", "Reach this computer from your phone or another computer, and add remote machines."),
    ("sync", "Carry Spaces, settings, skills and agents between your computers."),
    ("projects", "Per-project worktree command, ticket key, docs folders and cloud box choices."),
    ("about", "Your version, links, privacy and experimental features."),
    ("extensions", "Turn built-in features and extensions on or off."),
    ("workspaces", "Separate setups for work and personal projects, each with its colour, Claude account and keys."),
    ("cloudBoxes", "Run agents in cloud sandboxes: where they run, the monthly cap and your boxes."),
    ("connectors", "Let agents use your other apps' accounts, with your say on anything that changes something."),
    ("actions", "Buttons that run a command or open a page, for every project or just one."),
    ("openTargets", "The editors and apps a project or file can open in."),
    ("osIntegration", "Make Ghostex the default app for files and links, and check that it worked."),
    ("debugging", "Debug controls and diagnostic logs, for tracking down a problem."),
];

/// The intro of the page with rail id `id`.
pub fn page_intro(id: &str) -> Option<&'static str> {
    PAGE_INTROS
        .iter()
        .find(|(page, _)| *page == id)
        .map(|(_, intro)| *intro)
}
