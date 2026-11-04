//! The session card's hover button strip: which actions it offers and where the chevron splits it.
//!
//! SEE-ALSO: packages/shared/session-card-hover-actions.ts (deleted 2026-10-01), which carries the user decision that
//! defines this strip (whatever sits right of the chevron is always shown, whatever sits left of
//! it is hidden until the chevron is clicked, and an action enabled here is hidden from the row's
//! context menu).

use serde_json::Value;

/// The nine actions the strip can offer, in default order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverAction {
    Rename,
    Pin,
    Note,
    Snooze,
    CloseAfterDone,
    Tag,
    Park,
    Sleep,
    Close,
}

impl HoverAction {
    pub fn as_str(self) -> &'static str {
        match self {
            HoverAction::Rename => "rename",
            HoverAction::Pin => "pin",
            HoverAction::Note => "note",
            HoverAction::Snooze => "snooze",
            HoverAction::CloseAfterDone => "closeAfterDone",
            HoverAction::Tag => "tag",
            HoverAction::Park => "park",
            HoverAction::Sleep => "sleep",
            HoverAction::Close => "close",
        }
    }

    /// The settings id of an action, for a host routing a hover-button click back to its menu.
    pub fn from_id(value: &str) -> Option<Self> {
        Some(match value {
            "rename" => HoverAction::Rename,
            "pin" => HoverAction::Pin,
            "note" => HoverAction::Note,
            "snooze" => HoverAction::Snooze,
            "closeAfterDone" => HoverAction::CloseAfterDone,
            "tag" => HoverAction::Tag,
            "park" => HoverAction::Park,
            "sleep" => HoverAction::Sleep,
            "close" => HoverAction::Close,
            _ => return None,
        })
    }
}

const CHEVRON_ID: &str = "chevron";

/// One entry of the stored strip: an action or the chevron.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HoverButton {
    /// `None` is the chevron.
    action: Option<HoverAction>,
    enabled: bool,
}

impl HoverButton {
    fn id(&self) -> &'static str {
        match self.action {
            Some(action) => action.as_str(),
            None => CHEVRON_ID,
        }
    }
}

/// `DEFAULT_SESSION_CARD_HOVER_BUTTONS`: only Close shows at rest, and the chevron reveals Tag,
/// Park and Sleep.
fn defaults() -> Vec<HoverButton> {
    [
        (Some(HoverAction::Rename), false),
        (Some(HoverAction::Pin), false),
        (Some(HoverAction::Note), false),
        (Some(HoverAction::Snooze), false),
        (Some(HoverAction::CloseAfterDone), false),
        (Some(HoverAction::Tag), true),
        (Some(HoverAction::Park), true),
        (Some(HoverAction::Sleep), true),
        (None, true),
        (Some(HoverAction::Close), true),
    ]
    .into_iter()
    .map(|(action, enabled)| HoverButton { action, enabled })
    .collect()
}

/// The Unified sidebar's strip until the user changes it: Snooze, Park and Close always show on
/// hover, and the chevron brings out Tag and Sleep.
///
/// CDXC:Sidebar 2026-10-31 DECISION:
/// User (Q846, issue #359): "if i expand my strip then it's expanded for all session cards so snooze/park can be 1 click. i think let's save different strip for unified vs project sidebar. we show the strip related to the currently active sidebar in the settings. and we have a button that allows copying from the other view (just icon button pls with tooltip)". Each sidebar keeps its own strip: `sessionCardHoverButtons` stays the Project sidebar's, untouched for existing users, and `unifiedSessionCardHoverButtons` is the Unified sidebar's. The Unified default keeps the user's earlier Snooze and Park request one click away by putting them right of the chevron, beside Close. SEE-ALSO: `DEFAULT_UNIFIED_SESSION_CARD_HOVER_BUTTONS` in the settings catalog.
pub fn unified_default_hover_buttons() -> Value {
    let entry = |id: &str, enabled: bool| serde_json::json!({ "id": id, "enabled": enabled });
    Value::Array(vec![
        entry("rename", false),
        entry("pin", false),
        entry("note", false),
        entry("closeAfterDone", false),
        entry("tag", true),
        entry("sleep", true),
        entry(CHEVRON_ID, true),
        entry("snooze", true),
        entry("park", true),
        entry("close", true),
    ])
}

/// The strip the Unified sidebar's cards use: the user's, or the default until they change it.
pub fn unified_hover_buttons(settings: &crate::sidebar_view::SidebarSettings) -> Value {
    if settings.unified_card_hover_buttons.is_null() {
        unified_default_hover_buttons()
    } else {
        settings.unified_card_hover_buttons.clone()
    }
}

/// The strip as the user ordered it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverStrip {
    /// Hidden until the chevron is clicked.
    pub before: Vec<HoverAction>,
    /// Always shown on hover.
    pub after: Vec<HoverAction>,
    pub chevron: bool,
}

impl HoverStrip {
    /// Every enabled action, whichever side of the chevron it sits on. An action listed here is
    /// hidden from the row's context menu.
    pub(crate) fn enabled(&self) -> Vec<HoverAction> {
        self.before.iter().chain(&self.after).copied().collect()
    }

    pub(crate) fn includes(&self, action: HoverAction) -> bool {
        self.before.contains(&action) || self.after.contains(&action)
    }
}

/// `normalizeSessionCardHoverButtons` then `splitSessionCardHoverButtons`, over the raw settings
/// value.
pub fn hover_strip(setting: &Value) -> HoverStrip {
    split(&normalize(setting))
}

fn normalize(setting: &Value) -> Vec<HoverButton> {
    let Some(entries) = setting.as_array() else {
        return defaults();
    };
    // The earlier shape was a plain list of enabled action ids in no particular order; it keeps
    // the default order and leaves the chevron as the default has it.
    if entries.iter().all(Value::is_string) {
        let enabled: Vec<&str> = entries.iter().filter_map(Value::as_str).collect();
        return defaults()
            .into_iter()
            .map(|button| match button.action {
                None => button,
                Some(action) => HoverButton {
                    action: Some(action),
                    enabled: enabled.contains(&action.as_str()),
                },
            })
            .collect();
    }
    let mut items: Vec<HoverButton> = Vec::new();
    for entry in entries {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let Some(id) = object.get("id").and_then(Value::as_str) else {
            continue;
        };
        let button = if id == CHEVRON_ID {
            HoverButton {
                action: None,
                enabled: false,
            }
        } else {
            match HoverAction::from_id(id) {
                Some(action) => HoverButton {
                    action: Some(action),
                    enabled: false,
                },
                None => continue,
            }
        };
        if items.iter().any(|seen| seen.id() == button.id()) {
            continue;
        }
        items.push(HoverButton {
            enabled: object.get("enabled").and_then(Value::as_bool) == Some(true),
            ..button
        });
    }
    // A button added to a newer build is appended disabled, so it never appears on a card until
    // the user turns it on.
    for button in defaults() {
        if !items.iter().any(|seen| seen.id() == button.id()) {
            items.push(HoverButton {
                enabled: false,
                ..button
            });
        }
    }
    items
}

fn split(items: &[HoverButton]) -> HoverStrip {
    let chevron_index = items.iter().position(|item| item.action.is_none());
    let chevron = chevron_index.is_some_and(|index| items[index].enabled);
    if !chevron {
        return HoverStrip {
            before: Vec::new(),
            after: enabled_actions(items),
            chevron: false,
        };
    }
    let index = chevron_index.expect("checked above");
    HoverStrip {
        before: enabled_actions(&items[..index]),
        after: enabled_actions(&items[index + 1..]),
        chevron: true,
    }
}

fn enabled_actions(items: &[HoverButton]) -> Vec<HoverAction> {
    items
        .iter()
        .filter(|item| item.enabled)
        .filter_map(|item| item.action)
        .collect()
}

/// One button of the strip a row draws, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverSlot {
    /// The nth action left of the chevron.
    Before(usize),
    /// The chevron that folds the `before` actions in and out.
    Chevron,
    /// The nth action right of the chevron (or of nothing, when there is no chevron).
    After(usize),
}

/// The buttons a row draws while hovered, from the lengths of its resolved `before` and `after`
/// lists (an action the row cannot offer was already dropped from them). The chevron shows only
/// when it has something to fold.
///
/// CDXC:Sessions 2026-10-28 DECISION:
/// User: "if i disable all buttons for the session card in the sidebar please let the sidebar sessions not change when i hover over one of them". A row whose strip is empty draws nothing for hover: the renderer keeps the time, badges and status dots exactly as at rest, and only the usual hover background stays.
pub fn hover_slots(before: usize, chevron: bool, after: usize, expanded: bool) -> Vec<HoverSlot> {
    let mut slots = Vec::new();
    if expanded || !chevron {
        slots.extend((0..before).map(HoverSlot::Before));
    }
    if chevron && before > 0 {
        slots.push(HoverSlot::Chevron);
    }
    slots.extend((0..after).map(HoverSlot::After));
    slots
}

#[cfg(test)]
mod slot_tests {
    use super::{hover_slots, hover_strip, unified_default_hover_buttons, HoverAction, HoverSlot};

    #[test]
    fn the_unified_default_keeps_snooze_and_park_one_click_away() {
        let strip = hover_strip(&unified_default_hover_buttons());
        assert_eq!(strip.before, [HoverAction::Tag, HoverAction::Sleep]);
        assert_eq!(
            strip.after,
            [HoverAction::Snooze, HoverAction::Park, HoverAction::Close]
        );
        assert!(strip.chevron);
    }

    #[test]
    fn no_buttons_leaves_the_strip_empty() {
        assert!(hover_slots(0, false, 0, false).is_empty());
        assert!(hover_slots(0, false, 0, true).is_empty());
    }

    #[test]
    fn a_chevron_with_nothing_to_fold_draws_nothing() {
        assert!(hover_slots(0, true, 0, false).is_empty());
        assert!(hover_slots(0, true, 0, true).is_empty());
    }

    #[test]
    fn a_folded_chevron_hides_the_actions_before_it() {
        assert_eq!(
            hover_slots(2, true, 1, false),
            vec![HoverSlot::Chevron, HoverSlot::After(0)]
        );
    }

    #[test]
    fn an_expanded_chevron_shows_every_action_in_order() {
        assert_eq!(
            hover_slots(2, true, 1, true),
            vec![
                HoverSlot::Before(0),
                HoverSlot::Before(1),
                HoverSlot::Chevron,
                HoverSlot::After(0)
            ]
        );
    }

    #[test]
    fn without_a_chevron_the_before_actions_always_show() {
        assert_eq!(
            hover_slots(1, false, 1, false),
            vec![HoverSlot::Before(0), HoverSlot::After(0)]
        );
    }

    #[test]
    fn a_chevron_with_only_after_actions_draws_just_those() {
        assert_eq!(
            hover_slots(0, true, 2, false),
            vec![HoverSlot::After(0), HoverSlot::After(1)]
        );
    }
}
