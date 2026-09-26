//! The New/Edit Space dialog's result as a document edit.
//!
//! The dialog reports FIELD VALUES only (a mode, a Space id, a name, an icon, a colour, and at most
//! one member id), so this is where those values become a state change, and against whatever
//! document is held when the result arrives rather than against the one the dialog opened on. A
//! result naming a Space that has since been deleted changes nothing, because both the update and
//! the delete leave an unknown id alone.
//!
//! Create and edit SANITIZE their result, delete does not: `deleteSidebarSpace` builds the next
//! state by hand, which is what keeps a member id the daemon still holds from being dropped by a
//! sanitizer pass the TypeScript never makes.
//!
//! SEE-ALSO: packages/core-ui/spaces.ts (`applySidebarSpaceEditorResult`),
//! packages/core-ui/space-colors.ts, apps/desktop/src/app/gx_store/space_editor.rs. (The sidebar
//! page's half was frozen in the deleted `tooling/gx-core/sidebar-page-frozen/metadata.ts`.)

use serde_json::Value;

use crate::sidebar_view::spaces::RawSpace;
use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::{Space, SpacesState};

use super::collection_edits::base36;
use super::spaces::SpacesDocument;

/// `DEFAULT_SIDEBAR_SPACE_ICON`.
const DEFAULT_SPACE_ICON: &str = "stack";

/// `SIDEBAR_SPACE_COLORS`: the collection palette with the dark-theme gray taken out, because a
/// Space offers one Gray that draws as either hex depending on the theme
/// (`CDXC:Spaces 2026-09-21 DECISION` in packages/core-ui/space-colors.ts).
const SPACE_COLORS: &[&str] = &[
    "#4f5663", "#7c6df2", "#3aa675", "#d6873f", "#d75b72", "#3f8fc7", "#b36ad4", "#8c9b45",
    "#c95353", "#c4a23d", "#2f9b95", "#596fd1",
];

/// Which of the dialog's three buttons was pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceEditorMode {
    Create,
    Delete,
    Edit,
}

/// The dialog's result, as the app-modal window reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceEditorResult {
    pub mode: SpaceEditorMode,
    pub space_id: Option<String>,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub member_collection_id: Option<String>,
    pub member_project_id: Option<String>,
    /// The machine whose document the result edits. `None` is this computer.
    pub remote_machine_id: Option<String>,
}

impl SpaceEditorResult {
    /// The `applySidebarSpaceEditorResult` payload. `None` for a mode outside the three, which is
    /// what the dialog's own forwarder already refuses.
    pub fn from_json(message: &Value) -> Option<Self> {
        let text = |key: &str| message.get(key).and_then(Value::as_str).map(str::to_string);
        let mode = match message.get("mode").and_then(Value::as_str)? {
            "create" => SpaceEditorMode::Create,
            "delete" => SpaceEditorMode::Delete,
            "edit" => SpaceEditorMode::Edit,
            _ => return None,
        };
        Some(Self {
            mode,
            space_id: text("spaceId"),
            name: text("name"),
            icon: text("icon"),
            color: text("color"),
            member_collection_id: text("memberCollectionId"),
            member_project_id: text("memberProjectId"),
            remote_machine_id: text("remoteMachineId"),
        })
    }
}

/// The document after the dialog's result, `None` when the result writes nothing.
///
/// `None` is every `return state` in `applySidebarSpaceEditorResult`: a delete or an edit with no
/// Space id, and a create with no name. The caller must not write in that case, because the
/// TypeScript's caller (`updateSpaces`) is reached with the SAME OBJECT and the page's identity
/// test is what stops the push. Modelled as a refusal rather than as an unchanged write so a
/// nameless Space never reaches the daemon.
pub fn plan_space_editor_result(
    document: &SpacesDocument,
    result: &SpaceEditorResult,
    now_ms: i64,
) -> Option<SpacesDocument> {
    match result.mode {
        SpaceEditorMode::Delete => {
            let space_id = result.space_id.as_deref()?;
            delete_space(document, space_id)
        }
        SpaceEditorMode::Edit => {
            let space_id = result.space_id.as_deref()?;
            update_space(document, space_id, result)
        }
        SpaceEditorMode::Create => {
            // `result.name?.trim()`, and an empty one is the whole refusal: a Space with no name
            // would be named after its own generated id by the sanitizer.
            let name = js_trim(result.name.as_deref().unwrap_or_default());
            if name.is_empty() {
                return None;
            }
            let (space_id, created) = create_space(document, name, result, now_ms);
            let mut next = created;
            if let Some(collection_id) = result.member_collection_id.as_deref() {
                next = super::space_edits::toggle_space_member(
                    &next,
                    &space_id,
                    super::space_edits::SpaceMemberKind::Collection,
                    collection_id,
                );
            }
            if let Some(project_id) = result.member_project_id.as_deref() {
                next = super::space_edits::toggle_space_member(
                    &next,
                    &space_id,
                    super::space_edits::SpaceMemberKind::Project,
                    project_id,
                );
            }
            Some(next)
        }
    }
}

/// `deleteSidebarSpace`. `None` for an id the document does not hold, which is the TypeScript's
/// `if (!(spaceId in state.spaces)) return state`.
fn delete_space(document: &SpacesDocument, space_id: &str) -> Option<SpacesDocument> {
    if !document.state.spaces.contains_key(space_id) {
        return None;
    }
    let mut spaces = document.state.spaces.clone();
    spaces.remove(space_id);
    Some(SpacesDocument {
        state: SpacesState {
            order: document
                .state
                .order
                .iter()
                .filter(|candidate| *candidate != space_id)
                .cloned()
                .collect(),
            spaces,
        },
    })
}

/// `updateSidebarSpace`: the three fields the dialog can change, each left alone when the result
/// does not carry it, then the whole state sanitized.
fn update_space(
    document: &SpacesDocument,
    space_id: &str,
    result: &SpaceEditorResult,
) -> Option<SpacesDocument> {
    let space = document.state.spaces.get(space_id)?;
    let mut spaces = document.state.spaces.clone();
    spaces.insert(
        space_id.to_string(),
        Space {
            color: result.color.clone().unwrap_or_else(|| space.color.clone()),
            icon: result.icon.clone().unwrap_or_else(|| space.icon.clone()),
            name: result.name.clone().unwrap_or_else(|| space.name.clone()),
            member_collection_ids: space.member_collection_ids.clone(),
            member_project_ids: space.member_project_ids.clone(),
            space_id: space.space_id.clone(),
        },
    );
    Some(SpacesDocument {
        state: SpacesState {
            order: document.state.order.clone(),
            spaces,
        }
        .sanitized(),
    })
}

/// `createSidebarSpace`: a new Space goes LAST, and its id carries its position and a base-36
/// timestamp, so the host passes the clock in the way the collection ids do.
fn create_space(
    document: &SpacesDocument,
    name: &str,
    result: &SpaceEditorResult,
    now_ms: i64,
) -> (String, SpacesDocument) {
    let position = document.state.order.len() + 1;
    let space_id = format!("space-{position}-{}", base36(now_ms));
    let color = result.color.clone().unwrap_or_else(|| {
        SPACE_COLORS[document.state.order.len() % SPACE_COLORS.len()].to_string()
    });
    let icon = result
        .icon
        .clone()
        .unwrap_or_else(|| DEFAULT_SPACE_ICON.to_string());
    let mut order = document.state.order.clone();
    order.push(space_id.clone());
    let mut raw: Vec<(&str, RawSpace<'_>)> = document
        .state
        .spaces
        .iter()
        .map(|(id, space)| {
            (
                id.as_str(),
                RawSpace {
                    color: &space.color,
                    icon: &space.icon,
                    member_collection_ids: &space.member_collection_ids,
                    member_project_ids: &space.member_project_ids,
                    name: &space.name,
                },
            )
        })
        .collect();
    const EMPTY_MEMBERS: &[String] = &[];
    raw.push((
        space_id.as_str(),
        RawSpace {
            color: &color,
            icon: &icon,
            member_collection_ids: EMPTY_MEMBERS,
            member_project_ids: EMPTY_MEMBERS,
            name,
        },
    ));
    let state = SpacesState::sanitize(&order, raw);
    (space_id, SpacesDocument { state })
}
