//! The bar on top of a new chat's message box: one borderless breadcrumb, project › computer ›
//! where › detail, drawn from the document's `worktreeTray` key. gx-chat-core decides when it
//! shows, what it offers, what is picked, and builds the where and detail parts with their menus
//! (`packages/gx-chat-core/src/menus/draft_worktree.rs`); `tray_layout.rs` decides one row or a
//! two-row grid; this file measures the parts, draws them and opens their menus. The phone draws
//! the same bar in `apps/mobile/app/src/chat/native/composer/WorktreeTray.tsx`.
//!
//! The project and computer parts, and the base branch and worktree lists, open the search card
//! in `draft_project_picker.rs`; the Cloud Box boot card is
//! `box_picker.rs`; the hotkeys that open each part are `tray_hotkeys.rs`.
//!
//! CDXC:Worktrees 2026-10-31 DECISION:
//! User (Q828): the tray sits on top of the message box, attached to it like a tab, while the chat is
//! new.
//!
//! CDXC:Worktrees 2026-11-04 DECISION:
//! User (Q1227 "D. One breadcrumb", "dont have a border around the breadcrumb"): the chips and the
//! 4-way switch are gone; the bar reads as one path whose parts open their own menus, with no border
//! of their own (a hover wash shows what can be clicked). A part cut short shows its full words in
//! its tooltip, with its hotkey (Q1228).
use super::tray_layout::{NARROW_WIDTH, PartSize, TrayLayout, tray_layout};
use super::{appearance::ChatAppearance, composer::hotkey_chord, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, Window, div, px, rgb, svg,
};
use gpui_component::ElementExt as _;
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;

const WHERE_TRIGGER: &str = "chat-worktree-where";
const DETAIL_TRIGGER: &str = "chat-worktree-detail";
const CLOUD_TRIGGER: &str = "chat-cloud-box-picker";
const FAILED_COLOR: u32 = 0xef4444;
const STAR_COLOR: u32 = 0xf5b301;
const MENU_WIDTH: f32 = 280.0;
/// The message box's corner radius (`composer.rs`), which the tray is inset by.
const COMPOSER_RADIUS: f32 = 22.0;
/// The gap the composer's footer leaves between the tray and the box (`composer.rs`).
const FOOTER_GAP: f32 = 8.0;
/// The tray's own padding at its sides.
const TRAY_PADDING: f32 = 8.0;
/// A part: its height, the hover wash's inset at each side, its icon and the gap after it.
const PART_HEIGHT: f32 = 24.0;
const PART_PADDING: f32 = 6.0;
const ICON: f32 = 13.0;
const GAP: f32 = 5.0;
const FONT: f32 = 13.0;
/// The › between parts, with its margins.
const SEPARATOR: f32 = 16.0;
/// The least a cut part keeps of its words.
const MIN_WORDS: f32 = 36.0;

/// What a part does when it is clicked or its hotkey is pressed.
#[derive(Clone)]
enum PartAction {
    /// Only shown (This folder's branch, a computer with nowhere else to go).
    None,
    /// The project search, or the computers that have the project (`draft_project_picker.rs`).
    Place { machines: bool },
    /// The base branch or worktree list in the search card (`draft_project_picker.rs`).
    List {
        kind: super::draft_project_picker::DraftListKind,
        rows: Vec<Value>,
    },
    /// The chat menu with these rows, from the part.
    Menu {
        trigger: &'static str,
        rows: Vec<Value>,
        width: f32,
    },
}

#[derive(Clone)]
struct Part {
    id: &'static str,
    glyph: String,
    star: bool,
    prefix: Option<String>,
    label: String,
    muted: bool,
    tooltip: String,
    hotkey: Option<&'static str>,
    action: PartAction,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

/// The chat menu's rows with the core's `glyph` turned into the desktop's icon path.
fn menu_rows(rows: &Value) -> Vec<Value> {
    rows.as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let mut row = row.clone();
            if let Some(glyph) = row["glyph"].as_str().map(str::to_owned) {
                row["iconPath"] = json!(format!("titlebar/{glyph}.svg"));
            }
            row
        })
        .collect()
}

impl NativeChatView {
    /// Whether the bar is on screen (the hotkeys act only then).
    pub(super) fn worktree_tray_shown(&self) -> bool {
        self.snapshot
            .get("worktreeTray")
            .is_some_and(Value::is_object)
    }

    /// The breadcrumb's parts, the place row (project, computer) and the where row (where,
    /// detail).
    fn tray_parts(&self, tray: &Value, cx: &Context<Self>) -> [Vec<Part>; 2] {
        let mut place = Vec::new();
        if let Some(name) = tray["projectName"].as_str() {
            place.push(Part {
                id: "chat-draft-project",
                glyph: "folder".into(),
                star: false,
                prefix: None,
                label: name.to_owned(),
                muted: false,
                tooltip: format!("Project: {name}"),
                hotkey: Some("newChatPickProject"),
                action: PartAction::Place { machines: false },
                bounds: self.draft_project_chip_bounds.clone(),
            });
            if let Some((label, local, pickable)) = self.draft_machine_part(cx) {
                place.push(Part {
                    id: "chat-draft-machine",
                    glyph: if local { "device-desktop" } else { "server" }.into(),
                    star: false,
                    prefix: None,
                    tooltip: format!("Computer: {label}"),
                    label,
                    muted: !pickable,
                    hotkey: pickable.then_some("newChatPickComputer"),
                    action: if pickable {
                        PartAction::Place { machines: true }
                    } else {
                        PartAction::None
                    },
                    bounds: self.draft_machine_chip_bounds.clone(),
                });
            }
        }
        let mut choice = Vec::new();
        let place_of = &tray["where"];
        if let Some(label) = place_of["label"].as_str() {
            choice.push(Part {
                id: WHERE_TRIGGER,
                glyph: place_of["glyph"].as_str().unwrap_or("folder-open").into(),
                star: false,
                prefix: None,
                label: label.to_owned(),
                muted: false,
                tooltip: format!("Where it runs: {label}"),
                hotkey: Some("newChatPickWhere"),
                action: PartAction::Menu {
                    trigger: WHERE_TRIGGER,
                    rows: menu_rows(&place_of["menu"]),
                    width: MENU_WIDTH,
                },
                bounds: self.worktree_tray_where_bounds.clone(),
            });
        }
        let detail = &tray["detail"];
        if let Some(label) = detail["label"].as_str() {
            let cloud = detail["kind"] == "cloudBox";
            let has_menu = detail["menu"].is_array();
            let list_kind = match detail["kind"].as_str() {
                Some("baseBranch") => Some(super::draft_project_picker::DraftListKind::BaseBranch),
                Some("worktree") => Some(super::draft_project_picker::DraftListKind::Worktree),
                _ => None,
            };
            choice.push(Part {
                id: if cloud { CLOUD_TRIGGER } else { DETAIL_TRIGGER },
                glyph: detail["glyph"].as_str().unwrap_or("git-branch").into(),
                star: detail["star"] == true,
                prefix: detail["prefix"].as_str().map(str::to_owned),
                label: label.to_owned(),
                muted: !has_menu || detail["placeholder"] == true,
                tooltip: detail["tooltip"].as_str().unwrap_or(label).to_owned(),
                hotkey: has_menu.then_some("newChatPickBranch"),
                action: if let (true, Some(kind)) = (has_menu, list_kind) {
                    PartAction::List {
                        kind,
                        rows: menu_rows(&detail["menu"]),
                    }
                } else if has_menu {
                    PartAction::Menu {
                        trigger: if cloud { CLOUD_TRIGGER } else { DETAIL_TRIGGER },
                        rows: menu_rows(&detail["menu"]),
                        width: detail["menuWidth"]
                            .as_f64()
                            .map_or(MENU_WIDTH, |w| w as f32),
                    }
                } else {
                    PartAction::None
                },
                bounds: self.worktree_tray_detail_bounds.clone(),
            });
        }
        [place, choice]
    }

    /// Opens what a part opens, from a click or its hotkey.
    fn open_tray_part(
        &mut self,
        action: &PartAction,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            PartAction::None => {}
            PartAction::Place { machines } => self.toggle_draft_place_picker(*machines, window, cx),
            PartAction::List { kind, rows } => {
                self.toggle_draft_list_picker(*kind, rows.clone(), bounds, window, cx)
            }
            PartAction::Menu {
                trigger,
                rows,
                width,
            } => {
                if self.chat_menu_toggled_shut(trigger, cx) {
                    return;
                }
                // The where and Cloud Box menus open upward from their part with their left edges
                // lined up, as a composer menu does, instead of the side panel's place that put
                // them below the bar over the chat box (CDXC:Worktrees 2026-11-10 in
                // draft_project_picker.rs).
                self.menu_side = Some(super::option_menu::MenuSide::Left);
                self.show_chat_menu(rows.clone(), bounds, *width, window, cx);
            }
        }
    }

    /// A bar hotkey (`tray_hotkeys.rs`): opens its part's menu. A part the bar does not have
    /// (no other computer, nothing to pick next) does nothing.
    pub(super) fn run_tray_hotkey(
        &mut self,
        action_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tray) = self.snapshot.get("worktreeTray").cloned() else {
            return;
        };
        if tray["busy"] == true {
            return;
        }
        let parts = self.tray_parts(&tray, cx);
        let Some(part) = parts
            .iter()
            .flatten()
            .find(|part| part.hotkey == Some(action_id))
            .cloned()
        else {
            return;
        };
        self.open_tray_part(&part.action, part.bounds.get(), window, cx);
    }

    /// A part's natural width and the least it may be cut to, at the chat's zoom.
    fn measure_tray_part(&self, part: &Part, p: &ChatAppearance, window: &Window) -> PartSize {
        let s = p.scale;
        let mut style = window.text_style();
        style.font_family = p.font.clone().into();
        let text_width = |text: &str| {
            window
                .text_system()
                .shape_line(
                    text.to_owned().into(),
                    px(FONT * s),
                    &[style.to_run(text.len())],
                    None,
                )
                .width
                .as_f32()
        };
        let chrome = (PART_PADDING * 2.0 + ICON + GAP) * s
            + part
                .prefix
                .as_deref()
                .map_or(0.0, |prefix| text_width(prefix) + GAP * s);
        // A pixel of slack so a part measured whole is never drawn cut by rounding.
        let words = text_width(&part.label).ceil() + 1.0;
        PartSize {
            natural: chrome + words,
            min: chrome + words.min(MIN_WORDS * s),
        }
    }

    fn render_tray_part(
        &self,
        part: Part,
        width: f32,
        natural: f32,
        p: &ChatAppearance,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let clickable = !busy && !matches!(part.action, PartAction::None);
        let cut = width + 0.5 < natural;
        let bounds = part.bounds.clone();
        let tooltip = part.tooltip.clone();
        let hotkey = part.hotkey;
        let action = part.action.clone();
        div()
            .on_prepaint(move |painted, _, _| bounds.set(painted))
            .id(part.id)
            .role(gpui::Role::Button)
            .aria_label(part.tooltip.clone())
            .flex_shrink_0()
            .when(cut, |this| this.w(px(width)))
            .h(px(PART_HEIGHT * s))
            .px(px(PART_PADDING * s))
            .flex()
            .items_center()
            .gap(px(GAP * s))
            .rounded(px(6.0 * s))
            .text_size(px(FONT * s))
            .when(clickable, |this| {
                let trigger = part.bounds.clone();
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.border))
                    .on_click(cx.listener(move |chat, _, window, cx| {
                        chat.open_tray_part(&action, trigger.get(), window, cx);
                    }))
            })
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone())
                    .keys(hotkey_chord(hotkey))
                    .build(window, cx)
            })
            .child(
                svg()
                    .path(format!("titlebar/{}.svg", part.glyph))
                    .size(px(ICON * s))
                    .flex_shrink_0()
                    .text_color(if part.star {
                        rgb(STAR_COLOR).into()
                    } else {
                        p.muted
                    }),
            )
            .when_some(part.prefix, |this, prefix| {
                this.child(div().flex_shrink_0().text_color(p.muted).child(prefix))
            })
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(if part.muted { p.muted } else { p.primary })
                    .child(part.label),
            )
            .into_any_element()
    }

    fn tray_separator(p: &ChatAppearance) -> AnyElement {
        div()
            .flex_shrink_0()
            .w(px(SEPARATOR * p.scale))
            .flex()
            .justify_center()
            .child(
                svg()
                    .path("titlebar/chevron-right.svg")
                    .size(px(12.0 * p.scale))
                    .text_color(p.muted),
            )
            .into_any_element()
    }

    /// The breadcrumb, placed by `tray_layout`.
    fn render_tray_breadcrumb(
        &self,
        tray: &Value,
        p: &ChatAppearance,
        busy: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let parts = self.tray_parts(tray, cx);
        if parts.iter().all(Vec::is_empty) {
            return None;
        }
        let sizes = parts.clone().map(|row| {
            row.iter()
                .map(|part| self.measure_tray_part(part, p, window))
                .collect::<Vec<_>>()
        });
        // The room inside the tray: the box's width less the tray's inset, padding and border.
        let composer = self.composer_bounds.get().size.width.as_f32();
        let available = if composer > 0.0 {
            composer - (COMPOSER_RADIUS + TRAY_PADDING) * 2.0 * s - 2.0
        } else {
            0.0
        };
        let TrayLayout {
            two_rows,
            centered,
            widths,
            columns,
        } = tray_layout(&sizes, SEPARATOR * s, available, NARROW_WIDTH * s);
        let mut drawn = Vec::new();
        for (row, (parts, widths)) in parts.into_iter().zip(widths).enumerate() {
            let mut cells = Vec::new();
            for (index, (part, width)) in parts.into_iter().zip(widths).enumerate() {
                if index > 0 || (!two_rows && row > 0 && !drawn_is_empty(&drawn, &cells)) {
                    cells.push(Self::tray_separator(p));
                }
                let natural = sizes[row][index].natural;
                let element = self.render_tray_part(part, width, natural, p, busy, cx);
                cells.push(if two_rows {
                    div()
                        .flex_shrink_0()
                        .w(px(columns[index]))
                        .flex()
                        .child(element)
                        .into_any_element()
                } else {
                    element
                });
            }
            drawn.push(cells);
        }
        let breadcrumb = if two_rows {
            let grid_width = columns.iter().sum::<f32>()
                + SEPARATOR * s * (columns.len().saturating_sub(1)) as f32;
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(2.0 * s))
                .when(centered, |this| this.items_center())
                .children(
                    drawn
                        .into_iter()
                        .filter(|cells| !cells.is_empty())
                        .map(|cells| {
                            div()
                                .w(px(grid_width))
                                .flex()
                                .items_center()
                                .children(cells)
                        }),
                )
        } else {
            div()
                .w_full()
                .flex()
                .items_center()
                .when(centered, |this| this.justify_center())
                .children(drawn.into_iter().flatten())
        };
        Some(
            breadcrumb
                .id("chat-worktree-breadcrumb")
                .role(gpui::Role::Toolbar)
                .aria_label("Where this chat runs")
                .when(busy, |this| this.opacity(0.6))
                .into_any_element(),
        )
    }

    pub(crate) fn render_worktree_tray(
        &self,
        p: &ChatAppearance,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tray = self
            .snapshot
            .get("worktreeTray")
            .filter(|tray| tray.is_object())?
            .clone();
        let s = p.scale;
        let busy = tray["busy"] == true;
        let breadcrumb = self.render_tray_breadcrumb(&tray, p, busy, window, cx);
        let status = &tray["status"];
        let status_line = match status["state"].as_str() {
            Some("creating") | Some("opened") => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0 * s))
                    .min_w_0()
                    .px(px(PART_PADDING * s))
                    .text_size(px(12.5 * s))
                    .text_color(p.muted)
                    .child(
                        svg()
                            .path("titlebar/loader2.svg")
                            .size(px(13.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(status["label"].as_str().unwrap_or_default().to_owned()),
                    )
                    .into_any_element(),
            ),
            Some("failed") => {
                let message = status["message"].as_str().unwrap_or_default().to_owned();
                // A line of its own, like the boot card, so the reason is never cut to a word.
                Some(
                    div()
                        .id("chat-worktree-failure")
                        .flex()
                        .w_full()
                        .items_center()
                        .gap(px(6.0 * s))
                        .min_w_0()
                        .px(px(PART_PADDING * s))
                        .text_size(px(12.5 * s))
                        .text_color(rgb(FAILED_COLOR))
                        .child(
                            svg()
                                .path("titlebar/alert-triangle.svg")
                                .size(px(13.0 * s))
                                .flex_shrink_0()
                                .text_color(rgb(FAILED_COLOR)),
                        )
                        .child(
                            div()
                                .id("chat-worktree-failure-message")
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(message.clone())
                                .tooltip(move |window, cx| {
                                    gpui_component::tooltip::Tooltip::new(message.clone())
                                        .build(window, cx)
                                }),
                        )
                        .child(
                            div()
                                .id("chat-worktree-retry")
                                .role(gpui::Role::Button)
                                .aria_label("Try again")
                                .flex_shrink_0()
                                .h(px(PART_HEIGHT * s))
                                .px(px(10.0 * s))
                                .flex()
                                .items_center()
                                .rounded(px(6.0 * s))
                                .text_color(p.primary)
                                .chat_cursor_pointer()
                                .hover(|style| style.bg(p.border))
                                .child("Try again")
                                .on_click(cx.listener(|chat, _, window, cx| {
                                    chat.submit("send", window, cx);
                                })),
                        )
                        .into_any_element(),
                )
            }
            _ => None,
        };
        /*
        CDXC:Worktrees 2026-10-31 DECISION:
        User: "the bar above the composer must be transparent like it". The tray is part of the
        message box: the box's own fill and border (`composer_background`, which window glass turns
        into the bubble's wash), never the pane's solid background. It ends exactly on the box's top
        edge instead of reaching over its border, because two translucent fills stacked there read
        darker; the box's top border is the seam. It is inset by the box's corner radius so its sides
        meet the straight part of that edge, and the parts on it draw no fill of their own.
        */
        Some(
            div()
                .w_full()
                .relative()
                .px(px(COMPOSER_RADIUS * s))
                .mb(px(-FOOTER_GAP * s))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.0 * s))
                        .px(px(TRAY_PADDING * s))
                        .pt(px(5.0 * s))
                        .pb(px(6.0 * s))
                        .rounded_t(px(12.0 * s))
                        .border_1()
                        .border_b_0()
                        .border_color(p.composer_border)
                        .bg(p.composer_background)
                        .children(breadcrumb)
                        .children(status_line)
                        .when(status["card"].is_object(), |tray| {
                            tray.child(self.render_cloud_box_card(&status["card"], p))
                        }),
                )
                .into_any_element(),
        )
    }
}

/// Whether nothing has been drawn before this row (so a one-row breadcrumb puts no › before its
/// first part when the place row is empty).
fn drawn_is_empty(drawn: &[Vec<AnyElement>], cells: &[AnyElement]) -> bool {
    drawn.iter().all(Vec::is_empty) && cells.is_empty()
}
