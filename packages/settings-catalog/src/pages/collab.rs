use crate::rows::{row, section, Page, Section};

/// Settings > Collab (Ghostex Collab). Its choices are gxserver's (`/api/collab`), not keys of
/// the settings file, so every row here is a search row without a settings key.
pub(crate) fn page() -> Page {
    Page {
        id: "collab",
        title: "Collab",
        sections: vec![switch(), new_shares(), shared(), log()],
    }
}

fn switch() -> Section {
    section(
        "switch",
        "Ghostex Collab",
        vec![
            row("collabEnabled", "Ghostex Collab", "Collab is installed on this computer (Settings > Extensions installs it, with Touch ID or your system's equivalent; this page shows only once it is). Share is in each chat's header, and each share asks for your approval. Uninstall ends every share at once and removes Collab from this computer; it never asks."),
            row("collabOwnerName", "Your name", "The name guests see for you (\"Shared by …\", \"Waiting for …\"). Empty uses this computer's user name."),
        ],
    )
}

fn new_shares() -> Section {
    section(
        "newShares",
        "New shares start with",
        vec![
            row("collabMode", "Guest messages", "What a guest's message does: Ask me (you approve each one with Send, Edit or Decline), Send (straight to the agent, labelled with their name) or View only. Ask me by default; change it for one chat in its Share popover."),
            row("collabExpiry", "Link ends after", "1 hour (the default), 4 hours, 24 hours, or Until I stop it (the link stays live until you stop sharing). Stop sharing, or deleting the chat, ends a link at once. The Share popover's chip changes it too."),
            row("collabRequireCode", "Ask for a code", "Guests also type the six-digit code shown in the Share popover, so a link alone is not enough. On by default."),
        ],
    )
}

fn shared() -> Section {
    section(
        "shared",
        "Shared now",
        vec![row("collabShares", "Shared chats", "Every chat shared right now, how many are watching it and when its link ends, with Stop sharing for each and Stop all.")],
    )
}

fn log() -> Section {
    section(
        "log",
        "Guest messages log",
        vec![row("collabLog", "Guest messages log", "Every message guests sent to a shared chat and what happened to it (went straight to the agent, waited for you, sent, edited or declined), and who joined, left or was removed, newest first.")],
    )
}
