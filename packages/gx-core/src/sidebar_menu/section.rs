//! A section heading's context menu. Only the Parked heading has one.

use crate::sidebar_view::view::{GroupView, SectionView};
use crate::sidebar_view::SectionId;

use super::capabilities::can_sleep;
use super::commands::{message, MenuCommand};
use super::item::MenuItem;

/// The menu a right click on a section heading opens, or `None` for a heading that has none.
///
/// CDXC:Sessions 2026-10-09 DECISION:
/// User: right-clicking the Parked header offers Sleep All and Close All. Both act on every parked session of the project, the ones a collapsed heading hides too.
pub(crate) fn section_menu(group: &GroupView, section: &SectionView) -> Option<Vec<MenuItem>> {
    if section.id != SectionId::Parked || section.member_ids.is_empty() || group.core.is_stale {
        return None;
    }
    let sleepable: Vec<String> = group
        .core
        .sessions
        .iter()
        .filter(|session| {
            section
                .member_ids
                .iter()
                .any(|id| *id == session.row.sidebar_session_id)
                && !session.row.is_browser
                && can_sleep(&session.row)
        })
        .map(|session| session.row.sidebar_session_id.clone())
        .collect();
    Some(vec![
        MenuItem::row(
            "Sleep All",
            "moon",
            MenuCommand::command(message::sleep_sessions(&sleepable)),
        )
        .with_disabled(sleepable.is_empty()),
        MenuItem::separator(),
        MenuItem::row(
            "Close All",
            "x",
            MenuCommand::command(message::close_sessions(
                &section.member_ids,
                "sectionCloseAll",
            )),
        )
        .with_danger(),
    ])
}
