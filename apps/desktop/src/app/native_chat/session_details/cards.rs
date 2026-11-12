//! The panel's cards, one per section of the core's `sessionDetails`, in its order. Every label is
//! the core's; a row that does something sends the action the core put on it.

use super::super::{
    appearance::ChatAppearance, cursor::ChatCursor as _, disclosure_motion::FoldFrame,
    state::NativeChatView,
};
use crate::app::helpers::ThrottledAnimationExt;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px, rgb, svg,
};
use serde_json::Value;
use std::time::Duration;

/// Unscaled measures, shared with `panel_height` so the floating card knows its size up front.
const CARD_GAP: f32 = 8.0;
const CARD_PADDING_Y: f32 = 8.0;
const HEADER_HEIGHT: f32 = 24.0;
const ROW_HEIGHT: f32 = 24.0;
/// A task row (22) and the gap the pill leaves under it (2), as `agent_tasks.rs` draws them.
const TASK_ROW_PITCH: f32 = 24.0;
const SUBTITLE_HEIGHT: f32 = 20.0;
const BAR_HEIGHT: f32 = 14.0;
const SUMMARY_HEIGHT: f32 = 24.0;
const NOTE_HEIGHT: f32 = 20.0;

/// The disclosure keys of the panel's folds (`disclosure_motion.rs`).
const MESSAGES_KEY: &str = "session-details:context:messages";
const SKILLS_ALL_KEY: &str = "session-details:skills:all";
const BACKGROUND_FINISHED_KEY: &str = "session-details:background:finished";

fn card_key(id: &str) -> String {
    format!("session-details:{id}")
}

fn str_of(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

/// The card has something under its title to fold; the core says `foldable: false` for a Context
/// card whose usage line and bar are all it holds.
fn foldable(section: &Value) -> bool {
    section["foldable"] != false
}

/// The card's body is not drawn: the user folded it, or it has none to show.
fn shut(section: &Value) -> bool {
    section["collapsed"] == true || !foldable(section)
}

fn len(value: &Value, key: &str) -> usize {
    value[key].as_array().map_or(0, Vec::len)
}

/// The colour of a context category (`category_color` in the core) or a connector's status.
fn swatch(key: &str, p: &ChatAppearance) -> Hsla {
    let color = |hex: u32| Hsla::from(rgb(hex));
    match key {
        "messages" => color(0x4f8cff),
        "memory" => color(0x2fbf8f),
        "skills" => color(0xb48cff),
        "mcp" => color(0xe0a43a),
        "agents" => color(0xe879a6),
        "since" => color(0x4f8cff).opacity(0.45),
        "buffer" => p.muted.opacity(0.35),
        "free" => p.border,
        "connected" | "done" => color(0x3fb950),
        "pending" | "needsAuth" | "running" => color(0xf5a524),
        "failed" => color(0xf0605a),
        _ => p.muted,
    }
}

/// The floating card's content height for `details`, in unscaled pixels. `open` says how far a
/// fold is open (0 shut, 1 open) given its key and whether it is open now, so the card's window
/// follows a fold that is still easing instead of clipping it.
pub(super) fn panel_height(details: &Value, open: &mut dyn FnMut(&str, bool) -> f32) -> f32 {
    let mut total = 0.0;
    let sections = details["sections"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            total += CARD_GAP;
        }
        total += CARD_PADDING_Y * 2.0 + HEADER_HEIGHT;
        let id = section["id"].as_str().unwrap_or_default();
        // Context's usage line and bar stay under its title, folded or not.
        if id == "context" {
            total += SUMMARY_HEIGHT + BAR_HEIGHT;
        }
        let fold = open(&card_key(id), !shut(section));
        if fold <= 0.0 {
            continue;
        }
        let sketched = section["skeletonRows"].as_u64().unwrap_or(0) as f32 * ROW_HEIGHT;
        if section["loading"] == true {
            total += sketched * fold;
            continue;
        }
        let body = sketched
            + match id {
                "context" => {
                    let messages = len(section, "messages") as f32
                        * open(MESSAGES_KEY, section["messagesOpen"] == true);
                    let status = if section["refresh"].is_object() {
                        NOTE_HEIGHT
                    } else {
                        0.0
                    };
                    (len(section, "rows") + len(section, "deferred")) as f32 * ROW_HEIGHT
                        + messages * ROW_HEIGHT
                        + status
                }
                "connectors" => section["groups"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|group| SUBTITLE_HEIGHT + len(group, "rows").max(1) as f32 * ROW_HEIGHT)
                    .sum(),
                "skills" => {
                    let used = if section["usedEmpty"].is_string() {
                        len(section, "used").max(1)
                    } else {
                        len(section, "used")
                    };
                    let all = len(section, "all") as f32
                        * open(SKILLS_ALL_KEY, section["allOpen"] == true);
                    (used + 1) as f32 * ROW_HEIGHT + all * ROW_HEIGHT
                }
                "background" => {
                    let finished = len(section, "finished") as f32
                        * open(BACKGROUND_FINISHED_KEY, section["finishedOpen"] == true);
                    let toggle = usize::from(len(section, "finished") > 0);
                    (len(section, "running") + toggle) as f32 * ROW_HEIGHT + finished * ROW_HEIGHT
                }
                "changes" => ROW_HEIGHT,
                "tasks" => {
                    let fold = usize::from(section["foldAction"].is_object());
                    (len(section, "rows") + fold) as f32 * ROW_HEIGHT
                }
                _ => len(section, "rows") as f32 * ROW_HEIGHT,
            };
        total += body * fold;
    }
    if sections.is_empty() {
        total = NOTE_HEIGHT * 2.0;
    }
    total + 4.0
}

/// The chevron of a fold: at rest the right or down glyph, while the fold moves the right chevron
/// turned a quarter in step with the body (`frame.log`, 0 shut to 1 open).
fn fold_chevron(open: bool, motion: Option<FoldFrame>, size: f32, color: Hsla) -> AnyElement {
    let glyph = svg().size(px(size)).flex_shrink_0().text_color(color);
    match motion {
        Some(frame) => glyph
            .path("titlebar/chevron-right.svg")
            .with_transformation(gpui::Transformation::rotate(gpui::radians(
                std::f32::consts::FRAC_PI_2 * frame.log.clamp(0.0, 1.0),
            )))
            .into_any_element(),
        None => glyph
            .path(if open {
                "titlebar/chevron-down.svg"
            } else {
                "titlebar/chevron-right.svg"
            })
            .into_any_element(),
    }
}

/// One skeleton text bar, `width` of its row.
fn skeleton_bar(width: f32, p: &ChatAppearance) -> AnyElement {
    let (tint, _, _) = super::super::transcript_reveal::skeleton_look(p);
    div()
        .w(gpui::relative(width))
        .h(px(9.0 * p.scale))
        .rounded_full()
        .bg(tint)
        .into_any_element()
}

/// The card's `skeletonRows` rows that have not arrived yet: an icon slot and a text bar each,
/// pulsing like the transcript's skeleton, and still under Reduce Motion.
fn skeleton_rows(id: &str, section: &Value, p: &ChatAppearance) -> AnyElement {
    const WIDTHS: [f32; 5] = [0.62, 0.8, 0.48, 0.72, 0.56];
    let s = p.scale;
    let (tint, period, lowest) = super::super::transcript_reveal::skeleton_look(p);
    let rows = section["skeletonRows"].as_u64().unwrap_or(0) as usize;
    let column = div()
        .id(SharedString::from(format!("session-details-skeleton-{id}")))
        .role(gpui::Role::Status)
        .aria_label("Loading…")
        .flex()
        .flex_col()
        .min_w_0()
        .children((0..rows).map(|row| {
            div()
                .h(px(ROW_HEIGHT * s))
                .flex()
                .items_center()
                .gap(px(8.0 * s))
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(12.0 * s))
                        .rounded(px(3.0 * s))
                        .bg(tint),
                )
                .child(skeleton_bar(WIDTHS[row % WIDTHS.len()], p))
        }));
    if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
        return column.into_any_element();
    }
    column
        .with_throttled_animation(
            SharedString::from(format!("session-details-skeleton-pulse-{id}")),
            period,
            move |column, frame| {
                let dip = gpui::ease_in_out(if frame < 0.5 {
                    frame * 2.0
                } else {
                    (1.0 - frame) * 2.0
                });
                column.opacity(1.0 - (1.0 - lowest) * dip)
            },
        )
        .into_any_element()
}

/// CDXC:SessionDetails 2026-11-12 DECISION:
/// User: the Context card opens folded and, folded, still shows its usage at a glance. Its usage
/// line (percentage and tokens) and the overall usage bar sit under the title outside the fold;
/// the fold holds the breakdown. "Main bar" is that overall bar, the only bar the card draws.
fn context_summary(section: &Value, details: &Value, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    let mut column = div().flex().flex_col().min_w_0();
    let reading = section["percentLabel"].is_null() && details["loading"] == true;
    let percent = section["percentLabel"]
        .as_str()
        .unwrap_or("No usage reported yet")
        .to_string();
    column = column.child(
        div()
            .flex()
            .items_center()
            .h(px(SUMMARY_HEIGHT * s))
            .gap(px(6.0 * s))
            .map(|line| {
                if reading {
                    return line.child(skeleton_bar(0.4, p));
                }
                line.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.foreground)
                        .child(percent),
                )
            })
            .when_some(section["tokensLabel"].as_str(), |line, tokens| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.card_muted)
                        .child(tokens.to_string()),
                )
            }),
    );
    let mut bar = div()
        .flex()
        .w_full()
        .h(px(5.0 * s))
        .rounded(px(3.0 * s))
        .overflow_hidden()
        .bg(p.border);
    for segment in section["segments"].as_array().into_iter().flatten() {
        let fraction = segment["fraction"].as_f64().unwrap_or(0.0) as f32;
        if fraction <= 0.0 {
            continue;
        }
        bar = bar.child(
            div()
                .h_full()
                .w(gpui::relative(fraction))
                .bg(swatch(segment["color"].as_str().unwrap_or_default(), p)),
        );
    }
    column = column.child(div().h(px(BAR_HEIGHT * s)).flex().items_center().child(bar));
    column.into_any_element()
}

impl NativeChatView {
    /// Every card, top to bottom.
    pub(super) fn session_details_cards(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        self.session_details.drew = true;
        let details = self.snapshot["sessionDetails"].clone();
        let mut column = div().flex().flex_col().gap(px(CARD_GAP * s)).min_w_0();
        if let Some(error) = details["error"].as_str() {
            column = column.child(
                div()
                    .text_size(px(12.0 * s))
                    .text_color(swatch("failed", p))
                    .child(error.to_string()),
            );
        }
        for section in details["sections"].as_array().into_iter().flatten() {
            column = column.child(self.session_details_card(section, &details, p, cx));
        }
        // A fold that is opening or closing needs the next frame, in whichever window draws it.
        if self.disclosure_motion.borrow().running() {
            window.request_animation_frame();
        }
        column.into_any_element()
    }

    fn session_details_card(
        &mut self,
        section: &Value,
        details: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = str_of(section, "id");
        let collapsed = shut(section);
        let key = card_key(&id);
        let motion = self.disclosure_frame(&key, !collapsed, cx);
        let header = self.card_header(section, !collapsed, motion, p, cx);
        let loading = section["loading"] == true;
        let summary = (id == "context").then(|| context_summary(section, details, p));
        // The core keeps a folded card's rows, so a closing card has its body to ease away.
        let body = (!collapsed || motion.is_some()).then(|| {
            let body = match id.as_str() {
                _ if loading => skeleton_rows(&id, section, p),
                "context" => self.context_body(section, p, cx),
                "tasks" => self.tasks_body(section, p, cx),
                "connectors" => self.connectors_body(section, p, cx),
                "skills" => self.skills_body(section, p, cx),
                "changes" => self.changes_body(section, p, cx),
                "background" => self.background_body(section, p, cx),
                _ => self.plain_rows(&id, section, p, cx),
            };
            self.disclosure_body_motion(&key, motion, 0.0, body)
        });
        div()
            .id(SharedString::from(format!("session-details-card-{id}")))
            .role(gpui::Role::Group)
            .aria_label(str_of(section, "title"))
            .flex()
            .flex_col()
            .min_w_0()
            .px(px(10.0 * s))
            .py(px(CARD_PADDING_Y * s))
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(p.border)
            .bg(p.card_background)
            .text_size(px(13.0 * s))
            .child(header)
            .children(summary)
            .children(body)
            .into_any_element()
    }

    /// The title folds the card; the trailing bit is the section's own (a count, Manage, refresh).
    fn card_header(
        &mut self,
        section: &Value,
        open: bool,
        motion: Option<FoldFrame>,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = str_of(section, "id");
        let toggle = serde_json::json!({"type": "sessionDetailsSection", "section": id});
        let trailing: Option<AnyElement> = match id.as_str() {
            "context" => section["refresh"]
                .is_object()
                .then(|| self.refresh_button(section, p, cx)),
            "connectors" => section["manage"].is_object().then(|| {
                let action = section["manage"].clone();
                self.link_button("session-details-manage", "Manage", action, p, cx)
            }),
            _ => section["meta"].as_str().map(|meta| {
                div()
                    .flex_shrink_0()
                    .text_size(px(11.5 * s))
                    .text_color(p.card_muted)
                    .child(meta.to_string())
                    .into_any_element()
            }),
        };
        div()
            .flex()
            .items_center()
            .h(px(HEADER_HEIGHT * s))
            .gap(px(6.0 * s))
            .min_w_0()
            .child(
                div()
                    .id(SharedString::from(format!("session-details-fold-{id}")))
                    .flex()
                    .items_center()
                    .gap(px(4.0 * s))
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0 * s))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.muted)
                            .child(str_of(section, "title")),
                    )
                    // A card with nothing to unfold (Context without a breakdown) has a plain title.
                    .when(foldable(section), |title| {
                        title
                            .role(gpui::Role::Button)
                            .aria_label(format!(
                                "{} {}",
                                if open { "Hide" } else { "Show" },
                                str_of(section, "title")
                            ))
                            .chat_cursor_pointer()
                            .child(fold_chevron(open, motion, 12.0 * s, p.card_muted))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.session_details_action(&toggle, cx);
                            }))
                    }),
            )
            .children(trailing)
            .into_any_element()
    }

    /// CDXC:SessionDetails 2026-10-25 DECISION:
    /// User: "move the refresh back up it looks weird here. just take it back where it was before last turn". Refresh Context Breakdown sits at the end of the Context card's title row; this supersedes the same day's move to the breakdown's status line.
    fn refresh_button(
        &mut self,
        section: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let refreshing = section["refreshing"] == true;
        let action = section["refresh"].clone();
        let glyph = svg()
            .path("titlebar/refresh.svg")
            .size(px(12.0 * s))
            .text_color(p.card_muted);
        let glyph = if refreshing && !crate::app::helpers::gpui_macos_reduce_motion_enabled() {
            glyph
                .with_throttled_animation(
                    "session-details-refresh-spin",
                    Duration::from_secs(1),
                    |glyph, progress| {
                        glyph.with_transformation(gpui::Transformation::rotate(gpui::percentage(
                            progress,
                        )))
                    },
                )
                .into_any_element()
        } else {
            glyph.into_any_element()
        };
        div()
            .id("session-details-refresh")
            .role(gpui::Role::Button)
            .aria_label("Refresh Context Breakdown")
            .flex_shrink_0()
            .size(px(20.0 * s))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.0 * s))
            .when(!refreshing, |button| {
                button
                    .chat_cursor_pointer()
                    .hover(|button| button.bg(p.foreground.opacity(0.06)))
            })
            .child(glyph)
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(if refreshing {
                    "Counting…"
                } else {
                    "Refresh Context Breakdown"
                })
                .build(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if !refreshing {
                    this.session_details_action(&action, cx);
                }
            }))
            .into_any_element()
    }

    fn link_button(
        &mut self,
        id: &'static str,
        label: &'static str,
        action: Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex_shrink_0()
            .px(px(6.0 * s))
            .rounded(px(5.0 * s))
            .text_size(px(11.5 * s))
            .text_color(p.card_muted)
            .chat_cursor_pointer()
            .hover(|button| {
                button
                    .bg(p.foreground.opacity(0.06))
                    .text_color(p.foreground)
            })
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.session_details_action(&action, cx);
            }))
            .into_any_element()
    }

    /// One row: an icon or a swatch, the text, a muted detail, and whatever the row does.
    #[allow(clippy::too_many_arguments)]
    fn detail_row(
        &mut self,
        key: String,
        leading: Option<AnyElement>,
        text: String,
        detail: Option<String>,
        tooltip: Option<String>,
        action: Option<Value>,
        dim: bool,
        link: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.detail_row_folding(
            key, leading, text, detail, tooltip, action, dim, link, None, p, cx,
        )
    }

    /// [`Self::detail_row`] with a fold's chevron right after the text (the Messages row).
    #[allow(clippy::too_many_arguments)]
    fn detail_row_folding(
        &mut self,
        key: String,
        leading: Option<AnyElement>,
        text: String,
        detail: Option<String>,
        tooltip: Option<String>,
        action: Option<Value>,
        dim: bool,
        link: bool,
        chevron: Option<AnyElement>,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let color = if dim { p.card_muted } else { p.foreground };
        let mut row = div()
            .id(SharedString::from(format!("session-details-row-{key}")))
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .h(px(ROW_HEIGHT * s))
            .min_w_0()
            .px(px(2.0 * s))
            .rounded(px(5.0 * s))
            .children(leading)
            .child(match chevron {
                None => div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(color)
                    .when(link, |text| text.underline())
                    .child(text.clone()),
                Some(chevron) => div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(4.0 * s))
                    .text_color(color)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .when(link, |text| text.underline())
                            .child(text.clone()),
                    )
                    .child(chevron),
            })
            .when_some(detail, |row, detail| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .text_color(p.card_muted)
                        .child(detail),
                )
            });
        if let Some(tooltip) = tooltip.filter(|tooltip| !tooltip.is_empty()) {
            row = row.tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            });
        }
        if let Some(action) = action {
            row = row
                .role(gpui::Role::Button)
                .aria_label(text)
                .chat_cursor_pointer()
                .hover(|row| row.bg(p.foreground.opacity(0.06)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.session_details_action(&action, cx);
                }));
        }
        row.into_any_element()
    }

    fn plain_rows(
        &mut self,
        id: &str,
        section: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut column = div().flex().flex_col().min_w_0();
        for (index, row) in section["rows"].as_array().into_iter().flatten().enumerate() {
            let icon = match row["icon"].as_str() {
                Some("laptop") => Some("titlebar/device-laptop.svg"),
                Some("box") => Some("titlebar/box.svg"),
                Some("folder") => Some("titlebar/folder.svg"),
                Some("branch") => Some("titlebar/git-branch.svg"),
                Some("user") => Some("titlebar/user.svg"),
                Some("memory") => Some("titlebar/cpu.svg"),
                _ if id == "memory" => Some("titlebar/file-text.svg"),
                _ => None,
            };
            let leading = icon.map(|icon| {
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted)
                    .into_any_element()
            });
            let text = match row["badge"].as_str() {
                Some(badge) => format!("{} ({badge})", str_of(row, "text")),
                None => str_of(row, "text"),
            };
            let action = row["action"].is_object().then(|| row["action"].clone());
            column = column.child(self.detail_row(
                format!("{id}-{index}"),
                leading,
                text,
                row["detail"].as_str().map(str::to_string),
                row["tooltip"].as_str().map(str::to_string),
                action,
                false,
                row["link"] == true,
                p,
                cx,
            ));
        }
        column.into_any_element()
    }

    fn context_body(
        &mut self,
        section: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut column = div().flex().flex_col().min_w_0();
        if section["skeletonRows"].as_u64().unwrap_or(0) > 0 {
            column = column.child(skeleton_rows("context", section, p));
        }
        let messages_open = section["messagesOpen"] == true;
        let messages_key = MESSAGES_KEY;
        let messages_motion = (len(section, "messages") > 0)
            .then(|| self.disclosure_frame(messages_key, messages_open, cx))
            .flatten();
        for (index, row) in section["rows"].as_array().into_iter().flatten().enumerate() {
            let id = str_of(row, "id");
            let color = swatch(row["color"].as_str().unwrap_or_default(), p);
            let leading = div()
                .flex_shrink_0()
                .size(px(9.0 * s))
                .rounded(px(2.0 * s))
                .bg(color)
                .into_any_element();
            let detail = match row["percent"].as_str() {
                Some(percent) => format!("{}  {percent}", str_of(row, "tokens")),
                None => str_of(row, "tokens"),
            };
            // Messages unfolds into what they are made of.
            let action = (id == "messages" && len(section, "messages") > 0)
                .then(|| serde_json::json!({"type": "sessionDetailsList", "list": "messages"}));
            let chevron = action
                .is_some()
                .then(|| fold_chevron(messages_open, messages_motion, 12.0 * s, p.card_muted));
            column = column.child(self.detail_row_folding(
                format!("context-{index}"),
                Some(leading),
                str_of(row, "label"),
                Some(detail),
                None,
                action,
                row["dim"] == true,
                false,
                chevron,
                p,
                cx,
            ));
            if id == "messages" && (messages_open || messages_motion.is_some()) {
                let mut lines = div().flex().flex_col().min_w_0();
                for (part, line) in section["messages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    lines = lines.child(
                        div()
                            .flex()
                            .items_center()
                            .h(px(ROW_HEIGHT * s))
                            .pl(px(19.0 * s))
                            .gap(px(8.0 * s))
                            .text_size(px(12.0 * s))
                            .text_color(p.card_muted)
                            .id(SharedString::from(format!(
                                "session-details-message-{part}"
                            )))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(str_of(line, "label")),
                            )
                            .child(div().flex_shrink_0().child(str_of(line, "tokens"))),
                    );
                }
                column = column.child(self.disclosure_body_motion(
                    messages_key,
                    messages_motion,
                    0.0,
                    lines.into_any_element(),
                ));
            }
        }
        for (index, row) in section["deferred"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            column = column.child(
                self.detail_row(
                    format!("context-deferred-{index}"),
                    Some(
                        div()
                            .flex_shrink_0()
                            .size(px(9.0 * s))
                            .rounded(px(2.0 * s))
                            .border_1()
                            .border_color(p.card_muted)
                            .into_any_element(),
                    ),
                    str_of(row, "label"),
                    Some(str_of(row, "tokens")),
                    Some(
                        "Loaded only when Claude needs them; not in the context until then."
                            .to_string(),
                    ),
                    None,
                    true,
                    false,
                    p,
                    cx,
                ),
            );
        }
        let status = str_of(section, "status");
        // A failure wraps onto as many lines as it needs: cut at the card's edge it hid the
        // reason ("Couldn't count: Claude Code stopped before it…").
        let status_line = if section["statusError"] == true {
            div()
                .min_h(px(NOTE_HEIGHT * s))
                .pt(px(4.0 * s))
                .min_w_0()
                .text_color(swatch("failed", p))
        } else {
            div()
                .h(px(NOTE_HEIGHT * s))
                .flex()
                .items_end()
                .truncate()
                .text_color(p.card_muted)
        };
        // Agents other than Claude have no breakdown (and no refresh), so no status line under it.
        if section["refresh"].is_object() {
            column = column.child(status_line.text_size(px(11.5 * s)).child(status));
        }
        column.into_any_element()
    }

    fn connectors_body(
        &mut self,
        section: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut column = div().flex().flex_col().min_w_0();
        for (group_index, group) in section["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            column = column.child(
                div()
                    .h(px(SUBTITLE_HEIGHT * s))
                    .flex()
                    .items_end()
                    .text_size(px(11.0 * s))
                    .text_color(p.card_muted)
                    .child(str_of(group, "title")),
            );
            let rows = group["rows"].as_array().map_or(&[][..], Vec::as_slice);
            if rows.is_empty() {
                column = column.child(
                    div()
                        .h(px(ROW_HEIGHT * s))
                        .flex()
                        .items_center()
                        .text_size(px(12.0 * s))
                        .text_color(p.card_muted)
                        .child(group["empty"].as_str().unwrap_or("None").to_string()),
                );
            }
            for (index, row) in rows.iter().enumerate() {
                let dot = div()
                    .flex_shrink_0()
                    .size(px(7.0 * s))
                    .rounded_full()
                    .bg(swatch(row["status"].as_str().unwrap_or_default(), p))
                    .into_any_element();
                let action = row["action"].is_object().then(|| row["action"].clone());
                column = column.child(self.detail_row(
                    format!("connector-{group_index}-{index}"),
                    Some(dot),
                    str_of(row, "text"),
                    row["detail"].as_str().map(str::to_string),
                    None,
                    action,
                    false,
                    false,
                    p,
                    cx,
                ));
            }
        }
        column.into_any_element()
    }

    fn skills_body(
        &mut self,
        section: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut column = div().flex().flex_col().min_w_0();
        let icon = |p: &ChatAppearance| {
            svg()
                .path("titlebar/sparkles.svg")
                .size(px(13.0 * s))
                .flex_shrink_0()
                .text_color(p.muted)
                .into_any_element()
        };
        let used = section["used"].as_array().map_or(&[][..], Vec::as_slice);
        // An agent that doesn't say which skills it used has no "used" part at all.
        if used.is_empty() && section["usedEmpty"].is_string() {
            column = column.child(
                div()
                    .h(px(ROW_HEIGHT * s))
                    .flex()
                    .items_center()
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child(str_of(section, "usedEmpty")),
            );
        }
        for (index, row) in used.iter().enumerate() {
            let action = row["action"].is_object().then(|| row["action"].clone());
            column = column.child(self.detail_row(
                format!("skill-used-{index}"),
                Some(icon(p)),
                str_of(row, "text"),
                None,
                row["tooltip"].as_str().map(str::to_string),
                action,
                false,
                row["link"] == true,
                p,
                cx,
            ));
        }
        let open = section["allOpen"] == true;
        let all_key = SKILLS_ALL_KEY;
        let motion = self.disclosure_frame(all_key, open, cx);
        let toggle = section["allToggle"].clone();
        column = column.child(self.detail_row(
            "skills-all".to_string(),
            Some(fold_chevron(open, motion, 12.0 * s, p.card_muted)),
            str_of(section, "allLabel"),
            None,
            None,
            Some(toggle),
            true,
            false,
            p,
            cx,
        ));
        if open || motion.is_some() {
            let mut rows = div().flex().flex_col().min_w_0();
            for (index, row) in section["all"].as_array().into_iter().flatten().enumerate() {
                let action = row["action"].is_object().then(|| row["action"].clone());
                rows = rows.child(self.detail_row(
                    format!("skill-all-{index}"),
                    Some(icon(p)),
                    str_of(row, "text"),
                    None,
                    row["tooltip"].as_str().map(str::to_string),
                    action,
                    false,
                    row["link"] == true,
                    p,
                    cx,
                ));
            }
            column = column.child(self.disclosure_body_motion(
                all_key,
                motion,
                0.0,
                rows.into_any_element(),
            ));
        }
        column.into_any_element()
    }

    fn changes_body(
        &mut self,
        section: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let review = section["action"].clone();
        let numbers = div()
            .id("session-details-changes")
            .role(gpui::Role::Button)
            .aria_label("Review the changes")
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .flex_1()
            .min_w_0()
            .px(px(2.0 * s))
            .rounded(px(5.0 * s))
            .chat_cursor_pointer()
            .hover(|row| row.bg(p.foreground.opacity(0.06)))
            .child(
                svg()
                    .path("titlebar/file-diff.svg")
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .when(section["empty"] != true, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_color(swatch("done", p))
                        .child(str_of(section, "additions")),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(swatch("failed", p))
                        .child(str_of(section, "deletions")),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child(str_of(section, "detail")),
            )
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Review in the Changes view")
                    .build(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.session_details_action(&review, cx);
            }));
        let button = section["button"].is_object().then(|| {
            let label = str_of(&section["button"], "label");
            let action = section["button"]["action"].clone();
            div()
                .id("session-details-changes-button")
                .role(gpui::Role::Button)
                .aria_label(label.clone())
                .flex_shrink_0()
                .px(px(8.0 * s))
                .py(px(2.0 * s))
                .rounded(px(6.0 * s))
                .border_1()
                .border_color(p.border)
                .text_size(px(11.5 * s))
                .text_color(p.foreground)
                .chat_cursor_pointer()
                .hover(|button| button.bg(p.foreground.opacity(0.06)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.session_details_action(&action, cx);
                }))
        });
        div()
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .h(px(ROW_HEIGHT * s))
            .min_w_0()
            .child(numbers)
            .children(button)
            .into_any_element()
    }

    fn background_body(
        &mut self,
        section: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut column = div().flex().flex_col().min_w_0();
        let reduced_motion = crate::app::helpers::gpui_macos_reduce_motion_enabled();
        for (index, row) in section["running"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let stopping = row["stopping"] == true;
            let glyph = svg()
                .path("titlebar/loader2.svg")
                .size(px(12.0 * s))
                .flex_shrink_0()
                .text_color(swatch("running", p));
            let glyph = if reduced_motion {
                glyph.into_any_element()
            } else {
                glyph
                    .with_throttled_animation(
                        ("session-details-background", index),
                        Duration::from_secs(1),
                        |glyph, progress| {
                            glyph.with_transformation(gpui::Transformation::rotate(
                                gpui::percentage(progress),
                            ))
                        },
                    )
                    .into_any_element()
            };
            let stop = row["stop"].clone();
            let stop_button = stop.is_object().then(|| {
                div()
                    .id(SharedString::from(format!("session-details-stop-{index}")))
                    .role(gpui::Role::Button)
                    .aria_label("Stop this command")
                    .flex_shrink_0()
                    .size(px(18.0 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0 * s))
                    .border_1()
                    .border_color(p.border)
                    .when(!stopping, |button| {
                        button
                            .chat_cursor_pointer()
                            .hover(|button| button.bg(p.foreground.opacity(0.06)))
                    })
                    .child(
                        div()
                            .size(px(7.0 * s))
                            .rounded(px(1.0 * s))
                            .bg(if stopping { p.card_muted } else { p.muted }),
                    )
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(if stopping {
                            "Stopping…"
                        } else {
                            "Stop this command"
                        })
                        .build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !stopping {
                            this.session_details_action(&stop, cx);
                        }
                    }))
                    .into_any_element()
            });
            let detail = if stopping {
                Some("stopping…".to_string())
            } else {
                row["elapsed"].as_str().map(str::to_string)
            };
            let line = self.detail_row(
                format!("background-{index}"),
                Some(glyph),
                str_of(row, "text"),
                detail,
                row["tooltip"].as_str().map(str::to_string),
                None,
                stopping,
                false,
                p,
                cx,
            );
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0 * s))
                    .min_w_0()
                    .child(div().flex_1().min_w_0().child(line))
                    .children(stop_button),
            );
        }
        if let Some(label) = section["finishedLabel"].as_str() {
            let open = section["finishedOpen"] == true;
            let finished_key = BACKGROUND_FINISHED_KEY;
            let motion = self.disclosure_frame(finished_key, open, cx);
            let toggle = section["finishedToggle"].clone();
            column = column.child(self.detail_row(
                "background-finished".to_string(),
                Some(fold_chevron(open, motion, 12.0 * s, p.card_muted)),
                label.to_string(),
                None,
                None,
                Some(toggle),
                true,
                false,
                p,
                cx,
            ));
            if open || motion.is_some() {
                let mut rows = div().flex().flex_col().min_w_0();
                for (index, row) in section["finished"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let status = row["status"].as_str().unwrap_or_default();
                    let icon = match status {
                        "done" => "titlebar/circle-check.svg",
                        "failed" => "titlebar/circle-x.svg",
                        _ => "titlebar/player-stop.svg",
                    };
                    rows = rows.child(
                        self.detail_row(
                            format!("background-finished-{index}"),
                            Some(
                                svg()
                                    .path(icon)
                                    .size(px(13.0 * s))
                                    .flex_shrink_0()
                                    .text_color(swatch(status, p))
                                    .into_any_element(),
                            ),
                            str_of(row, "text"),
                            row["detail"].as_str().map(str::to_string),
                            row["tooltip"].as_str().map(str::to_string),
                            None,
                            true,
                            false,
                            p,
                            cx,
                        ),
                    );
                }
                column = column.child(self.disclosure_body_motion(
                    finished_key,
                    motion,
                    0.0,
                    rows.into_any_element(),
                ));
            }
        }
        column.into_any_element()
    }
}

impl NativeChatView {
    /// The Tasks card: the Tasks pill's rows (`agent_tasks.rs`) and its "Show more" line.
    fn tasks_body(
        &mut self,
        section: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let mut column = div().flex().flex_col().min_w_0();
        for row in section["rows"].as_array().into_iter().flatten() {
            column = column.child(
                div()
                    .h(px(TASK_ROW_PITCH * s))
                    .flex()
                    .items_center()
                    .child(self.task_row("session-details-task", row, p)),
            );
        }
        let label = str_of(section, "foldLabel");
        if !label.is_empty() && section["foldAction"].is_object() {
            let action = section["foldAction"].clone();
            column = column.child(
                div()
                    .id("session-details-tasks-more")
                    .role(gpui::Role::Button)
                    .aria_label(label.clone())
                    .h(px(ROW_HEIGHT * s))
                    .flex()
                    .items_center()
                    .chat_cursor_pointer()
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .hover(|line| line.text_color(p.foreground))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.session_details_action(&action, cx);
                    })),
            );
        }
        column.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn details() -> Value {
        json!({"sections": [
            {"id": "environment", "collapsed": false, "rows": [{}, {}]},
            {"id": "connectors", "collapsed": true, "groups": [{"rows": [{}, {}, {}]}]},
        ]})
    }

    #[test]
    fn a_folding_card_counts_the_part_of_its_body_still_showing() {
        let height = |connectors: f32| {
            panel_height(&details(), &mut |key, open| {
                if key == "session-details:connectors" {
                    connectors
                } else if open {
                    1.0
                } else {
                    0.0
                }
            })
        };
        let shut = height(0.0);
        let body = SUBTITLE_HEIGHT + 3.0 * ROW_HEIGHT;
        assert_eq!(height(1.0) - shut, body);
        assert_eq!(height(0.5) - shut, body * 0.5);
    }

    #[test]
    fn the_tasks_card_counts_its_rows_and_its_show_more_line() {
        let tasks = |more: bool| {
            let mut section = json!({"id": "tasks", "collapsed": false, "rows": [{}, {}, {}]});
            if more {
                section["foldAction"] = json!({"type": "sessionDetailsList"});
            }
            panel_height(&json!({"sections": [section]}), &mut |_, open| {
                f32::from(open)
            })
        };
        assert_eq!(tasks(true) - tasks(false), ROW_HEIGHT);
    }

    #[test]
    fn a_folded_context_card_keeps_its_usage_line_and_bar() {
        let context = |collapsed: bool| {
            let section = json!({"id": "context", "collapsed": collapsed, "rows": [{}, {}]});
            panel_height(&json!({"sections": [section]}), &mut |_, open| {
                f32::from(open)
            })
        };
        let card = CARD_PADDING_Y * 2.0 + HEADER_HEIGHT + 4.0;
        assert_eq!(context(true), card + SUMMARY_HEIGHT + BAR_HEIGHT);
        assert_eq!(context(false) - context(true), 2.0 * ROW_HEIGHT);
    }

    #[test]
    fn a_context_card_with_nothing_to_unfold_draws_no_body() {
        let height = |foldable: bool| {
            let section =
                json!({"id": "context", "collapsed": false, "foldable": foldable, "rows": [{}]});
            panel_height(&json!({"sections": [section]}), &mut |_, open| {
                f32::from(open)
            })
        };
        assert_eq!(height(true) - height(false), ROW_HEIGHT);
    }
}
