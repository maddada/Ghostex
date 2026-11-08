//! Chief's rows: the session each computer flags `isChief` (server/src/chief/), drawn under the
//! Spaces bar instead of in its project's list (`PresentationSession::listed_in_sidebar`).
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1734, Q1741, Q1747): Chief is "its own row but under spaces bar (stays static while switching space)", one per computer. With computers combined the row shows this computer's Chief with a "+N" that unfolds the other computers' Chief rows under it, the way a coordinator's threads fold; with Separate by machine on, each computer's tab shows its own computer's Chief.
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/chief.rs (the renderer), apps/mobile/app (the phone's row).

use crate::keys::{MachineId, SessionKey};
use crate::presentation_store::PresentationStore;
use crate::LOCAL_MACHINE_ID;

use super::groups::FocusKey;
use super::inputs::SidebarInputs;
use super::rows::{session_row, RowContext};
use super::view::SessionRow;

/// One computer's Chief.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChiefRowView {
    /// The machine tab id (`local` for this computer).
    pub machine_id: String,
    /// The computer's name, for the folded rows ("Chief · Hetzner").
    pub machine_label: String,
    pub is_local: bool,
    /// Its chat is the one open in the work area.
    pub is_focused: bool,
    pub row: SessionRow,
}

/// The Chief rows to draw: the selected machine's, or with `merging` every fed machine's, this
/// computer first. A machine whose Chief was never opened has none.
pub(crate) fn chief_rows(
    store: &PresentationStore,
    inputs: &SidebarInputs,
    selected: &MachineId,
    merging: bool,
    focus: &FocusKey,
    context: &RowContext<'_>,
) -> Vec<ChiefRowView> {
    let mut machines: Vec<MachineId> = vec![selected.clone()];
    if merging {
        machines = std::iter::once(MachineId::Local)
            .chain(
                inputs
                    .host
                    .machines
                    .iter()
                    .filter(|tab| !tab.is_local() && tab.fed && !tab.this_computer)
                    .map(|tab| MachineId::Remote(tab.machine_id.clone())),
            )
            .collect();
    }
    machines
        .into_iter()
        .filter_map(|machine| {
            let entry = store.machine(&machine)?;
            let loaded = entry.loaded()?;
            let session = loaded.server_sessions().find(|session| session.is_chief)?;
            // The local patch (a rename, a read mark) applies to it like to any row; a session
            // hidden locally is not drawn.
            let session = entry.effective_session(&session.project_id, &session.session_id)?;
            let key = SessionKey {
                machine: machine.clone(),
                project_id: session.project_id.clone(),
                session_id: session.session_id.clone(),
            };
            let machine_id = machine.remote_id().unwrap_or(LOCAL_MACHINE_ID).to_string();
            let machine_label = inputs
                .host
                .machine(&machine_id)
                .map(|tab| tab.label.trim().to_string())
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| "This computer".to_string());
            let is_focused = focus.focused_session_id.as_deref()
                == Some(session.session_id.as_str())
                && focus.active_project_id.as_deref() == Some(session.project_id.as_str());
            Some(ChiefRowView {
                is_local: machine.is_local(),
                is_focused,
                row: session_row(key, &session, None, None, context),
                machine_id,
                machine_label,
            })
        })
        .collect()
}
