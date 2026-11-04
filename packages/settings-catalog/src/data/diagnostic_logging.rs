use crate::json::{opt, Opt, J};

/// CDXC:Diagnostics 2026-11-03 DECISION:
/// User chose: diagnostic log areas turn themselves off after 3 days at most ("Yes, after 3 days"), so the "Never" choice is replaced by "3 days". Areas left on for good had filled the logs folder with 25 MB per area.
pub const DIAGNOSTIC_LOGGING_DURATION_OPTIONS: &[Opt] = &[
    opt("15 min", "15m"),
    opt("1 hour", "1h"),
    opt("3 days", "3d"),
];

/// CDXC:Diagnostics 2026-09-26 DECISION:
/// User: the Debugging page had far too many log switches. Scenarios nothing writes any more are deleted, the rest are merged into one switch per feature area, all switches share one turn-off timer, and rows show no log file names. Desktop performance profiling has no switch: launching with --profile is its opt-in.
/// Each id is the one an area kept from before the merge, so the Rust writers in apps/desktop/src/support_logs.rs and server/src/logging/logger.rs still read the same strings.
pub const DIAGNOSTIC_LOGGING_SCENARIOS: J = J::Arr(&[
    J::Obj(&[
        ("description", J::Str("Terminal focus and keyboard routing, pane tabs, terminal attach and sync (local and remote), and zmx refreshes.")),
        ("id", J::Str("native.terminal.focus")),
        ("label", J::Str("Terminals, panes, and keyboard focus")),
    ]),
    J::Obj(&[
        ("description", J::Str("Sidebar bootstrap and focus, browser and code editor panes, Quick Access, and chat frame readiness.")),
        ("id", J::Str("native.sidebar.refresh")),
        ("label", J::Str("Sidebar, browser, and editor panes")),
    ]),
    J::Obj(&[
        ("description", J::Str("Chat view state, composer draft saves, sends, clears, and restart recovery (desktop and gxserver).")),
        ("id", J::Str("gpui.sessionChat.viewState")),
        ("label", J::Str("Chat")),
    ]),
    J::Obj(&[
        ("description", J::Str("App modal host lifecycle, Settings hydration, and modal errors.")),
        ("id", J::Str("gpui.app.modal")),
        ("label", J::Str("Modals and Settings")),
    ]),
    J::Obj(&[
        ("description", J::Str("Project board create/start, title generation, Beads, and worktree setup.")),
        ("id", J::Str("native.project.board")),
        ("label", J::Str("Project board")),
    ]),
    J::Obj(&[
        ("description", J::Str("Remote gxserver install: approval, SSH setup, package upload, token read, and tunnel.")),
        ("id", J::Str("native.remote.gxserver.install")),
        ("label", J::Str("Remote machines")),
    ]),
    J::Obj(&[
        ("description", J::Str("Agent detection and working/idle/attention transitions from hooks and terminal titles.")),
        ("id", J::Str("gxserver.agentActivity")),
        ("label", J::Str("Agent activity")),
    ]),
    J::Obj(&[
        ("description", J::Str("With Session status on Combined: each time Ghostex's detection and a program's own status report disagree about a session, with what each said and which one won.")),
        ("id", J::Str("gxserver.programStatusMismatch")),
        ("label", J::Str("Program status mismatches")),
    ]),
    J::Obj(&[
        ("description", J::Str("Prompt editor window and composer lifecycle.")),
        ("id", J::Str("native.prompt.editor")),
        ("label", J::Str("Prompt editor")),
    ]),
    J::Obj(&[
        ("description", J::Str("Desktop app and gxserver startup, activation, window close, and shutdown.")),
        ("id", J::Str("native.host.lifecycle")),
        ("label", J::Str("App and server lifecycle")),
    ]),
    J::Obj(&[
        ("description", J::Str("gxserver API requests, typed operations, and repository cloning.")),
        ("id", J::Str("gxserver.requests")),
        ("label", J::Str("Server requests")),
    ]),
    J::Obj(&[
        ("description", J::Str("Background reads of the PRs and issues sessions link to: how many links each pass follows, how many were due, and the gh calls it made.")),
        ("id", J::Str("gxserver.sessionLinks")),
        ("label", J::Str("Session links")),
    ]),
    J::Obj(&[
        ("description", J::Str("Watches Claude, Codex and OpenCode through the state they write themselves and compares it with their hooks: which activity changes both saw, which only one saw, and how far apart.")),
        ("id", J::Str("gxserver.hooklessActivity")),
        ("label", J::Str("Hook-free activity comparison")),
    ]),
]);
