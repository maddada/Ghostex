//! Open and close for the "Add a link" dialog, shared by the desktop and the GPUI web build
//! (symlinked there).
//! SEE-ALSO: apps/desktop/src/app/window/add_session_link_modal.rs (the window and its gxserver
//! call), apps/desktop/src/app/native_sidebar/links_list.rs (the + button that opens it).
use crate::app::window::*;
use crate::*;
use ghostex_gx_core::SessionKey;

impl GhostexGpuiApp {
    /// Opens the dialog for `session`'s links, from its links list. A remote session whose
    /// machine is not connected opens nothing.
    pub(crate) fn open_gpui_add_session_link_modal(
        &mut self,
        session: &SessionKey,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(remote) = self.session_links_machine(session) else {
            return;
        };
        let config = AddSessionLinkModalConfig {
            project_id: session.project_id.clone(),
            session_id: session.session_id.clone(),
            remote,
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, _command, cx| {
            app.release_native_app_modal_window(GpuiAppModalKind::AddSessionLink, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::AddSessionLink,
            ADD_SESSION_LINK_MODAL_WIDTH,
            ADD_SESSION_LINK_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiAddSessionLinkModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }
}
