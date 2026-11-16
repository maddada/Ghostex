use crate::data::*;
use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "debugging",
        title: "Debugging",
        sections: vec![controls()],
    }
}

pub(crate) fn controls() -> Section {
    section(
        "controls",
        "Debug controls",
        vec![
            row("debuggingMode", "Show debug UI controls", "Show diagnostic logs. From the next start, also opens Chromium's DevTools port (9334) for developer tools."),
            row("diagnosticLogging", "Diagnostic logs", "Pick the areas to log while you reproduce an issue, and when logging turns off. Warnings, errors, and crashes are always captured.").options_of(DIAGNOSTIC_LOGGING_SCENARIOS, "label", "id"),
            // CDXC:AgentHooks 2026-11-13 DECISION: the user moved Use agent hooks here from Settings > Agents > Session resume hooks (Q3040 "B. Move it to Settings > Debugging": the same switch, out of sight, next to the other troubleshooting tools). Off by default (server/src/agent_hooks/hook_free.rs).
            row("useAgentHooks", "Use agent hooks", "Install hooks for Claude, Codex and OpenCode and follow them through their hooks. Ghostex follows these three without hooks, so turn this on only if it can't follow them on this computer. When off, a session Ghostex can't follow asks in its chat before turning that agent's hook on. Other agents always use their hooks."),
        ],
    )
}
