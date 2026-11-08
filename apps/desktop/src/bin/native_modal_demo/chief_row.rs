//! Chief's row preview (`GHOSTEX_NATIVE_MODAL_DEMO=chief-row`): the top of a 300px sidebar with
//! the Spaces bar, Chief's row under it and the start of the project list, drawn with the
//! sidebar's own row code (`native_sidebar/chief_row_view.rs`). States
//! (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `idle` (default), `working`, `asking`, `approval`, `open`
//! (its chat is open), `setup` (first open, no agent yet), `combined` (two more computers, folded),
//! `unfolded` (the same, unfolded).
use super::chief_row_view::*;
use gpui::{
    AnyElement, App, AppContext as _, Context, Hsla, IntoElement, ParentElement, Render, Styled,
    Window, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};

const WORKING: u32 = 0xc68a06;
const QUESTION: u32 = 0xf472b6;
const APPROVAL: u32 = 0xef4444;

struct ChiefRowPreview {
    state: String,
    light: bool,
}

fn hsla(hex: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = rgb(hex).into();
    color.a = alpha;
    color
}

fn dot(color: u32) -> AnyElement {
    div()
        .w(px(16.0))
        .h(px(16.0))
        .flex()
        .items_center()
        .justify_center()
        .child(div().size(px(8.0)).rounded_full().bg(rgb(color)))
        .into_any_element()
}

impl Render for ChiefRowPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (background, ink) = if self.light {
            (hsla(0xeef1f5, 1.0), 0x1f2328)
        } else {
            (hsla(0x1d2126, 1.0), 0xffffff)
        };
        let colors = ChiefRowColors {
            foreground: hsla(ink, 0.92),
            muted: hsla(ink, 0.5),
            icon: hsla(ink, 0.72),
            label: hsla(ink, 0.7),
            hover: hsla(ink, 0.06),
            selected_fill: if self.light {
                hsla(0xffffff, 1.0)
            } else {
                hsla(ink, 0.12)
            },
            selected_outline: hsla(ink, 0.16),
        };
        let state = self.state.as_str();
        let status = match state {
            "working" => Some(dot(WORKING)),
            "asking" => Some(dot(QUESTION)),
            "approval" => Some(dot(APPROVAL)),
            _ => None,
        };
        let combined = matches!(state, "combined" | "unfolded");
        let unfolded = state == "unfolded";
        let chip = combined.then(|| chief_others_chip(1.0, colors, 2, unfolded).into_any_element());
        let main = chief_main_row(1.0, colors, state == "open", state == "setup", chip, status);
        let space = |selected: bool, glyph: &str| {
            div()
                .size(px(28.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(colors.label)
                .when_selected(selected, colors)
                .child(glyph.to_string())
        };
        let session = |title: &str, indent: bool| {
            div()
                .h(px(28.0))
                .pl(px(if indent { 26.0 } else { 14.0 }))
                .flex()
                .items_center()
                .text_size(px(13.0))
                .text_color(colors.foreground)
                .child(title.to_string())
        };
        v_flex()
            .w(px(300.0))
            .h(px(330.0))
            .bg(background)
            .pt(px(14.0))
            .child(
                h_flex()
                    .w_full()
                    .h(px(40.0))
                    .pb(px(12.0))
                    .justify_center()
                    .gap(px(4.0))
                    .child(space(true, "G"))
                    .child(space(false, "M"))
                    .child(space(false, "W"))
                    .child(space(false, "…")),
            )
            .child(
                v_flex()
                    .w_full()
                    .px(px(6.5))
                    .pb(px(10.0))
                    .gap(px(2.0))
                    .child(main)
                    .children(unfolded.then(|| {
                        chief_other_row(
                            "hetzner",
                            1.0,
                            colors,
                            "Hetzner",
                            false,
                            Some(dot(WORKING)),
                        )
                    }))
                    .children(unfolded.then(|| {
                        chief_other_row("wn-desktop", 1.0, colors, "WN desktop", false, None)
                    })),
            )
            .child(
                div()
                    .h(px(28.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .text_size(px(12.0))
                    .text_color(colors.muted)
                    .child("Ghostex"),
            )
            .child(session("Main Coordinator", false))
            .child(session("Outdated dependencies audit", true))
            .child(session("Sidebar spacing pass", true))
    }
}

trait SelectedTile {
    fn when_selected(self, selected: bool, colors: ChiefRowColors) -> Self;
}

impl SelectedTile for gpui::Div {
    fn when_selected(self, selected: bool, colors: ChiefRowColors) -> Self {
        if selected {
            self.bg(colors.selected_fill)
                .border_1()
                .border_color(colors.selected_outline)
        } else {
            self
        }
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = if demo.state.is_empty() {
        "idle".to_string()
    } else {
        demo.state.clone()
    };
    let light = demo.palette.light;
    let _ = super::open_modal_window(
        300.0,
        330.0,
        move |_, cx| cx.new(|_| ChiefRowPreview { state, light }),
        cx,
    );
}
