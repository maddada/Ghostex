//! What GBots (server/src/gbots/computer/) read and start on Cloud Boxes: the user's boxes, this
//! month's spend, and a GBot chat's start on its box through the same create-with-reuse path a
//! new chat's Cloud Box pick takes.

use std::sync::Arc;

use ghostex_gx_protocol::cloud_box::{
    CloudBox, CloudBoxDraftRef, CloudBoxState, CloudSpend, CreateParams,
};

use super::{create, registry, spend};
use crate::domain::DomainStateError;
use crate::server::AppState;

/// The boxes a GBot may be given: every box not deleted or being deleted.
pub(crate) fn boxes(state: &AppState) -> Result<Vec<CloudBox>, DomainStateError> {
    Ok(registry::rows(&state.paths)?
        .into_iter()
        .filter(|row| !matches!(row.state, CloudBoxState::Deleted | CloudBoxState::Deleting))
        .collect())
}

/// This month's Cloud Box spend, as Settings > Cloud Boxes shows it.
pub(crate) fn month_spend(state: &AppState) -> CloudSpend {
    spend::read_spend(state)
}

/// Starts `draft`'s session on `box_id` (waking the box when it is stopped), in its own worktree
/// of the box's project; returns at once with the box row.
pub(crate) async fn start_draft_on_box(
    state: Arc<AppState>,
    box_project_id: &str,
    draft: CloudBoxDraftRef,
    box_id: &str,
) -> Result<CloudBox, DomainStateError> {
    let request = CreateParams {
        project_id: box_project_id.to_string(),
        provider: None,
        snapshot_id: None,
        size: None,
        blank: None,
        draft: Some(draft),
        reuse_box_id: Some(box_id.to_string()),
        idempotency_key: None,
    };
    Ok(create::create(state, request).await?.cloud_box)
}
