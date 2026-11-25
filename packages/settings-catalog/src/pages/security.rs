use crate::rows::{row, section, Page, Section};

/// Settings > Security: the security log gxserver keeps (`/api/securityLog`, server/src/audit/).
/// Nothing here is a key of the settings file, so every row is a search row without a key.
pub(crate) fn page() -> Page {
    Page {
        id: "security",
        title: "Security",
        sections: vec![approvals(), still_asks(), helper_names(), log()],
    }
}

/*
CDXC:Security 2026-11-10 DECISION:
User (Q2140 = B): "add a toggle to require touch id/pin etc for access but disabled by default, add it to security". Approving what still asks (a new phone or computer, a guest's message, an agent turning Easy Connect on) is a plain Approve; with this on, the approving device first asks for Touch ID or the Mac password, Windows Hello or the Windows password, the Linux password, or the phone's Face ID or fingerprint. gxserver keeps it per computer, so every device approving for that computer follows it (`/api/securityStatus` `requirePresence`).
*/
fn approvals() -> Section {
    section(
        "approvals",
        "Approvals",
        vec![
            row("approvalsRequirePresence", "Approve with", "One click: Approve on a card (a new phone or computer, a guest's message in Collab, an agent turning on Easy Connect) goes through at once. Touch ID or password: your computer asks for Touch ID, Windows Hello or your password first, and your phone for Face ID or your fingerprint. It applies to every device that approves for this computer."),
        ],
    )
}

/// "What still asks" (Q2173): the read-only list drawn from `ghostex_gx_protocol::approval::STILL_ASKS`.
fn still_asks() -> Section {
    section(
        "stillAsks",
        "What still asks",
        vec![row("stillAsks", "What still asks", "What asks for an Approve on this computer: a phone or computer asking to pair, a Collab guest's message in Approve mode, and, when an agent asks, turning on Easy Connect, letting a device connect through it, sharing a chat or turning on Send mode. Everything else works without asking, and your own clicks never ask.")],
    )
}

/// "Ghostex in permission prompts" (CDXC:HelperNames 2026-11-21 in
/// apps/desktop/src/app/window/settings_modal/tabs/security.rs): macOS and Windows only.
fn helper_names() -> Section {
    section(
        "helperNames",
        "Ghostex in permission prompts",
        vec![row("helperNames", "Ghostex Server and Ghostex Sessions", "The names macOS and Windows show when Ghostex's helpers ask for a permission. Ghostex Server is Ghostex's background service (your projects, phone and other computers). Ghostex Sessions keeps your terminals and agents running, and anything run in a session asks through it. Rows named zmx or gxserver in Privacy & Security are from Ghostex before 10.46 and can be removed. Keep Awake and Remote Login ask for your password in a dialog that names Ghostex.")],
    )
}

fn log() -> Section {
    section(
        "log",
        "Log",
        vec![
            row("securityLogCheck", "Security log check", "Whether the security log is intact. Each entry is chained to the one before it, so an entry that was changed or removed after it was written shows here."),
            row("securityLog", "Security log", "Every sensitive action on this computer, newest first: pairing a phone, Easy Connect changes, sharing a chat in Collab, letting a connected computer's agents in, Autopilot answering an approval, and attempts to change settings only you can change. Each says who asked, when and how it ended. The important ones also show in the notification bell and on your paired phones: anything that opens this computer to other people or devices (turning on Collab, sharing a chat, pairing a phone, turning on or widening Easy Connect, letting a computer in), a new owner or approver key, a protection turned off, anything an agent, a phone, another computer or a Collab guest did, the same request refused three times, and an edited log. `ghostex security log` reads the same list."),
        ],
    )
}
