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
        ],
    )
}
