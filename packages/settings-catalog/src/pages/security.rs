use crate::rows::{row, section, Page, Section};

/// Settings > Security: the security log gxserver keeps (`/api/securityLog`, server/src/audit/).
/// Nothing here is a key of the settings file, so every row is a search row without a key.
pub(crate) fn page() -> Page {
    Page {
        id: "security",
        title: "Security",
        sections: vec![log()],
    }
}

fn log() -> Section {
    section(
        "log",
        "Security log",
        vec![
            row("securityLogCheck", "Security log check", "Whether the security log is intact. Each entry is chained to the one before it, so an entry that was changed or removed after it was written shows here."),
            row("securityLog", "Security log", "Every sensitive action on this computer, newest first: pairing a phone, Easy Connect changes, sharing a chat in Collab, letting a connected computer's agents in, Autopilot answering an approval, and attempts to change settings only you can change. Each says who asked, when and how it ended. Each one also shows in the notification bell and on your paired phones. `ghostex security log` reads the same list."),
        ],
    )
}
