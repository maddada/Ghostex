use crate::data::*;
use crate::json::opt;
use crate::json::Opt;
use crate::rows::{row, section, Section};

pub(crate) fn theming() -> Section {
    section(
        "theming",
        "Interface",
        vec![
            row("sidebarTheme", "Appearance", "Follow the system appearance by default, or choose Light or Dark.").options(SIDEBAR_THEME_SETTING_OPTIONS),
            row("darkThemePreset", "Dark mode colour", "The colour of the sidebar and window in dark mode: one of sixteen colour squares, or the + square for a custom colour.").options(DARK_THEME_PRESET_OPTIONS),
            row("customSidebarTitlebarBackgroundTintColor", "Custom dark mode tint", "The custom dark mode colour, chosen in the colour picker the + square opens."),
            row("lightThemePreset", "Light mode colour", "The colour of the sidebar and window in light mode: one of sixteen colour squares, or the + square for a custom colour.").options(LIGHT_THEME_PRESET_OPTIONS),
            row("themeSidebarContrast", "Sidebar colourfulness", "How much of the theme colour shows in the sidebar, from Subtle (deeper, nearly neutral) through Vivid (lighter, more colourful) to Full, where dark mode shows the colour you picked itself. Colourfulness sets the sidebar and work area together, and how deep a custom colour is."),
            row("themeWorkAreaContrast", "Work area colourfulness", "How much of the theme colour shows in the work area (chat, terminals and views), from Subtle to Full. Colourfulness sets the sidebar and work area together."),
            row("customSidebarTitlebarLightBackgroundTintColor", "Custom light mode tint", "The custom light mode colour, chosen in the colour picker the + square opens."),
            row("sessionChatFontFamily", "App font", "Pick one of the fonts installed on this computer for the whole app: sidebar, chat, menus, dialogs and Settings, then press Apply. App default keeps Ghostex's own fonts. Terminals and code use the Monospace font."),
            row("terminalFontFamily", "Monospace font", "Font family for terminals, code in the Files view, diffs, code blocks and tool output in chat. Pick an installed monospace font, or type a Ghostty font-family name, then press Apply."),
            // CDXC:Settings 2026-11-05 DECISION: User (issue #425): one Interface scale section with Scale and a Scale areas separately switch that reveals the Sidebar, Chat, Files and Terminal font size sliders (interface_scale.rs has the full decision).
            row("interfaceScalePercent", "Scale (%)", "Scale the sidebar, every chat, files open in the Files view and the terminal text together, from 70% to 200%. The header, tabs, menus and dialogs keep their size. Cmd+= and Cmd+- (Ctrl on Windows and Linux) change it from anywhere, and Cmd+0 sets 100%. Default: 100%."),
            row("interfaceScaleAreasSeparately", "Scale areas separately", "Give the sidebar, chat, Files and terminal text their own sizes instead of one Scale. Cmd+= and Cmd+- then change only the area you are in. Default: off, or on when you had already changed one of those sizes."),
            row("agentManagerZoomPercent", "Sidebar zoom (%)", "Scale the sidebar, from 50% to 200%, when Scale areas separately is on."),
            row("sessionChatZoomPercent", "Chat zoom (%)", "Scale every desktop chat, including messages and the prompt composer, from 70% to 200% in 5% steps, when Scale areas separately is on. Cmd+= and Cmd+- in a chat change it too, and Cmd+0 sets 100%. Default: 100%."),
            row("filesZoomPercent", "Files zoom (%)", "Scale the text of a file open in the Files view (Markdown, text and code, with its line numbers) and the Changes diff, from 70% to 200% in 5% steps, when Scale areas separately is on. Cmd+= and Cmd+- (Ctrl on Windows and Linux) in an open file change it too, and Cmd+0 sets 100%. Default: 100%."),
            row("terminalFontSize", "Terminal font size", "The terminal text size in points, when Scale areas separately is on. Cmd+= and Cmd+- in a terminal change it for that terminal only until it restarts. Default: 13."),
            row("sessionChatTextStrengthPercent", "Text strength", "How strong agents' replies and your messages read in chat, on the computer and the phone. 0% is the standard colour; 100% is full white in dark mode and full black in light mode. Muted text, links, code blocks and status colours stay as they are."),
            row("sessionChatMutedThinking", "Show thinking in grey", "Show the agent's thinking in grey and its replies in the full text colour, so the two are easy to tell apart."),
            row("sessionChatTheme", "Chat theme", "Follow the app theme by default, or override chat with Light, Dark, or System.").options(SESSION_CHAT_THEME_OPTIONS),
            row("terminalColorScheme", "Terminal theme", "Follow the app theme by default, or override terminals with Light, Dark, or System.").options(SESSION_CHAT_THEME_OPTIONS),
            row("terminalGhosttyLightTheme", "Terminal palette in light mode", "Uses the light theme set in Ghostex's terminal config, or GitHub Light when none is set.").options(&ghostty_themes_without_unmanaged()),
            row("terminalGhosttyTheme", "Terminal palette in dark mode", "Uses the dark theme set in Ghostex's terminal config, or GitHub Dark when none is set.").options(GHOSTTY_THEME_SETTING_OPTIONS),
            row("windowGlass", "Transparency", "Let your desktop show through the window: Off, Dark only (light mode stays solid) or Always.").options(WINDOW_GLASS_OPTIONS),
            row("windowGlassSource", "Behind the glass", "Desktop and windows, your wallpaper, a picture you choose, or Live: a calm animation in your theme colours or your own video.").options(WINDOW_GLASS_SOURCE_OPTIONS),
            // CDXC:Settings 2026-10-29 DECISION (cleanup Q19): one switch keeps the light-mode picture, video, Live
            // background and tints the same as the dark-mode ones; off shows the light-mode rows again.
            row("windowGlassSameForLightAndDark", "Same for light and dark", "Use the dark-mode picture, video, Live background and tints in light mode too. Turn off to pick them for light mode separately."),
            row("windowGlassImagePlacement", "Picture position", "Where the wallpaper, custom picture or your own Live video sits behind the glass. Stays with the desktop can trail the window while you drag it.").options(WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS),
            row("windowGlassImageDark", "Picture for dark mode", "The picture the glass blurs in dark mode when Picture shows behind the glass."),
            row("windowGlassImageLight", "Picture for light mode", "The picture the glass blurs in light mode when Picture shows behind the glass."),
            row("windowGlassVideoDark", "Your video for dark mode", "The video file the glass plays in dark mode when Live is set to Your video."),
            row("windowGlassVideoLight", "Your video for light mode", "The video file the glass plays in light mode when Live is set to Your video."),
            row("windowGlassVideoOnlyOnPower", "Play only when plugged in", "Pause the Live animation or video while your computer runs on battery. It always pauses when Ghostex is in the background."),
            row("windowGlassLiveStyleDark", "Live background for dark mode", "The animation, or your own video, the glass shows in dark mode when Live shows behind the glass.").options(WINDOW_GLASS_LIVE_STYLE_OPTIONS).options(&[opt("Your video", "video")]),
            row("windowGlassLiveStyleLight", "Live background for light mode", "The animation, or your own video, the glass shows in light mode when Live shows behind the glass.").options(WINDOW_GLASS_LIVE_STYLE_OPTIONS).options(&[opt("Your video", "video")]),
            row("windowGlassLiveSpeed", "Live background speed", "How fast the Live animation moves, from a quarter of its pace to twice as fast."),
            row("windowGlassLiveBrightness", "Live background brightness", "How bright the Live animation glows behind the glass. Lower keeps it a subtle glow."),
            row("windowGlassBlurRadius", "Blur", "How soft what shows behind the window looks, in points. 0 shows it sharp. On Windows and Linux, Desktop and windows uses the system blur, so this sets the wallpaper, picture and video blur; on Windows, Blur on Windows picks that system blur."),
            row("windowGlassWindowsBlur", "Blur on Windows", "How Windows blurs what shows behind the window while Desktop and windows shows behind the glass. Windows has no blur amount to set, so pick Acrylic (Windows' own frosted blur), Plain blur (the same blur without Windows' grey frost, so the tints can go fully clear; it takes more work to draw) or Off (sharp, the fastest).").options(WINDOW_GLASS_WINDOWS_BLUR_OPTIONS),
            row("windowGlassMenuBlurRadius", "Menu blur", "How soft what shows behind menus, panels, dialogs and tooltips looks, in points. Menus, panels and dialogs blur a quarter more than this number; tooltips use it as is. 0 shows it sharp."),
            row("windowGlassSidebarOpacityDark", "Sidebar tint in dark mode", "How much of the desktop the sidebar hides in dark mode. Lower shows more of your desktop through it."),
            row("windowGlassWorkAreaTintDark", "Work area tint in dark mode", "How much of the desktop the work area hides in dark mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it."),
            row("windowGlassSidebarOpacityLight", "Sidebar tint in light mode", "How much of the desktop the sidebar hides in light mode. Lower shows more of your desktop through it."),
            row("windowGlassWorkAreaTintLight", "Work area tint in light mode", "How much of the desktop the work area hides in light mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it."),
            row("showActivePaneOutline", "Show active pane outline", "Show an outline around the currently focused pane."),
            row("workspaceActivePaneBorderColor", "Active pane outline colour", "Color of the outline around the currently focused pane."),
            row("workAreaFrame", "Frame the work area", "Show the work area as a card inside the window, with a thin margin all around, a hairline border and a light shadow. Off keeps the work area edge to edge."),
        ],
    )
}

/// `GHOSTTY_THEME_SETTING_OPTIONS` without "Use terminal config file": the light palette always names a theme.
fn ghostty_themes_without_unmanaged() -> Vec<Opt> {
    GHOSTTY_THEME_SETTING_OPTIONS
        .iter()
        .filter(|option| option.value != "__ghostex_ghostty_theme_unmanaged__")
        .copied()
        .collect()
}
