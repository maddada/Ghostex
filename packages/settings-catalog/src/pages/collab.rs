use crate::rows::{row, section, Page, Section};

/// Settings > Collab (Ghostex Collab). Its choices are gxserver's (`/api/collab`), not keys of
/// the settings file, so every row here is a search row without a settings key.
pub(crate) fn page() -> Page {
    Page {
        id: "collab",
        title: "Collab",
        sections: vec![switch(), new_shares(), shared()],
    }
}

fn switch() -> Section {
    section(
        "switch",
        "Ghostex Collab",
        vec![row("collabEnabled", "Ghostex Collab", "Share a chat through a link that opens it in any browser, with no app or account: the person you send it to can follow the chat live and write in it. Off by default; while it is off nothing can be shared and every link stops working. Share is in each chat's header.")],
    )
}

fn new_shares() -> Section {
    section(
        "newShares",
        "New shares start with",
        vec![
            row("collabMode", "Guest messages", "What a guest's message does: Ask me (you approve each one with Send, Edit or Decline), Send (straight to the agent, labelled with their name) or View only. Ask me by default; change it for one chat in its Share popover."),
            row("collabExpiry", "Link ends after", "1 hour, 24 hours (the default), 7 days, or never. Stop sharing ends a link at once."),
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
