//! The daemon's armed Delayed Send of a workspace session, added to the dialog's open message
//! when the dialog is opened by the hotkey or by the session's own bar (named by its shell id), so
//! it shows the countdown the row's menu item shows (gx-core `daemon_delayed_send_seed`).

use ghostex_gx_core::{SessionKey, daemon_delayed_send_seed};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_remote_scoped_session_id;
use crate::app::model::TerminalSessionId;

impl GhostexGpuiApp {
    /// Writes the daemon's Delayed Send fields of the session behind this workspace tab into the
    /// open message. Nothing for a tab no daemon session backs.
    pub(crate) fn gx_store_seed_daemon_delayed_send(
        &self,
        open_message: &mut Value,
        session_id: TerminalSessionId,
    ) {
        let local = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == session_id).then_some(key));
        let session = match local {
            Some(key) => Some(SessionKey::local(
                key.project_id.as_str(),
                key.session_id.as_str(),
            )),
            None => self
                .remote_attach_sessions
                .iter()
                .find(|(_, mapped)| **mapped == session_id)
                .and_then(|(key, _)| {
                    SessionKey::parse_remote_scoped_session_id(&gpui_remote_scoped_session_id(
                        &key.remote_machine_id,
                        &key.project_id,
                        &key.session_id,
                    ))
                }),
        };
        let (Some(session), Some(open)) = (session, open_message.as_object_mut()) else {
            return;
        };
        open.extend(daemon_delayed_send_seed(&self.gx_store.core, &session));
    }
}
