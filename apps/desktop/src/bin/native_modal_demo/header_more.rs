//! Preview of the chat header's trailing controls and its ⋯ menu (Q4196 C), drawn with the app's
//! own look file (app/helpers/titlebar/header_pins_look.rs): the 16px icon rule, the icon-only
//! button and the ⋯ rows with their pin. The header strip around them is laid out as
//! render/workarea_header/action_buttons.rs lays it out (Start and Commit in one outline, Work,
//! the context ring, ⋯, the divider and the two panel toggles); the menu shows Share and Open in a
//! popup under This chat, and Claude (on), Grok Bot and Voice under Agents. Point at a row
//! (`GHOSTEX_NATIVE_MODAL_DEMO_HOVER`) to see its pin. Dark theme only.
use gpui::{
    App, AppContext as _, Context, Entity, Focusable as _, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window, canvas, div, point, px, rgb, svg,
};
use gpui_component::Side;
use gpui_component::h_flex;
use gpui_component::menu::{PopupMenu, PopupMenuAppearance, PopupMenuItem};

// Stand-ins for the `crate::app` items the look file reads, with the app's dark colours.
fn titlebar_overlay_base() -> gpui::Rgba {
    rgb(0xffffff)
}
fn titlebar_popup_menu_foreground() -> Hsla {
    rgb(0xfcfcfc).into()
}
fn titlebar_active_text_color() -> Hsla {
    rgb(0xfcfcfc).into()
}
fn titlebar_icon_color() -> Hsla {
    rgb(0xfcfcfc).opacity(0.84).into()
}
fn titlebar_inactive_text_color() -> Hsla {
    rgb(0xfcfcfc).opacity(0.68).into()
}
fn titlebar_button_hover_color() -> Hsla {
    titlebar_overlay_base().opacity(0.08).into()
}
fn titlebar_active_segment_color() -> Hsla {
    titlebar_overlay_base().opacity(0.11).into()
}
fn titlebar_split_button_border_color() -> Hsla {
    gpui::white().opacity(0.16)
}
fn titlebar_svg_icon(path: &'static str, icon_size: f32, color: Hsla) -> impl IntoElement {
    svg().size(px(icon_size)).path(path).text_color(color)
}
fn titlebar_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyView {
    gpui_component::tooltip::Tooltip::new(text).build(window, cx)
}
const TITLEBAR_CONTROL_HEIGHT: f32 = 27.0;
const TITLEBAR_BUTTON_RADIUS: f32 = 7.0;
const TITLEBAR_POPUP_MENU_ROW_HEIGHT: f32 = 34.0;
const TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE: f32 = 13.0;
const TITLEBAR_ICON_PLAYER_PLAY: &str = "titlebar/player-play.svg";
const TITLEBAR_ICON_FOLDER_OPEN: &str = "titlebar/folder-open.svg";
const TITLEBAR_ICON_GIT_COMMIT: &str = "titlebar/git-commit.svg";
const TITLEBAR_ICON_PANEL_BOTTOM: &str = "titlebar/panel-bottom.svg";
const TITLEBAR_ICON_PANEL_RIGHT: &str = "titlebar/panel-right.svg";
const TITLEBAR_ICON_CHEVRON_DOWN: &str = "titlebar/chevron-down.svg";
const TITLEBAR_ICON_DOTS: &str = "titlebar/dots.svg";

#[path = "../../app/helpers/titlebar/header_pins_look.rs"]
mod look;
use look::*;

const MENU_WIDTH: f32 = 268.0;
const HEADER_WIDTH: f32 = 760.0;

fn menu_appearance() -> PopupMenuAppearance {
    PopupMenuAppearance {
        shadow: false,
        panel_radius: px(8.0),
        item_radius: px(6.0),
        padding: px(6.0),
        item_padding_x: px(10.0),
        item_height: px(34.0),
        separator_margin: px(6.0),
        background: rgb(0x1c1c1c).into(),
        foreground: titlebar_popup_menu_foreground(),
        border: rgb(0xffffff).opacity(0.12).into(),
        hover: rgb(0xffffff).opacity(0.08).into(),
    }
}

/// The section heading the app's menus draw (helpers/titlebar/popup_rows.rs
/// `titlebar_popup_git_section_label`).
fn section_label(label: &'static str) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| {
        div()
            .flex()
            .w_full()
            .min_h(px(22.0))
            .items_center()
            .text_size(px(11.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(titlebar_overlay_base().opacity(0.55))
            .child(label)
    })
    .disabled(true)
}

/// The context ring (native_chat/session_details/ring.rs), with the header's visible track.
fn ring(percent: f32, ink: Hsla) -> impl IntoElement {
    let size = 14.0;
    let track = ink.opacity(0.18);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = size * 0.4;
            for (fraction, color) in [(1.0, track), (percent / 100.0, ink)] {
                let mut path = gpui::PathBuilder::stroke(px(size * 0.125));
                let steps = (fraction * 96.0f32).ceil() as usize;
                for step in 0..=steps {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * fraction * step as f32 / steps as f32;
                    let at = center + point(px(angle.cos() * radius), px(angle.sin() * radius));
                    if step == 0 {
                        path.move_to(at);
                    } else {
                        path.line_to(at);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .size(px(size))
}

/// One part of the project pill: icon and label, then its arrow.
fn segment(icon: &'static str, label: &'static str, first: bool) -> impl IntoElement {
    let text = titlebar_icon_color();
    h_flex()
        .h_full()
        .items_center()
        .when(!first, |this| {
            this.child(
                div()
                    .w(px(1.0))
                    .h(px(15.0))
                    .bg(titlebar_split_button_border_color()),
            )
        })
        .child(
            h_flex()
                .h_full()
                .items_center()
                .gap(px(6.0))
                .pl(px(8.0))
                .pr(px(4.0))
                .text_size(px(12.5))
                .text_color(text)
                .child(header_icon(icon, text))
                .child(label),
        )
        .child(
            div()
                .flex()
                .h_full()
                .w(px(18.0))
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(
                    TITLEBAR_ICON_CHEVRON_DOWN,
                    10.0,
                    titlebar_inactive_text_color(),
                )),
        )
}

use gpui::prelude::FluentBuilder as _;

struct HeaderMoreDemo {
    menu: Entity<PopupMenu>,
    sections: Vec<(&'static str, Vec<HeaderMoreRow>)>,
    /// Controls pinned from the menu, drawn in the header before Start.
    pinned: Vec<HeaderSlot>,
}

fn build_menu(
    sections: Vec<(&'static str, Vec<HeaderMoreRow>)>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PopupMenu> {
    PopupMenu::build(window, cx, move |menu, _, _| {
        let mut menu = menu
            .appearance(menu_appearance())
            .min_w(px(MENU_WIDTH))
            .max_w(px(MENU_WIDTH))
            .check_side(Side::Right);
        for (index, (title, rows)) in sections.iter().cloned().enumerate() {
            if index > 0 {
                menu = menu.separator();
            }
            menu = menu.item(section_label(title));
            for row in rows {
                let action = RunWorkareaHeaderSlot {
                    slot: row.slot.key().to_string(),
                };
                menu =
                    menu.menu_element(Box::new(action), move |_, _| header_more_row(row.clone()));
            }
        }
        menu
    })
}

impl HeaderMoreDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let row = |slot, status: Option<&str>, live| HeaderMoreRow {
            slot,
            status: status.map(str::to_string),
            live,
        };
        let sections = vec![
            (
                "This chat",
                vec![
                    row(HeaderSlot::Share, None, false),
                    row(HeaderSlot::Popout, None, false),
                ],
            ),
            (
                "Agents",
                vec![
                    row(HeaderSlot::Claude, Some("On"), true),
                    row(HeaderSlot::GrokBot, None, false),
                    row(HeaderSlot::Voice, None, false),
                ],
            ),
        ];
        let menu = build_menu(sections.clone(), window, cx);
        menu.focus_handle(cx).focus(window, cx);
        Self {
            menu,
            sections,
            pinned: Vec::new(),
        }
    }

    /// A row's pin, as the app does it: the control moves to the header and the menu is re-read
    /// once the click is over (render/workarea_header/header_pins.rs `pin_header_slot_from_menu`).
    fn pin(&mut self, action: &PinWorkareaHeaderSlot, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slot) = HeaderSlot::from_key(&action.slot) else {
            return;
        };
        self.pinned.push(slot);
        for (_, rows) in self.sections.iter_mut() {
            rows.retain(|row| row.slot != slot);
        }
        self.sections.retain(|(_, rows)| !rows.is_empty());
        cx.defer_in(window, |this, window, cx| {
            this.menu = build_menu(this.sections.clone(), window, cx);
            this.menu.focus_handle(cx).focus(window, cx);
            cx.notify();
        });
    }
}

impl Render for HeaderMoreDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = titlebar_active_text_color();
        let pinned: Vec<_> = self
            .pinned
            .iter()
            .map(|slot| {
                header_icon_button(slot.key(), false)
                    .child(header_icon(slot.icon(), titlebar_icon_color()))
            })
            .collect();
        let header = h_flex()
            .w(px(HEADER_WIDTH))
            .h(px(36.0))
            .pl(px(14.0))
            .pr(px(9.0))
            .items_center()
            .bg(rgb(0x221c1d))
            .text_color(titlebar_icon_color())
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.0))
                    .text_color(text.opacity(0.92))
                    .child("Ghostex  \u{203a}  Calmer chat header"),
            )
            .children(pinned)
            // Claude's page is on, so its button shows though it is not pinned (after Grok Bot,
            // as the app orders them).
            .child(
                header_icon_button("demo-claude", true)
                    .mr(px(6.0))
                    .child(header_icon(HEADER_CLAUDE_ICON, text)),
            )
            // Start and Commit in one outline.
            .child(
                h_flex()
                    .h(px(TITLEBAR_CONTROL_HEIGHT))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(titlebar_split_button_border_color())
                    .overflow_hidden()
                    .items_center()
                    .child(segment(TITLEBAR_ICON_PLAYER_PLAY, "Start", true))
                    .child(segment(TITLEBAR_ICON_GIT_COMMIT, "Commit", false)),
            )
            .child(
                h_flex()
                    .ml(px(6.0))
                    .gap(px(2.0))
                    .items_center()
                    .child(
                        header_icon_button("demo-work", false)
                            .gap(px(5.0))
                            .px(px(7.0))
                            .text_size(px(12.0))
                            .text_color(text)
                            .child(header_icon("titlebar/layout-board-split.svg", text))
                            .child("Work")
                            .child(
                                div()
                                    .px(px(5.0))
                                    .h(px(16.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(8.0))
                                    .bg(rgb(0xf472b6))
                                    .text_size(px(10.5))
                                    .text_color(rgb(0x1a1a1a))
                                    .child("2"),
                            ),
                    )
                    .child(
                        header_icon_button("demo-ring", false)
                            .gap(px(5.0))
                            .px(px(7.0))
                            .text_size(px(12.0))
                            .text_color(text)
                            .child(
                                div()
                                    .flex()
                                    .size(px(HEADER_ICON_BOX))
                                    .items_center()
                                    .justify_center()
                                    .child(ring(40.0, text)),
                            )
                            .child("40%"),
                    )
                    .child(
                        header_icon_button("demo-more", true)
                            .child(header_icon(TITLEBAR_ICON_DOTS, titlebar_icon_color())),
                    )
                    .child(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .mx(px(5.0))
                            .bg(titlebar_split_button_border_color()),
                    )
                    .child(
                        header_icon_button("demo-bottom", false)
                            .child(header_icon(TITLEBAR_ICON_PANEL_BOTTOM, text)),
                    )
                    .child(
                        header_icon_button("demo-side", true)
                            .child(header_icon(TITLEBAR_ICON_PANEL_RIGHT, text)),
                    ),
            );
        div()
            .size_full()
            .on_action(cx.listener(Self::pin))
            .bg(rgb(0x18151a))
            .flex()
            .flex_col()
            .child(header)
            .child(
                h_flex()
                    .w(px(HEADER_WIDTH))
                    .justify_end()
                    .pr(px(80.0))
                    .pt(px(4.0))
                    .child(div().w(px(MENU_WIDTH + 2.0)).child(self.menu.clone())),
            )
    }
}

pub(super) fn open(_demo: &super::DemoEnv, cx: &mut App) {
    super::open_modal_window(
        HEADER_WIDTH,
        390.0,
        move |window, cx| cx.new(|cx| HeaderMoreDemo::new(window, cx)),
        cx,
    );
}
