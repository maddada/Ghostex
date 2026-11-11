use crate::data::*;
use crate::hotkey_label::hotkey_label;
use crate::json::opt;
use crate::platform_text::{
    copy_on_select_description, copy_on_select_options, paste_previewable_images_description,
};
use crate::rows::{row, section, Section};
use crate::Platform;

pub(crate) fn terminal(platform: Platform) -> Section {
    let mut settings = Vec::new();
    if platform == Platform::Windows {
        settings.extend([
            row("windowsTerminalBackend", "Windows environment", "PowerShell (default) runs native Windows projects and agents. WSL runs Linux projects. Changing environments prompts you to restart Ghostex.")
                .options(&[opt("PowerShell (Native)", "powershell"), opt("WSL (Linux)", "wsl")]),
            row("windowsWslDistribution", "WSL distribution", "Optional exact distro name from `wsl.exe --list --verbose`; blank uses automatic WSL2 discovery."),
            row("windowsPowerShell7", "PowerShell 7", "New terminals use PowerShell 7 when it is installed (Program Files, your user folder, the Microsoft Store or PATH) and Windows PowerShell 5.1 until then. Install PowerShell 7 runs winget for you; terminals that are already open keep their shell."),
        ]);
    }
    settings.extend([
        row("ghosttySettingsActions", "Ghostty settings actions", "Recommended terminal settings, Import from Ghostty (fonts and cursor, not the theme), the Ghostty app's config, Ghostex's own terminal config, Ghostty docs, and defaults.").options(&[opt("Open Ghostex config", "openGhostexTerminalConfigFile"), opt("Open Ghostty config", "openGhosttyConfigFile"), opt("Import from Ghostty", "importGhosttyConfig"), opt("Apply recommended", "applyRecommendedGhosttySettings"), opt("Reset to defaults", "resetGhosttySettingsToDefault"), opt("Ghostty docs", "openGhosttySettingsDocs")]),
        row("terminalBackgroundMode", "Terminal background", "Only changes the terminal panes. Black / white is pure black in dark mode and pure white in light mode.").options(&[opt("Black / white", "pure"), opt("Follow theme", "theme"), opt("Custom color", "custom")]),
        row("workspaceBackgroundColor", "Terminal background color", "Custom terminal background, painted behind terminal text in dark and light mode."),
        row("terminalBackgroundOpacity", "Background opacity (%)", "How much of the work area's glass tint terminals keep while transparency is on (Settings > Interface > Transparency). 100% matches the rest of the work area; lower shows more of what is behind the window. Off while transparency is off."),
        // CDXC:Settings 2026-10-29 DECISION (cleanup Q15, Q16, Q18): Background blur (Theme's Blur slider) and
        // Background Image Fit (the image always covers the pane) left this page; Show notification on
        // terminal bell moved to Notifications. Terminal width is one slider (100% is the full pane) with a
        // Match chat switch, and Padding is one slider that sets both axes.
        row("terminalBackgroundImage", "Background image", "Absolute path to an image drawn behind terminal panes."),
        row("terminalBackgroundImageOpacity", "Background image opacity", "Blend the background image toward the terminal background color."),
        row("terminalFontWeight", "Font weight", "Set terminal text weight."),
        row("terminalBoldUsesHeavierFont", "Bold text uses a heavier font", "Draw bold terminal text in a heavier weight. Turn off to keep bold text at your normal font weight, so it stands out by color only. A font-style-bold or font-variation-bold line in Ghostex's terminal config takes priority."),
        row("terminalLineHeight", "Line height", "Adjust terminal row height."),
        row("terminalLetterSpacing", "Letter spacing", "Adjust spacing between glyphs."),
        row("terminalViewWidthMode", "Match chat width", "Use the full pane, match the chat transcript, or set an independent terminal width.").options(TERMINAL_VIEW_WIDTH_MODE_OPTIONS),
        row("terminalViewWidthPercent", "Terminal width (%)", "Set the centered terminal body width as a percentage."),
        row("terminalWidthApplyToCommandPaneTerminals", "Use this width in the Commands pane too", "Apply the narrower terminal width to command pane terminals too."),
        row("terminalPaneHorizontalPaddingPx", "Padding", "Add left and right inner padding inside the terminal content area."),
        row("terminalPaneVerticalPaddingPx", "Vertical padding (set with Padding)", "Add top and bottom inner padding inside the terminal content area."),
        row("terminalCursorStyle", "Cursor style", "Choose the cursor shape.").options(&[opt("Line", "bar"), opt("Block", "block"), opt("Underline", "underline")]),
        row("terminalCursorStyleBlink", "Cursor blink", "Blink the terminal cursor."),
        row("clickToWakeSleepingSessions", "Keep sleeping tabs asleep until clicked", "Select sleeping pane tabs without waking them until the empty pane is clicked."),
        row("showQuickModelPickerInTerminal", "Model picker in terminal view", format!("Show a model button in the terminal bar and open the model picker with its shortcut ({} by default) in agent terminal sessions. Turn off to use terminal bindings.", hotkey_label("alt+p", platform))),
        row("showSessionIdInTerminalPanes", "Show session id in terminal panes", "Show the provider session id in the top-right corner of terminal panes."),
        // CDXC:Settings 2026-11-11 DECISION in apps/desktop settings_modal/model.rs (`RailTier`, Q2511): the four Commands pane rows moved here from Sidebar & Sessions; the pane is a terminal under or beside the work area, not part of the sidebar.
        row("commandsPanelDefaultHeightPx", "Commands pane height on double-click", "Height used when opening the command pane and when double-clicking its top resize rail."),
        row("commandsPanelSide", "Commands pane side", "Dock the command pane below the workspace or to its right.").options(COMMANDS_PANEL_SIDE_OPTIONS),
        row("commandsPanelAutoMinimize", "Auto-minimize Commands pane", "Minimize the Commands pane this long after you move focus elsewhere and the pointer leaves it. Commands keep running. Off keeps the pane open."),
        row("commandsPanelAutoMinimizeDelaySeconds", "Minimize after (set with Auto-minimize Commands pane)", "How long the Commands pane stays open after focus and the pointer leave it.").num_options(COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS),
        row("promptEditorBackend", format!("Editor for {} and $EDITOR prompts", hotkey_label("ctrl+g", platform)), format!("Choose which editor {} uses when a terminal prompt asks for $EDITOR.", hotkey_label("ctrl+g", platform))).options(PROMPT_EDITOR_BACKEND_OPTIONS),
    ]);
    // Custom shaders (`terminalShadersEnabled`) is the Terminal shaders switch in Settings >
    // Extensions > Experimental now (CDXC:Extensions 2026-11-09 in data/official_extensions.rs).
    section("terminal", "Terminal", settings)
}

pub(crate) fn terminal_behavior(platform: Platform) -> Section {
    section(
        "terminalBehavior",
        "Terminal Behavior",
        vec![
            row(
                "terminalScrollbackLimitMb",
                "Scrollback limit",
                "Set scrollback memory per terminal surface.",
            ),
            row(
                "terminalCopyOnSelect",
                "Copy on select",
                copy_on_select_description(platform),
            )
            .options(copy_on_select_options(platform)),
            row(
                "terminalConfirmCloseSurface",
                "Confirm close",
                "Confirm before closing terminal surfaces.",
            )
            .options(GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS),
            row(
                "terminalClipboardTrimTrailingSpaces",
                "Trim trailing spaces on copy",
                "Trim trailing whitespace when copying terminal text.",
            ),
            row(
                "terminalClipboardPasteProtection",
                "Paste protection",
                "Ask before pasting text Ghostty considers unsafe.",
            ),
            row(
                "terminalPastePreviewableImages",
                "Paste images as preview links",
                paste_previewable_images_description(platform),
            ),
            row(
                "terminalMouseHideWhileTyping",
                "Hide mouse while typing",
                "Hide the pointer while typing in the terminal.",
            ),
            row(
                "terminalScrollbar",
                "Scrollbar",
                "Control whether Ghostty shows its native scrollback scrollbar.",
            )
            .options(GHOSTTY_SCROLLBAR_OPTIONS),
        ],
    )
}

pub(crate) fn terminal_scrolling() -> Section {
    section(
        "terminalScrolling",
        "Terminal Scrolling",
        vec![
            row(
                "terminalMouseScrollMultiplierPrecision",
                "Trackpad scroll speed",
                "Trackpads and high-resolution scroll wheels. Ghostty default is 1.",
            ),
            row(
                "terminalMouseScrollMultiplierDiscrete",
                "Mouse wheel scroll speed",
                "Traditional notched mouse wheels. Ghostty default is 3.",
            ),
            row(
                "terminalScrollToBottomWhenTyping",
                "Scroll to bottom when typing",
                "Keep the prompt visible while typing.",
            ),
        ],
    )
}
