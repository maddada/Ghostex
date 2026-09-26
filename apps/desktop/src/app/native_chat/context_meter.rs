use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, canvas, div, point, px,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

/// CDXC:SessionChat 2026-09-23 DECISION:
/// User: a loading agent chat must keep its composer and status line visible, using the existing skeleton treatment for unknown values. The pre-view shell uses this same geometry.
pub(crate) fn status_line_skeleton(p: &ChatAppearance) -> AnyElement {
    static GEOMETRY: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../../../packages/gx-chat-core/visual/status-line-skeleton.json"
        ))
        .expect("status line skeleton geometry")
    });
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(STATUS_LINE_ROW_HEIGHT * p.scale))
        .gap(px(GEOMETRY["gap"].as_f64().unwrap() as f32 * p.scale))
        .children(GEOMETRY["widths"].as_array().unwrap().iter().map(|width| {
            div()
                .w(px(width.as_f64().unwrap() as f32 * p.scale))
                .h(px(GEOMETRY["height"].as_f64().unwrap() as f32 * p.scale))
                .rounded_full()
                .bg(p.muted.opacity(0.24))
        }))
        .into_any_element()
}

/// One status-line row: the value of `STATUS_LINE_ROW_HEIGHT_PX` in
/// `packages/gx-chat-core/src/menus/context/meter.rs`, which React applied as the
/// `min-height` of `.ghostex-chat-status-line.is-reserved`.
pub(super) const STATUS_LINE_ROW_HEIGHT: f32 = 16.0;

/// Width the status line's edit pen takes after the last item: the value of React's
/// `SESSION_CHAT_STATUS_LINE_EDIT_RESERVE_PX`.
const STATUS_LINE_EDIT_RESERVE: f32 = 18.0;

/// The hover group of the status line, which reveals its edit pen.
const CONTEXT_STATUS_GROUP: &str = "context-status";

/// The context meter's toggle key in `menu_toggle.rs`.
const CONTEXT_METER_TRIGGER: &str = "chat-context-meter";

thread_local! {
    /// CDXC:SessionChat 2026-09-19 WHY:
    /// React knew from its first render whether a session shows a status line, because it read the
    /// starred context details straight out of client storage. The native chat learns it from the
    /// chat core, whose answer arrives from the chat host after the first paint, so the box used to
    /// paint without the line's row and then jump when the answer landed. The last answer for this
    /// session, or failing that the last answer any session gave (kept on disk, so a fresh process
    /// has one too), stands until the controller speaks.
    static REMEMBERED_STATUS_LINES: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
    static LAST_STATUS_LINE_RESERVATION: Cell<Option<bool>> = const { Cell::new(None) };
}

fn status_line_state_path() -> std::path::PathBuf {
    crate::app::helpers::ghostex_state_root().join("gpui-session-chat-status-line.json")
}

/// The last answer any session gave, read from disk once per process.
fn last_status_line_reservation() -> bool {
    LAST_STATUS_LINE_RESERVATION.with(|cached| {
        if let Some(reserved) = cached.get() {
            return reserved;
        }
        let reserved = std::fs::read_to_string(status_line_state_path())
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|state| state["reserved"].as_bool())
            .unwrap_or(false);
        cached.set(Some(reserved));
        reserved
    })
}

fn remember_last_status_line_reservation(reserved: bool) {
    if last_status_line_reservation() == reserved {
        return;
    }
    LAST_STATUS_LINE_RESERVATION.with(|cached| cached.set(Some(reserved)));
    let path = status_line_state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, json!({ "reserved": reserved }).to_string());
}

pub(super) fn remembered_status_line_reservation(session: &str) -> bool {
    REMEMBERED_STATUS_LINES
        .with_borrow(|sessions| sessions.get(session).copied())
        .unwrap_or_else(last_status_line_reservation)
}

pub(super) fn ring(percentage: f32, appearance: &ChatAppearance) -> AnyElement {
    let scale = appearance.scale;
    let mut muted = gpui::Rgba::from(appearance.muted);
    if appearance.light {
        muted.r *= 0.75;
        muted.g *= 0.75;
        muted.b *= 0.75;
    }
    let muted = gpui::Hsla::from(muted).opacity(0.24);
    let ink = gpui::rgb(if appearance.light { 0x8b8b8b } else { 0xb9b9b9 });
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let center = bounds.center();
            let radius = 6.5 * scale;
            for (fraction, color) in [
                (1.0, muted),
                ((percentage / 100.0).clamp(0.0, 1.0), ink.into()),
            ] {
                if fraction <= 0.0 {
                    continue;
                }
                let mut path = gpui::PathBuilder::stroke(px(2.0 * scale));
                let steps = (fraction * 96.0).ceil() as usize;
                for step in 0..=steps {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * fraction * step as f32 / steps as f32;
                    let point = center + point(px(angle.cos() * radius), px(angle.sin() * radius));
                    if step == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
                if fraction < 1.0 {
                    for angle in [
                        -std::f32::consts::FRAC_PI_2,
                        -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * fraction,
                    ] {
                        let tip =
                            center + point(px(angle.cos() * radius), px(angle.sin() * radius));
                        window.paint_quad(
                            gpui::fill(
                                gpui::Bounds::new(
                                    tip - point(px(scale), px(scale)),
                                    gpui::size(px(2.0 * scale), px(2.0 * scale)),
                                ),
                                color,
                            )
                            .corner_radii(px(scale)),
                        );
                    }
                }
            }
        },
    )
    .size(px(16.0 * scale))
    .into_any_element()
}

impl NativeChatView {
    /// Whether the status line holds its row of space, by the core's rule
    /// (`status_line_reserved` in `menus/context/meter.rs`). The core
    /// answers it; until it has, the last answer for this session stands.
    pub(super) fn status_line_reserved(&self) -> bool {
        self.status_line_reserved || self.status_line_loading()
    }

    fn status_line_loading(&self) -> bool {
        !self.composer_ready
            || matches!(
                self.snapshot["status"].as_str(),
                Some("loading" | "starting")
            )
    }

    pub(super) fn adopt_status_line_reservation(&mut self) {
        let context = &self.snapshot["contextMeter"];
        if !context.is_object() {
            return;
        }
        let reserved = context["statusLineReserved"] == true
            || context["hasConfiguredItems"] == true
            || context["starred"]
                .as_array()
                .is_some_and(|items| !items.is_empty());
        self.status_line_reserved = reserved;
        let session = self.config.session_id.clone();
        REMEMBERED_STATUS_LINES.with_borrow_mut(|sessions| sessions.insert(session, reserved));
        remember_last_status_line_reservation(reserved);
    }

    pub(super) fn context_menu_rows(&self) -> Vec<Value> {
        vec![json!({"context":self.snapshot["contextMeter"]})]
    }

    /// CDXC:SessionChat 2026-09-24 DECISION: The user asked for the context window button's tooltip to say only "Statusline Config" instead of the usage summary.
    pub(super) fn render_context_meter(
        &self,
        appearance: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let context = &self.snapshot["contextMeter"];
        let bounds = Rc::new(Cell::new(Bounds::default()));
        let measured = bounds.clone();
        let percentage = context["usedPercentage"].as_f64().unwrap_or(0.0) as f32;
        div()
            .id("chat-context-meter")
            .role(gpui::Role::Button)
            .aria_label(text(context, "label"))
            .relative()
            .ml(px(6.0 * appearance.scale))
            .size(px(24.0 * appearance.scale))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .chat_cursor_pointer()
            .when(self.chat_menu_is_open(CONTEXT_METER_TRIGGER), |this| {
                this.bg(appearance.border)
            })
            .hover(|style| style.bg(appearance.border))
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Statusline Config").build(window, cx)
            })
            .child(ring(percentage, appearance))
            .on_click(cx.listener(move |chat, _, window, cx| {
                if chat.chat_menu_toggled_shut(CONTEXT_METER_TRIGGER, cx) {
                    return;
                }
                let width = if chat.snapshot["contextMeter"]["details"].is_array() {
                    320.0
                } else {
                    256.0
                };
                chat.show_chat_menu(chat.context_menu_rows(), bounds.get(), width, window, cx);
            }))
            .child(
                canvas(move |bounds, _, _| measured.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .into_any_element()
    }

    /// The pen that floats right of the status line's last item and opens Context details, the
    /// same `contextEdit` the pen in the context meter popover sends.
    fn context_status_edit(appearance: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let scale = appearance.scale;
        div()
            .id("context-status-edit")
            .role(gpui::Role::Button)
            .aria_label("Edit status line")
            .absolute()
            .left(gpui::relative(1.0))
            .top_0()
            .ml(px(2.0 * scale))
            .size(px(STATUS_LINE_ROW_HEIGHT * scale))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0 * scale))
            // CDXC:SessionChat 2026-09-23 DECISION: User: the pen appears only while the status
            // line is hovered.
            .opacity(0.0)
            .group_hover(CONTEXT_STATUS_GROUP, |style| style.opacity(0.55))
            .hover(|style| style.opacity(1.0))
            .chat_cursor_pointer()
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Edit status line").build(window, cx)
            })
            .child(
                gpui::svg()
                    .path("titlebar/pencil.svg")
                    .size(px(11.0 * scale))
                    .text_color(appearance.muted),
            )
            .on_click(cx.listener(|chat, _, window, cx| {
                chat.handle_action(
                    &super::actions::NativeChatAction {
                        command: json!({"type": "contextEdit"}),
                    },
                    window,
                    cx,
                );
            }))
            .into_any_element()
    }

    pub(super) fn render_context_status(
        &self,
        appearance: &ChatAppearance,
        window: &gpui::Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let scale = appearance.scale;
        if self.snapshot["contextMeter"]["starred"]
            .as_array()
            .is_none_or(Vec::is_empty)
            && (self.status_line_loading() || self.status_line_reserved)
        {
            return status_line_skeleton(appearance);
        }
        let mut style = window.text_style();
        style.font_family = appearance.font.clone().into();
        let mut widths = Vec::new();
        let mut lines = Vec::new();
        let mut line = div()
            .flex()
            .items_center()
            .justify_center()
            .w_full()
            .min_w_0();
        let starts = self.snapshot["contextStatusRows"].as_array();
        let starred = self.snapshot["contextMeter"]["starred"].as_array();
        let last = starred.map_or(0, Vec::len).saturating_sub(1);
        for (index, item) in starred.into_iter().flatten().enumerate() {
            let row_start = index == 0
                || starts.is_some_and(|starts| {
                    starts
                        .iter()
                        .any(|value| value.as_u64() == Some(index as u64))
                });
            if row_start && index > 0 {
                lines.push(line.into_any_element());
                line = div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .min_w_0();
            }
            if !row_start {
                line = line.child(
                    div()
                        .w(px(18.0 * scale))
                        .flex_shrink_0()
                        .text_center()
                        .text_size(px(7.0 * scale))
                        .text_color(appearance.muted.opacity(0.4))
                        .child("◆"),
                );
            }
            let value = text(item, "value");
            widths.push(
                window
                    .text_system()
                    .shape_line(
                        value.clone().into(),
                        px(11.0 * scale),
                        &[style.to_run(value.len())],
                        None,
                    )
                    .width
                    .as_f32(),
            );
            let copy = item["copy"]["text"].as_str().map(str::to_owned);
            let label = format!(
                "{}{}",
                text(item, "label"),
                if copy.is_some() {
                    " · Click to copy id"
                } else {
                    ""
                }
            );
            let value = div()
                .id(format!("context-status-{index}"))
                .min_w_0()
                .text_ellipsis()
                .child(value)
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
                })
                .when_some(copy, |item, copy| {
                    item.chat_cursor_pointer()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            crate::app::helpers::gpui_copy_to_clipboard(
                                gpui::ClipboardItem::new_string(copy.clone()),
                                cx,
                            );
                        }))
                });
            if index == last {
                line = line.child(
                    div()
                        .relative()
                        .min_w_0()
                        .child(value)
                        .child(Self::context_status_edit(appearance, cx)),
                );
            } else {
                line = line.child(value);
            }
        }
        if let Some(width) = widths.last_mut() {
            *width += STATUS_LINE_EDIT_RESERVE * scale;
        }
        lines.push(line.into_any_element());
        let chat = cx.weak_entity();
        div().group(CONTEXT_STATUS_GROUP).relative().flex().flex_col().w_full().min_w_0().min_h(px(STATUS_LINE_ROW_HEIGHT*scale)).px(px(4.0*scale))
            .text_size(px(11.0*scale)).line_height(px(STATUS_LINE_ROW_HEIGHT*scale)).text_color(appearance.muted.opacity(0.8)).children(lines)
            .child(canvas(move |bounds,window,cx| {
                let measurement=json!({"available":(bounds.size.width.as_f32()-8.0*scale).max(0.0),"widths":widths,"separator":18.0*scale});
                let chat=chat.clone();
                window.defer(cx,move |_,cx| {
                    let _=chat.update(cx,|chat,cx| {
                        if chat.context_status_measurements.as_ref()==Some(&measurement) {return;}
                        chat.context_status_measurements=Some(measurement.clone());
                        let mut command=measurement;
                        command["type"]="measureContextStatus".into();
                        chat.invoke(command,cx);
                    });
                });
            },|_,_,_,_|{}).absolute().size_full()).into_any_element()
    }
}
