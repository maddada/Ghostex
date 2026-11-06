//! What the Unified sidebar opens so a revealed card is drawn: its shelf, its project's group, and
//! the filter picks that leave it out.
//!
//! CDXC:Sidebar 2026-11-05 WHY:
//! User: "clicking on reveal active session in the sidebar doesn't work for working threads when that setting is enabled". With the Working section on (`unifiedWorkingSection`) a working card stays on the folded Working shelf even while it is open (CDXC:Sidebar 2026-10-30 in build.rs), and a reveal only changed the Project sidebar's state (`sidebar_view::reveal_plan`), whose headings the Unified list does not draw. The card was never drawn, so nothing scrolled to it or flashed. A reveal now opens what folds the card away, exactly as the Project sidebar's reveal opens a collapsed heading or group: the shelf (Moving, Snoozed, Parked or Drafts), the project group it sits in while Group by Project is on, and the filter picks the card does not match, the way the Project reveal lifts the tag filters. Opening a shelf stays opened (it is the same saved state a click on the shelf writes), as an opened Project heading does.

use super::build::{UnifiedFilter, UnifiedRow, UnifiedShelf, UnifiedSidebarView};
use super::saved::UnifiedSavedState;

/// The name a shelf's open or folded state is kept under: the shelf alone, or `<repo key>|<shelf>`
/// for a shelf inside a project group (CDXC:Sidebar 2026-10-31 in saved.rs).
pub fn unified_shelf_key(shelf: UnifiedShelf, repo_key: Option<&str>) -> String {
    // CDXC:Sidebar 2026-11-06 WHY: the key stays "Working" though the shelf is titled "Moving", so
    // shelves a person already opened or folded keep their saved state.
    let name = match shelf {
        UnifiedShelf::Working => "Working",
        other => unified_shelf_label(other),
    };
    match repo_key {
        Some(repo) => format!("{repo}|{name}"),
        None => name.to_string(),
    }
}

/// A shelf's heading.
///
/// CDXC:Sidebar 2026-11-06 DECISION:
/// User: "rename moving in the sidebar to moving please" (the sidebar's "Working" shelf is called "Moving", as the board's group is). Only the shelf's title, its setting's label, the Sort & Filter row, Reveal's words and the help change; the internal names (`UnifiedShelf::Working`, `unifiedWorkingSection`) and a session's own Working status stay.
pub fn unified_shelf_label(shelf: UnifiedShelf) -> &'static str {
    match shelf {
        UnifiedShelf::Drafts => "Drafts",
        UnifiedShelf::Working => "Moving",
        UnifiedShelf::Snoozed => "Snoozed",
        UnifiedShelf::Parked => "Parked",
        UnifiedShelf::Pinned => "Pinned",
        UnifiedShelf::Active => "Sessions",
    }
}

/// Whether a shelf is open: Drafts starts open and the rest folded, so the saved set holds the
/// ones toggled away from that.
pub fn unified_shelf_open(saved: &UnifiedSavedState, shelf: UnifiedShelf, key: &str) -> bool {
    saved.toggled_shelves.contains(key) != (shelf == UnifiedShelf::Drafts)
}

/// Where a card is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedRevealSpot {
    pub shelf: UnifiedShelf,
    /// [`super::RepoIdentity::key`] of the project group it sits under.
    pub repo_key: String,
}

/// Opens whatever folds the card at `spot` away. `grouped` is Group by Project. Returns whether
/// anything changed, so the caller writes the saved state only then.
pub fn open_unified_reveal(
    saved: &mut UnifiedSavedState,
    spot: &UnifiedRevealSpot,
    grouped: bool,
) -> bool {
    // Drafts stay above every group, so only the other shelves and cards sit inside one.
    let in_group = grouped && spot.shelf != UnifiedShelf::Drafts;
    let mut changed = in_group && saved.folded_groups.remove(&spot.repo_key);
    // Pinned and Sessions are no shelves: their cards are drawn whenever their group is.
    if matches!(spot.shelf, UnifiedShelf::Pinned | UnifiedShelf::Active) {
        return changed;
    }
    let key = unified_shelf_key(spot.shelf, in_group.then_some(spot.repo_key.as_str()));
    if !unified_shelf_open(saved, spot.shelf, &key) {
        if !saved.toggled_shelves.remove(&key) {
            saved.toggled_shelves.insert(key);
        }
        changed = true;
    }
    changed
}

/// The row of `sidebar_session_id` in a built list, and where it sits.
pub fn find_unified_row<'a>(
    view: &'a UnifiedSidebarView,
    sidebar_session_id: &str,
) -> Option<&'a UnifiedRow> {
    view.drafts
        .iter()
        .chain(&view.pinned)
        .chain(&view.active)
        .chain(&view.working)
        .chain(&view.snoozed)
        .chain(&view.parked)
        .find(|row| row.session.row.sidebar_session_id == sidebar_session_id)
}

impl UnifiedRow {
    pub fn reveal_spot(&self) -> UnifiedRevealSpot {
        UnifiedRevealSpot {
            shelf: self.shelf,
            repo_key: self.repo.key.clone(),
        }
    }
}

impl UnifiedFilter {
    /// The filter with every pick that leaves `row` out lifted, and the others kept: a repository,
    /// machine or Space pick the row does not match is cleared, and so is a text search it does
    /// not match. `None` when the filter already keeps the row.
    pub fn admitting(&self, row: &UnifiedRow) -> Option<Self> {
        if self.keeps(row) {
            return None;
        }
        let mut lifted = self.clone();
        if !lifted.keeps_repo(row) {
            lifted.repos.clear();
        }
        if !lifted.keeps_machine(row) {
            lifted.machines.clear();
        }
        if !lifted.keeps_space(row) {
            lifted.spaces.clear();
        }
        if !lifted.keeps(row) {
            lifted.query.clear();
        }
        Some(lifted)
    }
}
