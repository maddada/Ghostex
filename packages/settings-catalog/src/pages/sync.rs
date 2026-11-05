use crate::rows::{row, section, Page, Section};

/// Settings > Sync (server/src/settings_sync/, CDXC:SettingsSync 2026-11-06 in gx-core settings_sync).
pub(crate) fn page() -> Page {
    Page {
        id: "sync",
        title: "Sync",
        sections: vec![what_syncs(), computers(), changes()],
    }
}

fn what_syncs() -> Section {
    section(
        "whatSyncs",
        "What syncs",
        vec![
            row("syncSpaces", "Spaces", "Keep Spaces and project groups the same on your computers while they are connected to each other: their names, icons, colours, order and which projects are in them. On by default."),
            row("syncAppearance", "Appearance", "Keep the theme and the look the same on your computers. Coming soon."),
            row("syncAgentDefaults", "Agent defaults", "Keep the default agent, models and effort the same on your computers. Coming soon."),
            row("syncSoundsAndNotifications", "Sounds and notifications", "Keep sounds and notification choices the same on your computers. Coming soon."),
            row("syncKeybindings", "Keybindings", "Keep keyboard shortcuts the same on your computers. Coming soon."),
            row("syncCoordinator", "Coordinator", "Keep coordinator playbooks the same on your computers. Coming soon."),
        ],
    )
}

fn computers() -> Section {
    section(
        "syncComputers",
        "Your computers",
        vec![row("syncComputerList", "Computers that sync", "Every computer connected to this one (Remote machines, Easy Connect, Tailscale) joins. Remove one to stop syncing with it; Add back lets it sync again.")],
    )
}

fn changes() -> Section {
    section(
        "syncChanges",
        "Changed on two computers",
        vec![row("syncChangeNotes", "Recent changes", "When two computers changed the same thing while apart, the later change wins on both. Undo puts the other one back everywhere.")],
    )
}
