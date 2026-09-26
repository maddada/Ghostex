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

/// The card holds focus the whole visit and claims its keys as an action in its own context, the
/// way the composer does (keyboard.rs).
pub(super) fn register(cx: &mut gpui::App) {
    bind_toggle_chord(cx);
    if cx.has_global::<KeysBound>() {
        return;
    }
    cx.set_global(KeysBound);
    let keys = [
        "up",
        "down",
        "ctrl-p",
        "ctrl-n",
        "left",
        "right",
        "enter",
        "shift-enter",
        "escape",
        "tab",
        "shift-tab",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain((1..=9).map(|slot| format!("secondary-{slot}")))
    .chain(
        BUTTON_LETTERS
            .iter()
            .map(|(_, letter)| letter.to_lowercase()),
    );
    cx.bind_keys(keys.map(|key| {
        gpui::KeyBinding::new(&key, ModelMenuKey { key: key.clone() }, Some(KEY_CONTEXT))
    }));
}

/// CDXC:SessionChat 2026-09-24 DECISION:
/// User: each footer button's hotkey is "just the letter in a filled circle floating to the bottom right of the icon": R Reasoning, C Context, F Fast mode, A Account ("i want a as hotkey for accounts"). The picker has no search field ("just remove the search it's useless"), so the plain letter is the key; it works like a click, and pressing it again closes a side list it opened.
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
    cx.bind_keys([gpui::KeyBinding::new(
        &chord,
        ModelMenuKey {
            key: "toggle".into(),
        },
        Some(KEY_CONTEXT),
    )]);
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
    /// User: the model pop-up is the one model picker and is driven from the keyboard (docs/2026-09-24/model-popup-keyboard/): Up and Down move through the models and then the footer buttons and stop at the top and bottom instead of wrapping round ("make it not loop to the top when I press down while I'm at the bottom", 2026-09-24, superseding the wrap); Left and Right move the highlighted model's reasoning a level (a shake at either end or on a model without levels) and move along the footer; Enter uses the highlighted model and level in this session and Shift+Enter saves them as the agent's default, and either one closes the pop-up (2026-09-24); Cmd+1 to Cmd+9 only highlight that row and never apply it ("I should press enter to apply the model change", 2026-09-24, superseding Cmd+number picking); the footer buttons answer to their letters (see `BUTTON_LETTERS`); Tab and Shift+Tab switch agent tabs; Escape or the picker hotkey close without saving. The mouse keeps its old meaning: a click saves the default, a right-click this session only.
    fn model_menu_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.model_menu.as_mut() else {
            return false;
        };
        let rows = state.rows().len();
        let count = rows + state.traits().len();
        match key {
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
                self.model_menu_send(json!({"type":"modelMenuView","tab":tab}), cx);
            }
            "up" | "ctrl-p" | "down" | "ctrl-n" if count > 0 => {
                let up = key == "up" || key == "ctrl-p";
                // The footer is one stop below the last model (Left and Right move along its
                // buttons); the first model and the footer are the ends.
                state.active = if state.active >= rows {
                    if up && rows > 0 {
                        rows - 1
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
            "enter" | "shift-enter" => {
                let active = state.active;
                if active < rows {
                    if self.model_menu_pick_with_effort(active, key == "enter", cx) {
                        self.menu.update(cx, |menu, cx| menu.close(None, cx));
                    }
                } else {
                    self.activate_model_button(active - rows, key == "shift-enter", window, cx);
                }
            }
            "left" | "right" if state.active >= rows => {
                let buttons = count - rows;
                let index = state.active - rows;
                state.active = rows
                    + if key == "left" {
                        (index + buttons - 1) % buttons
                    } else {
                        (index + 1) % buttons
                    };
            }
            "left" | "right" => {
                let row = state.rows()[state.active].clone();
                let efforts = &self.menu.read(cx).model_efforts;
                let current = state.effort_for(&row, efforts);
                match super::state::step_effort(&row, &current, key == "right") {
                    Some(next) => {
                        let key = row["key"].as_str().unwrap_or_default().to_owned();
                        self.menu.update(cx, |menu, _| {
                            menu.model_efforts.insert(key, next);
                        });
                    }
                    None => state.shake_at = Some(std::time::Instant::now()),
                }
            }
            _ if key.len() == 1 => {
                let letter = key.to_uppercase();
                let Some(index) = state.traits().iter().position(|setting| {
                    setting["icon"]
                        .as_str()
                        .and_then(button_letter)
                        .is_some_and(|button| button == letter)
                }) else {
                    return false;
                };
                state.active = rows + index;
                self.activate_model_button(index, false, window, cx);
            }
            _ => {
                let Some(Ok(slot)) = key.strip_prefix("secondary-").map(str::parse::<u64>) else {
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
