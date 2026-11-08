//! Chief's row preview (`GHOSTEX_NATIVE_MODAL_DEMO=chief-row`): the top of a 300px sidebar with
//! the Spaces bar, Chief's row under it and the project header under that, drawn with the
//! sidebar's own row code (`native_sidebar/chief_row_view.rs`) and the project header's geometry,
//! so the two can be compared. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): `one` (default, one
//! Chief), `working`, `asking`, `open` (its chat is open), `setup` (first open, no agent yet),
//! `folded` (two more computers have one, folded), `unfolded` (the same, unfolded), `bots` (the
//! sidebar in Bots mode, where the row is not drawn: `chief_row_shown`), `unified` (the Unified
//! sidebar: a stand-in for its header, the row under it, then a Unified group header and cards),
//! and the home (CDXC:Chief 2026-11-08 in server/src/chief/home.rs): `home` (one computer: Chief's
//! row heads the home project and its loose sessions, then the next project), `home-combined`
//! (computers combined: the home section's title, then each computer's header with its own home
//! sessions), `home-earlier` (the home unfolded with Chief's earlier chats asleep in it),
//! `home-setup` (Main Hub before Chief of Staff's first open: the Pick an agent row), `home-off`
//! (the Main Hub extension off, the default: the home folder is an ordinary project),
//! `home-tooltip` (the Main Hub tooltip under the header, drawn in the tooltip bubble's look).
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
            CHIEF_LABEL,
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
        // `letter` on a tile of `tile`/`ink` (a favicon-less project's letter tile, project_header.rs).
        let project_header = |letter: &str, name: &str, tile: Hsla, letter_ink: Hsla| {
            div().pl(px(24.0)).pr(px(7.0)).child(
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
                                .with_transformation(gpui::Transformation::rotate(
                                    gpui::percentage(0.25),
                                )),
                        ),
                    )
                    .child(
                        div()
                            .size(px(16.0))
                            .rounded(px(3.0))
                            .bg(tile)
                            .text_size(px(10.0))
                            .text_color(letter_ink)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(letter.to_string()),
                    )
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::LIGHT)
                            .child(name.to_string()),
                    ),
            )
        };
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
        let bots = state == "bots";
        let unified = state == "unified";
        let shown = chief_row_shown(true, bots);
        // A stand-in for the Unified header (unified/header.rs): its 36px search row.
        let unified_header = div().h(px(36.0)).px(px(12.0)).flex().items_center().child(
            div()
                .flex_1()
                .h(px(28.0))
                .px(px(10.0))
                .rounded(px(5.0))
                .border_1()
                .border_color(hsla(ink, 0.12))
                .flex()
                .items_center()
                .text_size(px(13.0))
                .text_color(colors.muted)
                .child("Search sessions"),
        );
        // A Unified group header (unified/groups.rs: list column pl 6, header mx 3, px 6, 30px).
        let unified_group = |title: &str| {
            div().pl(px(6.0)).child(
                div()
                    .mx(px(3.0))
                    .h(px(30.0))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .text_size(px(12.0))
                    .text_color(colors.muted)
                    .child(title.to_string()),
            )
        };
        let home = state.starts_with("home");
        // A computer's pill after a chat's title (project_header.rs `machine_name_chip`'s look).
        let chip = |name: &str| {
            div()
                .flex_shrink_0()
                .px(px(5.0))
                .rounded(px(99.0))
                .border_1()
                .border_color(colors.muted.opacity(0.35))
                .text_size(px(10.0))
                .text_color(colors.muted)
                .font_weight(gpui::FontWeight::NORMAL)
                .child(name.to_string())
                .into_any_element()
        };
        // A loose session under the home header: its title, dimmed while asleep, the computer's
        // pill for a chat on another computer, and its dot.
        let home_chat =
            |title: &str, asleep: bool, machine: Option<&str>, status: Option<AnyElement>| {
                div().pl(px(24.0)).pr(px(7.0)).child(
                    h_flex()
                        .h(px(30.0))
                        .pl(px(8.0))
                        .pr(px(6.0))
                        .gap(px(8.0))
                        .font_weight(gpui::FontWeight::LIGHT)
                        .child(
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(5.0))
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .when(asleep, |title| title.text_color(colors.muted))
                                        .child(title.to_string()),
                                )
                                .children(machine.map(chip)),
                        )
                        .children(status),
                )
            };
        let home_session = |title: &str, asleep: bool, status: Option<AnyElement>| {
            home_chat(title, asleep, None, status)
        };
        // Main Hub's header: the project header with the flower and "Main Hub" (project_header.rs).
        let home_header = |fold: ChiefFold| {
            chief_block(1.0).pb(px(0.0)).child(chief_header_row(
                HOME_LABEL, 1.0, colors, fold, false, false, None, None,
            ))
        };
        // A section heading (sections.rs): 12px, light, faint.
        let heading = |label: &str| {
            div()
                .pl(px(24.0 + 3.0 + 5.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::LIGHT)
                .text_color(hsla(ink, 0.34))
                .child(label.to_string())
                .into_any_element()
        };
        let chief_row = |machine: Option<&str>, status: Option<AnyElement>| {
            home_chat(CHIEF_LABEL, false, machine, status).into_any_element()
        };
        let home_rows: Vec<AnyElement> = match state {
            "home" | "home-tooltip" => vec![
                home_header(ChiefFold::Unfolded).into_any_element(),
                heading("Pinned"),
                chief_row(None, Some(dot(WORKING))),
                heading("Sessions"),
                home_session("Preventing Mac screen lock", false, Some(dot(WORKING)))
                    .into_any_element(),
                home_session("Disk cleanup and move to /Volumes", false, None).into_any_element(),
                home_session("CPU usage investigation", true, None).into_any_element(),
            ],
            // Main Hub off (the default): the home folder is an ordinary project, no Chief of Staff.
            "home-off" => vec![
                div()
                    .child(project_header(
                        "M",
                        "madda",
                        colors.muted.opacity(0.12),
                        colors.muted,
                    ))
                    .into_any_element(),
                home_session("Preventing Mac screen lock", false, Some(dot(WORKING)))
                    .into_any_element(),
                home_session("Disk cleanup and move to /Volumes", false, None).into_any_element(),
                home_session("CPU usage investigation", true, None).into_any_element(),
            ],
            "home-setup" => vec![
                home_header(ChiefFold::Unfolded).into_any_element(),
                home_chat(
                    CHIEF_LABEL,
                    true,
                    None,
                    Some(
                        div()
                            .text_size(px(12.0))
                            .text_color(colors.muted)
                            .child("Pick an agent")
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
                home_session("Preventing Mac screen lock", false, None).into_any_element(),
                home_session("CPU usage investigation", true, None).into_any_element(),
            ],
            "home-earlier" => vec![
                home_header(ChiefFold::Unfolded).into_any_element(),
                heading("Pinned"),
                chief_row(None, None),
                heading("Sessions"),
                home_session("Preventing Mac screen lock", false, Some(dot(WORKING)))
                    .into_any_element(),
                home_session("Chief of Staff \u{b7} 8 Nov, 13:30", true, None).into_any_element(),
                home_session("Chief of Staff \u{b7} 7 Nov, 09:12", true, None).into_any_element(),
                home_session("CPU usage investigation", true, None).into_any_element(),
            ],
            "home-combined" => vec![
                home_header(ChiefFold::Unfolded).into_any_element(),
                heading("Pinned"),
                chief_row(None, Some(dot(WORKING))),
                chief_row(Some("Hetzner"), Some(dot(QUESTION))),
                chief_row(Some("WN"), None),
                heading("Sessions"),
                home_chat(
                    "Preventing Mac screen lock",
                    false,
                    None,
                    Some(dot(WORKING)),
                )
                .into_any_element(),
                home_chat("Restart the relay", false, Some("Hetzner"), None).into_any_element(),
                home_chat("Game launcher setup", true, Some("WN"), None).into_any_element(),
                home_chat("CPU usage investigation", true, None, None).into_any_element(),
            ],
            _ => Vec::new(),
        };
        // The card beside the sidebar, over the work area (window/main_hub_card.rs), in the
        // tooltip bubble's look (tooltips.rs `in_window_bubble`): the menu fill, its outline.
        let card_beside = state == "home-tooltip";
        let tooltip_card = card_beside.then(|| {
            let fill = if self.light {
                hsla(0xffffff, 1.0)
            } else {
                hsla(0x2b3038, 1.0)
            };
            div()
                .absolute()
                .top(px(14.0 + 40.0))
                .left(px(300.0 + 6.0))
                .py(px(10.0))
                .px(px(12.0))
                .rounded(px(8.0))
                .bg(fill)
                .border_1()
                .border_color(hsla(ink, 0.14))
                .shadow_lg()
                .child(main_hub_tooltip_content(
                    1.0,
                    hsla(ink, 1.0),
                    hsla(ink, 0.6),
                ))
        });
        let work_area = if self.light {
            hsla(0xf8f9fb, 1.0)
        } else {
            hsla(0x16191d, 1.0)
        };
        let column = v_flex()
            .relative()
            .flex_shrink_0()
            .w(px(300.0))
            .h(px(if home { 420.0 } else { 330.0 }))
            .bg(background)
            .text_color(hsla(ink, 1.0))
            .text_size(px(TEXT_SIZE))
            .font_family(super::ui_fonts::UI_FONT)
            .pt(px(14.0))
            .when(unified, |column| {
                column.child(unified_header).child(div().h(px(6.0)))
            })
            .when(!unified, |column| {
                column.child(
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
            })
            .when(bots, |column| {
                column.child(
                    div()
                        .pl(px(24.0))
                        .h(px(30.0))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .font_weight(gpui::FontWeight::LIGHT)
                        .child(div().text_color(colors.muted).child("+"))
                        .child("New bot"),
                )
            })
            .when(home, |column| {
                column
                    .children(home_rows)
                    .child(div().h(px(10.0)))
                    .child(project_header(
                        "G",
                        "Ghostex",
                        rgb(0x3b82f6).into(),
                        rgb(0xffffff).into(),
                    ))
                    .child(session("Main Coordinator"))
            })
            .when(shown && !home, |column| {
                column.child(chief_block(1.0).child(main).when(
                    fold == ChiefFold::Unfolded,
                    |block| {
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
                    },
                ))
            })
            .when(unified, |column| {
                column
                    .child(unified_group("Today"))
                    .child(session("Main Coordinator"))
                    .child(session("Outdated dependencies audit"))
            })
            .when(!bots && !unified && !home, |column| {
                column
                    .child(project_header(
                        "G",
                        "Ghostex",
                        rgb(0x3b82f6).into(),
                        rgb(0xffffff).into(),
                    ))
                    .child(session("Main Coordinator"))
                    .child(session("Outdated dependencies audit"))
            });
        // The sidebar, and with the card the work area beside it, the card painted last.
        h_flex()
            .relative()
            .bg(work_area)
            .child(column)
            .when(card_beside, |row| row.child(div().w(px(330.0)).h_full()))
            .children(tooltip_card)
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = if demo.state.is_empty() {
        "one".to_string()
    } else {
        demo.state.clone()
    };
    let light = demo.palette.light;
    let height = if state.starts_with("home") {
        420.0
    } else {
        330.0
    };
    // The card state shows the work area beside the sidebar too.
    let width = if state == "home-tooltip" {
        630.0
    } else {
        300.0
    };
    let _ = super::open_modal_window(
        width,
        height,
        move |_, cx| cx.new(|_| ChiefRowPreview { state, light }),
        cx,
    );
}
