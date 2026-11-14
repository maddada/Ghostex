use crate::json::{opt, Opt, J};

/// The tag list a first run and Reset to Default write: the six computed status entries first
/// (`type: "status"`, no `tag`), then the hand tags, separators and No tag.
///
/// CDXC:Sidebar 2026-11-14 SEE-ALSO: the status entries, their ids and order are the stored contract of gx-core sidebar_view/tags.rs (`StatusGroup`), packages/shared/session-tags.ts and the phone's src/contract/sessionTagList.ts.
pub const DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS: J = J::Arr(&[
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("status-needs-you")),
        ("type", J::Str("status")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("status-question")),
        ("type", J::Str("status")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("status-error")),
        ("type", J::Str("status")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("status-finished")),
        ("type", J::Str("status")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("status-working")),
        ("type", J::Str("status")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("status-idle")),
        ("type", J::Str("status")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("favorite")),
        ("tag", J::Str("favorite")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(false)),
        ("id", J::Str("high-priority")),
        ("tag", J::Str("high-priority")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(false)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(false)),
        ("id", J::Str("low-priority")),
        ("tag", J::Str("low-priority")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(false)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("separator-priority-progress")),
        ("type", J::Str("separator")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(false)),
        ("id", J::Str("todo")),
        ("tag", J::Str("todo")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(false)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("in-progress")),
        ("tag", J::Str("in-progress")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("testing")),
        ("tag", J::Str("testing")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("blocked")),
        ("tag", J::Str("blocked")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("on-hold")),
        ("tag", J::Str("on-hold")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("done")),
        ("tag", J::Str("done")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("separator-progress-type")),
        ("type", J::Str("separator")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("research")),
        ("tag", J::Str("research")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(false)),
        ("id", J::Str("bug")),
        ("tag", J::Str("bug")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(false)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(false)),
        ("id", J::Str("feature")),
        ("tag", J::Str("feature")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(false)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("design")),
        ("tag", J::Str("design")),
        ("type", J::Str("tag")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("separator-type-untagged")),
        ("type", J::Str("separator")),
        ("visible", J::Bool(true)),
    ]),
    J::Obj(&[
        ("enabled", J::Bool(true)),
        ("id", J::Str("untagged")),
        ("type", J::Str("untagged")),
        ("visible", J::Bool(true)),
    ]),
]);

/// CDXC:Sessions 2026-09-11 DECISION:
/// User: custom tag colors come from a preset list. The presets are the built-in tag hues, which are already tuned to read as a 15px stroke glyph on the dark sidebar; the collection palette's dim tones vanish at that size.
/// Mirrored in server/src/custom_session_tags.rs for the server-side fallback rotation.
pub const SESSION_TAG_COLOR_PRESETS: &[Opt] = &[
    opt("Gold", "#f3cc5f"),
    opt("Coral", "#ff8b6b"),
    opt("Amber", "#f0c66e"),
    opt("Mint", "#4ee6b8"),
    opt("Sky", "#59d9ff"),
    opt("Ice", "#95d7f6"),
    opt("Blue", "#8fb8ff"),
    opt("Lavender", "#d2a7ff"),
    opt("Pink", "#ff9ee7"),
    opt("Rose", "#ff5f73"),
    opt("Brick", "#a54646"),
    opt("Silver", "#d9dee6"),
    opt("Gray", "#8e949d"),
];

pub const SIDEBAR_SESSION_TAGS: &[&str] = &[
    "favorite",
    "high-priority",
    "low-priority",
    "todo",
    "research",
    "in-progress",
    "testing",
    "blocked",
    "on-hold",
    "done",
    "bug",
    "feature",
    "design",
];

pub const SIDEBAR_SESSION_TAG_LIST_SEPARATOR_IDS: &[&str] = &[
    "separator-priority-progress",
    "separator-progress-type",
    "separator-type-untagged",
];

pub const SIDEBAR_SESSION_TAG_OPTIONS: &[Opt] = &[
    opt("Favorite", "favorite"),
    opt("High Priority", "high-priority"),
    opt("Low Priority", "low-priority"),
    opt("Todo", "todo"),
    opt("In Progress", "in-progress"),
    opt("Testing", "testing"),
    opt("Blocked", "blocked"),
    opt("On Hold", "on-hold"),
    opt("Done", "done"),
    opt("Research", "research"),
    opt("Bug", "bug"),
    opt("Feature", "feature"),
    opt("Design", "design"),
];

/// The six computed status entries of the tag list, in their default order: what each is called.
pub const SIDEBAR_SESSION_STATUS_OPTIONS: &[Opt] = &[
    opt("Needs you", "status-needs-you"),
    opt("Question", "status-question"),
    opt("Error", "status-error"),
    opt("Finished (not yet read)", "status-finished"),
    opt("Working", "status-working"),
    opt("Idle", "status-idle"),
];
