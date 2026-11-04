//! Native GPUI "Add a link" dialog: the links list's + button (the header's "+N" dropdown, the
//! sidebar's card, the web page's card). One field takes `SPX-123`, `#123`, `owner/repo#123` or a
//! GitHub or Linear URL, and Enter links it to the session through `/api/addSessionLink`, the path
//! `ghostex link add` takes.
//!
//! CDXC:SessionLinks 2026-10-27 DECISION:
//! User: "also i need a button to add 1 manually (just write SPX-123 or #123 or paste a link to link it)". The list's popups never take the keyboard (they open without activating, so the main window keeps it), so the field lives in a small app modal, which does. It stays open while gxserver answers, so something it cannot read or GitHub does not know shows under the field instead of as a toast; Escape closes it. gxserver asks GitHub whether a bare number is a PR or an issue (`github_number_kind` in server/src/server/route_http/session_links.rs).
//! SEE-ALSO: apps/desktop/src/app/add_session_link_modal_lifecycle.rs (open), apps/desktop/src/app/native_sidebar/links_list.rs (the + button), apps/mobile/app/src/chat/native/SessionLinksSheet.tsx (the phone's own field).
use super::native_modal_kit::*;
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, Styled as _, Subscription, Window, px,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState};
use gpui_component::v_flex;
use serde_json::json;
use std::rc::Rc;

pub(crate) const ADD_SESSION_LINK_MODAL_WIDTH: f32 = 440.0;
pub(crate) const ADD_SESSION_LINK_MODAL_INITIAL_HEIGHT: f32 = 220.0;

const TITLE: &str = "Add a link";
const PLACEHOLDER: &str = "SPX-123, #123, owner/repo#123 or a link";

/// What the dialog tells its host once it is gone.
pub(crate) enum AddSessionLinkModalCommand {
    /// The link was added.
    Added,
    Cancel,
}

pub(crate) type AddSessionLinkModalHost = Rc<dyn Fn(AddSessionLinkModalCommand, &mut App)>;

pub(crate) struct AddSessionLinkModalConfig {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    /// The gxserver that owns the session: `None` for this computer's.
    pub(crate) remote: Option<GpuiRemoteGxserverRequestTarget>,
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiAddSessionLinkModalWindow {
    host: AddSessionLinkModalHost,
    palette: ModalPalette,
    project_id: String,
    session_id: String,
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    input: Entity<InputState>,
    text: String,
    busy: bool,
    error: Option<String>,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiAddSessionLinkModalWindow {
    pub(crate) fn new(
        config: AddSessionLinkModalConfig,
        host: AddSessionLinkModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        let subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.text = input.read(cx).value().to_string();
                    this.error = None;
                    cx.notify();
                }
            },
        )];
        input.update(cx, |input, cx| input.focus(window, cx));
        let mut this = Self {
            host,
            palette: config.palette,
            project_id: config.project_id,
            session_id: config.session_id,
            remote: config.remote,
            input,
            text: String::new(),
            busy: false,
            error: None,
            fit: ModalFit::new(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this._subscriptions
            .extend(super::popup_dismissal::close_app_modal_on_click_away(
                window, cx,
            ));
        this
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(AddSessionLinkModalCommand::Cancel, cx);
    }

    /// Sends the field to gxserver; the dialog closes once it is linked and shows gxserver's
    /// refusal otherwise.
    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let link = self.text.trim().to_string();
        if link.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let params = json!({
            "projectId": self.project_id,
            "sessionId": self.session_id,
            "link": link,
            "source": "user",
        });
        let remote = self.remote.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = gx_rpc(remote, "/api/addSessionLink", params).await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(_) => {
                    window.remove_window();
                    (this.host)(AddSessionLinkModalCommand::Added, cx);
                }
                Err(error) => {
                    this.busy = false;
                    this.error = Some(error.message);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn on_enter_action(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.add(window, cx);
    }

    fn on_escape_action(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.cancel(window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" if !event.is_held => self.add(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn render_body(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut body = v_flex().w_full().gap(px(8.0)).child(modal_text_input(
            &p,
            &self.input,
            self.busy,
            window,
            cx,
        ));
        if self.busy {
            body = body.child(modal_hint(&p, "Linking…"));
        }
        if let Some(error) = self.error.clone() {
            body = body.child(modal_error(&p, error));
        }
        body.into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let cancel = modal_action_button(
            &p,
            "add-session-link-cancel",
            "Cancel",
            None,
            ModalButtonTone::Neutral,
            false,
            |this, window, cx| this.cancel(window, cx),
            cx,
        );
        let add = modal_action_button(
            &p,
            "add-session-link-add",
            "Link",
            None,
            ModalButtonTone::Primary,
            self.busy || self.text.trim().is_empty(),
            |this, window, cx| this.add(window, cx),
            cx,
        );
        modal_footer(vec![cancel, add])
    }
}

impl Render for GpuiAddSessionLinkModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = vec![
            modal_header(
                &p,
                TITLE,
                Some("Link a PR, a GitHub issue or a Linear ticket to this session."),
            ),
            self.render_body(window, cx),
        ];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-add-session-link-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
        .capture_action(cx.listener(Self::on_enter_action))
        .capture_action(cx.listener(Self::on_escape_action))
    }
}

impl ModalCornerClose for GpuiAddSessionLinkModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(window, cx);
    }
}
