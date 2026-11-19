use crate::rows::{row, section, Page};
use crate::update_checks::UPDATE_CHECK_OPTIONS;

/// CDXC:Updates 2026-11-05 SEE-ALSO:
/// Settings > Updates (apps/desktop/src/app/window/settings_modal/tabs/updates/, the user's picks on
/// server/src/updates/mod.rs). The Updates and Up to date rows are keyed by UI ids so the search
/// and Ghostex Help can find them; Checking for updates holds the page's settings
/// (update_checks.rs).
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
                    row("updatesAutoUpdate", "Update automatically", "A switch on each card, off by default: Ghostex installs that item's updates by itself, only while it isn't in use (an agent while none of its sessions is working, the other tools while no agent is working), once a check finds them. On Ghostex's own card it is Keep this machine up to date."),
                    row("updatesCheckNow", "Check now", "Checks everything for a newer version now (Ghostex, agents, tools and extensions), whatever Checking for updates says."),
                ],
            ),
            section(
                "upToDate",
                "Up to date",
                vec![row("updatesUpToDate", "Up to date", "Everything already current, and the agents Ghostex can't read the version of (such as Cursor), with a button that runs their own updater.")],
            ),
            section(
                "checking",
                "Checking for updates",
                vec![
                    row("updatesCheckGhostex", "Ghostex", "How often Ghostex looks for a new version of itself on its own: every 15 minutes, 30 minutes, hour or day, or Only on Check now.").options(UPDATE_CHECK_OPTIONS),
                    row("updatesCheckAgents", "Agents", "How often Ghostex checks the agents turned on in Settings > Agents for updates. Agents you turned off are never checked, not even for their version.").options(UPDATE_CHECK_OPTIONS),
                    row("updatesCheckTools", "Tools", "How often Ghostex checks what it installed for you (account switchers, tools such as uv, the Easy Connect helper) for updates.").options(UPDATE_CHECK_OPTIONS),
                    row("updatesCheckExtensions", "Extensions", "How often Ghostex checks your extensions for updates.").options(UPDATE_CHECK_OPTIONS),
                ],
            ),
        ],
    }
}
