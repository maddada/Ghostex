//! No project: the project that holds every chat without a project of its own.
//!
//! - [`project`]: the project itself (a Ghostex-made project at its own managed folder, so it never
//!   collides with Main Hub's project at the home folder), whose chats start in the home folder,
//!   and the `no-project` alias a create may name it by.
//! - [`upgrade`]: the one-time move of Main Hub's chats into it.
//! - [`move_chat`]: Move to project, which takes one of its chats into a real project.
//! - [`moved_refs`]: a moved session's old ref keeps working.

mod move_chat;
mod moved_refs;
mod project;
mod upgrade;

pub(crate) use move_chat::{handle_move_session_to_project, MOVED_FROM_NO_PROJECT_KEY};
pub(crate) use moved_refs::follow_moved_refs;
pub(crate) use project::{
    ensure_no_project_project, is_no_project_project, place_create_alias, place_home_cwd,
    NO_PROJECT_ALIAS,
};

use std::sync::Arc;

use crate::server::AppState;

/// At gxserver start: makes sure the No project project exists (clients find it by its
/// `isNoProject` mark, so it is there before the first chat), then moves Main Hub's chats into it.
pub(crate) fn start_no_project_pass(state: Arc<AppState>) {
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || {
            let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
                return;
            };
            // Old refs of sessions moved earlier keep resolving (moved_refs.rs).
            moved_refs::load(&db);
            let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
            match ensure_no_project_project(&state, &db, &repository) {
                Ok(project) => {
                    if let Some(id) = project.get("projectId").and_then(serde_json::Value::as_str) {
                        moved_refs::backfill(&db, &state.metadata.server_id, id);
                    }
                    upgrade::move_main_hub_chats(&state, &db, &repository, &project)
                }
                Err(error) => eprintln!(
                    "Ghostex could not set up the No project project: {}",
                    error.message
                ),
            }
        })
        .await;
    });
}
