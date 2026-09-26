//! The actions a Quick Access row offers: the list behind its right-click menu and the footer's
//! Actions panel, and the accelerators that run one of them without opening either.
//!
//! CDXC:AppModal 2026-09-21 DECISION:
//! User: make Quick Access look like Raycast. Rows carry no hover buttons any more; everything a row
//! could do (open, star, tag, copy, edit, delete, remove) is listed here once, reached with
//! right-click, the Actions panel (Cmd+K) or the action's own hotkey. Supersedes the per-row icon
//! strips ported from the React rows.

use super::text::{format_hotkey_label, HotkeyPlatform};
use super::wire::{QuickAccessIcon, QuickAccessMenuItem};

/// Wire hotkeys by menu item id. Chosen to stay clear of the search field's own editing keys
/// (Cmd+A/C/V/X/Z).
const ACTION_HOTKEYS: &[(&str, &str)] = &[
    ("addPrompt", "cmd+n"),
    ("copyPath", "cmd+shift+c"),
    ("remove", "cmd+d"),
    ("prompt:open", "cmd+o"),
    ("prompt:favorite", "cmd+s"),
    ("prompt:save", "cmd+s"),
    ("prompt:tag", "cmd+t"),
    ("prompt:copy", "cmd+shift+c"),
    ("prompt:edit", "cmd+e"),
    ("prompt:delete", "cmd+d"),
];

pub(crate) const ACTIVATE_ACTION_ID: &str = "activate";

fn wire_hotkey(id: &str) -> Option<&'static str> {
    ACTION_HOTKEYS
        .iter()
        .find(|(action, _)| *action == id)
        .map(|(_, hotkey)| *hotkey)
}

/// `QUICK_ACCESS_ACTION_HOTKEYS`: the distinct wire hotkeys, first-seen order.
pub(crate) fn action_hotkeys() -> Vec<String> {
    let mut hotkeys: Vec<String> = Vec::new();
    for (_, hotkey) in ACTION_HOTKEYS {
        if !hotkeys.iter().any(|seen| seen == hotkey) {
            hotkeys.push((*hotkey).to_string());
        }
    }
    hotkeys
}

/// `actionItem(id, label, icon, options)`.
pub(crate) fn action_item(
    id: &str,
    label: &str,
    icon: &str,
    hotkey: Option<String>,
    danger: bool,
    disabled: bool,
    platform: HotkeyPlatform,
) -> QuickAccessMenuItem {
    let hotkey = hotkey.unwrap_or_else(|| {
        if id == ACTIVATE_ACTION_ID {
            "↵".to_string()
        } else {
            wire_hotkey(id)
                .map(|hotkey| format_hotkey_label(hotkey, platform))
                .unwrap_or_default()
        }
    });
    QuickAccessMenuItem {
        id: id.to_string(),
        label: label.to_string(),
        icon: QuickAccessIcon::asset(icon, None),
        hotkey,
        danger,
        disabled,
        separator: false,
    }
}

/// A plain action with no options.
pub(crate) fn item(
    id: &str,
    label: &str,
    icon: &str,
    platform: HotkeyPlatform,
) -> QuickAccessMenuItem {
    action_item(id, label, icon, None, false, false, platform)
}

/// `ACTION_SEPARATOR`.
pub(crate) fn separator() -> QuickAccessMenuItem {
    QuickAccessMenuItem {
        id: "separator".to_string(),
        label: String::new(),
        icon: QuickAccessIcon::None,
        hotkey: String::new(),
        danger: false,
        disabled: false,
        separator: true,
    }
}

/// `tidyActionItems`: drops leading, trailing and doubled separators.
pub(crate) fn tidy(items: Vec<QuickAccessMenuItem>) -> Vec<QuickAccessMenuItem> {
    let count = items.len();
    let flags: Vec<bool> = items.iter().map(|item| item.separator).collect();
    items
        .into_iter()
        .enumerate()
        .filter(|(index, item)| {
            !item.separator || (*index > 0 && *index < count - 1 && !flags[index - 1])
        })
        .map(|(_, item)| item)
        .collect()
}

/// `actionForHotkey`: the enabled item this wire hotkey runs, if the row offers one.
pub(crate) fn action_for_hotkey<'a>(
    items: &'a [QuickAccessMenuItem],
    hotkey: &str,
) -> Option<&'a QuickAccessMenuItem> {
    items
        .iter()
        .find(|item| !item.separator && !item.disabled && wire_hotkey(&item.id) == Some(hotkey))
}
