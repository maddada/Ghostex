//! Drag and drop: the moves that write an order rather than call the daemon.
//!
//! Per-concern files: `inventory` is the list a move is computed against, which is the
//! PROJECTION's membership and not the drawn list; `session_move` is the `moveSession` payload,
//! which decides the set and the order and posts one of two messages; `order_write` is what those
//! messages do, which is either an edit of the workspace session groups document (under the
//! pending-push guard in `crate::workspace_groups`) or the daemon's own `/api/updateSessionOrder`.
//!
//! `added_project` is the one placement that is not a gesture: the project the Add Project dialog
//! just added joining the open Space and moving to the top of it.
//!
//! The PROJECT moves are `project_inventory` (the group order they are computed against),
//! `project_move` (`moveGroup`, `moveToSpace`, `moveToCollection`, `moveCollection`, `moveSpace`,
//! `projectMembership` and `spaceMembership`) and `project_order_write` (what the `syncGroupOrder`
//! those post writes into the workspace session groups document). They are one piece because
//! `moveGroup` calls `updateNativeProjectDropMembership`: a project dragged into the middle of a
//! collection JOINS that collection in the same gesture, so a port that wrote only the project
//! order would reorder the row and silently drop it out of its folder.

mod added_project;
mod inventory;
mod order_write;
mod project_drop;
pub(crate) mod project_inventory;
mod project_move;
mod project_order_write;
mod session_drop;
mod session_handoff;
mod session_move;

pub use added_project::{
    plan_added_project_placement, plan_added_project_space_membership, AddedProjectPlacement,
};
pub use inventory::sidebar_group_membership;
pub use order_write::{
    owns_order_write_message, plan_order_write, OrderWrite, OrderWritePlan,
    ORDER_WRITE_MESSAGE_TYPES,
};
pub use project_drop::{project_body_drop_command, project_drop_command, project_drop_landing};
pub use project_inventory::sidebar_project_group_order;
pub use project_move::{
    owns_project_move_command, plan_project_move, ProjectMovePlan, ProjectWrite,
    PROJECT_MOVE_COMMAND_TYPES,
};
pub use project_order_write::{
    owns_project_order_message, plan_project_order_write, PROJECT_ORDER_MESSAGE_TYPE,
};
pub use session_drop::{
    owns_session_drop_command, plan_section_move, plan_session_drop, SessionDrop,
};
pub use session_handoff::{
    check_session_handoff, SessionHandoffCheck, SessionHandoffDrop, SESSION_DROP_ZONE_LIST_END,
};
pub use session_move::{owns_session_move_command, plan_session_move, SessionMovePlan};
