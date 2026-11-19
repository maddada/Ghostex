//! The where pill at the left end of the chat box's pill strip (pills_strip.rs): one pill split by
//! a hairline, the computer's own icon with the project on the left, the branch on the right.
//! Hovering it shows the card of where the session runs; a click opens the same details
//! above the box, where a click on a value copies it and the Folder row has Copy path and Open
//! folder. gx-chat-core words all of it (`wherePill`,
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

/// A detail row's height and the menu's padding around its rows (unscaled).
const ROW_HEIGHT: f32 = 24.0;
const ROWS_PAD_Y: f32 = 4.0;

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

/// The texts' widths at `tier` for the strip's fit: each measured width (a pixel over) under its
/// cap. The drawn labels take their text's own width under the same cap (`where_pill_element`).
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

/// The card's header: the project in bold, the computer's icon and name after it, muted.
///
/// CDXC:RemoteMachines 2026-11-19 DECISION:
/// User, of this header reading "This computer": "show the icon of the machine and also say its
/// actual name. Do not say 'this computer' ever." The computer is its icon (the pill's, the one its
/// computer tab draws) and its name, whichever computer it is.
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
            svg()
                .path(machine_icon(pill))
                .size(px(14.0 * s))
                .flex_shrink_0()
                .text_color(ink.secondary),
        )
        .child(
            div()
                .flex_shrink_0()
                .max_w(px(150.0 * s))
                .truncate()
                .text_color(ink.secondary)
                .child(text(card, "subtitle")),
        )
}

/// One detail row's line: an icon, the label, the value and its muted detail, or "Copied" with a
/// check in the detail's place for a moment after a click copied the value.
fn row_line(row: &Value, ink: CardInk, s: f32, copied: bool) -> gpui::Div {
    let icon = icon_path(row["icon"].as_str().unwrap_or("point"));
    let detail = row["detail"].as_str().map(str::to_string);
    let value = div()
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
                .child(text(row, "value")),
        );
    let value = if copied {
        value.child(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .gap(px(3.0 * s))
                .text_color(ink.secondary)
                .child(
                    svg()
                        .path(icon_path("check"))
                        .size(px(12.0 * s))
                        .text_color(ink.secondary),
                )
                .child("Copied"),
        )
    } else {
        value.when_some(detail, |line, detail| {
            line.child(
                div()
                    .flex_shrink_0()
                    .max_w(px(150.0 * s))
                    .truncate()
                    .text_color(ink.secondary)
                    .child(detail),
            )
        })
    };
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
                .child(text(row, "label")),
        )
        .child(value)
}

/// The hover card's rows, which only show.
fn card_rows(pill: &Value, ink: CardInk, s: f32) -> gpui::Div {
    let rows = pill["card"]["rows"].as_array().cloned().unwrap_or_default();
    div()
        .flex()
        .flex_col()
        .py(px(ROWS_PAD_Y * s))
        .children(rows.iter().map(|row| row_line(row, ink, s, false)))
}

/// How long "Copied" stays beside a value or on Copy path.
const COPIED_FOR: std::time::Duration = std::time::Duration::from_millis(1_400);

impl NativeChatView {
    /// The `wherePill` value, while the session has one.
    pub(super) fn where_pill(&self) -> Option<&Value> {
        Some(&self.snapshot["wherePill"]).filter(|pill| pill.is_object())
    }

    /// CDXC:Titlebar 2026-11-18 DECISION:
    /// User, of the header reading "Ghostex / Big Chungus": "no need for the name of the project
    /// in titlebar since we now show it in a pill above the composer", and for when, they picked
    /// "Only while the where pill shows it". The work area header leaves out "<project> /" while
    /// the chat it names has this pill on screen naming the project; terminal view, a hidden strip
    /// and the `WhereTier::Icon` tier (which drops the project at narrow widths) keep it
    /// (`workarea_header_where_pill_shows_project`). The app does not redraw for the chat's own
    /// changes, so a flip is told to it (`NativeChatEvent::WherePillProject`).
    pub(super) fn set_where_pill_shows_project(&mut self, shows: bool, cx: &mut Context<Self>) {
        if self.where_pill_shows_project != shows {
            self.where_pill_shows_project = shows;
            cx.emit(super::state::NativeChatEvent::WherePillProject);
        }
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
        let project = text(&pill, "project");
        let branch = pill["branch"].as_str().map(str::to_string);
        // CDXC:SessionChat 2026-11-18 WHY:
        // User, of "ghost… | ma…" for "ghostex | main": "why the pill is truncated now?". Each
        // label was exactly as wide as its measured text, so any measure short of the drawn text (a
        // picked app font measured as the chat font, fonts.rs `label_width`) cut even a short
        // name. A label takes its text's own width and only its tier's cap cuts it.
        let (project_w, branch_w) = tier.caps();
        let branch_w = branch.as_ref().map(|_| branch_w);
        let open = self.chat_menu_is_open(WHERE_TRIGGER);
        let bounds = self.pill_bounds.cell(WHERE_MENU);
        let measured_bounds = bounds.clone();
        let label = |content: String, cap: f32, color: Hsla| {
            div()
                .max_w(px(cap * s))
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
        Some(
            super::chat_pills::MENU_HEADER_HEIGHT
                + super::chat_pills::MENU_SECTION_GAP
                + rows * ROW_HEIGHT
                + 2.0 * ROWS_PAD_Y,
        )
    }

    /// Shows "Copied" for `id` (a row's id, or `copyPath`) for a moment.
    fn note_where_copied(&mut self, id: String, cx: &mut Context<Self>) {
        self.where_copied = Some(id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                if this.where_copied.as_deref() == Some(id.as_str()) {
                    this.where_copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// CDXC:SessionChat 2026-11-18 DECISION:
    /// User: "remove the buttons at the bottom, instead make clicking on different values there copy
    /// them also make folder have 2 buttons next to it for copy path and open folder". The details
    /// are the card's header and rows in the chat's own colours; a row with something to copy
    /// lights under the pointer and a click copies it, saying "Copied" in its detail's place, and
    /// the Folder row ends in Copy path and Open folder icon buttons. The menu stays open.
    /// Open folder reveals the folder in the system file manager (Finder, Explorer), so the core
    /// leaves that button out for a session on another computer: only Copy path is drawn there.
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
        let copied = self.where_copied.clone();
        let rows = pill["card"]["rows"].as_array().cloned().unwrap_or_default();
        let rows = div()
            .flex()
            .flex_col()
            .py(px(ROWS_PAD_Y * s))
            .px(px(2.0 * s))
            .children(rows.into_iter().map(|row| {
                let id = text(&row, "id");
                let buttons = row["buttons"].as_array().cloned().unwrap_or_default();
                let line = row_line(&row, ink, s, copied.as_deref() == Some(id.as_str()));
                let line = if row["copy"].as_str().is_some() {
                    let row_id = id.clone();
                    line.id(gpui::ElementId::Name(format!("where-pill-row-{id}").into()))
                        .flex_1()
                        .min_w_0()
                        .rounded(px(6.0 * s))
                        .role(gpui::Role::Button)
                        .aria_label(format!("Copy {}", text(&row, "label").to_lowercase()))
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.foreground.opacity(0.06)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.invoke(
                                json!({"type": "wherePill", "do": "copy", "row": row_id}),
                                cx,
                            );
                            this.note_where_copied(row_id.clone(), cx);
                        }))
                        .into_any_element()
                } else {
                    line.flex_1().min_w_0().into_any_element()
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0 * s))
                    .child(line)
                    .children(buttons.into_iter().map(|button| {
                        let mut command = button["action"].clone();
                        let button_id = text(&button, "id");
                        let opens = command["do"] == "openFolder";
                        if opens {
                            command["groupId"] = json!(group_id);
                        }
                        let done = !opens && copied.as_deref() == Some(button_id.as_str());
                        let label = text(&button, "label");
                        let icon = if done {
                            icon_path("check")
                        } else {
                            icon_path(button["icon"].as_str().unwrap_or("point"))
                        };
                        div()
                            .id(gpui::ElementId::Name(
                                format!("where-pill-{button_id}").into(),
                            ))
                            .role(gpui::Role::Button)
                            .aria_label(if done {
                                "Copied".to_string()
                            } else {
                                label.clone()
                            })
                            .size(px(ROW_HEIGHT * s))
                            .flex()
                            .items_center()
                            .justify_center()
                            .flex_shrink_0()
                            .rounded(px(6.0 * s))
                            .hover(|style| style.bg(p.foreground.opacity(0.08)))
                            .chat_cursor_pointer()
                            .child(svg().path(icon).size(px(14.0 * s)).text_color(p.card_muted))
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(label.clone())
                                    .build(window, cx)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if opens {
                                    this.close_pill_menu(cx);
                                    this.invoke(command.clone(), cx);
                                } else {
                                    this.invoke(command.clone(), cx);
                                    this.note_where_copied(button_id.clone(), cx);
                                }
                            }))
                    }))
            }))
            .into_any_element();
        Some(super::chat_pills::pill_menu_column(
            "chat-where-pill-menu",
            "Where this chat runs",
            header,
            Some(rows),
            None,
            p,
        ))
    }
}
