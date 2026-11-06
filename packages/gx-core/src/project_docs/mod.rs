//! The two client-owned documents the project moves write, and the ordering rule they share.
//!
//! `collections` and `spaces` are the documents and their guard policies; `collection_edits` and
//! `space_edits` are the functions the moves apply to them; `collection_menu` is the Project Group
//! menu's own three writes (Rename, a colour, Ungroup) and `space_editor` the New/Edit Space
//! dialog's result, neither of which is a move; `worktree_order` is
//! the nesting rule, which the sidebar list and the project drag must agree on and which therefore
//! has one implementation rather than one each.

mod collection_edits;
mod collection_menu;
mod collections;
mod space_editor;
mod space_edits;
mod spaces;
mod worktree_order;

pub use collection_edits::{
    create_collection, move_projects_to_collection, reorder_collection_projects,
};
pub use collection_menu::{
    owns_collection_menu_command, plan_collection_menu_edit, COLLECTION_MENU_COMMAND_TYPE,
    COLLECTION_MENU_DOCUMENT_ACTIONS,
};
pub use collections::{
    CollectionsDocument, COLLECTIONS_SYNC_DELAY_MS, COLLECTIONS_SYNC_RETRY_DELAY_MS,
};
pub use space_editor::{plan_space_editor_result, SpaceEditorMode, SpaceEditorResult};
pub use space_edits::{
    apply_space_row_reorder, keep_spaces_across_collection_edit, move_members_to_space,
    reorder_spaces, toggle_space_member, SpaceMemberKind,
};
pub use spaces::{SpacesDocument, SPACES_SYNC_DELAY_MS, SPACES_SYNC_RETRY_DELAY_MS};
pub use worktree_order::{
    can_drop_project_with_worktrees, move_projects_with_worktrees, order_projects_with_worktrees,
    DropPosition, ProjectOrderItem,
};
