//! A session's links: opening one where Settings says, removing one, and linking a suggested one.
//!
//! SEE-ALSO: apps/desktop/src/app/native_sidebar/link_chips.rs and
//! apps/desktop/src/app/native_sidebar/links_list.rs (what sends these),
//! server/src/server/route_http/session_links.rs (`/api/addSessionLink`, `/api/removeSessionLink`).

use ghostex_gx_core::SessionKey;
use serde_json::{Value, json};

use super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::model::{GpuiBrowserRendererOpenReuse, GpuiSidebarOpenBrowserUrlMessage};

impl GhostexGpuiApp {
    /// Answers `openSessionLink {url, sessionId}`, `removeSessionLink {sessionId, key}`,
    /// `restoreSessionLink {sessionId, key}`, `pinSessionLink {sessionId, key, pinned}` and
    /// `addSessionLink {sessionId, link}`; the session is
    /// named by its sidebar id.
    pub(crate) fn gx_store_run_session_links(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        match command.get("type").and_then(Value::as_str) {
            Some("setSessionLinksFolded") => {
                let (Some(section), Some(folded)) = (
                    command.get("section").and_then(Value::as_str),
                    command.get("folded").and_then(Value::as_bool),
                ) else {
                    return true;
                };
                let mut sections =
                    crate::app::native_sidebar::links_list::folded_session_link_groups();
                sections.retain(|known| known != section);
                if folded {
                    sections.push(section.to_string());
                }
                // A browser page has no settings write (its Settings patch answers with a toast),
                // so its folds live in the page's own copy of the settings until it reloads.
                #[cfg(target_arch = "wasm32")]
                {
                    let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
                    let mut object = settings.object().clone();
                    object.insert("sessionLinksFoldedGroups".to_string(), json!(sections));
                    crate::shared_settings::install(object);
                    cx.notify();
                }
                #[cfg(not(target_arch = "wasm32"))]
                self.handle_gpui_app_modal_update_settings_patch_message(
                    &json!({
                        "patch": { "sessionLinksFoldedGroups": sections },
                        "source": "sessionLinks:fold",
                    }),
                    cx,
                );
                true
            }
            Some("openSessionLink") => {
                if let Some(url) = command
                    .get("url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                {
                    let session_id = command.get("sessionId").and_then(Value::as_str);
                    self.open_session_link(url.to_string(), session_id, cx);
                }
                true
            }
            Some(
                kind @ ("removeSessionLink" | "addSessionLink" | "restoreSessionLink"
                | "pinSessionLink"),
            ) => {
                let Some(session) = command
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .and_then(SessionKey::parse_sidebar_session_id)
                else {
                    return true;
                };
                let Some(remote) = self.session_links_machine(&session) else {
                    return true;
                };
                let (path, params, failure) = if kind == "pinSessionLink" {
                    (
                        "/api/pinSessionLink",
                        json!({
                            "projectId": session.project_id,
                            "sessionId": session.session_id,
                            "key": command.get("key").cloned().unwrap_or(Value::Null),
                            "pinned": command.get("pinned").and_then(Value::as_bool).unwrap_or(true),
                        }),
                        "Couldn't pin the link",
                    )
                } else if kind == "restoreSessionLink" {
                    (
                        "/api/restoreSessionLink",
                        json!({
                            "projectId": session.project_id,
                            "sessionId": session.session_id,
                            "key": command.get("key").cloned().unwrap_or(Value::Null),
                        }),
                        "Couldn't restore the link",
                    )
                } else if kind == "removeSessionLink" {
                    (
                        "/api/removeSessionLink",
                        json!({
                            "projectId": session.project_id,
                            "sessionId": session.session_id,
                            "key": command.get("key").cloned().unwrap_or(Value::Null),
                        }),
                        "Couldn't remove the link",
                    )
                } else {
                    (
                        "/api/addSessionLink",
                        json!({
                            "projectId": session.project_id,
                            "sessionId": session.session_id,
                            "link": command.get("link").cloned().unwrap_or(Value::Null),
                            "source": "user",
                        }),
                        "Couldn't link it",
                    )
                };
                cx.spawn(async move |this, cx| {
                    if let Err(error) = gx_rpc(remote, path, params).await {
                        let _ = this.update(cx, |this, cx| {
                            this.dispatch_gpui_workspace_action_toast(
                                "error",
                                failure,
                                &error.message,
                                cx,
                            );
                        });
                    }
                })
                .detach();
                true
            }
            _ => false,
        }
    }

    /// Opens a PR, issue or Linear page in Ghostex's Browser, or in the default browser when
    /// Settings > General > Browser > Open session links in says so.
    ///
    /// CDXC:SessionLinks 2026-10-24 DECISION:
    /// User: "user can pick to open in embedded or external browser in settings (default to embedded)" (`sessionLinkOpenTarget`: `embedded` or `external`). A Work mode card's own chips keep opening the ticket in the Work view (CDXC:WorkMode 2026-10-09).
    /// A session's links list just opened (its card, the header's "+N" list): gxserver reads its
    /// links now instead of on their usual schedule (`/api/refreshSessionLinks`,
    /// server/src/server/session_links_sync.rs `want_session_links_now`).
    pub(crate) fn want_session_links_now(&self, key: &SessionKey, cx: &mut gpui::Context<Self>) {
        let Some(remote) = self.session_links_machine(key) else {
            return;
        };
        let params = json!({ "projectId": key.project_id, "sessionId": key.session_id });
        cx.spawn(async move |_, _| {
            let _ = gx_rpc(remote, "/api/refreshSessionLinks", params).await;
        })
        .detach();
    }

    /// The gxserver that owns `session`'s links: `Some(None)` for this computer's,
    /// `Some(Some(tunnel))` for a remote machine's while its tunnel is up, `None` while it is not.
    ///
    /// CDXC:SessionLinks 2026-10-27 WHY:
    /// The links card's pin, remove and undo, a suggestion's link and the card's refresh were sent only for this computer's sessions, so a remote machine's session opened no card and its links could not be changed. They go to the machine that owns the session, like a remote row's sleep or close (gx_store/sidebar_remote.rs); its gxserver allows these calls from a remote client (server/src/protocol.rs).
    pub(crate) fn session_links_machine(
        &self,
        session: &SessionKey,
    ) -> Option<Option<crate::app::model::GpuiRemoteGxserverRequestTarget>> {
        match session.machine.remote_id() {
            None => Some(None),
            Some(machine_id) => self
                .gpui_remote_gxserver_request_target(machine_id)
                .map(Some),
        }
    }

    /// `session_id` (a sidebar session id) is the session whose link it is: the page opens in its
    /// project (`gx_store_open_browser_url_in_session_project`).
    pub(crate) fn open_session_link(
        &mut self,
        url: String,
        session_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let external = crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("sessionLinkOpenTarget")
            .and_then(Value::as_str)
            == Some("external");
        if external {
            crate::app::helpers::gpui_open_external_url(&url, cx);
            return;
        }
        let message = GpuiSidebarOpenBrowserUrlMessage {
            url,
            reuse: GpuiBrowserRendererOpenReuse::Exact,
            from_quick_header: false,
            project_id: None,
        };
        self.gx_store_open_browser_url_in_session_project(message, session_id, cx);
    }
}
