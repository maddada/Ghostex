//! Main Hub's GBots section: each GBot's project (`gbotsBot` on the project row) is drawn as the
//! bot's row under Chief of Staff, with the bot's chats folding under it like a project's
//! sessions, and never in the projects list, a Space or the Bots space.
//!
//! The bot groups are ordinary project groups (built, focused, counted and walked like any), so a
//! bot chat's row, menu, status and next/previous order come for free; only where they are drawn
//! differs: right after the home group (and its session groups) in `SidebarView::order`, while
//! neither the home group nor the GBots heading is folded.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (Q2101 A): GBots show "under Chief of Staff in Main Hub": a GBots group between Chief of Staff and the loose sessions, each bot's chats folding under its row like a project's sessions; hidden with Main Hub. With Main Hub's group not drawn (another Space, the computers' homes not shown) no GBot is drawn either, and the Bots space keeps only Hermes, so a bot never shows twice.
//! SEE-ALSO: server/src/gbots/presentation.rs (`gbotsBot`, `gbotsProject`), apps/desktop/src/app/native_sidebar/gbots_rows.rs (the renderer), apps/mobile/app/src/contract/gbots.ts (the phone's same placement).

use ghostex_gx_protocol::presentation::PresentationProject;

/// The GBots heading of one home group, and the bot groups drawn under it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GbotsSectionView {
    /// The home group this section is drawn in.
    pub home_group_id: String,
    /// The heading's fold key in `collapsedGroupsById` (`ToggleGroupCollapsed`).
    pub storage_id: String,
    pub collapsed: bool,
    /// The bot groups, by bot name; their rows are drawn only while `collapsed` is off.
    pub group_ids: Vec<String>,
}

/// The fold key of the GBots heading of `home_group_id`.
pub fn gbots_section_storage_id(home_group_id: &str) -> String {
    format!("gbots:{home_group_id}")
}

/// GBots' projects by bot name, as `ghostex gbots list` orders them.
pub(crate) fn order_gbot_projects(projects: Vec<&PresentationProject>) -> Vec<String> {
    let mut projects = projects;
    projects.sort_by(|left, right| {
        let name = |project: &PresentationProject| {
            project
                .gbots_bot
                .as_ref()
                .map(|bot| bot.name.to_lowercase())
                .unwrap_or_default()
        };
        name(left)
            .cmp(&name(right))
            .then_with(|| left.project_id.cmp(&right.project_id))
    });
    projects
        .into_iter()
        .map(|project| project.project_id.clone())
        .collect()
}
