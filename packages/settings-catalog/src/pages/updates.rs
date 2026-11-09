use crate::rows::{row, section, Page};

/// CDXC:Updates 2026-11-05 SEE-ALSO:
/// Settings > Updates (apps/desktop/src/app/window/settings_modal/tabs/updates/, the user's picks on
/// server/src/updates/mod.rs). The page has no settings keys; these rows are keyed by UI ids so the
/// search and Ghostex Help can find it.
pub(crate) fn page() -> Page {
    Page {
        id: "updates",
        title: "Updates",
        sections: vec![
            section(
                "updates",
                "Updates",
                vec![
                    row("updatesUpdateAll", "Update all", "Updates everything the shown machine lists that has an update (Update all machines, beside the machine picker, updates every connected machine): the agents you use, Claude Swap and Codex Swap, Fast Computer & Browser Use, SpaceO, the tools Ghostex installed, extensions, remote machines, the VS Code view and the Chromium runtime. Ghostex itself updates last and restarts once; your agents keep running, and each agent whose CLI changed restarts into the same chat once it finishes its turn."),
                    row("updatesAutoUpdate", "Update automatically", "A switch on each card, off by default: Ghostex installs that item's updates by itself, only while it isn't in use (an agent while none of its sessions is working, the other tools while no agent is working). On Ghostex's own card it is Keep this machine up to date."),
                    row("updatesCheckNow", "Check now", "Checks every item for a newer version now instead of waiting for the next check (every half hour)."),
                ],
            ),
            section(
                "upToDate",
                "Up to date",
                vec![row("updatesUpToDate", "Up to date", "Everything already current, and the agents Ghostex can't read the version of (such as Cursor), with a button that runs their own updater.")],
            ),
        ],
    }
}
