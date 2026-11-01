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
            row("debuggingMode", "Show debug UI controls", "Show diagnostic logs."),
            row("diagnosticLogging", "Diagnostic logs", "Pick the areas to log while you reproduce an issue, and when logging turns off. Warnings, errors, and crashes are always captured.").options_of(DIAGNOSTIC_LOGGING_SCENARIOS, "label", "id"),
            row("claudeHookFreeActivity", "Track Claude without its hooks", "Follow Claude sessions' activity and conversation from the session files Claude writes itself instead of from its hooks. The hooks stay installed and are only compared."),
        ],
    )
}
