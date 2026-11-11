//! The chat header's Share popover (Ghostex Collab): reads gxserver (`/api/collab`,
//! server/collab/src/owner.rs, through gxserver's Collab shim) and runs what the owner clicks;
//! the look, every state of it, is window/share_popover_view.rs.
//!
//! - Collab not installed: Install right here (Touch ID or the phone), its download, then "Ready
//!   to share"; or "Don't show Share".
//! - Ready: what guest messages do, how long a link lasts (the chip also changes the default),
//!   Create link (Touch ID). Picking Send before the link exists turns it on right after (Touch ID
//!   again: Send is never a default).
//! - Live: the link and code (in memory for this run; Show asks for Touch ID after a restart), who
//!   is watching with Remove, a guest's waiting message with Review (the card above the chat box),
//!   the guest-message mode, the time left, Stop sharing (asks first).
//!
//! The popover is a window of its own; it grows and shrinks to fit what it shows.
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1615): "Where Share lives: a button in the chat header", with its popover holding the link, code, expiry, mode, viewers and Stop sharing.
//!
//! CDXC:Collab 2026-11-08 DECISION:
//! User (#495): sharing, Send mode and approving a guest's message need the owner's signed approval. The app's own call shows Touch ID at once (`approve_own_call` in gx_rpc's transport); if that is declined the request can still be approved on the phone, which the popover waits for. No read returns a link or a code: they come only with an approved Create link (or Show), and the app keeps them in memory for this run, never on disk.
//! SEE-ALSO: apps/desktop/src/app/render/workarea_header/share_button.rs (the button), apps/desktop/src/app/window/settings_modal/tabs/collab.rs (Settings > Collab).

use std::rc::Rc;

use super::share_popover_view::{
    self as view, Lifetime, MODES, SHARE_POPOVER_WIDTH, ShareAction, ShareModel, Stage,
};
use crate::app::gx_store::gx_rpc;
use crate::app::helpers::*;
use crate::*;
use ghostex_gx_core::SessionKey;
use gpui_component::ElementExt as _;
use serde_json::{Value, json};

pub(crate) const COLLAB_SHARE_POPUP_WIDTH: f32 = SHARE_POPOVER_WIDTH;
/// The first height the popover opens at (the Ready state); it then fits what it shows.
pub(crate) const COLLAB_SHARE_POPUP_HEIGHT: f32 = 300.0;
/// The tallest it grows (four guests and a waiting card fit).
const MAX_HEIGHT: f32 = 640.0;

pub(crate) struct CollabSharePanel {
    key: SessionKey,
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
    state: Option<Value>,
    error: Option<String>,
    busy: bool,
    copied: Option<&'static str>,
    /// A change waiting for the owner's approval: what it does, in gxserver's words, and the
    /// request id once gxserver answered with one.
    waiting: Option<(String, Option<String>)>,
    /// Which change waits: Install and Create link have their own band.
    waiting_kind: Option<Waiting>,
    /// Bumped by Cancel, so a call that answers later is ignored.
    generation: u64,
    /// What a new share's guest messages do, picked before Create link.
    mode: usize,
    lifetime_menu_open: bool,
    confirm_stop: bool,
    /// This chat's share was just stopped here: "Sharing ended" until the next change.
    just_stopped: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Waiting {
    Install,
    Create,
    Other,
}

thread_local! {
    /// Links and codes the owner revealed with an approval, by share id, for this run only.
    static REVEALED: std::cell::RefCell<std::collections::HashMap<String, (String, String)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn revealed(share_id: &str) -> Option<(String, String)> {
    REVEALED.with(|revealed| revealed.borrow().get(share_id).cloned())
}

/// Waits for approval request `id` to be decided: `Ok` once approved (gxserver then makes the
/// change itself), `Err` with a sentence when it was denied or lapsed. Polls
/// `/api/readApprovalRequest` (the requester may read its own request).
pub(crate) async fn wait_for_approval(id: String) -> Result<(), String> {
    // A pending request lapses after 10 minutes.
    for _ in 0..(11 * 60) {
        futures_timer_delay(Duration::from_secs(1)).await;
        let Ok(answer) = gx_rpc(None, "/api/readApprovalRequest", json!({ "id": id })).await else {
            continue;
        };
        match answer["request"]["status"].as_str().unwrap_or_default() {
            "approved" | "consumed" => return Ok(()),
            "denied" => return Err("You didn't approve it.".to_string()),
            "expired" => return Err("The approval request ran out. Try again.".to_string()),
            _ => {}
        }
    }
    Err("The approval request ran out. Try again.".to_string())
}

async fn futures_timer_delay(duration: Duration) {
    let (sender, receiver) = futures::channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        let _ = sender.send(());
    });
    let _ = receiver.await;
}

impl CollabSharePanel {
    pub(crate) fn new(
        key: SessionKey,
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        cx.spawn(async |this, cx| {
            loop {
                let Ok(fast) = this.update(cx, |panel, cx| {
                    panel.read(cx);
                    panel.installing()
                }) else {
                    break;
                };
                // Faster while Collab downloads, so the bar moves.
                cx.background_executor()
                    .timer(Duration::from_millis(if fast { 700 } else { 2000 }))
                    .await;
            }
        })
        .detach();
        Self {
            key,
            main_app,
            state: None,
            error: None,
            busy: false,
            copied: None,
            waiting: None,
            waiting_kind: None,
            generation: 0,
            mode: 1,
            lifetime_menu_open: false,
            confirm_stop: false,
            just_stopped: false,
        }
    }

    fn installing(&self) -> bool {
        self.state.as_ref().is_some_and(|state| {
            matches!(
                state["component"]["phase"].as_str(),
                Some("downloading" | "installing")
            )
        }) || self.waiting_kind == Some(Waiting::Install)
    }

    fn ids(&self) -> Value {
        json!({"projectId": self.key.project_id, "sessionId": self.key.session_id})
    }

    fn read(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/collab", json!({"op": "read"})).await;
            let _ = this.update(cx, |panel, cx| {
                if let Ok(value) = result {
                    if panel.state.is_none() {
                        // The new share's mode starts at the default Settings keeps.
                        panel.mode = MODES
                            .iter()
                            .position(|(id, _)| value["config"]["mode"] == *id)
                            .unwrap_or(1);
                    }
                    panel.state = Some(value);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// One call to gxserver; `kind` says it may wait for the owner's approval (and which band
    /// says so). `then` runs with the answer.
    fn call(
        &mut self,
        path: &'static str,
        params: Value,
        kind: Option<(Waiting, &str)>,
        then: impl FnOnce(&mut Self, Value, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.busy || self.waiting.is_some() {
            return;
        }
        self.busy = true;
        self.error = None;
        if let Some((waiting, summary)) = kind {
            // Touch ID shows while the call is out; the popover says what it is for.
            self.waiting = Some((summary.to_string(), None));
            self.waiting_kind = Some(waiting);
        }
        let generation = self.generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mut result = gx_rpc(None, path, params.clone()).await;
            // Touch ID wasn't used (declined, or no approver here): the phone may still approve;
            // gxserver makes the change itself then, and the repeated call answers with it.
            if let Some(approval) = result
                .as_ref()
                .err()
                .and_then(|error| error.approval_required())
            {
                let id = approval["id"].as_str().unwrap_or_default().to_string();
                let summary = approval["summary"].as_str().unwrap_or_default().to_string();
                let still = this
                    .update(cx, |panel, cx| {
                        if panel.generation != generation {
                            return false;
                        }
                        let kind = panel.waiting_kind.unwrap_or(Waiting::Other);
                        panel.waiting = Some((summary, Some(id.clone())));
                        panel.waiting_kind = Some(kind);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !still {
                    return;
                }
                result = match wait_for_approval(id.clone()).await {
                    Ok(()) => {
                        let mut again = params;
                        again["approvalId"] = json!(id);
                        gx_rpc(None, path, again).await
                    }
                    Err(message) => Err(crate::app::gx_store::GxRpcError::transport(path, message)),
                };
            }
            let _ = this.update(cx, |panel, cx| {
                if panel.generation != generation {
                    return;
                }
                panel.busy = false;
                panel.waiting = None;
                panel.waiting_kind = None;
                match result {
                    Ok(value) => then(panel, value, cx),
                    Err(error) => panel.error = Some(error.to_string()),
                }
                panel.read(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn collab(
        &mut self,
        params: Value,
        kind: Option<(Waiting, &str)>,
        then: impl FnOnce(&mut Self, Value, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        self.call("/api/collab", params, kind, then, cx);
    }

    fn keep_secrets(value: &Value) {
        let share = &value["share"];
        if let (Some(id), Some(link), Some(code)) = (
            share["shareId"].as_str(),
            share["link"].as_str(),
            share["code"].as_str(),
        ) {
            REVEALED.with(|revealed| {
                revealed
                    .borrow_mut()
                    .insert(id.to_string(), (link.to_string(), code.to_string()))
            });
        }
    }

    fn copy(&mut self, what: &'static str, text: String, cx: &mut gpui::Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.copied = Some(what);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1600))
                .await;
            let _ = this.update(cx, |panel, cx| {
                if panel.copied == Some(what) {
                    panel.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn share_id(&self) -> Option<String> {
        let state = self.state.as_ref()?;
        state["shares"].as_array()?.iter().find_map(|share| {
            (share["projectId"] == self.key.project_id.as_str()
                && share["sessionId"] == self.key.session_id.as_str())
            .then(|| share["shareId"].as_str().map(str::to_string))
            .flatten()
        })
    }

    /// Runs `change` in the main window (closing this popover, focusing the chat box).
    fn in_main_window(
        &self,
        change: impl FnOnce(&mut GhostexGpuiApp, &mut Window, &mut gpui::Context<GhostexGpuiApp>)
        + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        let main_app = self.main_app.clone();
        let Some(handle) = main_app
            .read_with(cx, |app, _| app.main_window_handle)
            .ok()
            .flatten()
        else {
            return;
        };
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, cx| {
                let _ = main_app.update(cx, |app, cx| change(app, window, cx));
            });
        });
    }

    fn act(&mut self, action: ShareAction, cx: &mut gpui::Context<Self>) {
        match action {
            ShareAction::Install => {
                self.call(
                    "/api/extensionComponents",
                    json!({"op": "install", "name": "collab"}),
                    Some((Waiting::Install, "Install Collab")),
                    |_, _, _| {},
                    cx,
                );
            }
            ShareAction::DontShowShare => {
                self.collab(
                    json!({"op": "hideShare", "hidden": true}),
                    None,
                    |panel, _, cx| {
                        panel.in_main_window(
                            |app, window, cx| {
                                app.set_gpui_titlebar_popup_open(
                                    GpuiTitlebarPopupKind::CollabShare,
                                    false,
                                    None,
                                    window,
                                    cx,
                                )
                            },
                            cx,
                        )
                    },
                    cx,
                );
            }
            ShareAction::CancelWait => {
                // The request itself lapses (or the owner denies it on the phone); the popover
                // stops waiting for it now.
                if let Some((_, Some(id))) = self.waiting.take() {
                    cx.spawn(async move |_, _| {
                        let _ = gx_rpc(None, "/api/denyApprovalRequest", json!({ "id": id })).await;
                    })
                    .detach();
                }
                self.generation += 1;
                self.busy = false;
                self.waiting = None;
                self.waiting_kind = None;
                cx.notify();
            }
            ShareAction::SetMode(index) => {
                let index = index.min(MODES.len() - 1);
                match self.share_id() {
                    // A live share: narrowing never asks, widening does (gxserver says which).
                    Some(share_id) => self.collab(
                        json!({"op": "setMode", "shareId": share_id, "mode": MODES[index].0}),
                        Some((Waiting::Other, "Change what guests may do")),
                        |_, _, _| {},
                        cx,
                    ),
                    None => {
                        self.mode = index;
                        // Ask me and View only are also the default; Send never is.
                        if MODES[index].0 != "send" {
                            self.collab(
                                json!({"op": "set", "patch": {"mode": MODES[index].0}}),
                                None,
                                |_, _, _| {},
                                cx,
                            );
                        }
                        cx.notify();
                    }
                }
            }
            ShareAction::ToggleLifetimeMenu => {
                self.lifetime_menu_open = !self.lifetime_menu_open;
                cx.notify();
            }
            ShareAction::PickLifetime(lifetime) => {
                self.lifetime_menu_open = false;
                // CDXC:Collab 2026-11-08 DECISION: User (Q1785) "A chip before Create link (changes the default too)".
                if let Some(config) = self
                    .state
                    .as_mut()
                    .and_then(|state| state.get_mut("config"))
                    .and_then(Value::as_object_mut)
                {
                    config.insert("expiry".to_string(), json!(lifetime.id()));
                }
                self.collab(
                    json!({"op": "set", "patch": {"expiry": lifetime.id()}}),
                    None,
                    |_, _, _| {},
                    cx,
                );
                cx.notify();
            }
            ShareAction::CreateLink => {
                self.just_stopped = false;
                self.lifetime_menu_open = false;
                let mut params = self.ids();
                params["op"] = json!("share");
                let send = MODES[self.mode].0 == "send";
                self.collab(
                    params,
                    Some((Waiting::Create, "Create the link")),
                    move |panel, value, cx| {
                        Self::keep_secrets(&value);
                        // Send was picked before the link existed: it is turned on now, with its
                        // own approval (never a default).
                        if send && let Some(share_id) = value["share"]["shareId"].as_str() {
                            let share_id = share_id.to_string();
                            panel.collab(
                                json!({"op": "setMode", "shareId": share_id, "mode": "send"}),
                                Some((Waiting::Other, "Let guests send straight to the agent")),
                                |_, _, _| {},
                                cx,
                            );
                        }
                    },
                    cx,
                );
            }
            ShareAction::ShowLink => {
                let mut params = self.ids();
                params["op"] = json!("share");
                self.collab(
                    params,
                    Some((Waiting::Other, "Show the link and code")),
                    |_, value, _| Self::keep_secrets(&value),
                    cx,
                );
            }
            ShareAction::CopyLink | ShareAction::CopyCode => {
                if let Some((link, code)) = self.share_id().and_then(|id| revealed(&id)) {
                    if action == ShareAction::CopyLink {
                        self.copy("link", link, cx);
                    } else {
                        self.copy("code", code, cx);
                    }
                }
            }
            ShareAction::RemoveGuest(viewer) => {
                if let Some(share_id) = self.share_id() {
                    self.collab(
                        json!({"op": "removeViewer", "shareId": share_id, "viewerId": viewer}),
                        None,
                        |_, _, _| {},
                        cx,
                    );
                }
            }
            ShareAction::Review => {
                // CDXC:Collab 2026-11-08 DECISION: User (Q1783) "Point at the card": the message's own card waits above the chat box; Review closes the popover and puts the keyboard there.
                self.in_main_window(
                    |app, window, cx| {
                        app.set_gpui_titlebar_popup_open(
                            GpuiTitlebarPopupKind::CollabShare,
                            false,
                            None,
                            window,
                            cx,
                        );
                        app.focus_collab_review(window, cx);
                    },
                    cx,
                );
            }
            ShareAction::AskStop => {
                self.confirm_stop = true;
                cx.notify();
            }
            ShareAction::KeepSharing => {
                self.confirm_stop = false;
                cx.notify();
            }
            ShareAction::Stop => {
                if let Some(share_id) = self.share_id() {
                    self.collab(
                        json!({"op": "stop", "shareId": share_id}),
                        None,
                        |panel, _, _| {
                            panel.confirm_stop = false;
                            panel.just_stopped = true;
                        },
                        cx,
                    );
                }
            }
        }
    }

    /// The model the view draws.
    pub(crate) fn model(&self) -> ShareModel {
        let state = self.state.clone().unwrap_or(Value::Null);
        let share_id = self.share_id();
        let mut stage = view::stage_from_read(
            &state,
            &self.key.project_id,
            &self.key.session_id,
            share_id.as_deref().and_then(revealed),
            self.confirm_stop,
            self.just_stopped,
        );
        match (self.waiting_kind, &stage) {
            (Some(Waiting::Install), Stage::NotInstalled { .. }) => stage = Stage::InstallWaiting,
            (Some(Waiting::Create), Stage::Ready | Stage::Ended) => stage = Stage::Creating,
            _ => {}
        }
        let light = chrome_uses_light_appearance();
        let glass = window_glass_active();
        let mut model = ShareModel::new(stage);
        model.mode = self.mode;
        model.lifetime = Lifetime::parse(state["config"]["expiry"].as_str().unwrap_or("hour"));
        model.lifetime_menu_open = self.lifetime_menu_open;
        model.waiting = (self.waiting_kind == Some(Waiting::Other))
            .then(|| self.waiting.as_ref().map(|(what, _)| what.clone()))
            .flatten();
        model.copied = self.copied;
        model.error = self.error.clone();
        model.notice = state["guestPage"]["message"].as_str().map(str::to_string);
        model.now_ms = chrono::Utc::now().timestamp_millis();
        model.light = light;
        model.glass = glass;
        // The header dropdowns' own surface, the Commit dropdown's (CDXC:Collab 2026-11-11).
        model.surface = Some(popup_menu_frame());
        model
    }
}

impl Render for CollabSharePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let model = self.model();
        let on: view::ShareHandler<Self> =
            Rc::new(|panel: &mut Self, action, _window, cx| panel.act(action, cx));
        let popover = view::render_share_popover(&model, &on, cx);
        div().size_full().child(
            div()
                .w(px(SHARE_POPOVER_WIDTH))
                .child(popover)
                // The window takes the popover's own height, whatever state it shows.
                .on_prepaint(|bounds, window, _| {
                    let height = bounds.size.height.as_f32().clamp(80.0, MAX_HEIGHT);
                    let current = window.bounds().size;
                    if (current.height.as_f32() - height).abs() > 0.5 {
                        let width = current.width;
                        window.on_next_frame(move |window, _| {
                            window.resize(size(width, px(height)));
                        });
                    }
                }),
        )
    }
}
