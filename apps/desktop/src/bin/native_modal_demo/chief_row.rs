//! Chief's row preview (`GHOSTEX_NATIVE_MODAL_DEMO=chief-row`): the top of a 300px sidebar with
//! the Spaces bar, Chief's row under it and the project header under that, drawn with the
//! sidebar's own row code (`native_sidebar/chief_row_view.rs`) and the project header's geometry,
//! so the two can be compared. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `one` (default, one
//! Chief), `working`, `asking`, `open` (its chat is open), `setup` (first open, no agent yet),
//! `folded` (two more computers have one, folded), `unfolded` (the same, unfolded).
use super::chief_row_view::*;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext as _, Context, Hsla, IntoElement, ParentElement, Render, Styled,
    Window, div, px, rgb, svg,
};
use gpui_component::{h_flex, v_flex};

const WORKING: u32 = 0xc68a06;
const QUESTION: u32 = 0xf472b6;
/// The sidebar's text size (native_sidebar/appearance.rs `SIDEBAR_TEXT_SIZE`).
const TEXT_SIZE: f32 = 15.55;

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
            (hsla(0xeef1f5, 1.0), 0x262626)
        } else {
            (hsla(0x1d2126, 1.0), 0xd8d8d8)
        };
        let colors = ChiefRowColors {
            muted: hsla(ink, 0.55),
            hover: hsla(ink, 0.06),
            selected_fill: if self.light {
                hsla(0xe7e7e7, 1.0)
            } else {
                hsla(0xffffff, 0.12)
            },
        };
        let state = self.state.as_str();
        let status = match state {
            "working" => Some(dot(WORKING)),
            "asking" => Some(dot(QUESTION)),
            _ => None,
        };
        let fold = match state {
            "folded" => ChiefFold::Folded,
            "unfolded" => ChiefFold::Unfolded,
            _ => ChiefFold::Single,
        };
        let outline = || {
            div()
                .absolute()
                .inset_0()
                .rounded(px(5.0))
                .border_1()
                .border_color(hsla(ink, 0.12))
                .into_any_element()
        };
        let open = state == "open";
        let main = chief_header_row(
            1.0,
            colors,
            fold,
            open,
            state == "setup",
            open.then(outline),
            status,
        );
        let space = |selected: bool, glyph: &str| {
            div()
                .size(px(28.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(colors.muted)
                .when(selected, |tile| {
                    tile.bg(colors.selected_fill)
                        .border_1()
                        .border_color(hsla(ink, 0.16))
                })
                .child(glyph.to_string())
        };
        // The project header as project_header.rs lays it out, in its block (render.rs).
        let project_header = div().pl(px(24.0)).pr(px(7.0)).child(
            h_flex()
                .relative()
                .h(px(30.0))
                .ml(px(-15.0))
                .mr(px(3.0))
                .pl(px(23.0))
                .pr(px(6.0))
                .gap(px(10.0))
                .child(
                    div().absolute().left_0().top(px(7.0)).child(
                        svg()
                            .path("titlebar/chevron-right.svg")
                            .size(px(16.0))
                            .text_color(colors.muted)
                            .with_transformation(gpui::Transformation::rotate(gpui::percentage(
                                0.25,
                            ))),
                    ),
                )
                .child(
                    div()
                        .size(px(16.0))
                        .rounded(px(3.0))
                        .bg(rgb(0x3b82f6))
                        .text_size(px(10.0))
                        .text_color(rgb(0xffffff))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child("G"),
                )
                .child(div().font_weight(gpui::FontWeight::LIGHT).child("Ghostex")),
        );
        let session = |title: &str| {
            div().pl(px(24.0)).child(
                div()
                    .h(px(30.0))
                    .pl(px(8.0))
                    .flex()
                    .items_center()
                    .font_weight(gpui::FontWeight::LIGHT)
                    .child(title.to_string()),
            )
        };
        v_flex()
            .w(px(300.0))
            .h(px(330.0))
            .bg(background)
            .text_color(hsla(ink, 1.0))
            .text_size(px(TEXT_SIZE))
            .font_family(super::ui_fonts::UI_FONT)
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
                chief_block(1.0)
                    .child(main)
                    .when(fold == ChiefFold::Unfolded, |block| {
                        block
                            .child(chief_other_row(
                                "hetzner",
                                1.0,
                                colors,
                                "Hetzner",
                                false,
                                None,
                                Some(dot(WORKING)),
                            ))
                            .child(chief_other_row(
                                "wn-desktop",
                                1.0,
                                colors,
                                "WN desktop",
                                false,
                                None,
                                None,
                            ))
                    }),
            )
            .child(project_header)
            .child(session("Main Coordinator"))
            .child(session("Outdated dependencies audit"))
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = if demo.state.is_empty() {
        "one".to_string()
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
