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
            row("collabEnabled", "Ghostex Collab", "Share a chat through a link that opens it in any browser, with no app or account: the person you send it to can follow the chat live and write messages you approve. Off by default; turning it on asks for Touch ID (Windows Hello, or your system password on Linux), and so does each share. While it is off nothing can be shared and every link stops working. Share is in each chat's header."),
            row("collabOwnerName", "Your name", "The name guests see for you (\"Shared by …\", \"Waiting for …\"). Empty uses this computer's user name."),
        ],
    )
}

fn new_shares() -> Section {
    section(
        "newShares",
        "New shares start with",
        vec![
            row("collabMode", "Guest messages", "What a guest's message does: Ask me (you approve each one with Touch ID, or Edit or Decline it) or View only. Ask me by default. Send (straight to the agent, labelled with their name) is turned on for one chat in its Share popover, with Touch ID."),
            row("collabExpiry", "Link ends after", "1 hour (the default), 4 hours or 24 hours, at most. Stop sharing ends a link at once, and deleting the chat ends its link too."),
        ],
    )
}

fn shared() -> Section {
    section(
        "shared",
        "Shared now",
        vec![row("collabShares", "Shared chats", "Every chat shared right now, who is watching it and when its link ends, with Stop sharing for each and Stop all. Guests always type the eight-digit code that comes with the link, and you get a notification when one joins.")],
    )
}

fn log() -> Section {
    section(
        "log",
        "Guest messages log",
        vec![row("collabLog", "Guest messages log", "Every message guests sent to a shared chat and what happened to it (went straight to the agent, waited for you, sent, edited or declined), and who joined, left or was removed, newest first.")],
    )
}
