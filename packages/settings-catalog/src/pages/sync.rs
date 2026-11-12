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
            row("syncSpaces", "Sidebar organization", "Keep the sidebar organized the same way on your computers while they are connected to each other: your Spaces (names, icons, colours, order and which projects are in each, in which order), project groups, Project or Unified sidebar, Separate by machine and the other sidebar settings that arrange it, and your Session Tags and their order. The sidebar's width, animations and card details stay on each computer. On by default."),
            row("syncAppearance", "Appearance", "Keep the theme, colours, fonts, the chat's look and the glass the same on your computers. The glass picture or video, the interface scale and zoom stay on each computer. Off by default."),
            row("syncAgentDefaults", "Agent defaults", "Keep the default agent, how new sessions open, the title agent, Accept all, and each agent's default model and effort the same on your computers. Off by default."),
            row("syncAgents", "Agents", "Keep which agents are on in Settings > Agents, their names, commands and flags, and their order the same on your computers. An agent a computer doesn't have shows Not installed here, with Install and Install all missing; nothing installs until you click. Off by default."),
            row("syncSkills", "Skills", "Keep your own skills in ~/.agents/skills and the Ghostex skills you installed the same on your computers. Skills over 512 KB stay on each computer, and a skill removed on one computer is put aside on the others, not deleted. Off by default."),
            row("syncAccounts", "Accounts", "Keep your Claude and Codex account list the same on your computers: names, numbers, Use this account and the sidebar pins. Logins stay on each computer, because Claude and Codex sign one of two computers out when they share a login; an account that arrives shows Sign in on this computer. Off by default."),
            row("syncSoundsAndNotifications", "Sounds and notifications", "Keep completion and copy sounds, attention notifications and the finished-work border the same on your computers. Off by default."),
            row("syncKeybindings", "Keybindings", "Keep keyboard shortcuts the same on your computers of the same system (a Mac with a Mac, Windows with Windows). Off by default."),
            row("syncCoordinator", "Coordinator", "Keep coordinator settings and your playbook overrides the same on your computers. Off by default."),
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
