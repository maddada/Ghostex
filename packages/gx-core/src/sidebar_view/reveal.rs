//! What has to change for one row to be on screen: the port of `applyNativeSidebarReveal`.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! Revealing a session is the one command that reads the list to decide what to change: which
//! Space shows the row, which group holds it, which collection holds that group, which heading it
//! falls under, and whether the compact list would still leave it out. The answers are read from
//! the list that is already built wherever they are in it, and the list is only built again for
//! the questions that one cannot answer.
//!
//! The probe lifts the Space filter as well as Show Hidden and the tag filters, and that is the
//! whole point of it rather than a detail: the drawn list holds only the groups the selected Space
//! shows, so a row in another Space is not in it, and asking the drawn list where a row is would
//! answer "nowhere" for exactly the rows a reveal exists to fetch. `reveal.ts` looks the group up
//! in `state.groupOrder`, which is the unfiltered inventory, and this is the same question asked
//! of a list built with nothing filtering it.
//!
//! The collection a group belongs to is read from `GroupView::collection_id`, which is the
//! collections document, not from the drawn collections: a hidden collection is dropped from the
//! drawn list while its projects are still drawn, and passing `None` there would answer the Space
//! question on a different branch from the one the list itself took.
//!
//! The cost is one clone of the inputs, reused, and between zero and two builds: none when the row
//! is already drawn, one when something filters it out or its heading is closed, two when both.
//! The second build keeps whichever tag filters the plan leaves in place, because the compact list
//! it measures is the one the user will be looking at.
//!
//! Ported from the deleted sidebar page's `reveal.ts` and `rememberNativeSidebarFocus` (frozen in
//! the deleted `tooling/gx-core/sidebar-page-frozen/space-navigation.ts`; see git history).

use crate::core::Core;
use crate::keys::{MachineId, ProjectKey, SessionKey};

use crate::sidebar_ui::SidebarUiIntent;

use super::inputs::{SectionId, SidebarInputs, SidebarUiState, LOCAL_MACHINE_ID};
use super::model::SidebarViewModel;
use super::spaces::{resolve_selected_space, space_for_group, SpacesState};
use super::tags::matches_tag_filters;
use super::view::{GroupView, SidebarView};

/// The changes that put a row on screen. Every field is already compared against what the sidebar
/// state holds, so a field that is `false` or `None` needs nothing done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarRevealPlan {
    /// The machine tab the row lives on, when it is not the selected one. Applied FIRST: every
    /// other field of the plan is keyed by that machine's section.
    pub select_machine: Option<String>,
    /// The group holding the row.
    pub group_id: String,
    /// The id the group's own UI state is keyed by.
    pub storage_id: String,
    /// The Space that shows the group, when it is not the one the section is filtered by.
    pub select_space: Option<String>,
    /// The Space the revealed row is remembered under, and the section that memory is keyed by.
    /// `rememberSidebarSpaceSession`, which a reveal does as well as an ordinary focus change.
    pub remember_space: Option<FocusedRowSpace>,
    /// The collection holding that group, when it is in one and that collection is collapsed.
    pub collapsed_collection_storage_id: Option<String>,
    /// The heading the row falls under, when it is collapsed.
    pub collapsed_section: Option<SectionId>,
    /// The group is collapsed.
    pub collapsed_group: bool,
    /// The group or its collection is hidden and Show Hidden is off.
    pub show_hidden: bool,
    /// The ticked tag filters leave the row out.
    pub clear_tag_filters: bool,
    /// The compact list would still leave the row out.
    pub expand_list: bool,
}

impl SidebarRevealPlan {
    /// The changes to the sidebar's own state that carry this plan out, in the order they must be
    /// applied, against the state `ui` holds now. The Space memory (`remember_space`) is not one of
    /// them: the host applies it as its own step, after these, because it is counted separately.
    ///
    /// One function for every caller (the reveal a publish carries, the slot hotkey's reveal, and
    /// the gates) rather than one copy per host path, so a rule cannot change for one of them only.
    pub fn intents(&self, ui: &SidebarUiState) -> Vec<SidebarUiIntent> {
        let mut intents = Vec::new();
        // The machine before everything else: every other field of the plan is keyed by that
        // machine's section, so applying the Space or a collapse first would write them under the
        // section the user is leaving. `rememberNativeSidebarFocus` switches the tab the same way.
        if let Some(machine_id) = &self.select_machine {
            intents.push(SidebarUiIntent::SelectMachine {
                machine_id: machine_id.clone(),
            });
        }
        // The Space next: it decides which groups the section draws at all, which is what the
        // TypeScript does by running `rememberNativeSidebarFocus` before everything else.
        if let Some(space_id) = &self.select_space {
            intents.push(SidebarUiIntent::SelectSpace {
                space_id: space_id.clone(),
            });
        }
        if self.show_hidden {
            intents.push(SidebarUiIntent::ToggleShowHidden);
        }
        if self.clear_tag_filters {
            for tag in &ui.selected_tag_filters {
                intents.push(SidebarUiIntent::ToggleTagFilter { tag: tag.clone() });
            }
        }
        if let Some(storage_id) = &self.collapsed_collection_storage_id {
            intents.push(SidebarUiIntent::ToggleCollectionCollapsed {
                storage_id: storage_id.clone(),
            });
        }
        if self.collapsed_group {
            intents.push(SidebarUiIntent::ToggleGroupCollapsed {
                group_id: self.group_id.clone(),
            });
        }
        if let Some(section) = self.collapsed_section {
            intents.push(SidebarUiIntent::ToggleSection {
                storage_id: self.storage_id.clone(),
                section,
            });
        }
        if self.expand_list
            && !ui
                .collapse
                .expanded_session_lists
                .contains(&self.storage_id)
        {
            intents.push(SidebarUiIntent::ToggleSessionListExpanded {
                storage_id: self.storage_id.clone(),
            });
        }
        intents
    }
}

/// Works out what has to change for `sidebar_session_id` to be drawn, reading `view` (the list as
/// it stands) first.
///
/// A row on another machine moves the machine tab first, the way `rememberNativeSidebarFocus` does
/// with `reveal`. `None` when that machine's rows are not in the store: switching to a tab the
/// store cannot build would replace the list with an empty one, which is worse than not revealing.
pub fn reveal_plan(
    core: &Core,
    inputs: &SidebarInputs,
    view: &SidebarView,
    sidebar_session_id: &str,
    now_ms: u64,
) -> Option<SidebarRevealPlan> {
    // The machine has to be settled before anything else: the section key, the storage ids and the
    // Space memory below all belong to whichever machine's section the row is in.
    let (switched, other_machine) = inputs_for_row_machine(core, inputs, sidebar_session_id);
    // A reveal SWITCHES the tab, which is the one thing an ordinary focus change does not do.
    let select_machine = other_machine.then(|| {
        switched
            .as_ref()
            .expect("moved inputs accompany another machine")
            .ui
            .selected_machine_id
            .clone()
    });
    let inputs = switched.as_ref().unwrap_or(inputs);
    let parking = inputs.settings.enable_session_parking;
    // One clone, reused for both questions a rebuild can answer.
    let mut probe: Option<SidebarInputs> = None;
    let mut built: Option<SidebarView> = None;
    // The drawn list is the OTHER machine's when the tab moves, so it is not asked at all.
    let drawn = select_machine
        .is_none()
        .then(|| locate(view, sidebar_session_id, parking, now_ms))
        .flatten();
    let found = match drawn {
        Some(found) => found,
        None => {
            // Nothing filtering: no Space, no Show Hidden, no tags. The row is then wherever it is.
            let probe = probe.insert(unfiltered(inputs));
            let list = built.insert(SidebarViewModel::build_from_scratch(core, probe, now_ms));
            locate(list, sidebar_session_id, parking, now_ms)?
        }
    };
    let located = built.as_ref().unwrap_or(view);
    let group = located.group(&found.group_id)?;
    let collection_id = group.collection_id.clone();
    let collection_storage_id = collection_id
        .as_ref()
        .map(|collection_id| format!("{}:{collection_id}", inputs.ui.section_key()));

    let mut plan = SidebarRevealPlan {
        collapsed_group: inputs
            .ui
            .collapse
            .collapsed_groups
            .contains(&found.group_id),
        collapsed_collection_storage_id: collection_storage_id.clone().filter(|storage_id| {
            inputs
                .ui
                .collapse
                .collapsed_collections
                .contains(storage_id)
        }),
        collapsed_section: inputs
            .ui
            .collapse
            .section_collapse
            .get(&found.storage_id)
            .copied()
            .unwrap_or_default()
            .get(found.section)
            .then_some(found.section),
        show_hidden: !inputs.ui.show_hidden
            && (inputs
                .ui
                .hidden_items
                .group_ids
                .iter()
                .any(|id| *id == found.group_id)
                || collection_storage_id.is_some_and(|storage_id| {
                    inputs
                        .ui
                        .hidden_items
                        .collection_keys
                        .iter()
                        .any(|key| *key == storage_id)
                })),
        // `applyNativeSidebarReveal` asks the row itself, not why it was missing: a ticked filter
        // the row does not match is lifted whether or not something else also kept it out.
        clear_tag_filters: !inputs.ui.selected_tag_filters.is_empty()
            && !matches_tag_filters(
                found.effective_tag.as_deref(),
                &inputs.ui.selected_tag_filters,
            ),
        select_space: space_for_reveal(core, inputs, group, collection_id.as_deref()),
        remember_space: space_of_group(core, inputs, group, collection_id.as_deref()).map(
            |space_id| FocusedRowSpace {
                section_key: inputs.ui.section_key(),
                // The reveal's own `select_space` above carries the move; this is only the memory.
                follow: false,
                space_id,
            },
        ),
        select_machine,
        group_id: found.group_id.clone(),
        storage_id: found.storage_id.clone(),
        expand_list: false,
    };
    if found.drawn {
        return Some(plan);
    }
    // The row is in the group but in no heading's drawn rows, so either its heading is closed or
    // the compact list cut it. Only a list with that heading open tells the two apart.
    let probe = match probe.as_mut() {
        Some(probe) => probe,
        None => probe.insert(unfiltered(inputs)),
    };
    // With the filters the user will still have afterwards, which is not always none:
    // `applyNativeSidebarReveal` works the compact list out after it has decided whether to clear
    // them, and a filter that stays in place takes rows out of the list the cut is measured on.
    probe.ui.selected_tag_filters = if plan.clear_tag_filters {
        Vec::new()
    } else {
        inputs.ui.selected_tag_filters.clone()
    };
    probe
        .ui
        .collapse
        .section_collapse
        .entry(found.storage_id.clone())
        .or_default()
        .set(found.section, false);
    let opened = SidebarViewModel::build_from_scratch(core, probe, now_ms);
    plan.expand_list = !opened
        .group(&plan.group_id)
        .is_some_and(|group| drawn_in_sections(group, sidebar_session_id));
    Some(plan)
}

/// The inputs with everything that hides a row lifted: the Space, Show Hidden and the tag filters.
pub(crate) fn unfiltered(inputs: &SidebarInputs) -> SidebarInputs {
    let mut probe = inputs.clone();
    probe.ui.show_hidden = true;
    probe.ui.selected_tag_filters.clear();
    // Not by clearing the section's Space: an absent selection resolves to the section's FIRST
    // Space, which filters just as hard. Only turning Spaces off draws every group.
    probe.settings.sidebar_spaces_enabled = false;
    probe
}

/// The Space the focused row belongs to, the section its memory is keyed by, and whether the
/// section moves to it.
///
/// CDXC:Sidebar 2026-09-21 WHY:
/// **The row's machine decides everything here, not the machine tab.**
/// `rememberNativeSidebarFocus` resolves the group from `state.groupOrder`, which holds EVERY
/// machine's groups, and then asks `describeNativeSidebarMachine(ui, thatMachine)`: the section
/// key, the Spaces document and the stored selection all belong to the row's machine, and the tab
/// is NOT switched (only a reveal switches it). A first cut of this port took the section from the
/// selected tab instead, so with the tab on a remote machine every focus on a local row answered
/// `None`: the drawn list is one machine's, and `unfiltered` lifts Show Hidden, the tag filters and
/// Spaces but deliberately not the machine, so neither list held the row. `reveal_plan` had the
/// right shape one function away, which is the fifth time in this port that a fix existed beside
/// the defect; the machine move is shared with it now rather than written twice.
///
/// The drawn list answers without building anything only when the row is on the SELECTED machine,
/// because that is the only machine it holds. Otherwise the question is asked of a list built with
/// nothing filtering, exactly as `rememberNativeSidebarFocus` asks it of `state.groupOrder`.
/// Asking the drawn list instead would answer `None` for every row in another Space too, because
/// `assemble` keeps a group only when the selection shows it and `space_for_group` returns that
/// same selection under the same test.
///
/// The build is the reason the caller must only ask when the focused row CHANGED: one build per
/// focus into a row of another Space or another machine is the cost of the feature, one per publish
/// would not be.
pub fn space_for_focused_row(
    core: &Core,
    inputs: &SidebarInputs,
    view: &SidebarView,
    sidebar_session_id: &str,
    now_ms: u64,
) -> Option<FocusedRowSpace> {
    // Spaces off is the whole answer, and it is the common case: `describeNativeSidebarMachine`
    // reads no Spaces state at all then, so neither the follow nor the memory has anything to say
    // and nothing below runs.
    if !inputs.settings.sidebar_spaces_enabled {
        return None;
    }
    let (moved, other_machine) = inputs_for_row_machine(core, inputs, sidebar_session_id);
    let inputs = moved.as_ref().unwrap_or(inputs);
    let section_key = inputs.ui.section_key();
    // The drawn list is the SELECTED machine's, so it can only answer for a row on it.
    let drawn = (!other_machine)
        .then(|| find_group(view, sidebar_session_id))
        .flatten();
    let space_id = match drawn {
        Some(group) => space_of_group(core, inputs, group, group.collection_id.as_deref())?,
        None => {
            let unfiltered_inputs = unfiltered(inputs);
            let built = SidebarViewModel::build_from_scratch(core, &unfiltered_inputs, now_ms);
            let group = find_group(&built, sidebar_session_id)?;
            space_of_group(core, inputs, group, group.collection_id.as_deref())?
        }
    };
    // `rememberNativeSidebarFocus` writes the section's Space when the setting is on, and writes it
    // unconditionally, so a section that has never been written gets its first value here.
    let follow = inputs.settings.sidebar_space_follow_active_session
        && section_space_moves(inputs, &section_key, &space_id);
    Some(FocusedRowSpace {
        section_key,
        space_id,
        follow,
    })
}

/// The inputs as the ROW's machine sees them, and whether that machine is a different one.
///
/// `None` for the first element means the row is on the selected machine and the caller's own
/// inputs are the answer. A machine the store holds no rows for is left alone: nothing could be
/// resolved in it anyway, and it is the same machine `reveal_plan` refuses to switch to.
fn inputs_for_row_machine(
    core: &Core,
    inputs: &SidebarInputs,
    sidebar_session_id: &str,
) -> (Option<SidebarInputs>, bool) {
    let Some(machine_id) = row_machine_id(sidebar_session_id)
        .filter(|machine_id| *machine_id != inputs.ui.selected_machine_id)
        .filter(|machine_id| {
            core.presentation()
                .loaded(&machine_key(machine_id))
                .is_some()
        })
    else {
        return (None, false);
    };
    let mut moved = inputs.clone();
    moved.ui.selected_machine_id = machine_id;
    (Some(moved), true)
}

fn find_group<'a>(view: &'a SidebarView, sidebar_session_id: &str) -> Option<&'a GroupView> {
    view.groups.iter().find(|group| {
        group
            .core
            .sessions
            .iter()
            .any(|session| session.row.sidebar_session_id == sidebar_session_id)
    })
}

/// Where a row sits in a list.
struct Located {
    group_id: String,
    storage_id: String,
    section: SectionId,
    /// The row is one of the rows its heading draws.
    drawn: bool,
    /// The tag the row is filtered by.
    effective_tag: Option<String>,
}

fn locate(
    view: &SidebarView,
    sidebar_session_id: &str,
    enable_parking: bool,
    now_ms: u64,
) -> Option<Located> {
    let group = find_group(view, sidebar_session_id)?;
    let row = group
        .core
        .sessions
        .iter()
        .find(|session| session.row.sidebar_session_id == sidebar_session_id)
        .map(|session| session.row.as_ref());
    let drawn = drawn_in_sections(group, sidebar_session_id);
    let section = group
        .core
        .sections
        .iter()
        .find(|section| {
            section
                .session_ids
                .iter()
                .any(|session_id| session_id == sidebar_session_id)
        })
        .map(|section| section.id)
        // A row its heading does not draw is in no heading's list, so its heading is worked out
        // from the row itself, exactly as the sections are built.
        .or_else(|| row.map(|row| super::ordering::section_of(row, enable_parking, now_ms)))?;
    Some(Located {
        group_id: group.core.group_id.clone(),
        storage_id: group.core.storage_id.clone(),
        section,
        drawn,
        effective_tag: row.and_then(|row| row.effective_tag.clone()),
    })
}

/// `resolveSidebarSpaceForRevealedGroup`: the Space that shows the group, preferring the one the
/// section is already filtered by. `None` only when Spaces are off or the section has no Spaces.
///
/// One function rather than two: the reveal, the follow and the per-Space session memory all ask
/// exactly this question, and the reveal once had its own copy of the "has it moved" test as well,
/// which is how the follow and the drawing could have drifted by one section key.
fn space_of_group(
    core: &Core,
    inputs: &SidebarInputs,
    group: &GroupView,
    collection_id: Option<&str>,
) -> Option<String> {
    if !inputs.settings.sidebar_spaces_enabled {
        return None;
    }
    let machine = machine_key(&inputs.ui.selected_machine_id);
    let spaces = SpacesState::from_wire(
        core.presentation()
            .machine(&machine)?
            .side_state()
            .spaces
            .as_ref()?,
    );
    let section_key = inputs.ui.section_key();
    let selection = resolve_selected_space(
        &spaces,
        inputs
            .ui
            .collapse
            .selected_space_by_section
            .get(&section_key)
            .map(String::as_str),
    );
    let context = group.core.project_context.as_ref();
    Some(space_for_group(
        &spaces,
        &selection,
        context.map(|context| context.project_id.as_str()),
        collection_id,
        context
            .and_then(|context| context.worktree.as_ref())
            .map(|worktree| worktree.parent_project_id.as_str()),
    ))
}

/// Whether writing this Space into the section changes anything.
///
/// `rememberNativeSidebarFocus` writes it unconditionally, so a section that has never been written
/// gets its first value even when the resolved Space is the one it already falls back to. That
/// matters most for a REMOTE section, which a cross-machine reveal is usually the first thing ever
/// to touch: leaving it unwritten means the section keeps resolving to whichever Space happens to
/// be first, and moves on its own when the order does.
fn section_space_moves(inputs: &SidebarInputs, section_key: &str, space_id: &str) -> bool {
    match inputs
        .ui
        .collapse
        .selected_space_by_section
        .get(section_key)
    {
        None => true,
        Some(stored) => stored != space_id,
    }
}

/// The reveal's own use of the two above: the Space to select, when it is not already selected.
fn space_for_reveal(
    core: &Core,
    inputs: &SidebarInputs,
    group: &GroupView,
    collection_id: Option<&str>,
) -> Option<String> {
    let space_id = space_of_group(core, inputs, group, collection_id)?;
    section_space_moves(inputs, &inputs.ui.section_key(), &space_id).then_some(space_id)
}

/// The Space a focused row belongs to, and whether the section has to move to it.
///
/// Both answers come from one question, because `rememberNativeSidebarFocus` asks it once and does
/// two things with it: it always remembers the row under that Space, and it moves the section there
/// only when `sidebarSpaceFollowActiveSession` is on. Splitting them would mean building the
/// unfiltered list twice for one focus change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusedRowSpace {
    /// The section the memory is keyed by, which is the SELECTED machine's.
    pub section_key: String,
    /// The Space that shows the row.
    pub space_id: String,
    /// The section must move to that Space.
    pub follow: bool,
}

/// The machine tab a sidebar row belongs to, from its id alone.
///
/// A session row names its machine (`remote:<machine>:session:…`, or nothing at all for a local
/// one). A browser row is `gpui-browser:<workspace project id>:<tab>`, and the workspace project id
/// names the machine the same way. Anything else reads as this computer's, which is what every
/// unprefixed id is.
fn row_machine_id(sidebar_session_id: &str) -> Option<String> {
    if let Some(key) = SessionKey::parse_sidebar_session_id(sidebar_session_id) {
        return Some(machine_tab_id(&key.machine));
    }
    let browser = sidebar_session_id.strip_prefix("gpui-browser:")?;
    let (encoded_project, _) = browser.split_once(':')?;
    let project_id = crate::keys::decode_uri_component(encoded_project)?;
    let project = ProjectKey::parse_workspace_project_id(&project_id)?;
    Some(machine_tab_id(&project.machine))
}

fn machine_tab_id(machine: &MachineId) -> String {
    match machine.remote_id() {
        None => LOCAL_MACHINE_ID.to_string(),
        Some(machine_id) => machine_id.to_string(),
    }
}

pub(crate) fn machine_key(machine_id: &str) -> MachineId {
    if machine_id == LOCAL_MACHINE_ID {
        MachineId::Local
    } else {
        MachineId::Remote(machine_id.to_string())
    }
}

fn drawn_in_sections(group: &GroupView, sidebar_session_id: &str) -> bool {
    group.core.sections.iter().any(|section| {
        section
            .session_ids
            .iter()
            .any(|session_id| session_id == sidebar_session_id)
    })
}
