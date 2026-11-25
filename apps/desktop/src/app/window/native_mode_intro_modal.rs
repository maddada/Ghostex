//! First-launch desktop modal announcing Native mode (Option F "Bold minimal", `Q4773`/`Q4774`).
//!
//! Centered, calm layout at 440px: three agent logo tiles (Antigravity, Claude Code, Codex),
//! headline + concise copy (stability, smoother streaming, better performance, "More agents coming
//! soon"), three non-repetitive benefit pills, stacked full-width primary ("Turn on Native mode")
//! and secondary ("Not now") buttons, and a footer note ("Terminal stays available; switch back
//! any time."). Closes via Not now, Esc, or the window's corner close button.
//!
//! CDXC:AgentProviders 2026-11-24 DECISION:
//! User picked Option F ("Bold minimal") from `native-announce/modal/index.html` with copy notes:
//! drop "Real approvals", avoid repeating copy between subtitle and chips, focus on stability,
//! smoother streaming and better performance, add "More agents coming soon", keep "Terminal stays
//! available; switch back any time", and use Terminal / Native and Antigravity / Claude Code /
//! Codex names. Primary button enables Native for new chats of all 3 agents and safely switches
//! idle existing chats and idle coordinators (skipping coordinator worker threads and anything
//! mid-turn). Shows once after updating on desktop only.
//! SEE-ALSO: apps/desktop/src/app/native_mode_intro.rs (trigger + host),
//! apps/desktop/src/shared_settings/native_mode_intro.rs (one-time migration marker),
//! apps/desktop/src/bin/native_modal_demo/native_mode_intro.rs (preview binary).

use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, Context, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Window,
    div, px, rgb, svg,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

pub(crate) const NATIVE_MODE_INTRO_MODAL_WIDTH: f32 = 500.0;
pub(crate) const NATIVE_MODE_INTRO_MODAL_INITIAL_HEIGHT: f32 = 340.0;

const TITLE: &str = "Chat, straight from your agents.";
const SUBTITLE: &str = "Native mode connects Claude Code, Codex, Antigravity, OpenCode, Pi, OMP, Hermes Agent, Cursor CLI and Grok Build directly instead of reading a terminal screen. More agents coming soon.";
const FOOTER_NOTE: &str = "Terminal-based stays available; switch back any time.";

const CHIPS: [(&str, &str); 3] = [
    ("titlebar/shield-check.svg", "Greater stability"),
    ("titlebar/bolt.svg", "Smoother streaming"),
    ("titlebar/sparkles.svg", "Better performance"),
];

/// What the Native mode announcement modal asks the host to do.
pub(crate) enum NativeModeIntroCommand {
    /// Turn Native on for Antigravity, Claude Code, and Codex and switch safe existing chats.
    TurnOn,
    /// Not now, Escape, or the corner close button: dismiss permanently without changing settings.
    Dismiss,
}

pub(crate) type NativeModeIntroHost = Rc<dyn Fn(NativeModeIntroCommand, &mut App)>;

pub(crate) struct NativeModeIntroModalConfig {
    pub(crate) palette: ModalPalette,
}

pub(crate) struct GpuiNativeModeIntroModalWindow {
    host: NativeModeIntroHost,
    palette: ModalPalette,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiNativeModeIntroModalWindow {
    pub(crate) fn new(
        config: NativeModeIntroModalConfig,
        host: NativeModeIntroHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: config.palette,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn turn_on(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(NativeModeIntroCommand::TurnOn, cx);
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(NativeModeIntroCommand::Dismiss, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Never bind bare Enter to Turn on: this modal auto-focuses on open and could catch a
        // prompt submit right as it appears.
        if event.keystroke.key.as_str() == "escape" {
            self.dismiss(window, cx);
            cx.stop_propagation();
        }
    }

    fn is_dark(&self) -> bool {
        let s = self.palette.surface;
        (s.r + s.g + s.b) < 1.5
    }

    fn render_logo_trio(&self) -> AnyElement {
        let p = self.palette;
        let dark = self.is_dark();
        let agy_color = if dark { rgb(0x749bff) } else { rgb(0x4b74e8) };
        let claude_color = if dark { rgb(0xd97757) } else { rgb(0xd06847) };
        let codex_color = p.foreground;
        let logos: [(&str, gpui::Rgba); 3] = [
            ("agent-icons/antigravity-cli.svg", agy_color),
            ("agent-icons/claude.svg", claude_color),
            ("agent-icons/codex.svg", codex_color),
        ];
        h_flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .children(logos.into_iter().map(|(path, color)| {
                div()
                    .size(px(40.0))
                    .rounded(px(11.0))
                    .bg(hsla(p.raised))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(svg().path(path).size(px(20.0)).text_color(hsla(color)))
            }))
            .into_any_element()
    }

    fn render_chips(&self) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .children(CHIPS.iter().map(|(icon, label)| {
                h_flex()
                    .items_center()
                    .gap(px(5.0))
                    .px(px(9.0))
                    .py(px(4.0))
                    .rounded_full()
                    .bg(hsla(p.panel))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(hsla(p.foreground))
                    .whitespace_nowrap()
                    .child(
                        svg()
                            .path(*icon)
                            .size(px(12.0))
                            .text_color(hsla(p.foreground)),
                    )
                    .child(*label)
            }))
            .into_any_element()
    }

    fn render_stacked_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let primary_fills = p.primary_button_fills();
        let secondary_fills = neutral_button_fills(p.light);
        let primary = div()
            .id("native-mode-intro-turn-on")
            .role(gpui::Role::Button)
            .aria_label("Turn on Native mode")
            .w_full()
            .h(px(36.0))
            .px(px(14.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .bg(hsla(primary_fills.rest))
            .text_color(hsla(p.primary_button_text()))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .font_weight(FontWeight::MEDIUM)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(hsla(primary_fills.hover)))
            .active(move |style| style.bg(hsla(primary_fills.pressed)))
            .on_click(cx.listener(|this, _, window, cx| {
                this.turn_on(window, cx);
            }))
            .child("Turn on Native mode");

        let secondary = div()
            .id("native-mode-intro-not-now")
            .role(gpui::Role::Button)
            .aria_label("Not now")
            .w_full()
            .h(px(34.0))
            .px(px(14.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .bg(hsla(secondary_fills.rest))
            .text_color(hsla(p.foreground))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .font_weight(FontWeight::MEDIUM)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(hsla(secondary_fills.hover)))
            .active(move |style| style.bg(hsla(secondary_fills.pressed)))
            .on_click(cx.listener(|this, _, window, cx| {
                this.dismiss(window, cx);
            }))
            .child("Not now");

        v_flex()
            .w_full()
            .gap(px(8.0))
            .mt(px(2.0))
            .child(primary)
            .child(secondary)
            .into_any_element()
    }
}

impl Render for GpuiNativeModeIntroModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let top_pad = 28.0;
        let side_pad = 26.0;
        let bottom_pad = 22.0;

        let inner = v_flex()
            .w_full()
            .items_center()
            .gap(px(16.0))
            .child(self.render_logo_trio())
            .child(
                v_flex()
                    .w_full()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(21.5))
                            .line_height(px(26.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(p.foreground))
                            .child(TITLE),
                    )
                    .child(
                        div()
                            .w_full()
                            .max_w(px(420.0))
                            .text_center()
                            .text_size(px(13.5))
                            .line_height(px(20.0))
                            .text_color(hsla(p.muted))
                            .child(SUBTITLE),
                    ),
            )
            .child(self.render_chips())
            .child(self.render_stacked_buttons(cx))
            .child(
                div()
                    .w_full()
                    .pt(px(10.0))
                    .border_t_1()
                    .border_color(hsla(p.hairline))
                    .text_center()
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(hsla(p.muted))
                    .child(FOOTER_NOTE),
            );

        div()
            .id("ghostex-gpui-native-mode-intro")
            .size_full()
            .overflow_hidden()
            .bg(hsla(p.surface))
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                v_flex()
                    .size_full()
                    .pt(px(top_pad))
                    .px(px(side_pad))
                    .pb(px(bottom_pad))
                    .on_children_prepainted(self.fit.listener(top_pad + bottom_pad))
                    .child(inner),
            )
    }
}

impl ModalCornerClose for GpuiNativeModeIntroModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss(window, cx);
    }
}
