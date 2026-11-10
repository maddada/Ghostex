//! The Claude and Grok Bot buttons in the work area header's right group, directly left of the
//! Voice button ("Talk to the voice agent"): each turns its page on and off (claude_web/takeover.rs)
//! and shows only while its built-in extension is on and its view can open here.
//!
//! CDXC:Extensions 2026-11-10 DECISION:
//! User: "please move the grok and claude buttons to be on the left of the Talk to the voice agent button in the header". This supersedes the 2026-11-04 choice of the sidebar's bottom bar, right of Commands ("move the claude button from the top of the sidebar to the bottom of it please"), and the same day's "keep it at the top in this case please" for the header's copy of the sidebar's top row while Claude is on: neither the bottom bar nor that row draws them now, so each button exists once. They sit after the pinned extension buttons, Prompt Editor and the jobs button, in Claude then Grok Bot order, and behave as the pinned extension buttons do when the header is narrow (they keep their size). Each shows as on while its page is, and clicking one while the other is on switches pages in place (claude_web/place.rs). They stay in the header while Claude or Grok Bot is on (`HeaderControl::WebBots`), so the page closes from where it was opened.
//! SEE-ALSO: claude_web/header.rs (what the header keeps while Claude is on), native_sidebar/sessions.rs (the session click).
//!
//! CDXC:Extensions 2026-10-25 DECISION:
//! User (2026-10-25): clicking a session in the sidebar while Claude is on turns Claude off and shows the session. Grok Bot works the same way (claude_web/place.rs).

use std::time::Duration;

use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use super::place::WebPlace;
use crate::app::helpers::*;
use crate::*;

/// One of the header's bot buttons.
struct WebBotButton {
    /// The view it opens (`TitlebarMode::Extension`).
    id: String,
    title: String,
    icon: &'static str,
    /// Its view is the one on screen.
    showing: bool,
}

impl GhostexGpuiApp {
    /// The buttons to show now: Claude's and Grok Bot's, each while it is on and its view can open
    /// here.
    fn web_bot_buttons(&self) -> Vec<WebBotButton> {
        WebPlace::ALL
            .into_iter()
            .filter(|place| place.enabled() && self.titlebar_mode_available(place.mode()))
            .map(|place| WebBotButton {
                id: place.view_id_str().to_string(),
                title: place.title().to_string(),
                icon: place.icon(),
                showing: self.web_place_on() == Some(place),
            })
            .collect()
    }

    /// The header's Claude and Grok Bot buttons, in the header's own look (the pinned extension
    /// buttons': a 34pt-tall slot, hover fill, a bright icon and the fill while on).
    pub(crate) fn render_workarea_header_web_bot_buttons(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        self.web_bot_buttons()
            .into_iter()
            .map(|button| {
                let WebBotButton {
                    id,
                    title,
                    icon,
                    showing,
                } = button;
                let click_id = id.clone();
                let tooltip = title.clone();
                div()
                    .id(gpui::SharedString::from(format!(
                        "ghostex-gpui-workarea-header-web-bot-{id}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(title)
                    .aria_selected(showing)
                    .flex_shrink_0()
                    .flex()
                    .h(px(TITLEBAR_CONTROL_HEIGHT))
                    .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
                    .items_center()
                    .justify_center()
                    .when(cfg!(target_os = "windows"), |this| this.occlude())
                    .cursor_default()
                    .when(showing, |this| this.bg(titlebar_active_segment_color()))
                    .hover(move |this| {
                        if showing {
                            this.bg(titlebar_active_segment_color())
                        } else {
                            this.bg(titlebar_button_hover_color())
                        }
                    })
                    .child(titlebar_svg_icon(
                        icon,
                        16.0,
                        if showing {
                            titlebar_active_text_color()
                        } else {
                            titlebar_icon_color()
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            app.toggle_sidebar_extension_view(&click_id, window, cx);
                        }),
                    )
                    .managed_discrete_tooltip_with_placement(
                        ManagedTooltipPlacement::Left,
                        Duration::from_millis(300),
                        move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx),
                    )
                    .into_any_element()
            })
            .collect()
    }

    /// A button: turns its page on, or off when it is on.
    pub(crate) fn toggle_sidebar_extension_view(
        &mut self,
        extension_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(place) = WebPlace::from_view_id(extension_id) {
            self.toggle_web_place(place, window, cx);
        }
    }

    /// A session click: turns Claude or Grok Bot off so the session shows. Returns whether one was on.
    pub(crate) fn close_sidebar_extension_view(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.turn_claude_off(cx) {
            return false;
        }
        self.sync_browser_address_input_if_shown(window, cx);
        true
    }
}
