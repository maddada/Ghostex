//! Active projects: the star tile first in the Spaces row, a view that lists the projects being
//! worked on from every Space in one list.
//!
//! CDXC:Spaces 2026-11-12 DECISION:
//! User: "i wanna add a feature called 'active projects' and it should just show the projects that i'm actively working on from all spaces in 1 place so i can jump between them easily without switching spaces". Then, on the design page: a project is active while a session of it works, waits on the user or still runs something in the background, or was used in the last 8 hours (Q2651 B, no pins); the list is flat, in the Spaces row's order and each Space's own project order, with no Space headers (Q2652 D); a project row is the one its own Space draws, unfolded with every session (Q2653 A); a project leaves the list as soon as neither rule holds, with no fade (Q2654 A); and it works in the Project and the Unified sidebar, across every connected computer (Q2655 A). The project holding the session open now always stays, so opening a session from the list never makes it vanish under the pointer. Selecting it never moves another Space's selection, and opening a session from it keeps it selected.
//! SEE-ALSO: packages/settings-catalog/src/spaces.rs (`ACTIVE_PROJECTS_KEY`), assemble.rs (the list), unified_sidebar/build.rs (the Unified scope), apps/desktop/src/app/native_sidebar/active_projects.rs (the star tile).

use super::inputs::{SidebarInputs, LOCAL_MACHINE_ID};
use super::view::SessionRow;

/// The reserved Space id the Active projects tile selects. No saved Space can take it: a Space id
/// is made by the Space editor, never typed.
pub const ACTIVE_SPACE_ID: &str = "active";

/// What the tile and the Unified empty list call it.
pub const ACTIVE_SPACE_LABEL: &str = "Active projects";

/// The star the tile draws (`star-filled.svg`) and its colour, the menus' star yellow.
pub const ACTIVE_SPACE_ICON: &str = "starFilled";
pub const ACTIVE_SPACE_COLOR: &str = "#e8b931";

/// How long a project stays active after it was last used.
pub const ACTIVE_RECENT_MS: i64 = 8 * 60 * 60 * 1000;

/// Whether `row` keeps its project in Active projects at `now_ms`, and when that ends because time
/// passes alone (`None` while it works or waits, which only a store change ends).
pub(crate) fn row_keeps_active(row: &SessionRow, now_ms: u64) -> (bool, Option<u64>) {
    if row.is_browser {
        return (false, None);
    }
    if row.activity == "working" || row.needs_you().is_some() || row.shows_background_work() {
        return (true, None);
    }
    match row.timing.last_interaction_ms {
        Some(at) if at + ACTIVE_RECENT_MS > now_ms as i64 => {
            (true, u64::try_from(at + ACTIVE_RECENT_MS).ok())
        }
        _ => (false, None),
    }
}

/// [`row_keeps_active`] over a project's rows: whether any keeps it, and the earliest time the
/// answer can change with no store change.
pub(crate) fn rows_keep_active<'a>(
    rows: impl IntoIterator<Item = &'a SessionRow>,
    now_ms: u64,
) -> (bool, Option<u64>) {
    let mut active = false;
    let mut until: Option<u64> = None;
    for row in rows {
        let (keeps, ends) = row_keeps_active(row, now_ms);
        if keeps {
            // A working row holds it with no end; a recent one until its 8 hours are up.
            match ends {
                None if !active || until.is_some() => {
                    active = true;
                    until = None;
                }
                None => {}
                Some(ends) => {
                    if !active {
                        active = true;
                        until = Some(ends);
                    } else if let Some(current) = until {
                        until = Some(current.max(ends));
                    }
                }
            }
        }
    }
    (active, until)
}

/// Whether the Project sidebar's list takes every machine's projects (`merged.rs`): always unless
/// Separate by machine, and with it while this computer's Active projects is selected, so the list
/// still spans every connected computer (Q2655 A). The machine tabs stay (assemble.rs).
pub fn project_list_merges_machines(inputs: &SidebarInputs) -> bool {
    !inputs.settings.separate_by_machine || active_projects_selected_here(inputs)
}

fn active_projects_selected_here(inputs: &SidebarInputs) -> bool {
    inputs.settings.active_projects
        && inputs.settings.sidebar_spaces_enabled
        && inputs.ui.selected_machine_id == LOCAL_MACHINE_ID
        && inputs
            .ui
            .collapse
            .selected_space_by_section
            .get(&inputs.ui.section_key())
            .map(String::as_str)
            == Some(ACTIVE_SPACE_ID)
}
