use super::super::window::ChatOptionMenuPanel;
use gpui::{Context, ScrollStrategy, Window};
use serde_json::json;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = ghostex_gpui, no_json)]
pub(super) struct ModelMenuKey {
    key: String,
}
/// The model picker hotkey last bound to close the pop-up, so a changed binding is bound again.
struct ModelMenuKeysRegistered(Option<String>);
impl gpui::Global for ModelMenuKeysRegistered {}

pub(super) const KEY_CONTEXT: &str = "ChatModelMenu";
/// The picker's search field, which holds focus on a long list.
///
/// CDXC:SessionChat 2026-11-02 WHY:
/// The field's own bindings (Up, Down, Left, Right, Enter, Tab, Escape) sit deeper than the card's and would win, so these keys are bound again for the field inside the card; bound later at the same depth, they reach the picker first. Letters are not, so they type.
const SEARCH_KEY_CONTEXT: &str = "ChatModelMenu > Input";

/// The card holds focus the whole visit and claims its keys as an action in its own context, the
/// way the composer does (keyboard.rs).
pub(super) fn register(cx: &mut gpui::App) {
    bind_toggle_chord(cx);
    if cx.has_global::<KeysBound>() {
        return;
    }
    cx.set_global(KeysBound);
    let navigation = [
        "up",
        "down",
        "ctrl-p",
        "ctrl-n",
        "left",
        "right",
        "enter",
        "alt-enter",
        "escape",
        "tab",
        "shift-tab",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain((1..=9).map(|slot| format!("alt-{slot}")));
    let letters = BUTTON_LETTERS
        .iter()
        .map(|(_, letter)| letter.to_lowercase());
    let binding = |key: String, context: &str| {
        crate::app::hotkeys::gpui_runtime_key_binding(
            &key,
            ModelMenuKey { key: key.clone() },
            Some(context),
            cx,
        )
    };
    let bindings: Vec<_> = navigation
        .clone()
        .chain(letters)
        .map(|key| binding(key, KEY_CONTEXT))
        .chain(navigation.map(|key| binding(key, SEARCH_KEY_CONTEXT)))
        .collect();
    cx.bind_keys(bindings);
}

/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: each footer button's hotkey is "just the letter in a filled circle floating to the bottom right of the icon": R Reasoning, C Context, F Fast mode, A Account ("i want a as hotkey for accounts"). The picker had no search field ("just remove the search it's useless"), so the plain letter is the key; on a list long enough for the 2026-11-02 search field (`MODEL_MENU_SEARCH_THRESHOLD` in gx-chat-core), letters type into the field instead.
const BUTTON_LETTERS: [(&str, &str); 4] = [
    ("reasoning", "R"),
    ("context", "C"),
    ("fast", "F"),
    ("account", "A"),
];

/// The letter the footer button with this icon answers to.
pub(super) fn button_letter(icon: &str) -> Option<&'static str> {
    BUTTON_LETTERS
        .iter()
        .find(|(kind, _)| *kind == icon)
        .map(|(_, letter)| *letter)
}

struct KeysBound;
impl gpui::Global for KeysBound {}

/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: Option+P opens the model pop-up and pressing it again closes it without saving ("I don't want to have two interfaces for picking the model"). The pop-up holds key status in its own window, so the picker hotkey is bound inside it as a close key; a changed binding is bound again the next time the pop-up opens.
fn bind_toggle_chord(cx: &mut gpui::App) {
    let chord = crate::app::hotkeys::gpui_configured_hotkey_key("openModelPicker")
        .and_then(|key| crate::app::hotkeys::gpui_keystroke_from_shared_hotkey(&key));
    if cx
        .try_global::<ModelMenuKeysRegistered>()
        .is_some_and(|bound| bound.0 == chord)
    {
        return;
    }
    cx.set_global(ModelMenuKeysRegistered(chord.clone()));
    let Some(chord) = chord else {
        return;
    };
    let binding = crate::app::hotkeys::gpui_runtime_key_binding(
        &chord,
        ModelMenuKey {
            key: "toggle".into(),
        },
        Some(KEY_CONTEXT),
        cx,
    );
    cx.bind_keys([binding]);
}

impl ChatOptionMenuPanel {
    pub(super) fn model_menu_key_action(
        &mut self,
        action: &ModelMenuKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model_menu_key(&action.key, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
        }
    }

    /// True when the picker used the key.
    ///
    /// CDXC:SessionChat 2026-09-24 DECISION:
    /// User: the model pop-up is the one model picker and is driven from the keyboard (docs/2026-09-24/model-popup-keyboard/): Up and Down move through the models and then the footer buttons and stop at the top and bottom instead of wrapping round ("make it not loop to the top when I press down while I'm at the bottom", 2026-09-24, superseding the wrap); Left and Right move the highlighted model's reasoning a level (a shake at either end or on a model without levels) and move along the footer; Enter saves the highlighted model and level as the agent's default and Option+Enter uses them in this session only, and either one closes the pop-up ("I want holding option and hitting enter to do 'this session only' but otherwise we always don't do 'this session only'", 2026-09-29, superseding the 2026-09-24 Enter for this session and Shift+Enter for the default); Option+Enter on a footer button or a side-list choice likewise applies it to this session only; Option+1 to Option+9 only highlight that row and never apply it ("I should press enter to apply the model change", 2026-09-24; the modifier is Option, not Cmd, "since we use option + p for showing this modal", 2026-09-26); the footer buttons answer to their letters (see `BUTTON_LETTERS`); Tab and Shift+Tab switch agent tabs; Escape or the picker hotkey close without saving. The mouse keeps its old meaning: a click saves the default, a right-click this session only.
    fn model_menu_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.model_menu.as_mut() else {
            return false;
        };
        let rows = state.rows().len();
        // The cursor runs through the models, the Legacy models row (when the tab has one), then
        // the footer buttons.
        let fold = state.fold_stop();
        let footer = state.footer_start();
        let count = footer + state.traits().len();
        let search_enabled = state.view["searchEnabled"] == true;
        match key {
            // Escape empties a typed filter first; only an empty one closes the picker.
            "escape" if search_enabled && !state.search.read(cx).value().is_empty() => {
                state
                    .search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                state.active = 0;
                state.scroll.scroll_to_item(0, ScrollStrategy::Top);
                self.model_menu_send(json!({"type":"modelMenuView","query":""}), cx);
            }
            "escape" | "toggle" => self.menu.update(cx, |menu, cx| menu.close(None, cx)),
            "tab" | "shift-tab" => {
                let tabs = state.view["tabs"].as_array().cloned().unwrap_or_default();
                if tabs.is_empty() {
                    return true;
                }
                let current = tabs
                    .iter()
                    .position(|tab| tab["active"] == true)
                    .unwrap_or(0);
                let next = if key == "tab" {
                    (current + 1) % tabs.len()
                } else {
                    (current + tabs.len() - 1) % tabs.len()
                };
                let tab = tabs[next]["id"].clone();
                if search_enabled {
                    state
                        .search
                        .update(cx, |input, cx| input.set_value("", window, cx));
                }
                self.model_menu_send(json!({"type":"modelMenuView","tab":tab,"query":""}), cx);
            }
            "up" | "ctrl-p" | "down" | "ctrl-n" if count > 0 => {
                let up = key == "up" || key == "ctrl-p";
                // The Legacy models row is one stop below the last model and the footer one stop
                // below that (Left and Right move along its buttons); the first model and the
                // footer are the ends.
                state.active = if state.active >= footer {
                    if up {
                        fold.or(rows.checked_sub(1)).unwrap_or(state.active)
                    } else {
                        state.active
                    }
                } else if fold == Some(state.active) {
                    if up && rows > 0 {
                        rows - 1
                    } else if !up && count > footer {
                        footer
                    } else {
                        state.active
                    }
                } else if up {
                    state.active.saturating_sub(1)
                } else if state.active + 1 < rows || count > rows {
                    state.active + 1
                } else {
                    state.active
                };
                if state.active < rows {
                    state.last_row = state.rows()[state.active]["key"]
                        .as_str()
                        .map(str::to_owned);
                    state
                        .scroll
                        .scroll_to_item(state.active, ScrollStrategy::Nearest);
                }
            }
            "enter" | "alt-enter" => {
                let active = state.active;
                let session_only = key == "alt-enter";
                if active < rows {
                    if self.model_menu_pick_with_effort(active, session_only, cx) {
                        self.menu.update(cx, |menu, cx| menu.close(None, cx));
                    }
                } else if fold == Some(active) {
                    let open = state.fold_open();
                    self.model_menu_send(json!({"type":"modelMenuView","legacyOpen":!open}), cx);
                } else {
                    self.activate_model_button(active - footer, session_only, window, cx);
                }
            }
            // Right opens the Legacy models list and Left shuts it, as on any disclosure row.
            "left" | "right" if fold == Some(state.active) => {
                let open = state.fold_open();
                if open == (key == "left") {
                    self.model_menu_send(json!({"type":"modelMenuView","legacyOpen":!open}), cx);
                }
            }
            "left" | "right" if state.active >= footer => {
                let buttons = count - footer;
                let index = state.active - footer;
                state.active = footer
                    + if key == "left" {
                        (index + buttons - 1) % buttons
                    } else {
                        (index + 1) % buttons
                    };
            }
            "left" | "right" => {
                let row = state.rows()[state.active].clone();
                let current = state.effort_for(&row, self.menu.read(cx).model_effort.as_deref());
                match super::state::step_effort(&row, &current, key == "right") {
                    Some(next) => {
                        self.menu.update(cx, |menu, _| {
                            menu.model_effort = Some(next);
                        });
                    }
                    None => state.shake_at = Some(std::time::Instant::now()),
                }
            }
            _ if key.len() == 1 => {
                if search_enabled {
                    return false;
                }
                let letter = key.to_uppercase();
                let Some(index) = state.traits().iter().position(|setting| {
                    setting["icon"]
                        .as_str()
                        .and_then(button_letter)
                        .is_some_and(|button| button == letter)
                }) else {
                    return false;
                };
                state.active = footer + index;
                self.activate_model_button(index, false, window, cx);
            }
            _ => {
                let Some(Ok(slot)) = key.strip_prefix("alt-").map(str::parse::<u64>) else {
                    return false;
                };
                if let Some(index) = state
                    .rows()
                    .iter()
                    .position(|row| row["shortcut"].as_u64() == Some(slot))
                {
                    state.active = index;
                    state.last_row = state.rows()[index]["key"].as_str().map(str::to_owned);
                    state.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                }
            }
        }
        true
    }
}
