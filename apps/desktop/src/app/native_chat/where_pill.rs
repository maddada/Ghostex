//! The where pill at the left end of the chat box's pill strip (pills_strip.rs): one pill split by
//! a hairline, the computer's own icon with the project on the left, the branch on the right.
//! Hovering it shows the card of where the session runs; a click opens the same details
//! above the box with Copy branch and Open folder. gx-chat-core words all of it (`wherePill`,
//! packages/gx-chat-core/src/extras/where_pill.rs, where the user's decision lives).
//!
//! The pill counts in the strip's fit (`pills_strip.rs`, `plan_strip_with_where`): the panel pills
//! drop their labels first, then the pill shortens its texts (`WhereTier::Short`), then drops the
//! project (`WhereTier::Icon`), and only then do panel pills go into the "+N" pill.
//!
//! CDXC:SessionChat 2026-11-18 DECISION:
//! User, of the pill reading "Ghostex · This computer | main": "no need to mention the name of the
//! machine here. instead just show the icon of the machine that i set instead of the pc icon shown
//! there on the left". The pill no longer names the computer (the hover card and the details still
//! do); its icon is the computer's own, the one the sidebar draws for it (picked in Settings >
//! Remote, else its system's), and today's plain icons only while that is not known yet.

use super::{appearance::ChatAppearance, cursor::ChatCursor as _, state::NativeChatView};
use crate::app::native_sidebar::tooltips::CardInk;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, canvas, div, px, svg,
};
use serde_json::{Value, json};

/// The pill's menu id and its menu-toggle key (chat_pills.rs `trigger_id`).
pub(super) const WHERE_MENU: &str = "wherePill";
pub(super) const WHERE_TRIGGER: &str = "chat-pill-where";

/// The details menu's width and the hover card's (unscaled).
pub(super) const WHERE_MENU_WIDTH: f32 = 380.0;
const CARD_WIDTH: f32 = 340.0;

/// A detail row's height, the menu's padding around its rows and its actions row (unscaled).
const ROW_HEIGHT: f32 = 24.0;
const ROWS_PAD_Y: f32 = 4.0;
const ACTIONS_HEIGHT: f32 = 30.0;

/// The pill's inner measures (unscaled): each half's side padding, an icon, the gap after it, and
/// the hairline between the halves.
const PAD: f32 = 8.0;
const ICON: f32 = 14.0;
const GAP: f32 = 5.0;
const HAIRLINE: f32 = 1.0;
const BORDER: f32 = 1.0;

/// How much of the pill shows, widest first (`plan_strip_with_where`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WhereTier {
    /// Project | branch.
    #[default]
    Full,
    /// Project | branch, shorter.
    Short,
    /// The computer's icon | branch, shorter.
    Icon,
}

impl WhereTier {
    pub(super) const ALL: [WhereTier; 3] = [WhereTier::Full, WhereTier::Short, WhereTier::Icon];

    pub(super) fn index(self) -> usize {
        match self {
            WhereTier::Full => 0,
            WhereTier::Short => 1,
            WhereTier::Icon => 2,
        }
    }

    /// The widest the project and the branch may be (unscaled); 0 hides one.
    fn caps(self) -> (f32, f32) {
        match self {
            WhereTier::Full => (170.0, 220.0),
            WhereTier::Short => (130.0, 150.0),
            WhereTier::Icon => (0.0, 110.0),
        }
    }
}

/// The pill's texts, measured (unscaled).
struct Measured {
    project: f32,
    branch: Option<f32>,
}

/// The texts' widths at `tier`: each measured width (a pixel over, so it never cuts) under its cap.
fn text_widths(measured: &Measured, tier: WhereTier) -> (f32, Option<f32>) {
    let (project, branch) = tier.caps();
    let fit = |width: f32, cap: f32| {
        if cap <= 0.0 {
            0.0
        } else {
            (width + 1.0).min(cap)
        }
    };
    (
        fit(measured.project, project),
        measured.branch.map(|width| fit(width, branch)),
    )
}

/// The pill's border box width at `tier` (unscaled), the sum of what `where_pill_element` draws.
fn pill_width(measured: &Measured, tier: WhereTier) -> f32 {
    let (project, branch) = text_widths(measured, tier);
    let mut width = 2.0 * BORDER + PAD + ICON + PAD;
    if project > 0.0 {
        width += GAP + project;
    }
    if let Some(branch) = branch {
        width += HAIRLINE + PAD + ICON + PAD;
        if branch > 0.0 {
            width += GAP + branch;
        }
    }
    width
}

fn icon_path(name: &str) -> String {
    format!("titlebar/{name}.svg")
}

/// The computer's icon: this computer's own and the remote computer's own mark (picked for it, else
/// its system's), a Cloud Box's cloud or a box's.
fn machine_icon(pill: &Value) -> String {
    let machine = &pill["machine"];
    match machine["kind"].as_str() {
        Some("cloudBox") => icon_path("cloud"),
        Some("box") => icon_path("box"),
        Some("remote") => {
            let mark = machine["serverId"]
                .as_str()
                .map(crate::app::gx_chat::session_directory::computer_icon)
                .unwrap_or("titlebar/server.svg");
            // An unknown computer's mark is the plain desktop, which reads as this computer.
            if mark == "titlebar/device-desktop.svg" {
                icon_path("server")
            } else {
                mark.to_string()
            }
        }
        _ => crate::app::gx_chat::session_directory::this_computer_icon()
            .map_or_else(|| icon_path("device-desktop"), str::to_string),
    }
}

fn branch_icon(pill: &Value) -> String {
    icon_path(if pill["worktree"].as_bool() == Some(true) {
        "git-fork"
    } else {
        "git-branch"
    })
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

/// The card's header: the project in bold, the computer after it, muted.
fn card_header(pill: &Value, ink: CardInk, s: f32) -> gpui::Div {
    let card = &pill["card"];
    div()
        .flex()
        .items_center()
        .gap(px(7.0 * s))
        .px(px(2.0 * s))
        .pb(px(7.0 * s))
        .border_b_1()
        .border_color(ink.hairline)
        .text_size(px(13.0 * s))
        .child(
            svg()
                .path(icon_path("folder"))
                .size(px(14.0 * s))
                .flex_shrink_0()
                .text_color(ink.secondary),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(ink.foreground)
                .child(text(card, "title")),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex_shrink_0()
                .max_w(px(150.0 * s))
                .truncate()
                .text_color(ink.secondary)
                .child(text(card, "subtitle")),
        )
}

/// The detail rows: an icon, the label, the value and its muted detail.
fn card_rows(pill: &Value, ink: CardInk, s: f32) -> gpui::Div {
    let rows = pill["card"]["rows"].as_array().cloned().unwrap_or_default();
    div()
        .flex()
        .flex_col()
        .py(px(ROWS_PAD_Y * s))
        .children(rows.into_iter().map(|row| {
            let icon = icon_path(row["icon"].as_str().unwrap_or("point"));
            let detail = row["detail"].as_str().map(str::to_string);
            div()
                .h(px(ROW_HEIGHT * s))
                .flex()
                .items_center()
                .gap(px(8.0 * s))
                .px(px(2.0 * s))
                .text_size(px(12.5 * s))
                .child(
                    svg()
                        .path(icon)
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(ink.secondary),
                )
                .child(
                    div()
                        .w(px(88.0 * s))
                        .flex_shrink_0()
                        .truncate()
                        .text_color(ink.secondary)
                        .child(text(&row, "label")),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(ink.foreground)
                                .child(text(&row, "value")),
                        )
                        .when_some(detail, |line, detail| {
                            line.child(
                                div()
                                    .flex_shrink_0()
                                    .max_w(px(150.0 * s))
                                    .truncate()
                                    .text_color(ink.secondary)
                                    .child(detail),
                            )
                        }),
                )
        }))
}

impl NativeChatView {
    /// The `wherePill` value, while the session has one.
    pub(super) fn where_pill(&self) -> Option<&Value> {
        Some(&self.snapshot["wherePill"]).filter(|pill| pill.is_object())
    }

    /// The pill's border box width at each tier (unscaled), before it is drawn, for the strip's
    /// fit (`plan_strip_with_where`).
    pub(super) fn where_pill_widths(&self, p: &ChatAppearance, cx: &gpui::App) -> Option<[f32; 3]> {
        let pill = self.where_pill()?;
        let width = |label: &str| {
            super::fonts::label_width(label, &p.font, FontWeight::NORMAL, 13.0 * p.scale, cx)
                / p.scale
        };
        let measured = Measured {
            project: width(&text(pill, "project")),
            branch: pill["branch"].as_str().map(width),
        };
        Some(WhereTier::ALL.map(|tier| pill_width(&measured, tier)))
    }

    /// The pill at `tier`.
    pub(super) fn where_pill_element(
        &self,
        tier: WhereTier,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let pill = self.where_pill()?.clone();
        let s = p.scale;
        let width = |label: &str| {
            super::fonts::label_width(label, &p.font, FontWeight::NORMAL, 13.0 * s, cx) / s
        };
        let project = text(&pill, "project");
        let branch = pill["branch"].as_str().map(str::to_string);
        let measured = Measured {
            project: width(&project),
            branch: branch.as_deref().map(width),
        };
        let (project_w, branch_w) = text_widths(&measured, tier);
        let open = self.chat_menu_is_open(WHERE_TRIGGER);
        let bounds = self.pill_bounds.cell(WHERE_MENU);
        let measured_bounds = bounds.clone();
        let label = |content: String, w: f32, color: Hsla| {
            div()
                .w(px(w * s))
                .flex_shrink_0()
                .truncate()
                .text_color(color)
                .child(content)
        };
        let half = || {
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(GAP * s))
                .px(px(PAD * s))
                .py(px(2.0 * s))
        };
        let icon = |path: String| {
            svg()
                .path(path)
                .size(px(ICON * s))
                .flex_shrink_0()
                .text_color(p.card_muted)
        };
        let left = half()
            .child(icon(machine_icon(&pill)))
            .when(project_w > 0.0, |half| {
                half.child(label(project.clone(), project_w, p.foreground))
            });
        let right = branch.clone().zip(branch_w).map(|(branch, branch_w)| {
            half()
                .child(icon(branch_icon(&pill)))
                .when(branch_w > 0.0, |half| {
                    half.child(label(branch, branch_w, p.foreground))
                })
        });
        let card = pill.clone();
        Some(
            div()
                .id(WHERE_TRIGGER)
                .relative()
                .role(gpui::Role::Button)
                .aria_label(text(&pill, "ariaLabel"))
                .aria_expanded(open)
                .flex()
                .items_center()
                .flex_shrink_0()
                .rounded_full()
                .border(px(BORDER * s))
                .border_color(p.foreground.opacity(0.1))
                .text_size(px(13.0 * s))
                .line_height(px(19.0 * s))
                .whitespace_nowrap()
                .chat_cursor_pointer()
                .when(open, |pill| pill.bg(p.foreground.opacity(0.08)))
                .hover(|style| style.bg(p.foreground.opacity(0.06)))
                .child(left)
                .when_some(right, |pill, right| {
                    pill.child(
                        div()
                            .w(px(HAIRLINE))
                            .h(px(23.0 * s))
                            .flex_shrink_0()
                            .bg(p.foreground.opacity(0.1)),
                    )
                    .child(right)
                })
                // The hover card, A's card from the mockups; not while the details are open.
                .when(!open, |element| {
                    element.tooltip(move |window, cx| {
                        let card = card.clone();
                        crate::app::native_sidebar::tooltips::frosted_element_tooltip(
                            move |ink, _, _| {
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(card_header(&card, ink, 1.0))
                                    .child(card_rows(&card, ink, 1.0))
                            },
                            CARD_WIDTH,
                            window,
                            cx,
                        )
                    })
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.toggle_pill_menu(WHERE_MENU, bounds.get(), window, cx);
                }))
                .child(
                    canvas(
                        move |painted, _, _| measured_bounds.set(painted),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset(px(-s)),
                )
                .into_any_element(),
        )
    }

    /// The details menu's content height (unscaled), or `None` once the pill is gone, which closes
    /// an open menu (pill_menu.rs `pill_menu_changed`).
    pub(super) fn where_menu_height(&self) -> Option<f32> {
        let pill = self.where_pill()?;
        let rows = pill["card"]["rows"].as_array().map_or(0, Vec::len) as f32;
        let actions = pill["actions"]
            .as_array()
            .is_some_and(|actions| !actions.is_empty());
        Some(
            super::chat_pills::MENU_HEADER_HEIGHT
                + super::chat_pills::MENU_SECTION_GAP
                + rows * ROW_HEIGHT
                + 2.0 * ROWS_PAD_Y
                + if actions {
                    super::chat_pills::MENU_SECTION_GAP + ACTIONS_HEIGHT
                } else {
                    0.0
                },
        )
    }

    /// The details: the card's header and rows in the chat's own colours, then Copy branch and
    /// Open folder.
    pub(super) fn where_menu_body(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pill = self.where_pill()?.clone();
        let s = p.scale;
        let ink = CardInk {
            foreground: p.foreground,
            secondary: p.card_muted,
            hairline: p.foreground.opacity(0.08),
        };
        let header = div()
            .h(px(super::chat_pills::MENU_HEADER_HEIGHT * s))
            .flex()
            .flex_col()
            .justify_end()
            .px(px(4.0 * s))
            .child(card_header(&pill, ink, s).pb(px(6.0 * s)))
            .into_any_element();
        let rows = card_rows(&pill, ink, s).px(px(2.0 * s)).into_any_element();
        let group_id = {
            let key = if self.config.machine_id == crate::app::gx_chat::LOCAL_MACHINE_ID {
                ghostex_gx_core::ProjectKey::local(self.config.project_id.clone())
            } else {
                ghostex_gx_core::ProjectKey::remote(
                    self.config.machine_id.clone(),
                    self.config.project_id.clone(),
                )
            };
            key.to_sidebar_group_id()
        };
        let actions = pill["actions"].as_array().cloned().unwrap_or_default();
        let footer = (!actions.is_empty()).then(|| {
            div()
                .h(px(ACTIONS_HEIGHT * s))
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .px(px(4.0 * s))
                .children(actions.into_iter().map(|action| {
                    let mut command = action["action"].clone();
                    if command["do"] == "openFolder" {
                        command["groupId"] = json!(group_id);
                    }
                    let id = text(&action, "id");
                    div()
                        .id(gpui::ElementId::Name(format!("where-pill-{id}").into()))
                        .role(gpui::Role::Button)
                        .aria_label(text(&action, "label"))
                        .flex()
                        .items_center()
                        .flex_shrink_0()
                        .gap(px(6.0 * s))
                        .h(px(26.0 * s))
                        .px(px(10.0 * s))
                        .rounded(px(7.0 * s))
                        .bg(p.foreground.opacity(0.06))
                        .hover(|style| style.bg(p.foreground.opacity(0.1)))
                        .text_size(px(12.5 * s))
                        .text_color(p.foreground)
                        .chat_cursor_pointer()
                        .child(
                            svg()
                                .path(icon_path(action["icon"].as_str().unwrap_or("point")))
                                .size(px(14.0 * s))
                                .flex_shrink_0()
                                .text_color(p.card_muted),
                        )
                        .child(text(&action, "label"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.close_pill_menu(cx);
                            this.invoke(command.clone(), cx);
                        }))
                }))
                .into_any_element()
        });
        Some(super::chat_pills::pill_menu_column(
            "chat-where-pill-menu",
            "Where this chat runs",
            header,
            Some(rows),
            footer,
            p,
        ))
    }
}
