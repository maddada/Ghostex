//! The chat header's Share button (Ghostex Collab): "Share" on a chat of this computer, "Shared"
//! with how many are watching once its link is live; it opens the Share popover
//! (window/collab_share.rs).
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1615): "Where Share lives: a button in the chat header." It shows on every chat of this computer, Ghostex Collab on or off (the popover offers to turn it on), and is the one place that says a chat is shared, so the header gets no second "Shared" pill.

use std::cell::{Cell, RefCell};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::ElementExt as _;

use crate::app::helpers::*;
use crate::*;
use ghostex_gx_core::SessionKey;

thread_local! {
    /// Where the button was last painted, for its popover to hang from.
    static ANCHOR: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
    /// The chat the open popover is about.
    static TARGET: RefCell<Option<SessionKey>> = const { RefCell::new(None) };
}

/// The chat the Share popover was opened for.
pub(crate) fn share_target() -> Option<SessionKey> {
    TARGET.with(|target| target.borrow().clone())
}

impl GhostexGpuiApp {
    /// The focused chat, when it can be shared from here (it runs on this computer).
    fn share_button_chat(
        &self,
    ) -> Option<(
        SessionKey,
        gpui::Entity<crate::app::native_chat::state::NativeChatView>,
    )> {
        let session = self.session_details_target_session()?;
        let key = self.workspace_terminal_session_key(session)?;
        if key.machine != ghostex_gx_core::MachineId::Local {
            return None;
        }
        Some((key, self.native_chat_views.get(&session)?.clone()))
    }

    pub(crate) fn render_workarea_header_share_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let (_, chat) = self.share_button_chat()?;
        let collab = chat.read(cx).snapshot["collab"].clone();
        let shared = collab.is_object();
        let watching = collab["viewerCount"].as_u64().unwrap_or(0);
        let label = match (shared, watching) {
            (false, _) => "Share".to_string(),
            (true, 0) => "Shared".to_string(),
            (true, count) => format!("Shared \u{b7} {count}"),
        };
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::CollabShare);
        let text = titlebar_active_text_color();
        let tooltip = if shared {
            collab["watching"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| "Shared through Ghostex Collab".to_string())
        } else {
            "Share this chat through a link".to_string()
        };
        Some(
            div()
                .id("ghostex-gpui-workarea-header-share")
                .role(gpui::Role::Button)
                .aria_label(label.clone())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .h(px(TITLEBAR_CONTROL_HEIGHT))
                .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
                .rounded(px(TITLEBAR_BUTTON_RADIUS))
                .cursor_default()
                .text_size(px(12.0))
                .text_color(text)
                .when(cfg!(target_os = "windows"), |this| this.occlude())
                .when(open, |this| this.bg(titlebar_button_hover_color()))
                .hover(|this| this.bg(titlebar_button_hover_color()))
                .child(titlebar_svg_icon("titlebar/users.svg", 14.0, text))
                .when(shared, |this| {
                    this.child(div().size(px(6.0)).rounded_full().bg(gpui::rgb(0x4ade80)))
                })
                .child(label)
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.toggle_collab_share_popover(window, cx);
                    }),
                )
                .on_prepaint(|bounds, _, _| ANCHOR.with(|anchor| anchor.set(Some(bounds))))
                .into_any_element(),
        )
    }

    /// Opens (or closes) the Share popover under the header button.
    pub(crate) fn toggle_collab_share_popover(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::CollabShare);
        let Some((key, _)) = self.share_button_chat() else {
            return;
        };
        let same = share_target().as_ref() == Some(&key);
        if open {
            self.set_gpui_titlebar_popup_open(
                GpuiTitlebarPopupKind::CollabShare,
                false,
                None,
                window,
                cx,
            );
            if same {
                return;
            }
        }
        TARGET.with(|target| *target.borrow_mut() = Some(key));
        let anchor = ANCHOR.with(|anchor| anchor.get());
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::CollabShare,
            true,
            anchor,
            window,
            cx,
        );
    }
}
