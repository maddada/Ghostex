//! Settings > Autopilot's "Supervisor model" and "When it's at its limit" rows: a button with the
//! pick's agent logo, model and effort that opens the new session's model picker under it (agent
//! tabs, search on a long list, starred rows, Legacy models, Reasoning and Context Window, the key
//! reminder). gxserver projects what it shows (`supervisorMenu`) and saves a pick
//! (`supervisorPick`), server/src/server/autopilot_supervisor_menu.rs.
//!
//! CDXC:Autopilot 2026-11-26 DECISION:
//! Issue #765, user: "show the model picker we show when user is on new session, he can pick any agent and any model and any effort there". Mockup answers: Q5008 "picker" (the Supervisor model row opens the new session's picker) and Q5009 "fallback row" (a "When it's at its limit" row with the same picker, plus an "Automatic" row at its top). The card keeps the chat picker's sizes (native_chat/option_menu/model_menu/style.rs: 304px card, 12px radius, 40px tab bar, 36px search, 216px list, 30px buttons, 26px key reminder) in Settings' own palette, and, as on the mockup, the open tab's underline is the text colour, not the accent.
//! SEE-ALSO: native_chat/option_menu/model_menu/render.rs (the chat's picker this one draws like).

use super::super::super::fields::icon;
use super::super::super::fields::row::{CONTROL_HEIGHT, RowSpec, setting_row, settings_icon};
use super::super::super::model::SettingsTabId;
use super::super::super::palette::SettingsPalette;
use super::super::super::store::store_gxserver_rpc;
use super::{AutopilotTab, CALL_TIMEOUT};
use crate::app::window::native_modal_kit::{MODAL_RADIUS_CONTROL, capture_child_bounds, hsla};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, AppContext as _, Bounds, Context, Entity, FocusHandle, Hsla,
    InteractiveElement as _, IntoElement as _, KeyDownEvent, MouseDownEvent, ParentElement as _,
    Pixels, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, anchored, deferred, div, point, px, rgb, svg,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize};
use serde_json::{Value, json};
use std::cell::Cell;
use std::rc::Rc;

const CARD_WIDTH: f32 = 304.0;
const FLYOUT_WIDTH: f32 = 232.0;
const CARD_RADIUS: f32 = 12.0;
const ITEM_RADIUS: f32 = 8.0;
const BAR_HEIGHT: f32 = 40.0;
const SEARCH_HEIGHT: f32 = 36.0;
const LIST_HEIGHT: f32 = 216.0;
const TRAIT_ROW_HEIGHT: f32 = 30.0;
const HINTS_HEIGHT: f32 = 26.0;
const BUTTON_WIDTH: f32 = 240.0;

/// The two rows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Supervisor,
    Fallback,
}

impl Slot {
    fn wire(self) -> &'static str {
        match self {
            Self::Supervisor => "supervisor",
            Self::Fallback => "fallback",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Supervisor => 0,
            Self::Fallback => 1,
        }
    }
}

/// The open picker: what gxserver last projected and what this side owns (the open tab, the
/// search text, the Legacy fold, the keyboard cursor, the level Left and Right moved to, the open
/// side list).
pub(crate) struct SupervisorPicker {
    slot: Slot,
    menu: Value,
    automatic: bool,
    tab: Option<String>,
    legacy_open: bool,
    active: usize,
    browsed_effort: Option<String>,
    flyout: Option<usize>,
    search: Entity<InputState>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    _search_subscription: Subscription,
}

/// The triggers' frames, which the card opens under.
pub(crate) type PickerTriggers = [Rc<Cell<Option<Bounds<Pixels>>>>; 2];

struct Tones {
    text: Hsla,
    muted: Hsla,
    surface: Hsla,
    star: Hsla,
    ink: Hsla,
}

impl Tones {
    fn new(p: &SettingsPalette) -> Self {
        let text = hsla(p.foreground);
        let ink: Hsla = rgb(if p.light { 0x000000 } else { 0xffffff }).into();
        Self {
            text,
            muted: hsla(p.muted),
            surface: hsla(p.popup_background),
            star: rgb(if p.light { 0xd97706 } else { 0xfbbf24 }).into(),
            ink,
        }
    }

    fn ink(&self, alpha: f32) -> Hsla {
        self.ink.opacity(alpha)
    }
}

/// An agent's logo in its brand tone; a white mark turns dark on the light surface (the chat
/// picker's `agent_logo`).
fn agent_logo(icon: &str, light: bool) -> gpui::Svg {
    let color = crate::app::helpers::workspace_tab_agent_icon_accent_color(icon);
    let color = if light && matches!(color, 0xffffff | 0xedecec) {
        0x27272a
    } else {
        color
    };
    svg()
        .path(format!("agent-icons/{icon}.svg"))
        .flex_shrink_0()
        .text_color(rgb(color))
}

fn rows(menu: &Value) -> &[Value] {
    menu["rows"].as_array().map_or(&[][..], Vec::as_slice)
}

fn traits(menu: &Value) -> &[Value] {
    menu["traits"].as_array().map_or(&[][..], Vec::as_slice)
}

fn selected_row(menu: &Value) -> usize {
    rows(menu)
        .iter()
        .position(|row| row["selected"] == true)
        .unwrap_or(0)
}

/// The level Left or Right moves `row` to from `current` (the chat picker's `step_effort`).
fn step_effort(row: &Value, current: &str, forward: bool) -> Option<String> {
    let efforts = row["efforts"]
        .as_array()
        .filter(|efforts| !efforts.is_empty())?;
    let value = |index: usize| efforts.get(index)?["value"].as_str().map(str::to_owned);
    match efforts.iter().position(|entry| entry["value"] == current) {
        None => value(if forward { 0 } else { efforts.len() - 1 }),
        Some(index) if forward => value(index + 1),
        Some(index) => index.checked_sub(1).and_then(value),
    }
}

fn offers_effort(row: &Value, effort: &str) -> bool {
    row["efforts"]
        .as_array()
        .is_some_and(|efforts| efforts.iter().any(|entry| entry["value"] == effort))
}

impl AutopilotTab {
    /// One of the two rows: its title and line, and the button that opens its picker.
    pub(super) fn supervisor_pick_row(
        &mut self,
        p: &SettingsPalette,
        slot: Slot,
        spec: RowSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let labels = self
            .view
            .as_ref()
            .map(|view| view["supervisorLabels"][slot.wire()].clone())
            .unwrap_or(Value::Null);
        let open = self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.slot == slot);
        let label = labels["label"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| "Automatic".to_string());
        let icon = labels["icon"].as_str().map(str::to_owned);
        let p_copy = *p;
        let trigger = div()
            .id(SharedString::from(format!(
                "autopilot-{}-pick",
                slot.wire()
            )))
            .role(gpui::Role::ComboBox)
            .aria_value(label.clone())
            .aria_expanded(open)
            .w(px(BUTTON_WIDTH))
            .max_w_full()
            .min_w_0()
            .h(px(CONTROL_HEIGHT))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(if open { p.focus_border } else { p.hairline }))
            .bg(hsla(p.raised))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .cursor_pointer()
            .hover(move |this| this.bg(hsla(p_copy.raised_hover)))
            .on_click(cx.listener(move |page, _, window, cx| {
                if page
                    .picker
                    .as_ref()
                    .is_some_and(|picker| picker.slot == slot)
                {
                    page.close_picker(cx);
                } else {
                    page.open_picker(slot, window, cx);
                }
            }))
            .when_some(icon, |this, icon| {
                this.child(agent_logo(&icon, p.light).size(px(16.0)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(label),
            )
            .child(settings_icon(icon::SELECTOR, 16.0, p.muted).flex_shrink_0());
        let cell = self.picker_triggers[slot.index()].clone();
        let popover = open
            .then(|| cell.get())
            .flatten()
            .map(|bounds| self.render_picker(p, bounds, window, cx));
        let control = div()
            .flex_shrink_0()
            .w(px(BUTTON_WIDTH))
            .max_w_full()
            .min_w_0()
            .on_children_prepainted(capture_child_bounds(cell, 0))
            .child(trigger)
            .when_some(popover, |this, popover| this.child(popover))
            .into_any_element();
        setting_row(
            p,
            match slot {
                Slot::Supervisor => "autopilotSupervisor",
                Slot::Fallback => "autopilotSupervisorFallback",
            },
            spec,
            None,
            control,
            cx,
        )
    }

    fn open_picker(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search models…"));
        let subscription = cx.subscribe_in(
            &search,
            window,
            |page: &mut Self, _, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    page.picker_menu_call(cx);
                }
            },
        );
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.picker = Some(SupervisorPicker {
            slot,
            menu: Value::Null,
            automatic: false,
            tab: None,
            legacy_open: false,
            active: 0,
            browsed_effort: None,
            flyout: None,
            search,
            focus,
            scroll: ScrollHandle::new(),
            _search_subscription: subscription,
        });
        self.picker_menu_call(cx);
        cx.notify();
    }

    fn close_picker(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        cx.notify();
    }

    /// The view this side keeps, as `supervisorMenu` and `supervisorPick` take it.
    fn picker_params(&self, op: &str, cx: &Context<Self>) -> Option<Value> {
        let picker = self.picker.as_ref()?;
        Some(json!({
            "op": op,
            "slot": picker.slot.wire(),
            "tab": picker.tab,
            "query": picker.search.read(cx).value().to_string(),
            "legacyOpen": picker.legacy_open,
            "favorites": crate::app::gx_chat::model_favorites(),
        }))
    }

    /// Asks gxserver for what the open picker shows.
    fn picker_menu_call(&mut self, cx: &mut Context<Self>) {
        let Some(params) = self.picker_params("supervisorMenu", cx) else {
            return;
        };
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/autopilot",
            params,
            CALL_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let Some(picker) = page.picker.as_mut() else {
                        return;
                    };
                    match result {
                        Ok(answer) if answer["menu"].is_object() => {
                            let moved = answer["menu"]["tab"] != picker.menu["tab"];
                            picker.menu = answer["menu"].clone();
                            picker.automatic = answer["automatic"] == true;
                            if picker.tab.is_none() {
                                picker.tab = picker.menu["tab"].as_str().map(str::to_owned);
                            }
                            if moved {
                                picker.active = selected_row(&picker.menu);
                                picker.browsed_effort = None;
                            }
                            let count = rows(&picker.menu).len();
                            picker.active = picker.active.min(count.saturating_sub(1));
                        }
                        Ok(_) => {
                            page.error = Some("Ghostex has no model lineup yet.".to_string());
                            page.picker = None;
                        }
                        Err(error) => {
                            page.error = Some(error);
                            page.picker = None;
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Saves a pick (`key` with its level, a footer button's value, or Automatic). A row pick
    /// closes the card as the chat's picker does; a footer pick keeps it open.
    fn picker_pick(&mut self, change: Value, close: bool, cx: &mut Context<Self>) {
        let Some(mut params) = self.picker_params("supervisorPick", cx) else {
            return;
        };
        if let (Some(params), Some(change)) = (params.as_object_mut(), change.as_object()) {
            params.extend(change.clone());
        }
        params["limit"] = json!(self.log_limit);
        if close {
            self.picker = None;
        } else if let Some(picker) = self.picker.as_mut() {
            picker.flyout = None;
        }
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/autopilot",
            params,
            CALL_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    match result {
                        Ok(answer) => {
                            page.view = Some(answer);
                            page.error = None;
                            if page.picker.is_some() {
                                page.picker_menu_call(cx);
                            }
                        }
                        Err(error) => page.error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
        cx.notify();
    }

    fn pick_row(&mut self, index: usize, keyboard: bool, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let Some(row) = rows(&picker.menu).get(index) else {
            return;
        };
        let effort = picker
            .browsed_effort
            .clone()
            .filter(|effort| offers_effort(row, effort))
            .or_else(|| {
                keyboard
                    .then(|| row["effort"].as_str().map(str::to_owned))
                    .flatten()
            });
        let mut change = json!({ "key": row["key"] });
        if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
            change["effort"] = json!(effort);
        }
        self.picker_pick(change, true, cx);
    }

    fn picker_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let count = rows(&picker.menu).len();
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                if picker.flyout.take().is_none() {
                    self.close_picker(cx);
                }
            }
            "up" | "down" if count > 0 => {
                cx.stop_propagation();
                let forward = event.keystroke.key == "down";
                picker.active = if forward {
                    (picker.active + 1).min(count - 1)
                } else {
                    picker.active.saturating_sub(1)
                };
                picker.browsed_effort = None;
                picker.scroll.scroll_to_item(picker.active);
            }
            "left" | "right" if count > 0 => {
                cx.stop_propagation();
                let row = &rows(&picker.menu)[picker.active];
                let current = picker
                    .browsed_effort
                    .clone()
                    .unwrap_or_else(|| row["effort"].as_str().unwrap_or_default().to_owned());
                if let Some(next) = step_effort(row, &current, event.keystroke.key == "right") {
                    picker.browsed_effort = Some(next);
                }
            }
            "enter" if count > 0 => {
                cx.stop_propagation();
                let index = picker.active;
                self.pick_row(index, true, cx);
            }
            _ => return,
        }
        cx.notify();
    }

    fn render_picker(
        &mut self,
        p: &SettingsPalette,
        trigger: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tones = Tones::new(p);
        let light = p.light;
        let Some(picker) = self.picker.as_ref() else {
            return div().into_any_element();
        };
        let menu = picker.menu.clone();
        let mut card = div()
            .id("autopilot-supervisor-picker")
            .track_focus(&picker.focus)
            .w(px(CARD_WIDTH))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .rounded(px(CARD_RADIUS))
            .border_1()
            .border_color(tones.ink(0.12))
            .bg(tones.surface)
            .shadow_lg()
            .overflow_hidden()
            .text_size(px(13.0))
            .text_color(tones.text)
            .on_key_down(
                cx.listener(|page, event: &KeyDownEvent, _, cx| page.picker_key(event, cx)),
            );
        if picker.slot == Slot::Fallback {
            card = card.child(self.render_automatic(&tones, picker.automatic, cx));
        }
        if menu.is_object() {
            card = card
                .child(self.render_tabs(&menu, &tones, light, cx))
                .when(menu["searchEnabled"] == true, |card| {
                    card.child(
                        div()
                            .flex_shrink_0()
                            .h(px(SEARCH_HEIGHT))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .border_b_1()
                            .border_color(tones.ink(0.08))
                            .child(
                                svg()
                                    .path("titlebar/search.svg")
                                    .size(px(13.0))
                                    .flex_shrink_0()
                                    .text_color(tones.muted),
                            )
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&picker.search)
                                        .with_size(ComponentSize::Small)
                                        .appearance(false)
                                        .bordered(false)
                                        .focus_bordered(false)
                                        .w_full()
                                        .px(px(0.0))
                                        .py(px(0.0))
                                        .text_size(px(13.0))
                                        .text_color(tones.text),
                                ),
                            ),
                    )
                })
                .child(self.render_rows(&menu, &tones, light, cx))
                .child(self.render_traits(&menu, &tones, cx))
                .child(self.render_hints(&menu, &tones));
        } else {
            card = card.child(
                div()
                    .h(px(LIST_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(tones.muted)
                    .child("Loading models…"),
            );
        }
        let flyout = picker
            .flyout
            .and_then(|index| traits(&menu).get(index).cloned())
            .map(|setting| self.render_flyout(&setting, &tones, cx));
        let viewport = window.viewport_size();
        let below = trigger.origin.y + trigger.size.height + px(4.0);
        let opens_above = viewport.height - below < px(420.0) && trigger.origin.y > px(420.0);
        let position = if opens_above {
            point(trigger.origin.x, trigger.origin.y - px(4.0))
        } else {
            point(trigger.origin.x, below)
        };
        let trigger_frame = trigger;
        let content = div()
            .flex()
            .items_start()
            .gap(px(4.0))
            .on_mouse_down_out(cx.listener(move |page, event: &MouseDownEvent, _, cx| {
                if trigger_frame.contains(&event.position) {
                    return;
                }
                page.close_picker(cx);
            }))
            .child(card)
            .when_some(flyout, |this, flyout| this.child(flyout));
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(position)
                .anchor(if opens_above {
                    gpui::Anchor::BottomLeft
                } else {
                    gpui::Anchor::TopLeft
                })
                .snap_to_window_with_margin(px(8.0))
                .child(content),
        )
        .with_priority(1)
        .into_any_element()
    }

    /// The fallback picker's first row: no pick of its own, only Opus 5.5 then GPT 6.1 Sol.
    fn render_automatic(
        &self,
        tones: &Tones,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = tones.ink(0.05);
        div()
            .flex_shrink_0()
            .p(px(4.0))
            .border_b_1()
            .border_color(tones.ink(0.08))
            .child(
                div()
                    .id("autopilot-fallback-automatic")
                    .role(gpui::Role::MenuItemRadio)
                    .aria_label("Automatic")
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(ITEM_RADIUS))
                    .border_1()
                    .border_color(if selected {
                        tones.ink(0.09)
                    } else {
                        gpui::transparent_black()
                    })
                    .when(selected, |row| row.bg(tones.ink(0.11)))
                    .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                    .cursor_pointer()
                    .on_click(cx.listener(|page, _, _, cx| {
                        page.picker_pick(json!({ "automatic": true }), true, cx)
                    }))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child("Automatic"),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(tones.muted)
                            .child("Opus 5.5 · Medium, then GPT 6.1 Sol · High"),
                    ),
            )
            .into_any_element()
    }

    fn render_tabs(
        &self,
        menu: &Value,
        tones: &Tones,
        light: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut bar = div()
            .flex_shrink_0()
            .h(px(BAR_HEIGHT))
            .px(px(4.0))
            .border_b_1()
            .border_color(tones.ink(0.08))
            .flex()
            .items_center()
            .gap(px(2.0));
        for (index, tab) in menu["tabs"].as_array().into_iter().flatten().enumerate() {
            let active = tab["active"] == true;
            let id = tab["id"].as_str().unwrap_or_default().to_owned();
            let name = tab["name"].as_str().unwrap_or_default().to_owned();
            let hover = tones.ink(0.06);
            let icon = match tab["icon"].as_str() {
                Some(icon) => agent_logo(icon, light).size(px(16.0)),
                None => svg()
                    .path("titlebar/star-filled.svg")
                    .size(px(15.0))
                    .text_color(if active { tones.text } else { tones.muted }),
            };
            let underline = tones.text;
            bar = bar.child(
                div()
                    .id(("autopilot-picker-tab", index))
                    .role(gpui::Role::Tab)
                    .aria_label(name.clone())
                    .relative()
                    .size(px(32.0))
                    .rounded(px(ITEM_RADIUS))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(!active, |tab| tab.hover(move |style| style.bg(hover)))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(name.clone()).build(window, cx)
                    })
                    .on_click(cx.listener(move |page, _, _, cx| {
                        if let Some(picker) = page.picker.as_mut() {
                            picker.tab = Some(id.clone());
                            picker.legacy_open = false;
                            picker.flyout = None;
                        }
                        page.picker_menu_call(cx);
                    }))
                    .child(icon.opacity(if active { 1.0 } else { 0.72 }))
                    .when(active, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .bottom(px(-4.0))
                                .left(px(6.0))
                                .right(px(6.0))
                                .h(px(2.0))
                                .rounded(px(1.0))
                                .bg(underline),
                        )
                    }),
            );
        }
        if menu["addAgent"] == true {
            let hover = tones.ink(0.06);
            bar = bar.child(div().flex_1()).child(
                div()
                    .id("autopilot-picker-add-agent")
                    .role(gpui::Role::Button)
                    .aria_label("Add agents")
                    .size(px(32.0))
                    .rounded(px(ITEM_RADIUS))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .tooltip(|window, cx| {
                        gpui_component::tooltip::Tooltip::new("Add agents in Settings > Agents")
                            .build(window, cx)
                    })
                    .on_click(cx.listener(|page, _, _, cx| {
                        page.picker = None;
                        page.store.update(cx, |store, cx| {
                            store.set_active_tab(SettingsTabId::Agents, cx)
                        });
                    }))
                    .child(
                        svg()
                            .path("titlebar/plus.svg")
                            .size(px(15.0))
                            .text_color(tones.muted),
                    ),
            );
        }
        bar.into_any_element()
    }

    fn render_row(
        &self,
        index: usize,
        row: &Value,
        active: bool,
        tones: &Tones,
        light: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = row["label"].as_str().unwrap_or_default().to_owned();
        let selected = row["selected"] == true;
        let favorite = row["favorite"] == true;
        let key = row["key"].as_str().unwrap_or_default().to_owned();
        let star_hover = tones.ink(0.08);
        let body = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0))
            .when(row["showAgent"] == true, |body| {
                body.child(
                    agent_logo(row["icon"].as_str().unwrap_or_default(), light).size(px(14.0)),
                )
            })
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(label.clone()),
            );
        let item = div()
            .id(("autopilot-picker-row", index))
            .role(gpui::Role::MenuItemRadio)
            .aria_label(label)
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(ITEM_RADIUS))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_1()
            .border_color(if selected {
                tones.ink(0.09)
            } else {
                gpui::transparent_black()
            })
            .when(selected, |item| item.bg(tones.ink(0.11)))
            .when(active && !selected, |item| item.bg(tones.ink(0.05)))
            .cursor_pointer()
            .on_mouse_move(cx.listener(move |page, _, _, cx| {
                if let Some(picker) = page.picker.as_mut()
                    && picker.active != index
                {
                    picker.active = index;
                    picker.browsed_effort = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |page, _, _, cx| page.pick_row(index, false, cx)))
            .child(body)
            .child(
                div()
                    .id(("autopilot-picker-star", index))
                    .role(gpui::Role::Button)
                    .aria_label(if favorite { "Remove star" } else { "Star" })
                    .flex_shrink_0()
                    .size(px(22.0))
                    .rounded(px(ITEM_RADIUS))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(move |style| style.bg(star_hover))
                    .on_click(cx.listener(move |page, _, _, cx| {
                        cx.stop_propagation();
                        crate::app::gx_chat::toggle_model_favorite(&key);
                        page.picker_menu_call(cx);
                    }))
                    .child(
                        svg()
                            .path(if favorite {
                                "titlebar/star-filled.svg"
                            } else {
                                "titlebar/star.svg"
                            })
                            .size(px(13.0))
                            .text_color(if favorite { tones.star } else { tones.muted }),
                    ),
            );
        div().pb(px(2.0)).child(item).into_any_element()
    }

    fn render_legacy_toggle(
        &self,
        legacy: &Value,
        tones: &Tones,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = legacy["open"] == true;
        let hover = tones.ink(0.05);
        let label = format!(
            "{} ({})",
            legacy["label"].as_str().unwrap_or("Legacy models"),
            legacy["count"].as_u64().unwrap_or(0)
        );
        div()
            .id("autopilot-picker-legacy")
            .role(gpui::Role::Button)
            .aria_expanded(open)
            .h(px(26.0))
            .px(px(8.0))
            .mb(px(2.0))
            .rounded(px(ITEM_RADIUS))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(12.0))
            .text_color(tones.muted)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |page, _, _, cx| {
                if let Some(picker) = page.picker.as_mut() {
                    picker.legacy_open = !open;
                }
                page.picker_menu_call(cx);
            }))
            .child(
                svg()
                    .path(if open {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(12.0))
                    .text_color(tones.muted),
            )
            .child(label)
            .into_any_element()
    }

    fn render_rows(
        &self,
        menu: &Value,
        tones: &Tones,
        light: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(picker) = self.picker.as_ref() else {
            return div().into_any_element();
        };
        let active = picker.active;
        let all = rows(menu);
        let mut list = div()
            .id("autopilot-picker-list")
            .h(px(LIST_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&picker.scroll)
            .p(px(4.0));
        let legacy = menu["legacy"].is_object().then(|| menu["legacy"].clone());
        let fold_at = legacy
            .as_ref()
            .filter(|legacy| legacy["open"] == true)
            .map(|_| {
                all.iter()
                    .position(|row| row["legacy"] == true)
                    .unwrap_or(all.len())
            });
        for (index, row) in all.iter().enumerate() {
            if Some(index) == fold_at
                && let Some(legacy) = legacy.as_ref()
            {
                list = list.child(self.render_legacy_toggle(legacy, tones, cx));
            }
            list = list.child(self.render_row(index, row, index == active, tones, light, cx));
        }
        if fold_at == Some(all.len())
            && let Some(legacy) = legacy.as_ref()
        {
            list = list.child(self.render_legacy_toggle(legacy, tones, cx));
        }
        if all.is_empty() {
            list = list.child(
                div()
                    .px(px(8.0))
                    .py(px(24.0))
                    .text_center()
                    .text_color(tones.muted)
                    .child(menu["emptyText"].as_str().unwrap_or("No models").to_owned()),
            );
        }
        let pinned = legacy
            .filter(|legacy| legacy["open"] != true)
            .map(|legacy| {
                div()
                    .flex_shrink_0()
                    .px(px(4.0))
                    .child(self.render_legacy_toggle(&legacy, tones, cx))
            });
        div()
            .flex()
            .flex_col()
            .child(list)
            .when_some(pinned, |this, pinned| this.child(pinned))
            .into_any_element()
    }

    /// Reasoning (following the highlighted row and the level Left and Right moved to) and
    /// Context Window.
    fn render_traits(&self, menu: &Value, tones: &Tones, cx: &mut Context<Self>) -> AnyElement {
        let Some(picker) = self.picker.as_ref() else {
            return div().into_any_element();
        };
        let mut tray = div()
            .flex_shrink_0()
            .border_t_1()
            .border_color(tones.ink(0.08))
            .p(px(4.0))
            .flex()
            .gap(px(4.0));
        let row = rows(menu).get(picker.active);
        for (index, setting) in traits(menu).iter().enumerate() {
            let effort = setting["id"] == "effort";
            // While the cursor rests on another row or Left and Right moved the level, Reasoning
            // shows that row's level, as the chat picker's does.
            let browsed = effort
                .then(|| {
                    let row = row
                        .filter(|row| row["selected"] != true || picker.browsed_effort.is_some())?;
                    let level = picker
                        .browsed_effort
                        .clone()
                        .filter(|level| offers_effort(row, level))
                        .or_else(|| row["effort"].as_str().map(str::to_owned))?;
                    row["efforts"]
                        .as_array()?
                        .iter()
                        .find(|entry| entry["value"] == level.as_str())
                        .and_then(|entry| entry["label"].as_str().map(str::to_owned))
                })
                .flatten();
            let value = browsed.unwrap_or_else(|| {
                setting["valueLabel"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            });
            let disabled = setting["disabled"] == true;
            let lit = picker.flyout == Some(index);
            let label = setting["label"].as_str().unwrap_or_default().to_owned();
            let glyph = if effort {
                "titlebar/brain.svg"
            } else {
                "titlebar/chart-bar.svg"
            };
            let hover = tones.ink(0.06);
            tray = tray.child(
                div()
                    .id(("autopilot-picker-setting", index))
                    .role(gpui::Role::MenuItem)
                    .aria_label(format!("{label}: {value}"))
                    .aria_expanded(lit)
                    .flex_auto()
                    .flex_shrink_0()
                    .h(px(TRAIT_ROW_HEIGHT))
                    .px(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .rounded(px(ITEM_RADIUS))
                    .text_size(px(12.0))
                    .opacity(if disabled { 0.42 } else { 1.0 })
                    .when(lit, |button| button.bg(tones.ink(0.11)))
                    .when(!disabled, |button| {
                        button
                            .cursor_pointer()
                            .hover(move |style| style.bg(hover))
                            .on_click(cx.listener(move |page, _, _, cx| {
                                if let Some(picker) = page.picker.as_mut() {
                                    picker.flyout = (picker.flyout != Some(index)).then_some(index);
                                }
                                cx.notify();
                            }))
                    })
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
                    })
                    .child(
                        svg()
                            .path(glyph)
                            .size(px(14.0))
                            .flex_shrink_0()
                            .text_color(tones.muted),
                    )
                    .child(div().truncate().child(value)),
            );
        }
        tray.into_any_element()
    }

    /// A footer button's side list: its values, the current one checked.
    fn render_flyout(&self, setting: &Value, tones: &Tones, cx: &mut Context<Self>) -> AnyElement {
        let id = setting["id"].as_str().unwrap_or_default().to_owned();
        let mut list = div()
            .id("autopilot-picker-flyout")
            .w(px(FLYOUT_WIDTH))
            .flex_shrink_0()
            .p(px(4.0))
            .rounded(px(CARD_RADIUS))
            .border_1()
            .border_color(tones.ink(0.12))
            .bg(tones.surface)
            .shadow_lg()
            .text_size(px(12.5))
            .text_color(tones.text);
        for (index, choice) in setting["choices"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let selected = choice["selected"] == true;
            let value = choice["value"].as_str().unwrap_or_default().to_owned();
            let label = choice["label"].as_str().unwrap_or_default().to_owned();
            let hover = tones.ink(0.05);
            let id = id.clone();
            list = list.child(
                div()
                    .id(("autopilot-picker-choice", index))
                    .role(gpui::Role::MenuItemRadio)
                    .aria_label(label.clone())
                    .h(px(28.0))
                    .px(px(8.0))
                    .rounded(px(ITEM_RADIUS))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .when(selected, |row| row.bg(tones.ink(0.11)))
                    .when(!selected, |row| row.hover(move |style| style.bg(hover)))
                    .on_click(cx.listener(move |page, _, _, cx| {
                        page.picker_pick(
                            json!({ "trait": { "id": id, "value": value } }),
                            false,
                            cx,
                        )
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .when(selected, |row| {
                        row.child(
                            svg()
                                .path("titlebar/check.svg")
                                .size(px(13.0))
                                .text_color(tones.muted),
                        )
                    }),
            );
        }
        list.into_any_element()
    }

    fn render_hints(&self, menu: &Value, tones: &Tones) -> AnyElement {
        let levels = self
            .picker
            .as_ref()
            .and_then(|picker| rows(menu).get(picker.active))
            .is_some_and(|row| row["efforts"].as_array().is_some_and(|e| !e.is_empty()));
        let mut hints = vec!["↑↓ Move"];
        if levels {
            hints.push("←→ Reasoning");
        }
        hints.extend(["↵ Pick", "Esc Close"]);
        div()
            .flex_shrink_0()
            .h(px(HINTS_HEIGHT))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .border_t_1()
            .border_color(tones.ink(0.08))
            .text_size(px(10.5))
            .text_color(tones.muted)
            .children(hints.into_iter().map(|hint| div().child(hint)))
            .into_any_element()
    }
}
