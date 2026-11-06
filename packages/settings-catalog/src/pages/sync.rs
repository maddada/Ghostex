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
            row("syncAppearance", "Appearance", "Keep the theme, colours, fonts, the chat's look and the glass the same on your computers. The glass picture or video, the interface scale and zoom stay on each computer. Off by default."),
            row("syncAgentDefaults", "Agent defaults", "Keep the default agent, how new sessions open, the title agent, Accept all, and each agent's default model and effort the same on your computers. Off by default."),
            row("syncSoundsAndNotifications", "Sounds and notifications", "Keep completion and copy sounds, attention notifications and the finished-work border the same on your computers. Off by default."),
            row("syncKeybindings", "Keybindings", "Keep keyboard shortcuts the same on your computers of the same system (a Mac with a Mac, Windows with Windows). Off by default."),
            row("syncCoordinator", "Coordinator", "Keep coordinator settings and your edited playbooks (the one for all projects and each project's own, matched by repository) the same on your computers. Off by default."),
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
