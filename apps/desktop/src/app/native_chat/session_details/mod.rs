//! The Session details panel beside the chat: docked as its own column when the pane is wide
//! enough, else a floating card under the header's context ring that closes when the chat is
//! clicked. The core (`packages/gx-chat-core/src/extras/session_details/`) words every row; this
//! decides where the panel goes and draws it (`cards.rs`).
//!
//! The app (the header's ring button) says whether this chat's panel is wanted
//! (`set_session_details_wanted`); this view tells the core whether it is on screen
//! (`sessionDetailsVisible`), which is what makes the core read and poll.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User: "i want this sidebar to be inside the chat view", "when it's not enough space then it floats", and the button "not floating when there's space for it, but it should be floating when there isn't enough space width wise". Picked from the mockups: a docked column with the transcript re-centered in what is left (Q1), and when narrow "a floating card that closes when you click the chat" (Q5). The floating card is a chat menu window (option_menu/pill_menu.rs) like the working row's pill menus, never an overlay inside the pane (AGENTS.md, native layout discipline). The pane docks from the chat's own narrow breakpoint (notice.rs), 1,070px at 100% zoom.
//! SEE-ALSO: apps/desktop/src/app/render/workarea_header/session_details_button.rs (the button), apps/gpui-web/src/shell.rs (the web header's button).

mod cards;
mod choices;
mod ring;

pub(crate) use choices::{DISMISS_SETTLE, SessionDetailsChoices};
pub(crate) use ring::{session_details_ring, session_details_ring_label};

use super::{appearance::ChatAppearance, state::NativeChatView};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;

/// The panel's width, docked or floating, in unscaled pixels.
pub(crate) const PANEL_WIDTH: f32 = 292.0;
/// The pane width (unscaled) from which the panel docks rather than floats: the chat's narrow
/// breakpoint (notice.rs).
const DOCK_MIN_WIDTH: f32 = 1_070.0;
/// The menu-toggle key of the floating panel (menu_toggle.rs, chat_pills.rs).
pub(super) const SESSION_DETAILS_TRIGGER: &str = "chat-session-details";
/// The pill-menu id the floating panel is drawn under (chat_pills.rs).
pub(super) const SESSION_DETAILS_MENU: &str = "sessionDetails";

/// What the view keeps about its panel between frames.
#[derive(Default)]
pub(crate) struct SessionDetailsView {
    /// The app wants this chat's panel on screen (the header button is on and this is the chat it
    /// belongs to).
    wanted: bool,
    /// Where the header's button was last painted, in the window's content, which the floating
    /// card opens under.
    anchor: Option<Rc<Cell<Bounds<Pixels>>>>,
    /// The whole pane, panel included, as last laid out.
    pane_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The panel is docked this frame.
    pub(super) docked: bool,
    /// The docked column's slide in and out, on the window panels' own tween.
    pub(super) motion: crate::app::panel_motion::PanelMotion,
    /// The pane was wide enough to dock last frame; a change of it snaps instead of sliding.
    fits: bool,
    /// The floating card is about to open; nothing opens it again until it has.
    opening: bool,
    /// The card is being closed by this view (it docked, or the button turned it off), not by a
    /// click on the chat.
    closing: bool,
    /// What the core was last told.
    reported_visible: bool,
    /// The docked column's scroll.
    pub(super) scroll: ScrollHandle,
    /// The cards were drawn since the panel was last off screen: their fold motions are watching
    /// the states they were drawn in (`DisclosureMotions::forget`).
    pub(super) drew: bool,
    /// The tab on screen: 0 the first (the session and its Context card), 1 the second (the
    /// context meter's details). Every opening starts on the first.
    pub(super) tab: usize,
    /// The next frame scrolls the panel to the Context card (a ring opened it).
    pub(super) to_context: bool,
    /// The floating card's scroll.
    pub(super) menu_scroll: ScrollHandle,
    /// How wide the docked column was last drawn, sliding included; 0 while it is not docked.
    pub(super) dock_width: f32,
}

impl NativeChatView {
    /// The app's say: whether this chat's panel is wanted, and where the header button is.
    pub(crate) fn set_session_details_wanted(
        &mut self,
        wanted: bool,
        anchor: Option<Rc<Cell<Bounds<Pixels>>>>,
        cx: &mut Context<Self>,
    ) {
        if self.session_details.wanted == wanted {
            return;
        }
        self.session_details.wanted = wanted;
        // Both rings open it on the first tab, at the Context card (the decision at `TABS` in
        // gx-chat-core `session_details/document.rs`).
        if wanted {
            self.session_details.tab = 0;
            self.session_details.to_context = true;
        }
        if anchor.is_some() {
            self.session_details.anchor = anchor;
        }
        cx.notify();
    }

    /// The panel applies to this chat (the extension is on and the chat knows its agent).
    pub(crate) fn session_details_available(&self) -> bool {
        self.snapshot["sessionDetails"]["enabled"] == true
    }

    /// The header ring's percentage and label, from this chat's context use.
    pub(crate) fn session_details_ring(&self) -> (Option<f64>, String) {
        let ring = &self.snapshot["sessionDetails"]["ring"];
        (
            ring["percent"].as_f64(),
            ring["label"].as_str().unwrap_or_default().to_string(),
        )
    }

    /// The panel can go beside this chat: it applies to it, and the chat is drawn in its pane with
    /// its composer (not a maximized composer or a hidden pane).
    fn session_details_placeable(&self) -> bool {
        self.session_details_available() && self.maximized_window.is_none() && !self.pane_hidden
    }

    /// A click on the Tasks pill: where the panel can go, the app shows this chat's panel, whose
    /// Tasks card lists the tasks, and the pill steps aside while it shows (CDXC:SessionDetails
    /// 2026-10-25 in gx-chat-core's extras/chat_pills.rs). False leaves the click to the pill's own
    /// menu.
    pub(super) fn show_session_details_for_tasks(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.session_details_placeable() {
            return false;
        }
        self.host("showSessionDetails", json!({}), cx);
        true
    }

    /// The chat box's context ring, where the panel can go: the panel opens (or, open, comes back)
    /// on its first tab at the Context card, in place of the Context window popover it replaces.
    /// False leaves the click to the popover, for a chat with no panel.
    pub(super) fn show_session_details_for_context(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.session_details_placeable() {
            return false;
        }
        self.session_details.tab = 0;
        self.session_details.to_context = true;
        if !self.session_details.wanted {
            self.host("showSessionDetails", json!({}), cx);
        }
        cx.notify();
        true
    }

    /// Scrolls the panel to the Context card, once, when a ring asked for it.
    fn scroll_session_details_to_context(&mut self, scale: f32) {
        if !std::mem::take(&mut self.session_details.to_context) {
            return;
        }
        let mut motions = self.disclosure_motion.borrow_mut();
        let offset = cards::context_offset(&self.snapshot["sessionDetails"], &mut |key, open| {
            motions.open_factor(key, open)
        });
        let point = gpui::point(px(0.0), px(-offset * scale));
        self.session_details.scroll.set_offset(point);
        self.session_details.menu_scroll.set_offset(point);
    }

    fn session_details_menu_open(&self, cx: &Context<Self>) -> bool {
        self.option_menu
            .as_ref()
            .is_some_and(|menu| menu.read(cx).pill() == Some(SESSION_DETAILS_TRIGGER))
    }

    /// Decides this frame's placement: docked, floating, or none, and keeps the core told.
    pub(super) fn sync_session_details(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wanted = self.session_details.wanted && self.session_details_placeable();
        let pane = self.session_details.pane_bounds.get().size.width.as_f32();
        let fits = pane >= DOCK_MIN_WIDTH * p.scale;
        let docked = wanted && fits;
        if self.session_details.docked != docked {
            self.session_details.docked = docked;
            // The transcript re-wraps at its new width.
            self.list.remeasure();
        }
        // CDXC:SessionDetails 2026-10-25 DECISION:
        // User: "please when showing the session details and hiding them let them slide in and out like we do for the sidepanel itself (this is only when the session details panel isn't floating)", and of the side panel: "doesn't resize chat until it settles". The docked column slides on the window panels' tween (panel_motion/tween.rs): its cards keep their width inside a clipping frame, fade in over the end of an opening and out over the start of a closing, and the transcript holds its text width until the slide settles (`hold_text_width_while_sliding`). A dock that comes or goes because the pane got wider or narrower, or a chat coming on screen, snaps; the floating card is unchanged.
        if self.pane_hidden || fits != self.session_details.fits {
            self.session_details.fits = fits;
            self.session_details.motion.reset();
        }
        let frame = self.session_details.motion.sample(
            docked,
            PANEL_WIDTH * p.scale,
            0.0,
            cx.reduce_motion(),
            web_time::Instant::now(),
        );
        if frame.animating {
            window.request_animation_frame();
        }
        let floating = wanted && !docked && pane > 0.0;
        let menu_open = self.session_details_menu_open(cx);
        if floating && !menu_open && !self.session_details.opening {
            self.session_details.opening = true;
            cx.defer_in(window, |this, window, cx| {
                this.session_details.opening = false;
                this.open_session_details_menu(window, cx);
            });
        } else if !floating && menu_open {
            self.session_details.closing = true;
            self.close_pill_menu(cx);
        }
        let visible = docked || (floating && (menu_open || self.session_details.opening));
        // CDXC:SessionDetails 2026-10-29 DECISION:
        // User: "please make expand/collapse here animate nicely like we did for other expand/collapse". Each card and the inner folds (Messages, all skills, finished tasks) ease like the chat's disclosures (`disclosure_motion.rs`), docked or floating. The panel opens still in the state it opens in, so the states the folds were last drawn in are forgotten once the panel is off screen and its slide has finished.
        if !visible && !frame.animating && std::mem::take(&mut self.session_details.drew) {
            self.disclosure_motion
                .borrow_mut()
                .forget("session-details:");
        }
        if visible != self.session_details.reported_visible {
            self.session_details.reported_visible = visible;
            self.invoke(
                json!({"type": "sessionDetailsVisible", "visible": visible}),
                cx,
            );
        }
    }

    /// The floating card, under the header's button and as wide as the docked column.
    fn open_session_details_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session_details.wanted || self.session_details.docked {
            return;
        }
        let Some(height) = self.session_details_menu_height() else {
            return;
        };
        let pane = self.bounds.get();
        let anchor = self
            .session_details
            .anchor
            .as_ref()
            .map(|anchor| anchor.get())
            .filter(|anchor| anchor.size.width > px(0.0))
            // No header (a chat drawn alone): the pane's own top-right corner.
            .unwrap_or_else(|| {
                Bounds::new(
                    gpui::point(pane.origin.x + pane.size.width - px(8.0), pane.origin.y),
                    gpui::size(px(0.0), px(0.0)),
                )
            });
        self.menu_side = Some(super::option_menu::MenuSide::Right);
        self.show_chat_menu(
            vec![json!({"pillMenu": {"id": SESSION_DETAILS_MENU, "height": height}})],
            anchor,
            PANEL_WIDTH,
            window,
            cx,
        );
        if let Some(menu) = &self.option_menu {
            menu.update(cx, |menu, _| menu.set_pill(SESSION_DETAILS_TRIGGER));
        }
    }

    /// The floating card went away. Closed by a click on the chat (or Escape), the panel is off
    /// until the button is pressed again (the app waits to see the chat is still shown,
    /// `SessionDetailsChoices::settle`); closed by this view, or because the chat left the
    /// screen, nothing changes.
    pub(super) fn session_details_menu_closed(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.session_details.closing) || self.pane_hidden {
            return;
        }
        if self.session_details.wanted && !self.session_details.docked {
            self.session_details.wanted = false;
            self.host("sessionDetailsDismissed", json!({}), cx);
            cx.notify();
        }
    }

    /// The floating card's content height, capped at most of the pane; its body scrolls.
    pub(super) fn session_details_menu_height(&self) -> Option<f32> {
        if !self.session_details_available() {
            return None;
        }
        let scale = ChatAppearance::current(&self.snapshot).scale;
        let pane = f32::from(self.bounds.get().size.height) / scale;
        let mut motions = self.disclosure_motion.borrow_mut();
        let details = &self.snapshot["sessionDetails"];
        // Sized for the taller tab, so switching tabs never cuts the card; the other one scrolls.
        let content = cards::panel_height(details, &mut |key, open| motions.open_factor(key, open))
            .max(cards::usage_height(details));
        Some(content.min((pane - 56.0).max(200.0)))
    }

    /// The floating card's body, drawn with this view's listeners.
    pub(super) fn session_details_menu_body(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.session_details_available() {
            return None;
        }
        // The cards sit on the popup's menu tone, not on the chat: they take the light washes they
        // take over window glass, since the chat's own solid fill draws them as black slabs.
        let p = p.clone().on_window_glass(true);
        let body = self.session_details_cards(&p, window, cx);
        self.scroll_session_details_to_context(p.scale);
        Some(
            div()
                .id("chat-session-details-menu")
                .role(gpui::Role::Group)
                .aria_label("Session details")
                .size_full()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.session_details.menu_scroll)
                .child(body)
                .into_any_element(),
        )
    }

    /// The docked column, or `None` while the panel is not docked.
    pub(super) fn render_session_details_dock(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // A closing slide keeps drawing the column until its frame has shut.
        let frame = self.session_details.motion.frame();
        if !self.session_details.docked && frame.extent <= 0.0 {
            self.session_details.dock_width = 0.0;
            return None;
        }
        let s = p.scale;
        self.session_details.dock_width = if frame.animating {
            frame.extent.max(0.0)
        } else {
            PANEL_WIDTH * s
        };
        let body = self.session_details_cards(p, window, cx);
        self.scroll_session_details_to_context(s);
        let column = div()
            .id("chat-session-details-dock")
            .role(gpui::Role::Group)
            .aria_label("Session details")
            .flex_shrink_0()
            .w(px(PANEL_WIDTH * s))
            .h_full()
            .min_h_0()
            .border_l_1()
            .border_color(p.border)
            .overflow_y_scroll()
            .track_scroll(&self.session_details.scroll)
            .when(self.under_workarea_header, |column| {
                // The header floats over the pane's top: the column starts below it.
                column.pt(px(crate::app::consts::WORKAREA_HEADER_HEIGHT))
            })
            .child(
                div()
                    .p(px(10.0 * s))
                    .opacity(frame.opening_content_opacity() * frame.closing_content_opacity())
                    .child(body),
            )
            .into_any_element();
        Some(if frame.animating {
            crate::app::panel_motion::clip_panel_horizontally(frame, true, column)
                .into_any_element()
        } else {
            column
        })
    }

    /// The pane measure the dock decision reads: the chat and the panel together.
    pub(super) fn session_details_pane_bounds(&self) -> Rc<Cell<Bounds<Pixels>>> {
        self.session_details.pane_bounds.clone()
    }

    /// Sends one of the panel's actions to the core.
    pub(super) fn session_details_action(&mut self, action: &Value, cx: &mut Context<Self>) {
        if action.is_object() {
            self.invoke(action.clone(), cx);
        }
    }
}
