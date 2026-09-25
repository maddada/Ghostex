use super::super::super::state::NativeChatView;
use super::super::window::{ChatOptionMenu, ChatOptionMenuPanel};
use super::style::{
    BAR_HEIGHT, BUTTON_GAP, CARD_WIDTH, ERROR_HEIGHT, HINTS_HEIGHT, LIST_HEIGHT, TRAIT_ROW_HEIGHT,
    button_lines,
};
use gpui::{Bounds, Context, Entity, Pixels, ScrollStrategy, UniformListScrollHandle, Window};
use serde_json::{Value, json};
use std::collections::HashMap;

/// CDXC:SessionChat 2026-09-21 SEE-ALSO:
/// What the picker shows (tabs, row order, search ranking, favorites, the footer rows and the pill's label) is decided in the core (packages/gx-chat-core/src/menus/picker/model_menu.rs) and projected as the snapshot's `modelMenu`; the core answers `modelMenuView`, `modelMenuPick`, `modelMenuTrait` and `modelMenuFavorite`. This side owns only the cursor and the open side list.
pub(in crate::app::native_chat::option_menu) struct ModelMenuState {
    /// The snapshot's `modelMenu` this panel last drew.
    pub(super) view: Value,
    /// Model rows first, then the footer buttons, as one cursor the arrows walk straight through.
    pub(super) active: usize,
    pub(super) flyout: Option<usize>,
    pub(super) scroll: UniformListScrollHandle,
    /// The model row the cursor last rested on, which the Reasoning button follows while the cursor is on the footer.
    pub(super) last_row: Option<String>,
    /// When Left or Right last hit an end, which plays the Reasoning button's shake.
    pub(super) shake_at: Option<std::time::Instant>,
}

fn live_view(menu: &Entity<ChatOptionMenu>, cx: &gpui::App) -> Value {
    menu.read(cx)
        .chat
        .upgrade()
        .map(|chat| chat.read(cx).snapshot["modelMenu"].clone())
        .unwrap_or(Value::Null)
}

/// The parts of the view that change the card's height, carried on the panel's row so the window
/// is measured before it exists and re-placed when they change.
fn shape(view: &Value) -> Value {
    json!({
        "lines": button_lines(view["traits"].as_array().map_or(0, Vec::len)).0,
        "error": view["error"].is_string(),
    })
}

fn selected_row(view: &Value) -> usize {
    view["rows"]
        .as_array()
        .and_then(|rows| rows.iter().position(|row| row["selected"] == true))
        .unwrap_or(0)
}

/// The card's height without the 14px of padding and border `open_panel_at` adds for a row menu.
pub(in crate::app::native_chat::option_menu) fn menu_height(marker: &Value) -> f32 {
    let lines = marker["lines"].as_u64().unwrap_or(0) as f32;
    let tray = if lines > 0.0 {
        9.0 + lines * TRAIT_ROW_HEIGHT + (lines - 1.0) * BUTTON_GAP
    } else {
        0.0
    };
    let error = if marker["error"] == true {
        ERROR_HEIGHT
    } else {
        0.0
    };
    2.0 + BAR_HEIGHT + error + LIST_HEIGHT + tray + HINTS_HEIGHT - 14.0
}

impl ModelMenuState {
    pub(in crate::app::native_chat::option_menu) fn new(
        rows: &[Value],
        menu: &Entity<ChatOptionMenu>,
        window: &mut Window,
        cx: &mut Context<ChatOptionMenuPanel>,
    ) -> Option<Self> {
        rows.first()?.get("modelMenu")?.as_object()?;
        let view = live_view(menu, cx);
        // Key presses reach the card only through the window's native keyboard owner, which a GPUI focus handle alone does not claim (native_chat/focus.rs).
        crate::app::native_chat::focus::reclaim_keyboard_focus(window);
        Some(Self {
            active: selected_row(&view),
            view,
            flyout: None,
            scroll: UniformListScrollHandle::new(),
            last_row: None,
            shake_at: None,
        })
    }

    pub(super) fn rows(&self) -> &[Value] {
        self.view["rows"].as_array().map_or(&[], Vec::as_slice)
    }

    pub(super) fn traits(&self) -> &[Value] {
        self.view["traits"].as_array().map_or(&[], Vec::as_slice)
    }

    /// The row the Reasoning button follows: the highlighted model, else the one the cursor last
    /// rested on, else the model in use.
    pub(super) fn browsed_row(&self) -> Option<&Value> {
        let rows = self.rows();
        rows.get(self.active)
            .or_else(|| {
                let key = self.last_row.as_deref()?;
                rows.iter().find(|row| row["key"].as_str() == Some(key))
            })
            .or_else(|| rows.iter().find(|row| row["selected"] == true))
    }

    /// The level a pick of `row` carries: where Left and Right left it, else the row's own.
    pub(super) fn effort_for(&self, row: &Value, efforts: &HashMap<String, String>) -> String {
        row["key"]
            .as_str()
            .and_then(|key| efforts.get(key))
            .cloned()
            .or_else(|| row["effort"].as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    /// The footer buttons as drawn: the Reasoning button follows the browsed row.
    pub(super) fn display_traits(&self, efforts: &HashMap<String, String>) -> Vec<Value> {
        let row = self.browsed_row();
        let effort = row.map(|row| self.effort_for(row, efforts));
        self.traits()
            .iter()
            .map(|setting| reasoning_for(setting, row, effort.as_deref()))
            .collect()
    }
}

/// The Reasoning button for the highlighted model: its levels, with the one Left and Right moved to
/// marked. `browse` names the row, and `browseCurrent` says whether it is the model in use, whose
/// level a choice applies at once as it always did.
fn reasoning_for(setting: &Value, row: Option<&Value>, effort: Option<&str>) -> Value {
    let (Some(row), Some(effort)) = (row, effort) else {
        return setting.clone();
    };
    let Some(efforts) = row["efforts"]
        .as_array()
        .filter(|efforts| !efforts.is_empty())
    else {
        return setting.clone();
    };
    let selected = row["selected"] == true;
    if setting["id"] != "effort"
        || (selected
            && setting["choices"].as_array().is_some_and(|choices| {
                choices
                    .iter()
                    .any(|choice| choice["selected"] == true && choice["value"] == effort)
            }))
    {
        return setting.clone();
    }
    let mut setting = setting.clone();
    setting["disabled"] = false.into();
    if let Some(label) = efforts
        .iter()
        .find(|entry| entry["value"] == effort)
        .and_then(|entry| entry["label"].as_str())
    {
        setting["valueLabel"] = label.into();
    }
    setting["choices"] = efforts
        .iter()
        .map(|entry| {
            json!({
                "value": entry["value"],
                "label": entry["label"],
                "selected": entry["value"] == effort,
                "isDefault": false,
            })
        })
        .collect();
    if let Some(object) = setting.as_object_mut() {
        object.remove("toggle");
    }
    setting["browse"] = row["key"].clone();
    setting["browseCurrent"] = selected.into();
    setting
}

/// The level Left or Right moves `row` to from `current`, or `None` at an end or on a model without levels.
pub(super) fn step_effort(row: &Value, current: &str, forward: bool) -> Option<String> {
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

impl ChatOptionMenuPanel {
    pub(super) fn model_menu_send(&mut self, command: Value, cx: &mut Context<Self>) {
        self.menu.update(cx, |menu, cx| menu.dispatch(command, cx));
    }

    /// The chat published a new snapshot: redraw from it, and re-place the card when its height changed.
    pub(in crate::app::native_chat::option_menu) fn model_menu_changed(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let next = live_view(&self.menu, cx);
        let Some(state) = self.model_menu.as_mut() else {
            return;
        };
        if !next.is_object() {
            self.menu.update(cx, |menu, cx| menu.close(None, cx));
            return;
        }
        if next == state.view {
            return;
        }
        let mut marker = shape(&next);
        let current = &self.rows[0]["modelMenu"];
        if marker["lines"] != current["lines"] || marker["error"] != current["error"] {
            marker["reopen"] = true.into();
            let depth = self.depth;
            self.menu.update(cx, |menu, cx| {
                menu.reopen(depth, vec![json!({"modelMenu":marker})], cx)
            });
            return;
        }
        let moved = next["tab"] != state.view["tab"];
        state.view = next;
        if moved {
            state.active = selected_row(&state.view);
            state
                .scroll
                .scroll_to_item(state.active, ScrollStrategy::Top);
        }
        let count = state.rows().len() + state.traits().len();
        state.active = state.active.min(count.saturating_sub(1));
        cx.notify();
    }

    /// Picks row `index`, true when the pick was sent. `effort` is the level the pick carries (the
    /// keyboard's, or one browsed from the Reasoning list); `None` leaves the model's level as a
    /// plain click always did.
    pub(super) fn model_menu_pick(
        &mut self,
        index: usize,
        secondary: bool,
        effort: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.model_menu.as_ref() else {
            return false;
        };
        if state.view["disabled"] == true {
            return false;
        }
        let Some(key) = state.rows().get(index).map(|row| row["key"].clone()) else {
            return false;
        };
        let mut command = json!({"type":"modelMenuPick","key":key,"secondary":secondary});
        if let Some(effort) = effort {
            command["effort"] = effort.into();
        }
        self.model_menu_send(command, cx);
        true
    }

    /// The pick the keyboard makes: the row with the level Left and Right left it on.
    pub(super) fn model_menu_pick_with_effort(
        &mut self,
        index: usize,
        secondary: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let effort = self.model_menu.as_ref().and_then(|state| {
            let row = state.rows().get(index)?;
            row["efforts"]
                .as_array()
                .is_some_and(|efforts| !efforts.is_empty())
                .then(|| state.effort_for(row, &self.menu.read(cx).model_efforts))
        });
        self.model_menu_pick(index, secondary, effort, cx)
    }

    /// The level a mouse pick of row `index` carries: only one the keyboard or the Reasoning list moved.
    pub(super) fn model_menu_browsed_effort(&self, index: usize, cx: &gpui::App) -> Option<String> {
        let state = self.model_menu.as_ref()?;
        let key = state.rows().get(index)?["key"].as_str()?;
        self.menu.read(cx).model_efforts.get(key).cloned()
    }
}

impl NativeChatView {
    /// Opens the model pop-up from Option+P or a terminal's model pill, or closes it when it is up.
    /// `trigger` is the terminal pill's frame in `window`; `None` anchors to the composer's model pill.
    /// A view that has no snapshot yet (created for this press) opens it as soon as one arrives.
    /// The next menu this view opens belongs to another surface; see `ChatOptionMenu::is_outside_pane`.
    pub(crate) fn mark_menu_outside_pane(&mut self) {
        self.menu_outside_pane = true;
    }

    pub(crate) fn toggle_model_menu(
        &mut self,
        trigger: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trigger = trigger.unwrap_or_else(|| self.model_pill_bounds.get());
        if !self.snapshot["modelMenu"].is_object() {
            self.pending_model_menu = Some((trigger, window.window_handle()));
            return;
        }
        self.pending_model_menu = None;
        self.show_model_menu(trigger, window, cx);
    }

    /// Opens a pop-up asked for before the first snapshot, once the snapshot carries the picker.
    pub(in crate::app::native_chat) fn open_pending_model_menu(&mut self, cx: &mut Context<Self>) {
        // A pop-up the terminal view asked for is hosted by this hidden view on purpose; only the
        // pane's own pending pop-up is dropped when the pane goes off screen.
        if self.pane_hidden && !self.menu_outside_pane {
            self.pending_model_menu = None;
            return;
        }
        if !self.snapshot["modelMenu"].is_object() {
            return;
        }
        let Some((trigger, handle)) = self.pending_model_menu.take() else {
            return;
        };
        let chat = cx.entity();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                chat.update(cx, |chat, cx| chat.show_model_menu(trigger, window, cx));
            });
        });
    }

    pub(in crate::app::native_chat) fn show_model_menu(
        &mut self,
        trigger: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.snapshot["modelMenu"].is_object() {
            return;
        }
        if self.chat_menu_toggled_shut(
            super::super::super::menu_toggle::option_pill_trigger_id("model"),
            cx,
        ) {
            return;
        }
        super::keys::register(cx);
        // Every visit starts on the session's own agent with an empty search.
        self.invoke(json!({"type":"modelMenuView","tab":null,"query":""}), cx);
        let marker = shape(&self.snapshot["modelMenu"]);
        self.show_chat_menu(
            vec![json!({"modelMenu":marker})],
            trigger,
            CARD_WIDTH,
            window,
            cx,
        );
    }
}
