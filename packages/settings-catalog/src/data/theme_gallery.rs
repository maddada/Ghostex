use crate::json::J;

/// The example themes Settings > Interface shows under "Theme it in your words": whole looks an
/// agent made by prompting (`ghostex theme set`), each with the short prompt that describes it.
/// Apply sends `changes` to `/api/theme` `set` as one change, the way `ghostex theme set` does, so
/// it lands in the theme history and Undo puts back what it replaced.
///
/// Per theme: `id` (its picture, apps/desktop/assets/modals/settings/theme-gallery/<id>.webp);
/// `name`; `label` (the one word under its tile, when the name's first word would not say it);
/// `art` (an art theme, whose `windowArt` artwork is apps/desktop/assets/themes/art/<id>.webp); `prompt` (what Copy prompt copies); `changes` (each `key: value` exactly as
/// `ghostex theme set` takes it; `dark.<colour>` is a theme colour). The themes are dark mode
/// only.
///
/// CDXC:Theming 2026-11-21 DECISION:
/// User: "non of the themes should change the fonts at all pls. keep fonts as they are", and of the other looks next to their own (Inkstone, their Windows look and since then the default dark look): "adjust the others so the colors lean the same way this does". No look names a font (a font key here would change the user's fonts), and every look follows Inkstone's way: a muted colour at depth 92 to 96 with no extra contrast, paper-white text, grey text, a muted accent, borders 8% and dividers 6%, near-neutral cards, controls, menus and chat box in the look's own hue, selection 9% and hover 5%, muted success, warning and danger, Inkstone's glass tints (82 in the sidebar, 88 in the work area) and a slow, dim Live background (speed 0.3 to 0.5, brightness 25 to 35). Each keeps its own hue, accent, Live style and terminal palette, so it stays the look it was named for. Supersedes the 2026-11-18 neon accents, tinted borders, -4 contrast and the fonts named as installed on Windows.
///
/// CDXC:Theming 2026-11-21 DECISION:
/// User (through the coordinator, after picking six dark art themes from the concept page): "They are the main themes: first in the theme gallery and in `ghostex theme`", and "Ghostex's current default look shows as Default, first". The row is Default (`theme_gallery`, the shipped defaults of every key a look sets, so it always matches the catalog), then the six art looks (`art: true`, each its artwork behind an opaque window through `windowArt`, its palette from the concept page as the theme colours, Colourfulness Full with the page's background as the colour, the way a preset paints its own), then the agent-made looks. Every other look sets `windowArt` to none, so applying one takes the artwork away.
///
/// CDXC:Theming 2026-11-18 WHY:
/// Each theme sets the custom colour's depth (`customSidebarTitlebarBackgroundDarknessPercent`)
/// itself, because a depth another theme left behind would change how deep its colour shows.
pub const THEME_GALLERY: J = J::Arr(&[
    J::Obj(&[
        ("id", J::Str("moonwater")),
        ("name", J::Str("Moonwater")),
        ("label", J::Str("Moonwater")),
        ("art", J::Bool(true)),
        (
            "prompt",
            J::Str("Give Ghostex the Moonwater art theme: a quiet mountain night in ink and silver, painted behind a solid window."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#121d28")),
                ("themeSidebarContrast", J::Str("-24")),
                ("themeWorkAreaContrast", J::Str("-24")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("85")),
                ("windowArt", J::Str("moonwater")),
                ("windowArtDim", J::Str("30")),
                ("windowArtBlur", J::Str("0")),
                ("windowGlassSidebarOpacityDark", J::Str("86")),
                ("windowGlassWorkAreaTintDark", J::Str("76")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#b9d6e7")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Ghostex Nord")),
                ("dark.accent", J::Str("#b9d6e7")),
                ("dark.focusRing", J::Str("#b9d6e7")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#e5eaf0")),
                ("dark.mutedText", J::Str("#a0adbd")),
                ("dark.userMessage", J::Str("#1d2a37")),
                ("dark.chatBox", J::Str("#141f2acc")),
                ("dark.surface", J::Str("#182430cc")),
                ("dark.control", J::Str("#1d2a37cc")),
                ("dark.menu", J::Str("#141f2af2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cbf8c")),
                ("dark.warning", J::Str("#d9b46a")),
                ("dark.danger", J::Str("#e06c75")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("glasshouse")),
        ("name", J::Str("Glasshouse")),
        ("label", J::Str("Glasshouse")),
        ("art", J::Bool(true)),
        (
            "prompt",
            J::Str("Give Ghostex the Glasshouse art theme: a secret garden after dark, with warm points of light, painted behind a solid window."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#15211a")),
                ("themeSidebarContrast", J::Str("-24")),
                ("themeWorkAreaContrast", J::Str("-24")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("85")),
                ("windowArt", J::Str("glasshouse")),
                ("windowArtDim", J::Str("30")),
                ("windowArtBlur", J::Str("0")),
                ("windowGlassSidebarOpacityDark", J::Str("86")),
                ("windowGlassWorkAreaTintDark", J::Str("76")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#b3c98c")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Ghostex Everforest Dark")),
                ("dark.accent", J::Str("#b3c98c")),
                ("dark.focusRing", J::Str("#b3c98c")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#e9ebdb")),
                ("dark.mutedText", J::Str("#a4b0a0")),
                ("dark.userMessage", J::Str("#213128")),
                ("dark.chatBox", J::Str("#17231ccc")),
                ("dark.surface", J::Str("#1c2a22cc")),
                ("dark.control", J::Str("#213128cc")),
                ("dark.menu", J::Str("#17231cf2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cbf8c")),
                ("dark.warning", J::Str("#d9b46a")),
                ("dark.danger", J::Str("#e06c75")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("red-earth")),
        ("name", J::Str("Red Earth")),
        ("label", J::Str("Earth")),
        ("art", J::Bool(true)),
        (
            "prompt",
            J::Str("Give Ghostex the Red Earth art theme: sculpted dunes with the warmth of late embers, painted behind a solid window."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#27191e")),
                ("themeSidebarContrast", J::Str("-24")),
                ("themeWorkAreaContrast", J::Str("-24")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("85")),
                ("windowArt", J::Str("red-earth")),
                ("windowArtDim", J::Str("30")),
                ("windowArtBlur", J::Str("0")),
                ("windowGlassSidebarOpacityDark", J::Str("86")),
                ("windowGlassWorkAreaTintDark", J::Str("76")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#e2a68d")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("SeedFlip Ember")),
                ("dark.accent", J::Str("#e2a68d")),
                ("dark.focusRing", J::Str("#e2a68d")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#f0e4da")),
                ("dark.mutedText", J::Str("#b4a19b")),
                ("dark.userMessage", J::Str("#38262a")),
                ("dark.chatBox", J::Str("#291b20cc")),
                ("dark.surface", J::Str("#302025cc")),
                ("dark.control", J::Str("#38262acc")),
                ("dark.menu", J::Str("#291b20f2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cbf8c")),
                ("dark.warning", J::Str("#d9b46a")),
                ("dark.danger", J::Str("#e06c75")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("deep-current")),
        ("name", J::Str("Deep Current")),
        ("label", J::Str("Current")),
        ("art", J::Bool(true)),
        (
            "prompt",
            J::Str("Give Ghostex the Deep Current art theme: a calm descent through luminous ocean currents, painted behind a solid window."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#0e2326")),
                ("themeSidebarContrast", J::Str("-24")),
                ("themeWorkAreaContrast", J::Str("-24")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("85")),
                ("windowArt", J::Str("deep-current")),
                ("windowArtDim", J::Str("30")),
                ("windowArtBlur", J::Str("0")),
                ("windowGlassSidebarOpacityDark", J::Str("86")),
                ("windowGlassWorkAreaTintDark", J::Str("76")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#7ecdc5")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Ghostex Kanagawa Wave")),
                ("dark.accent", J::Str("#7ecdc5")),
                ("dark.focusRing", J::Str("#7ecdc5")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#e0ecea")),
                ("dark.mutedText", J::Str("#95adaa")),
                ("dark.userMessage", J::Str("#183438")),
                ("dark.chatBox", J::Str("#102528cc")),
                ("dark.surface", J::Str("#142c30cc")),
                ("dark.control", J::Str("#183438cc")),
                ("dark.menu", J::Str("#102528f2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cbf8c")),
                ("dark.warning", J::Str("#d9b46a")),
                ("dark.danger", J::Str("#e06c75")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("night-garden")),
        ("name", J::Str("Night Garden")),
        ("label", J::Str("Garden")),
        ("art", J::Bool(true)),
        (
            "prompt",
            J::Str("Give Ghostex the Night Garden art theme: wildflowers and soft twilight in a plum dusk, painted behind a solid window."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#261a29")),
                ("themeSidebarContrast", J::Str("-24")),
                ("themeWorkAreaContrast", J::Str("-24")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("85")),
                ("windowArt", J::Str("night-garden")),
                ("windowArtDim", J::Str("30")),
                ("windowArtBlur", J::Str("0")),
                ("windowGlassSidebarOpacityDark", J::Str("86")),
                ("windowGlassWorkAreaTintDark", J::Str("76")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#d8a8ce")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Rose Pine")),
                ("dark.accent", J::Str("#d8a8ce")),
                ("dark.focusRing", J::Str("#d8a8ce")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#eee4ec")),
                ("dark.mutedText", J::Str("#b2a2b1")),
                ("dark.userMessage", J::Str("#39283b")),
                ("dark.chatBox", J::Str("#281c2bcc")),
                ("dark.surface", J::Str("#302233cc")),
                ("dark.control", J::Str("#39283bcc")),
                ("dark.menu", J::Str("#281c2bf2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cbf8c")),
                ("dark.warning", J::Str("#d9b46a")),
                ("dark.danger", J::Str("#e06c75")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("nocturne-canyon")),
        ("name", J::Str("Nocturne Canyon")),
        ("label", J::Str("Canyon")),
        ("art", J::Bool(true)),
        (
            "prompt",
            J::Str("Give Ghostex the Nocturne Canyon art theme: a vast desert landscape under a hazy moon, painted behind a solid window."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#27201a")),
                ("themeSidebarContrast", J::Str("-24")),
                ("themeWorkAreaContrast", J::Str("-24")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("85")),
                ("windowArt", J::Str("nocturne-canyon")),
                ("windowArtDim", J::Str("30")),
                ("windowArtBlur", J::Str("0")),
                ("windowGlassSidebarOpacityDark", J::Str("86")),
                ("windowGlassWorkAreaTintDark", J::Str("76")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#ddb985")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Ghostex Gruvbox Material Dark")),
                ("dark.accent", J::Str("#ddb985")),
                ("dark.focusRing", J::Str("#ddb985")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#f0e7d9")),
                ("dark.mutedText", J::Str("#b5a898")),
                ("dark.userMessage", J::Str("#3b3027")),
                ("dark.chatBox", J::Str("#29221ccc")),
                ("dark.surface", J::Str("#322922cc")),
                ("dark.control", J::Str("#3b3027cc")),
                ("dark.menu", J::Str("#29221cf2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cbf8c")),
                ("dark.warning", J::Str("#d9b46a")),
                ("dark.danger", J::Str("#e06c75")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("firefly-forest")),
        ("name", J::Str("Firefly Forest")),
        (
            "prompt",
            J::Str("Theme Ghostex like a forest at night: deep moss glass, warm paper text, soft lime accents and slowly drifting light."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#4a5f4f")),
                ("themeSidebarContrast", J::Str("0")),
                ("themeWorkAreaContrast", J::Str("0")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("96")),
                ("windowGlass", J::Str("frosted")),
                ("windowArt", J::Str("none")),
                ("windowGlassSource", J::Str("live")),
                ("windowGlassLiveStyleDark", J::Str("drift")),
                ("windowGlassLiveSpeed", J::Str("0.4")),
                ("windowGlassLiveBrightness", J::Str("30")),
                ("windowGlassSidebarOpacityDark", J::Str("82")),
                ("windowGlassWorkAreaTintDark", J::Str("88")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#a9c46c")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Ghostex Everforest Dark")),
                ("dark.accent", J::Str("#a9c46c")),
                ("dark.focusRing", J::Str("#a9c46c")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#e8e6d6")),
                ("dark.mutedText", J::Str("#979c8b")),
                ("dark.userMessage", J::Str("#252b26")),
                ("dark.chatBox", J::Str("#151a16cc")),
                ("dark.surface", J::Str("#181e19cc")),
                ("dark.control", J::Str("#1e251fcc")),
                ("dark.menu", J::Str("#141914f2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cc48c")),
                ("dark.warning", J::Str("#d9b46a")),
                ("dark.danger", J::Str("#e06c75")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("ember-forge")),
        ("name", J::Str("Ember Forge")),
        (
            "prompt",
            J::Str("Give Ghostex a warm forge feel: deep charcoal-brown glass, warm paper text, muted copper accents and a slow ember glow."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#5e4a40")),
                ("themeSidebarContrast", J::Str("0")),
                ("themeWorkAreaContrast", J::Str("0")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("96")),
                ("windowGlass", J::Str("frosted")),
                ("windowArt", J::Str("none")),
                ("windowGlassSource", J::Str("live")),
                ("windowGlassLiveStyleDark", J::Str("nebula")),
                ("windowGlassLiveSpeed", J::Str("0.3")),
                ("windowGlassLiveBrightness", J::Str("30")),
                ("windowGlassSidebarOpacityDark", J::Str("82")),
                ("windowGlassWorkAreaTintDark", J::Str("88")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#d9824a")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("SeedFlip Ember")),
                ("dark.accent", J::Str("#d9824a")),
                ("dark.focusRing", J::Str("#d9824a")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#efe3d4")),
                ("dark.mutedText", J::Str("#a3958a")),
                ("dark.userMessage", J::Str("#2d2520")),
                ("dark.chatBox", J::Str("#1a1512cc")),
                ("dark.surface", J::Str("#1e1814cc")),
                ("dark.control", J::Str("#251e19cc")),
                ("dark.menu", J::Str("#171310f2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#a8bf7c")),
                ("dark.warning", J::Str("#ddb36a")),
                ("dark.danger", J::Str("#e06c6c")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("synthwave-84")),
        ("name", J::Str("Synthwave '84")),
        (
            "prompt",
            J::Str("Give Ghostex an \u{2019}80s night-drive feel: deep violet glass, soft white text, muted pink accents and a soft cyan glow."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#4f4468")),
                ("themeSidebarContrast", J::Str("0")),
                ("themeWorkAreaContrast", J::Str("0")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("96")),
                ("windowGlass", J::Str("frosted")),
                ("windowArt", J::Str("none")),
                ("windowGlassSource", J::Str("live")),
                ("windowGlassLiveStyleDark", J::Str("mesh")),
                ("windowGlassLiveSpeed", J::Str("0.5")),
                ("windowGlassLiveBrightness", J::Str("35")),
                ("windowGlassSidebarOpacityDark", J::Str("82")),
                ("windowGlassWorkAreaTintDark", J::Str("88")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#d77cc8")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Synthwave")),
                ("dark.accent", J::Str("#d77cc8")),
                ("dark.focusRing", J::Str("#6fc6c4")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#ece4ea")),
                ("dark.mutedText", J::Str("#a094a6")),
                ("dark.userMessage", J::Str("#2b2637")),
                ("dark.chatBox", J::Str("#18151dcc")),
                ("dark.surface", J::Str("#1c1822cc")),
                ("dark.control", J::Str("#231e2acc")),
                ("dark.menu", J::Str("#16131bf2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#8fcfae")),
                ("dark.warning", J::Str("#e0c47a")),
                ("dark.danger", J::Str("#e6707a")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("polar-night")),
        ("name", J::Str("Polar Night")),
        (
            "prompt",
            J::Str("Theme Ghostex in deep ink navy with a slow, faint blue haze, cool white text and subtle jade accents."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#404e78")),
                ("themeSidebarContrast", J::Str("0")),
                ("themeWorkAreaContrast", J::Str("0")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("92")),
                ("windowGlass", J::Str("frosted")),
                ("windowArt", J::Str("none")),
                ("windowGlassSource", J::Str("live")),
                ("windowGlassLiveStyleDark", J::Str("aurora")),
                ("windowGlassLiveSpeed", J::Str("0.3")),
                ("windowGlassLiveBrightness", J::Str("30")),
                ("windowGlassSidebarOpacityDark", J::Str("82")),
                ("windowGlassWorkAreaTintDark", J::Str("88")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#7cbf9f")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Ghostex Nord")),
                ("dark.accent", J::Str("#7cbf9f")),
                ("dark.focusRing", J::Str("#7cbf9f")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#e6e4de")),
                ("dark.mutedText", J::Str("#8c95a3")),
                ("dark.userMessage", J::Str("#26304a")),
                ("dark.chatBox", J::Str("#181e2bcc")),
                ("dark.surface", J::Str("#1b2232cc")),
                ("dark.control", J::Str("#212939cc")),
                ("dark.menu", J::Str("#171d2af2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#a3c48e")),
                ("dark.warning", J::Str("#dcbd7a")),
                ("dark.danger", J::Str("#d9827c")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("inkstone")),
        ("name", J::Str("Inkstone")),
        (
            "prompt",
            J::Str("Give Ghostex cool blue-black surfaces, warm paper-white text and muted cinnabar accents."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#46506a")),
                ("themeSidebarContrast", J::Str("0")),
                ("themeWorkAreaContrast", J::Str("0")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("96")),
                ("windowGlass", J::Str("frosted")),
                ("windowArt", J::Str("none")),
                ("windowGlassSource", J::Str("live")),
                ("windowGlassLiveStyleDark", J::Str("ink")),
                ("windowGlassLiveSpeed", J::Str("0.3")),
                ("windowGlassLiveBrightness", J::Str("25")),
                ("windowGlassSidebarOpacityDark", J::Str("82")),
                ("windowGlassWorkAreaTintDark", J::Str("88")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#d8664f")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Ghostex Kanagawa Dragon")),
                ("dark.accent", J::Str("#d8664f")),
                ("dark.focusRing", J::Str("#d8664f")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#ece4d6")),
                ("dark.mutedText", J::Str("#9b968d")),
                ("dark.userMessage", J::Str("#24272e")),
                ("dark.chatBox", J::Str("#15171bcc")),
                ("dark.surface", J::Str("#181a1fcc")),
                ("dark.control", J::Str("#1e2026cc")),
                ("dark.menu", J::Str("#141519f2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cbf8c")),
                ("dark.warning", J::Str("#d9b46a")),
                ("dark.danger", J::Str("#e06c75")),
            ]),
        ),
    ]),
    J::Obj(&[
        ("id", J::Str("orchid-dusk")),
        ("name", J::Str("Orchid Dusk")),
        (
            "prompt",
            J::Str("Give Ghostex muted plum glass with a faint, slow silk haze, soft white text and dusty rose accents."),
        ),
        (
            "changes",
            J::Obj(&[
                ("darkThemePreset", J::Str("custom")),
                ("customSidebarTitlebarBackgroundTintColor", J::Str("#725868")),
                ("themeSidebarContrast", J::Str("0")),
                ("themeWorkAreaContrast", J::Str("0")),
                ("customSidebarTitlebarBackgroundDarknessPercent", J::Str("94")),
                ("windowGlass", J::Str("frosted")),
                ("windowArt", J::Str("none")),
                ("windowGlassSource", J::Str("live")),
                ("windowGlassLiveStyleDark", J::Str("silk")),
                ("windowGlassLiveSpeed", J::Str("0.3")),
                ("windowGlassLiveBrightness", J::Str("30")),
                ("windowGlassSidebarOpacityDark", J::Str("82")),
                ("windowGlassWorkAreaTintDark", J::Str("88")),
                ("showActivePaneOutline", J::Str("false")),
                ("workspaceActivePaneBorderColor", J::Str("#d69bb5")),
                ("workspacePaneGap", J::Str("6")),
                ("sessionChatTextStrengthPercent", J::Str("0")),
                ("terminalGhosttyTheme", J::Str("Rose Pine")),
                ("dark.accent", J::Str("#d69bb5")),
                ("dark.focusRing", J::Str("#d69bb5")),
                ("dark.border", J::Str("8%")),
                ("dark.divider", J::Str("6%")),
                ("dark.text", J::Str("#ece4df")),
                ("dark.mutedText", J::Str("#9e939c")),
                ("dark.userMessage", J::Str("#33283a")),
                ("dark.chatBox", J::Str("#1d181fcc")),
                ("dark.surface", J::Str("#211b24cc")),
                ("dark.control", J::Str("#28202bcc")),
                ("dark.menu", J::Str("#1b161df2")),
                ("dark.selection", J::Str("9%")),
                ("dark.hover", J::Str("5%")),
                ("dark.success", J::Str("#9cc49a")),
                ("dark.warning", J::Str("#e0bd84")),
                ("dark.danger", J::Str("#e2727a")),
            ]),
        ),
    ]),
]);

/// The id and name of the look that puts Ghostex's shipped look back.
pub const DEFAULT_LOOK_ID: &str = "default";
pub const DEFAULT_LOOK_NAME: &str = "Default";

/// A setting's catalog default for `platform`, written the way `ghostex theme set` takes it.
fn default_text(platform: crate::Platform, key: &str) -> Option<String> {
    if let Some((appearance, role)) = key.split_once('.') {
        let roles = crate::default_value(crate::theme_roles::THEME_ROLES_SETTING)?;
        return Some(
            roles
                .get(appearance)
                .and_then(|roles| roles.get(role))
                .and_then(J::as_str)
                .unwrap_or("default")
                .to_string(),
        );
    }
    Some(
        match crate::availability::default_value_on(platform, key)? {
            J::Str(text) => text.to_string(),
            J::Bool(flag) => flag.to_string(),
            J::Num(number) => crate::json::Json::Num(*number).to_pretty_string(),
            _ => return None,
        },
    )
}

/// The themes row as Settings and `ghostex theme` show it: Default first, then `THEME_GALLERY`.
/// Default sets every key any look sets back to `platform`'s shipped value (a theme colour with no
/// shipped value to `default`), so applying it puts Ghostex's own look back.
pub fn theme_gallery(platform: crate::Platform) -> crate::json::Json {
    use crate::json::{Json, ToJson};
    let mut keys: Vec<&'static str> = Vec::new();
    for look in THEME_GALLERY.as_array() {
        for (key, _) in look.get("changes").map(J::entries).unwrap_or_default() {
            if !keys.contains(key) {
                keys.push(key);
            }
        }
    }
    let changes = keys
        .into_iter()
        .filter_map(|key| default_text(platform, key).map(|value| (key, Json::Str(value))));
    let default = Json::obj([
        ("id", Json::str(DEFAULT_LOOK_ID)),
        ("name", Json::str(DEFAULT_LOOK_NAME)),
        (
            "prompt",
            Json::str(
                "Put Ghostex's own look back: its default colours, glass and Live background.",
            ),
        ),
        ("changes", Json::obj(changes)),
    ]);
    let mut looks = vec![default];
    if let Json::Arr(rest) = THEME_GALLERY.to_json() {
        looks.extend(rest);
    }
    Json::Arr(looks)
}

#[cfg(test)]
mod tests {
    use super::THEME_GALLERY;
    use crate::data::theme_presets::THEME_PRESETS;

    /// The font settings of Settings > Text & Fonts (and the terminal's font size): a theme never
    /// sets one (CDXC:Theming 2026-11-21 above).
    const FONT_KEYS: [&str; 7] = [
        "sessionChatFontFamily",
        "terminalFontFamily",
        "terminalFontSize",
        "terminalFontWeight",
        "terminalBoldUsesHeavierFont",
        "terminalLineHeight",
        "terminalLetterSpacing",
    ];

    #[test]
    fn no_look_or_preset_changes_a_font() {
        for look in THEME_GALLERY.as_array() {
            let id = look.get("id").and_then(|id| id.as_str()).unwrap();
            let changes = look.get("changes").unwrap().entries();
            for (key, _) in changes {
                assert!(!FONT_KEYS.contains(key), "{id} sets {key}");
                assert!(!key.to_lowercase().contains("font"), "{id} sets {key}");
            }
        }
        for preset in THEME_PRESETS.as_array() {
            for (key, _) in preset.entries() {
                assert!(!key.to_lowercase().contains("font"), "a preset names {key}");
            }
        }
    }
}
