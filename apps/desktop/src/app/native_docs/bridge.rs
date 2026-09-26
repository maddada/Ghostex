//! The Docs file requests (`list`, `read`, `save`, `rename`, ...), shared by the native Docs view
//! and the Docs page it replaces. Both send the same JSON the page always posted.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    /// Runs one Docs file request on the background executor against the current project (local,
    /// or remote through its gxserver), performs its side effect (copy path, reveal, add to session
    /// context) and hands the response JSON to `done` on the main thread.
    ///
    /// CDXC:Docs 2026-07-11:
    /// The request used to run synchronously inside the bridge event handler, but
    /// manage_files_bridge_result shells out to `git` (rev-parse/check-ignore/cat-file, up to six
    /// calls, no timeout) and reads files/directories. A stuck git (index.lock, network filesystem,
    /// slow hook) beach-balled the app, so it runs on the background executor.
    pub(crate) fn run_docs_files_request(
        &mut self,
        payload: String,
        cx: &mut gpui::Context<Self>,
        done: impl FnOnce(&mut Self, serde_json::Value, &mut gpui::Context<Self>) + 'static,
    ) {
        let snapshot = self.latest_sidebar_project_snapshot.clone();
        let additional_docs_folders_text =
            gpui_manage_additional_docs_folders_text(&self.sidebar_runtime_settings_snapshot);
        let global_docs_directory_text =
            gpui_global_docs_directory_text(&self.sidebar_runtime_settings_snapshot);
        let remote_context = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .and_then(|project_id| {
                gpui_remote_project_reference_from_project_id(project_id.0.as_str())
            })
            .map(|reference| {
                let target =
                    self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str());
                (reference, target)
            });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let outcome = background
                .spawn(async move {
                    match remote_context {
                        Some((reference, target)) => {
                            run_remote_manage_files_bridge_request_for_project_snapshot(
                                &payload,
                                snapshot.as_ref(),
                                &additional_docs_folders_text,
                                &reference,
                                target.as_ref(),
                            )
                        }
                        None => run_manage_files_bridge_request_for_project_snapshot(
                            &payload,
                            snapshot.as_ref(),
                            &additional_docs_folders_text,
                            &global_docs_directory_text,
                        ),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let ManageFilesBridgeOutcome {
                    action,
                    request_id,
                    mut response,
                    side_effect,
                } = outcome;
                if let Some(side_effect) = side_effect
                    && let Err(error) =
                        this.perform_manage_files_bridge_side_effect(side_effect, cx)
                {
                    response = manage_files_bridge_error_response(&action, &request_id, &error);
                }
                done(this, response, cx);
            });
        })
        .detach();
    }
}
