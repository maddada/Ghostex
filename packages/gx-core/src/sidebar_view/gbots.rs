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
//! CDXC:GBots 2026-11-09 DECISION:
//! User (plan piece 6, mockup Q2101 A): departments are small sub-headings under the GBots heading, each folding its bots; the GBots of no department come first, right under the heading, then each department in its order with its lead first (marked with a crown), the rest by name. A department never shows outside GBots.
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
    /// Every bot group, in drawing order; their rows are drawn only while `collapsed` is off.
    pub group_ids: Vec<String>,
    /// The bots of no department, drawn right under the heading.
    pub loose_group_ids: Vec<String>,
    /// The departments, each a sub-heading over its bots.
    pub departments: Vec<GbotsDepartmentView>,
}

/// One department's sub-heading under the GBots heading, and its bots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GbotsDepartmentView {
    pub department_id: String,
    pub name: String,
    /// A Ghostex icon name, when the department has one.
    pub icon: Option<String>,
    /// Its fold key in `collapsedGroupsById` (`ToggleGroupCollapsed`).
    pub storage_id: String,
    pub collapsed: bool,
    /// Its bots, the lead first, then by name.
    pub group_ids: Vec<String>,
}

/// The fold key of a department's sub-heading under the GBots heading of `home_group_id`.
pub fn gbots_department_storage_id(home_group_id: &str, department_id: &str) -> String {
    format!("gbots-department:{home_group_id}:{department_id}")
}

/// The fold key of the GBots heading of `home_group_id`.
pub fn gbots_section_storage_id(home_group_id: &str) -> String {
    format!("gbots:{home_group_id}")
}

/// GBots' projects in drawing order, as `ghostex gbots list` orders them: the bots of no
/// department by name, then each department (its order, then its name) with its lead first.
pub(crate) fn order_gbot_projects(projects: Vec<&PresentationProject>) -> Vec<String> {
    let mut projects = projects;
    projects.sort_by_key(|project| {
        let bot = project.gbots_bot.as_ref();
        let department = bot.and_then(|bot| bot.department_id.as_ref());
        (
            department.is_some(),
            bot.map(|bot| bot.department_order).unwrap_or_default(),
            bot.and_then(|bot| bot.department_name.as_ref())
                .map(|name| name.to_lowercase())
                .unwrap_or_default(),
            department.cloned().unwrap_or_default(),
            !bot.is_some_and(|bot| bot.is_lead),
            bot.map(|bot| bot.name.to_lowercase()).unwrap_or_default(),
            project.project_id.clone(),
        )
    });
    projects
        .into_iter()
        .map(|project| project.project_id.clone())
        .collect()
}
