use crate::json::J;

pub const DEFAULT_SESSION_CARD_HOVER_BUTTONS: J = J::Arr(&[
    J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("rename"))]),
    J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("pin"))]),
    J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("note"))]),
    J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("snooze"))]),
    J::Obj(&[
        ("enabled", J::Bool(false)),
        ("id", J::Str("closeAfterDone")),
    ]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("tag"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("park"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("sleep"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("chevron"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("close"))]),
]);

/// The Unified sidebar's own strip until the user changes it: Snooze, Park and Close always show,
/// and the chevron brings out Tag and Sleep. Keep in step with gx-core's
/// `unified_default_hover_buttons` (CDXC:Sidebar 2026-10-31 there).
pub const DEFAULT_UNIFIED_SESSION_CARD_HOVER_BUTTONS: J = J::Arr(&[
    J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("rename"))]),
    J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("pin"))]),
    J::Obj(&[("enabled", J::Bool(false)), ("id", J::Str("note"))]),
    J::Obj(&[
        ("enabled", J::Bool(false)),
        ("id", J::Str("closeAfterDone")),
    ]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("tag"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("sleep"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("chevron"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("snooze"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("park"))]),
    J::Obj(&[("enabled", J::Bool(true)), ("id", J::Str("close"))]),
]);

pub const SESSION_CARD_HOVER_BUTTON_LABELS: &[(&str, &str)] = &[
    ("chevron", "Chevron (hides the buttons to its left)"),
    ("close", "Close"),
    ("closeAfterDone", "Close After Done"),
    ("note", "Note"),
    ("park", "Park"),
    ("pin", "Pin"),
    ("rename", "Rename"),
    ("sleep", "Sleep"),
    ("snooze", "Snooze"),
    ("tag", "Tag"),
];
