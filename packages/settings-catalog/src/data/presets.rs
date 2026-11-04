use crate::json::{opt, Opt, J};

pub const DARK_THEME_PRESET_CONTROLS: J = J::Obj(&[
    (
        "gray",
        J::Obj(&[
            ("darknessPercent", J::Num(96.0)),
            ("tintColor", J::Str("#808080")),
        ]),
    ),
    (
        "black",
        J::Obj(&[
            ("darknessPercent", J::Num(100.0)),
            ("tintColor", J::Str("#000000")),
        ]),
    ),
    (
        "slate",
        J::Obj(&[
            ("darknessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#4a6a8a")),
        ]),
    ),
    (
        "midnight",
        J::Obj(&[
            ("darknessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#1f3a8a")),
        ]),
    ),
    (
        "blue",
        J::Obj(&[
            ("darknessPercent", J::Num(96.0)),
            ("tintColor", J::Str("#336699")),
        ]),
    ),
    (
        "indigo",
        J::Obj(&[
            ("darknessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#4b4fa6")),
        ]),
    ),
    (
        "teal",
        J::Obj(&[
            ("darknessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#2f7f7f")),
        ]),
    ),
    (
        "green",
        J::Obj(&[
            ("darknessPercent", J::Num(96.0)),
            ("tintColor", J::Str("#3f7a5f")),
        ]),
    ),
    (
        "forest",
        J::Obj(&[
            ("darknessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#2e6a3a")),
        ]),
    ),
    (
        "olive",
        J::Obj(&[
            ("darknessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#6b6b35")),
        ]),
    ),
    (
        "amber",
        J::Obj(&[
            ("darknessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#8a6a2a")),
        ]),
    ),
    (
        "orange",
        J::Obj(&[
            ("darknessPercent", J::Num(96.0)),
            ("tintColor", J::Str("#8a5330")),
        ]),
    ),
    (
        "red",
        J::Obj(&[
            ("darknessPercent", J::Num(96.0)),
            ("tintColor", J::Str("#884444")),
        ]),
    ),
    (
        "rose",
        J::Obj(&[
            ("darknessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#8a4a5c")),
        ]),
    ),
    (
        "pink",
        J::Obj(&[
            ("darknessPercent", J::Num(96.0)),
            ("tintColor", J::Str("#854f7a")),
        ]),
    ),
    (
        "purple",
        J::Obj(&[
            ("darknessPercent", J::Num(96.0)),
            ("tintColor", J::Str("#6c4f8f")),
        ]),
    ),
]);

/// CDXC:Theming 2026-09-22 DECISION:
/// User: two Theme dropdowns, one for the light theme and one for the dark theme, list preset themes plus
/// Custom; Custom reveals that appearance's background contrast and tint controls.
pub const DARK_THEME_PRESET_OPTIONS: &[Opt] = &[
    opt("Graphite", "gray"),
    opt("Black", "black"),
    opt("Slate", "slate"),
    opt("Midnight", "midnight"),
    opt("Blue", "blue"),
    opt("Indigo", "indigo"),
    opt("Teal", "teal"),
    opt("Green", "green"),
    opt("Forest", "forest"),
    opt("Olive", "olive"),
    opt("Amber", "amber"),
    opt("Orange", "orange"),
    opt("Red", "red"),
    opt("Rose", "rose"),
    opt("Pink", "pink"),
    opt("Purple", "purple"),
    opt("Custom", "custom"),
];

pub const LIGHT_THEME_PRESET_CONTROLS: J = J::Obj(&[
    (
        "gray",
        J::Obj(&[
            ("lightnessPercent", J::Num(96.0)),
            ("tintColor", J::Str("#808080")),
        ]),
    ),
    (
        "white",
        J::Obj(&[
            ("lightnessPercent", J::Num(100.0)),
            ("tintColor", J::Str("#ffffff")),
        ]),
    ),
    (
        "slate",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#4a6a8a")),
        ]),
    ),
    (
        "midnight",
        J::Obj(&[
            ("lightnessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#1f3a8a")),
        ]),
    ),
    (
        "blue",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#336699")),
        ]),
    ),
    (
        "indigo",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#4b4fa6")),
        ]),
    ),
    (
        "teal",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#2f7f7f")),
        ]),
    ),
    (
        "green",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#3f7a5f")),
        ]),
    ),
    (
        "forest",
        J::Obj(&[
            ("lightnessPercent", J::Num(94.0)),
            ("tintColor", J::Str("#2e6a3a")),
        ]),
    ),
    (
        "olive",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#6b6b35")),
        ]),
    ),
    (
        "amber",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#8a6a2a")),
        ]),
    ),
    (
        "orange",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#8a5330")),
        ]),
    ),
    (
        "red",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#884444")),
        ]),
    ),
    (
        "rose",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#8a4a5c")),
        ]),
    ),
    (
        "pink",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#854f7a")),
        ]),
    ),
    (
        "purple",
        J::Obj(&[
            ("lightnessPercent", J::Num(95.0)),
            ("tintColor", J::Str("#6c4f8f")),
        ]),
    ),
]);

pub const LIGHT_THEME_PRESET_OPTIONS: &[Opt] = &[
    opt("Graphite", "gray"),
    opt("White", "white"),
    opt("Slate", "slate"),
    opt("Midnight", "midnight"),
    opt("Blue", "blue"),
    opt("Indigo", "indigo"),
    opt("Teal", "teal"),
    opt("Green", "green"),
    opt("Forest", "forest"),
    opt("Olive", "olive"),
    opt("Amber", "amber"),
    opt("Orange", "orange"),
    opt("Red", "red"),
    opt("Rose", "rose"),
    opt("Pink", "pink"),
    opt("Purple", "purple"),
    opt("Custom", "custom"),
];

pub const SIDEBAR_SETTINGS_PRESETS: J = J::Arr(&[
    J::Obj(&[
        ("id", J::Str("recommended")),
        ("label", J::Str("Recommended")),
        (
            "settings",
            J::Obj(&[
                ("showProjectIcons", J::Bool(true)),
                ("hideSessionAgentIconUntilHover", J::Bool(false)),
                ("hideBrowserFaviconUntilHover", J::Bool(false)),
                ("hideLastActiveTimeOnSessionCards", J::Bool(true)),
                ("hideProjectHeaderDiffStats", J::Bool(false)),
                ("showProjectEditorDiffFileCount", J::Bool(false)),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("codex")),
        ("label", J::Str("Codex")),
        (
            "settings",
            J::Obj(&[
                ("showProjectIcons", J::Bool(true)),
                ("hideSessionAgentIconUntilHover", J::Bool(true)),
                ("hideBrowserFaviconUntilHover", J::Bool(false)),
                ("hideLastActiveTimeOnSessionCards", J::Bool(false)),
                ("hideProjectHeaderDiffStats", J::Bool(true)),
                ("showProjectEditorDiffFileCount", J::Bool(false)),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("minimal")),
        ("label", J::Str("Minimal")),
        (
            "settings",
            J::Obj(&[
                ("showProjectIcons", J::Bool(false)),
                ("hideSessionAgentIconUntilHover", J::Bool(true)),
                ("hideBrowserFaviconUntilHover", J::Bool(true)),
                ("hideLastActiveTimeOnSessionCards", J::Bool(true)),
                ("hideProjectHeaderDiffStats", J::Bool(true)),
                ("showProjectEditorDiffFileCount", J::Bool(false)),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("detailed")),
        ("label", J::Str("Detailed")),
        (
            "settings",
            J::Obj(&[
                ("showProjectIcons", J::Bool(true)),
                ("hideSessionAgentIconUntilHover", J::Bool(false)),
                ("hideBrowserFaviconUntilHover", J::Bool(false)),
                ("hideLastActiveTimeOnSessionCards", J::Bool(false)),
                ("hideProjectHeaderDiffStats", J::Bool(false)),
                ("showProjectEditorDiffFileCount", J::Bool(false)),
            ]),
        ),
    ]),
]);

pub const SIDEBAR_SETTINGS_PRESET_KEYS: &[&str] = &[
    "showProjectIcons",
    "hideSessionAgentIconUntilHover",
    "hideBrowserFaviconUntilHover",
    "hideLastActiveTimeOnSessionCards",
    "hideProjectHeaderDiffStats",
    "showProjectEditorDiffFileCount",
];

/// CDXC:Theming 2026-09-08 WHY:
/// Custom tint calibration must stay stable when the first-run background default changes.
pub const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_COLOR: &str = "#040607";

pub const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS: &[(&str, &str)] = &[
    ("#000000", "#000000"),
    ("#ffffff", "#0e0e0e"),
    ("#808080", "#0e0e0e"),
    ("#88d7ff", "#0a0f12"),
    ("#4f6672", "#0c0e10"),
    ("#884444", "#0d0005"),
    ("#8a5330", "#100502"),
    ("#8a6a2f", "#110a02"),
    ("#657a3f", "#0c1005"),
    ("#3f7a5f", "#031006"),
    ("#2f7d66", "#03100c"),
    ("#287c7f", "#031011"),
    ("#336699", "#0c0e11"),
    ("#4f5f96", "#080912"),
    ("#6c4f8f", "#0a0611"),
    ("#854f7a", "#100611"),
    ("#8a4f5f", "#100409"),
    ("#4a6a8a", "#070d14"),
    ("#1f3a8a", "#02061a"),
    ("#4b4fa6", "#08081c"),
    ("#2f7f7f", "#021213"),
    ("#2e6a3a", "#031205"),
    ("#6b6b35", "#0e0f03"),
    ("#8a6a2a", "#130c02"),
    ("#8a4a5c", "#12040b"),
];

pub const CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS: &[(&str, &str)] = &[
    ("#000000", "#f1f1f2"),
    ("#ffffff", "#f1f1f2"),
    ("#808080", "#f1f1f2"),
    ("#88d7ff", "#edf4fa"),
    ("#4f6672", "#eef1f3"),
    ("#884444", "#f7ecec"),
    ("#8a5330", "#f8f0e9"),
    ("#8a6a2f", "#f7f3e8"),
    ("#657a3f", "#f1f5ea"),
    ("#3f7a5f", "#ecf4ee"),
    ("#2f7d66", "#eaf4f0"),
    ("#287c7f", "#eaf4f4"),
    ("#336699", "#ecf1f7"),
    ("#4f5f96", "#eff0f7"),
    ("#6c4f8f", "#f2edf7"),
    ("#854f7a", "#f7ecf3"),
    ("#8a4f5f", "#f7ecef"),
    ("#4a6a8a", "#ebf1f8"),
    ("#1f3a8a", "#edeff8"),
    ("#4b4fa6", "#eeeef8"),
    ("#2f7f7f", "#ebf4f5"),
    ("#2e6a3a", "#edf7f0"),
    ("#6b6b35", "#f4f4ec"),
    ("#8a6a2a", "#f6f2ec"),
    ("#8a4a5c", "#f7edf0"),
];

pub const CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_COLOR: &str = "#f1f1f2";
