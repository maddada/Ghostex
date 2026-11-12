//! Spaces: the saved sidebar filters, which projects each one claims, and the built-in Other view.
//!
//! CDXC:Spaces 2026-09-07 DECISION:
//! User: each project belongs to at most one Space everywhere, including membership inherited from its group.
//! Existing duplicates keep the first Space in sidebar order; assigning another Space moves the member.
//!
//! Ported from `spaces.ts` and `sidebar-app/space-filtering.ts` in packages/core-ui and
//! `sidebar-spaces-other.ts` in packages/shared (all deleted 2026-10-01; see git history).
//!
//! SEE-ALSO: server/src/sidebar_spaces.rs normalizes the same invariant for every client and CLI write.

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_protocol::SidebarSpacesState as WireSpacesState;

use super::text::js_trim;

/// The reserved id of the built-in Other view.
pub const OTHER_SPACE_ID: &str = "other";
pub(crate) const OTHER_SPACE_LABEL: &str = "Other";
pub(crate) const OTHER_SPACE_ICON: &str = "layoutDashboard";
const DEFAULT_SPACE_ICON: &str = "stack";

const MAX_SPACES: usize = 256;
const MAX_MEMBER_IDS_PER_LIST: usize = 512;
const MAX_ID_CHARS: usize = 256;
const MAX_NAME_CHARS: usize = 256;
const MAX_ICON_CHARS: usize = 256;

/// The colours a Space falls back to, shared with the collection palette.
const SPACE_COLOR_PRESETS: &[&str] = &[
    "#4f5663", "#808080", "#7c6df2", "#3aa675", "#d6873f", "#d75b72", "#3f8fc7", "#b36ad4",
    "#8c9b45", "#c95353", "#c4a23d", "#2f9b95", "#596fd1",
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Space {
    pub space_id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub member_collection_ids: Vec<String>,
    pub member_project_ids: Vec<String>,
    /// The workspace the Space belongs to; `None` = the default workspace.
    pub workspace_id: Option<String>,
}

/// One machine's Spaces after client normalization.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpacesState {
    pub order: Vec<String>,
    pub spaces: BTreeMap<String, Space>,
}

/// One Space as it arrives, before the client sanitizer: the fields it reads, whether they came
/// off the wire or off a state this client has just edited.
pub(crate) struct RawSpace<'a> {
    pub(crate) color: &'a str,
    pub(crate) icon: &'a str,
    pub(crate) member_collection_ids: &'a [String],
    pub(crate) member_project_ids: &'a [String],
    pub(crate) name: &'a str,
    pub(crate) workspace_id: Option<&'a str>,
}

impl SpacesState {
    /// `sanitizeSidebarSpacesState`: the order array is authoritative, a project belongs to at
    /// most one Space, and every kept Space has a name, an icon, and a `#rrggbb` colour.
    pub fn from_wire(state: &WireSpacesState) -> Self {
        Self::sanitize(
            &state.order,
            state
                .spaces
                .iter()
                .map(|(space_id, space)| {
                    (
                        space_id.as_str(),
                        RawSpace {
                            color: &space.color,
                            icon: &space.icon,
                            member_collection_ids: &space.member_collection_ids,
                            member_project_ids: &space.member_project_ids,
                            name: &space.name,
                            workspace_id: space.workspace_id.as_deref(),
                        },
                    )
                })
                .collect(),
        )
    }

    /// The same sanitizer over a state this client built itself, which is what every `spaces.ts`
    /// editor ends in: `createSidebarSpace` and `updateSidebarSpace` hand their result straight to
    /// `sanitizeSidebarSpacesState`, so a new Space's colour, name and icon are bounded and the
    /// at-most-one-Space rule is enforced before the document is pushed.
    ///
    /// `deleteSidebarSpace` is the one editor that does NOT sanitize, so it does not call this.
    pub(crate) fn sanitized(&self) -> Self {
        Self::sanitize(
            &self.order,
            self.spaces
                .iter()
                .map(|(space_id, space)| {
                    (
                        space_id.as_str(),
                        RawSpace {
                            color: &space.color,
                            icon: &space.icon,
                            member_collection_ids: &space.member_collection_ids,
                            member_project_ids: &space.member_project_ids,
                            name: &space.name,
                            workspace_id: space.workspace_id.as_deref(),
                        },
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn sanitize(order: &[String], raw_spaces: Vec<(&str, RawSpace<'_>)>) -> Self {
        let mut candidates: Vec<(String, RawSpace<'_>)> = Vec::new();
        // A Space the `order` array does not name follows in id order here, where `Object.keys`
        // gives the TypeScript the document's own order, so the two can draw the Space rows in a
        // different sequence. Accepted rather than fixed: the wire type is a `BTreeMap`, so the
        // document order is gone before this runs, and every document the daemon writes has an
        // `order` array naming every Space it stores.
        for (raw_id, space) in raw_spaces {
            let Some(space_id) = bounded_text(raw_id, MAX_ID_CHARS) else {
                continue;
            };
            if candidates.iter().any(|(id, _)| *id == space_id) {
                continue;
            }
            candidates.push((space_id, space));
        }
        let mut ordered: Vec<String> = Vec::new();
        for entry in order {
            let Some(space_id) = bounded_text(entry, MAX_ID_CHARS) else {
                continue;
            };
            if candidates.iter().any(|(id, _)| *id == space_id) && !ordered.contains(&space_id) {
                ordered.push(space_id);
            }
        }
        for (space_id, _) in &candidates {
            if !ordered.contains(space_id) {
                ordered.push(space_id.clone());
            }
        }
        let mut sanitized = Self::default();
        let mut assigned_collections: BTreeSet<String> = BTreeSet::new();
        let mut assigned_projects: BTreeSet<String> = BTreeSet::new();
        for space_id in ordered {
            if sanitized.order.len() >= MAX_SPACES {
                break;
            }
            let Some((_, candidate)) = candidates.iter().find(|(id, _)| *id == space_id) else {
                continue;
            };
            let space = Space {
                color: sanitize_space_color(candidate.color, sanitized.order.len()),
                icon: bounded_text(candidate.icon, MAX_ICON_CHARS)
                    .unwrap_or_else(|| DEFAULT_SPACE_ICON.to_string()),
                member_collection_ids: sanitize_member_ids(
                    candidate.member_collection_ids,
                    &mut assigned_collections,
                ),
                member_project_ids: sanitize_member_ids(
                    candidate.member_project_ids,
                    &mut assigned_projects,
                ),
                name: bounded_text(candidate.name, MAX_NAME_CHARS)
                    .unwrap_or_else(|| space_id.clone()),
                space_id: space_id.clone(),
                workspace_id: candidate
                    .workspace_id
                    .and_then(|id| bounded_text(id, MAX_ID_CHARS)),
            };
            sanitized.spaces.insert(space_id.clone(), space);
            sanitized.order.push(space_id);
        }
        sanitized
    }

    pub(crate) fn ordered(&self) -> Vec<&Space> {
        self.order
            .iter()
            .filter_map(|space_id| self.spaces.get(space_id))
            .collect()
    }
}

fn bounded_text(value: &str, max_chars: usize) -> Option<String> {
    let text = js_trim(value);
    if text.is_empty() || text.chars().count() > max_chars {
        return None;
    }
    Some(text.to_string())
}

fn sanitize_member_ids(values: &[String], seen: &mut BTreeSet<String>) -> Vec<String> {
    let mut member_ids = Vec::new();
    for entry in values {
        if member_ids.len() >= MAX_MEMBER_IDS_PER_LIST {
            break;
        }
        let Some(member_id) = bounded_text(entry, MAX_ID_CHARS) else {
            continue;
        };
        if seen.contains(&member_id) {
            continue;
        }
        seen.insert(member_id.clone());
        member_ids.push(member_id);
    }
    member_ids
}

fn sanitize_space_color(value: &str, fallback_index: usize) -> String {
    let color = js_trim(value);
    if color.len() == 7
        && color.starts_with('#')
        && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return color.to_lowercase();
    }
    SPACE_COLOR_PRESETS[fallback_index % SPACE_COLOR_PRESETS.len()].to_string()
}

/// The view a section is filtered by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SpaceSelection {
    Space(String),
    Other,
    /// The Active projects tile (`active_projects.rs`).
    Active,
}

impl SpaceSelection {
    pub(crate) fn space_id(&self) -> &str {
        match self {
            SpaceSelection::Space(space_id) => space_id,
            SpaceSelection::Other => OTHER_SPACE_ID,
            SpaceSelection::Active => super::active_projects::ACTIVE_SPACE_ID,
        }
    }

    pub(crate) fn is_other(&self) -> bool {
        matches!(self, SpaceSelection::Other)
    }

    pub(crate) fn is_active_projects(&self) -> bool {
        matches!(self, SpaceSelection::Active)
    }
}

/// `resolveSelectedSidebarSpace`: the stored Space when it still exists, else the section's first
/// Space, else Other. Active projects resolves only while `active_projects` (its switch) is on;
/// off, a stored pick of it falls back like a deleted Space.
pub(crate) fn resolve_selected_space(
    state: &SpacesState,
    selected_space_id: Option<&str>,
    active_projects: bool,
) -> SpaceSelection {
    if selected_space_id == Some(OTHER_SPACE_ID) {
        return SpaceSelection::Other;
    }
    if active_projects && selected_space_id == Some(super::active_projects::ACTIVE_SPACE_ID) {
        return SpaceSelection::Active;
    }
    if let Some(space_id) = selected_space_id.filter(|id| state.spaces.contains_key(*id)) {
        return SpaceSelection::Space(space_id.to_string());
    }
    match state
        .order
        .iter()
        .find(|space_id| state.spaces.contains_key(*space_id))
    {
        Some(space_id) => SpaceSelection::Space(space_id.clone()),
        None => SpaceSelection::Other,
    }
}

/// `isSidebarSpaceProject`: a grouped project follows its collection, an ungrouped one its own id,
/// and a worktree its parent project.
pub(crate) fn space_claims_project(
    space: &Space,
    project_id: &str,
    collection_id: Option<&str>,
    parent_project_id: Option<&str>,
) -> bool {
    match collection_id {
        Some(collection_id) => space
            .member_collection_ids
            .iter()
            .any(|member| member == collection_id),
        None => {
            let owner = parent_project_id.unwrap_or(project_id);
            space
                .member_project_ids
                .iter()
                .any(|member| member == owner)
        }
    }
}

/// Whether the selected view shows a group. A row whose project cannot be resolved is invisible in
/// a Space and visible in Other; a user-made session group is asked
/// with the project it was made in.
///
/// Active projects answers yes for every project: which ones its list draws depends on their
/// sessions, which the list itself decides (assemble.rs). Everywhere else this is asked (a reveal,
/// a focus remembered, a project placed) the answer keeps Active projects selected, so opening a
/// session from it never switches Space (CDXC:Spaces 2026-11-12 in active_projects.rs).
pub(crate) fn selection_shows_project(
    selection: &SpaceSelection,
    state: &SpacesState,
    project_id: Option<&str>,
    collection_id: Option<&str>,
    parent_project_id: Option<&str>,
) -> bool {
    let Some(project_id) = project_id else {
        return selection.is_other();
    };
    match selection {
        SpaceSelection::Space(space_id) => state.spaces.get(space_id).is_some_and(|space| {
            space_claims_project(space, project_id, collection_id, parent_project_id)
        }),
        SpaceSelection::Other => !state
            .ordered()
            .into_iter()
            .any(|space| space_claims_project(space, project_id, collection_id, parent_project_id)),
        SpaceSelection::Active => true,
    }
}

/// `resolveSidebarSpaceForRevealedGroup`: the Space that shows a group, preferring the current
/// selection.
pub(crate) fn space_for_group(
    state: &SpacesState,
    selection: &SpaceSelection,
    project_id: Option<&str>,
    collection_id: Option<&str>,
    parent_project_id: Option<&str>,
) -> String {
    if selection_shows_project(
        selection,
        state,
        project_id,
        collection_id,
        parent_project_id,
    ) {
        return selection.space_id().to_string();
    }
    let Some(project_id) = project_id else {
        return OTHER_SPACE_ID.to_string();
    };
    state
        .order
        .iter()
        .find(|space_id| {
            state.spaces.get(*space_id).is_some_and(|space| {
                space_claims_project(space, project_id, collection_id, parent_project_id)
            })
        })
        .cloned()
        .unwrap_or_else(|| OTHER_SPACE_ID.to_string())
}

/// Where a project sorts in Space order: its Space's place in the row, Other after every Space
/// (Active projects lists its projects in this order, `active_projects.rs`).
pub(crate) fn space_rank_of_project(
    state: &SpacesState,
    project_id: Option<&str>,
    collection_id: Option<&str>,
    parent_project_id: Option<&str>,
) -> usize {
    let ordered = state.ordered();
    let Some(project_id) = project_id else {
        return ordered.len();
    };
    ordered
        .iter()
        .position(|space| space_claims_project(space, project_id, collection_id, parent_project_id))
        .unwrap_or(ordered.len())
}
