use crate::data::*;
use crate::json::opt;
use crate::json::Opt;
use crate::rows::{row, section, Section};
use crate::Platform;

/// CDXC:Settings 2026-11-18 DECISION:
/// User (Q4153, after arranging the Interface tab on a page): "let's split interface into multiple tabs also please to make each one more focused", "themes / glass / colors need to be in 1 area. text and fonts need to be in separate page for sure", "close button on the right and frame the main area and other ones like that need to be in separate place", and (Q4154's note) "under a new section in the sidebar called interface". The one `theming` section is four, one per page of the rail's Interface group: Themes (keeps the `theming` id and the `theme` tab, so old deep links and searches land there), Text & Fonts, Size & Zoom and Window & Layout. Every setting keeps its key, default and behaviour; only the page it is on changed.
/// CDXC:Settings 2026-11-19 DECISION:
/// User, on Size & Zoom's "Sidebar size" row, whose Open searched Sidebar & Sessions for "Sidebar Interface Size" and found nothing: "why not just move the settings related to size here in interface instead of linking somewhere else? pls bring settings related to each area in interface to that page". Each Interface page holds its own settings and links nowhere: Sidebar width on double-click (from Sidebar & Sessions) and Commands pane height on double-click (from Terminal) are Size & Zoom's Panel sizes; the terminal's Font weight, Bold text uses a heavier font, Line height and Letter spacing (from Terminal) are Text & Fonts' Terminal text; Terminal background, its colour and image, the image opacity and Background opacity (from Terminal) join Themes' Terminal colours; Animations and Close side panel with its last tab (from Sidebar & Sessions) and Commands pane side (from Terminal) are Window & Layout's Panels. Each lives in one place; keys, defaults and behaviour are unchanged. Chat's Transcript width and Terminal's width and padding stay with the view they shape.
/// SEE-ALSO: layout.rs (`GENERAL_GROUPS`' `appearance`), help/mod.rs (the four tabs), apps/desktop/src/app/window/settings_modal/model.rs (`RailTier::Interface`).
pub(crate) fn theming() -> Section {
    let rows = vec![
            row("sidebarTheme", "Appearance", "Follow the system appearance by default, or choose Light or Dark.").options(SIDEBAR_THEME_SETTING_OPTIONS),
            row("darkThemePreset", "Dark mode colour", "The colour of the sidebar and window in dark mode: one of sixteen colour squares, or the + square for a custom colour.").options(DARK_THEME_PRESET_OPTIONS),
            row("customSidebarTitlebarBackgroundTintColor", "Custom dark mode tint", "The custom dark mode colour, chosen in the colour picker the + square opens."),
            row("lightThemePreset", "Light mode colour", "The colour of the sidebar and window in light mode: one of sixteen colour squares, or the + square for a custom colour.").options(LIGHT_THEME_PRESET_OPTIONS),
            row("customSidebarTitlebarLightBackgroundTintColor", "Custom light mode tint", "The custom light mode colour, chosen in the colour picker the + square opens."),
            row("themeSidebarContrast", "Sidebar colourfulness", "How much of the theme colour shows in the sidebar, from Subtle (deeper, nearly neutral) through Vivid (lighter, more colourful) to Full, which shows the colour you picked itself. Colourfulness sets the sidebar and work area together, and how deep a custom colour is."),
            row("themeWorkAreaContrast", "Work area colourfulness", "How much of the theme colour shows in the work area (chat, terminals and views), from Subtle to Full. Colourfulness sets the sidebar and work area together."),
            row("windowArt", "Art behind the window", "The artwork painted behind the sidebar and the work area in dark mode, with the window solid so your desktop never shows through: any artwork from Artwork in Themes, or Your image, a picture you choose. An artwork downloads the first time it shows. None goes back to Window transparency.").options(WINDOW_ART_OPTIONS),
            row("windowArtLight", "Art behind the window in light mode", "The artwork a light theme paints behind the sidebar and the work area in light mode, with the window solid. None keeps light mode as Window transparency sets it.").options(WINDOW_ART_OPTIONS),
            row("windowArtDim", "Art dimming", "How much the artwork is darkened, in percent, so text over it stays easy to read."),
            row("windowArtBlur", "Art blur", "How soft the artwork looks, in points. 0 keeps it sharp."),
            row("windowArtStyle", "Art style", "How the artwork is drawn behind the window: ASCII (characters over a soft blurred copy of it) or Dither (halftone dots). Both rise from the bottom of the window and fade out toward the top, and a chat's text sits on a soft panel of the theme's colour.").options(WINDOW_ART_STYLE_OPTIONS),
            row("windowArtMonochrome", "Black and white art", "Draw the artwork's characters or dots in the theme's own text colour. Turn off to draw them in the artwork's own colours."),
            row("windowArtImage", "Your image for dark mode", "The picture Your image paints behind the window in dark mode, as Ghostex keeps a copy of it on this computer. Pick it with Your image… at the end of Artwork in Themes. It stays on this computer when Settings sync."),
            row("windowArtImageLight", "Your image for light mode", "The picture Your image paints behind the window in light mode, as Ghostex keeps a copy of it on this computer. It stays on this computer when Settings sync."),
            row("savedThemes", "Your themes", "Looks you saved with Save current theme in Themes. Each keeps its colours, artwork and transparency, so one click switches back to it. They sync with Appearance; a Your image picture stays on the computer it was picked on."),
            row("windowGlass", "Transparency", "Let your desktop show through the window: Off, Dark only (light mode stays solid) or Always.").options(WINDOW_GLASS_OPTIONS),
            row("windowGlassSource", "Behind the glass", "Fully transparent (everything behind the window, blurred), your wallpaper, a picture you choose, or Animated: a calm animation in your theme colours or your own video.").options(WINDOW_GLASS_SOURCE_OPTIONS),
            // CDXC:Settings 2026-10-29 DECISION (cleanup Q19): one switch keeps the light-mode picture, video, Animated
            // background and tints the same as the dark-mode ones; off shows the light-mode rows again.
            row("windowGlassSameForLightAndDark", "Same for light and dark", "Use the dark-mode picture, video, Animated background and tints in light mode too. Turn off to pick them for light mode separately."),
            row("windowGlassImagePlacement", "Picture position", "Where the wallpaper, custom picture or your own video sits behind the glass. Stays with the desktop can trail the window while you drag it.").options(WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS),
            row("windowGlassImageDark", "Picture for dark mode", "The picture the glass blurs in dark mode when Picture shows behind the glass."),
            row("windowGlassImageLight", "Picture for light mode", "The picture the glass blurs in light mode when Picture shows behind the glass."),
            row("windowGlassVideoDark", "Your video for dark mode", "The video file the glass plays in dark mode when Animated is set to Your video."),
            row("windowGlassVideoLight", "Your video for light mode", "The video file the glass plays in light mode when Animated is set to Your video."),
            row("windowGlassVideoOnlyOnPower", "Play only when plugged in", "Pause the animation or video while your computer runs on battery. It always pauses when Ghostex is in the background."),
            row("windowGlassLiveStyleDark", "Animated background for dark mode", "The animation, or your own video, the glass shows in dark mode when Animated shows behind the glass.").options(WINDOW_GLASS_LIVE_STYLE_OPTIONS).options(&[opt("Your video", "video")]),
            row("windowGlassLiveStyleLight", "Animated background for light mode", "The animation, or your own video, the glass shows in light mode when Animated shows behind the glass.").options(WINDOW_GLASS_LIVE_STYLE_OPTIONS).options(&[opt("Your video", "video")]),
            row("windowGlassLiveSpeed", "Animated background speed", "How fast the Animated background moves, from a quarter of its pace to twice as fast."),
            row("windowGlassLiveBrightness", "Animated background brightness", "How bright the Animated background glows behind the glass. Lower keeps it a subtle glow."),
            row("windowGlassBlurRadius", "Blur", "How soft what shows behind the window looks, in points. 0 shows it sharp. On Windows and Linux, Fully transparent uses the system blur, so this sets the wallpaper, picture and video blur; on Windows, Blur on Windows picks that system blur."),
            row("windowGlassWindowsBlur", "Blur on Windows", "How Windows blurs what shows behind the window while it is Fully transparent. Windows has no blur amount to set, so pick Acrylic (Windows' own frosted blur), Plain blur (the same blur without Windows' grey frost, so the tints can go fully clear; it takes more work to draw) or Off (sharp, the fastest).").options(WINDOW_GLASS_WINDOWS_BLUR_OPTIONS),
            row("windowGlassMenuBlurRadius", "Menu blur", "How soft what shows behind menus, panels, dialogs and tooltips looks, in points. Menus, panels and dialogs blur a quarter more than this number; tooltips use it as is. 0 shows it sharp."),
            row("windowGlassSidebarOpacityDark", "Sidebar tint in dark mode", "How much of the desktop the sidebar hides in dark mode. Lower shows more of your desktop through it."),
            row("windowGlassWorkAreaTintDark", "Work area tint in dark mode", "How much of the desktop the work area hides in dark mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it."),
            row("windowGlassSidebarOpacityLight", "Sidebar tint in light mode", "How much of the desktop the sidebar hides in light mode. Lower shows more of your desktop through it."),
            row("windowGlassWorkAreaTintLight", "Work area tint in light mode", "How much of the desktop the work area hides in light mode, set on its own so either area can be the darker one. Lower shows more of your desktop through it."),
            row("sessionChatTheme", "Chat theme", "Follow the app theme by default, or override chat with Light, Dark, or System.").options(SESSION_CHAT_THEME_OPTIONS),
            row("themeRoles", "Theme colours", "Borders, dividers, text, grey text, the accent, links, cards, controls, menus, selected and hovered rows, the focus ring, your messages, the chat box, code blocks and the status colours, each set apart for dark and light mode as an opacity (4%), a step off the window colour (base+4% or base+4%/80%) or a colour (#7dd3fc), with a colour picker and an opacity slider. Your messages, the chat box, cards and the accent are in Chat colours. Empty keeps the theme's own; Reset puts it back. Agents change them with `ghostex theme set`."),
            row("terminalColorScheme", "Terminal theme", "Follow the app theme by default, or override terminals with Light, Dark, or System.").options(SESSION_CHAT_THEME_OPTIONS),
            row("terminalGhosttyTheme", "Terminal palette in dark mode", "Uses the dark theme set in Ghostex's terminal config, or Ghostex Kanagawa Dragon when none is set.").options(GHOSTTY_THEME_SETTING_OPTIONS),
            row("terminalGhosttyLightTheme", "Terminal palette in light mode", "Uses the light theme set in Ghostex's terminal config, or GitHub Light when none is set.").options(&ghostty_themes_without_unmanaged()),
            row("terminalBackgroundMode", "Terminal background", "Only changes the terminal panes. Black / white is pure black in dark mode and pure white in light mode.").options(&[opt("Black / white", "pure"), opt("Follow theme", "theme"), opt("Custom color", "custom")]),
            row("workspaceBackgroundColor", "Terminal background color", "Custom terminal background, painted behind terminal text in dark and light mode."),
            row("terminalBackgroundOpacity", "Terminal background opacity (%)", "How much of the work area's glass tint terminals keep while transparency is on (Transparency, higher on this page). 100% matches the rest of the work area; lower shows more of what is behind the window. Off while transparency is off."),
            row("terminalBackgroundImage", "Terminal background image", "Absolute path to an image drawn behind terminal panes."),
            row("terminalBackgroundImageOpacity", "Background image opacity", "Blend the background image toward the terminal background color."),
    ];
    section("theming", "Themes", rows)
}

/// Settings > Text & Fonts (CDXC:Settings 2026-11-18 at [`theming`]).
pub(crate) fn text_fonts() -> Section {
    let rows = vec![
            row("sessionChatFontFamily", "App font", "Pick one of the fonts installed on this computer for the whole app: sidebar, chat, menus, dialogs and Settings, then press Apply. App default keeps Ghostex's own fonts. Terminals and code use the Monospace font."),
            row("terminalFontFamily", "Monospace font", "Font family for terminals, code in the Files view, diffs, code blocks and tool output in chat. Pick an installed monospace font, or type a Ghostty font-family name, then press Apply."),
            row("terminalFontWeight", "Font weight", "Set terminal text weight."),
            row("terminalBoldUsesHeavierFont", "Bold text uses a heavier font", "Draw bold terminal text in a heavier weight. Turn off to keep bold text at your normal font weight, so it stands out by color only. A font-style-bold or font-variation-bold line in Ghostex's terminal config takes priority."),
            row("terminalLineHeight", "Line height", "Adjust terminal row height."),
            row("terminalLetterSpacing", "Letter spacing", "Adjust spacing between glyphs."),
            row("sessionChatTextStrengthPercent", "Text strength", "How strong agents' replies and your messages read in chat, on the computer and the phone. 0% is the standard colour; 100% is full white in dark mode and full black in light mode. Muted text, links, code blocks and status colours stay as they are."),
            row("sessionChatMutedThinking", "Show thinking in grey", "Show the agent's thinking in grey, so it reads apart from the agent's replies."),
    ];
    section("textFonts", "Text & Fonts", rows)
}

/// Settings > Size & Zoom (CDXC:Settings 2026-11-18 at [`theming`]).
pub(crate) fn size_zoom(platform: Platform) -> Section {
    let mut rows = vec![
            // CDXC:Settings 2026-11-05 DECISION: User (issue #425): one Interface scale section with Scale and a Scale areas separately switch that reveals the Sidebar, Chat, Files and Terminal font size sliders (interface_scale.rs has the full decision).
            row("interfaceScalePercent", "Scale (%)", "Scale the sidebar, every chat, files open in the Files view and the terminal text together, from 70% to 200%. The header, tabs, menus and dialogs keep their size. Cmd+= and Cmd+- (Ctrl on Windows and Linux) change it from anywhere, and Cmd+0 sets 100%. Default: 100%."),
            row("interfaceScaleAreasSeparately", "Scale areas separately", "Give the sidebar, chat, Files and terminal text their own sizes instead of one Scale. Cmd+= and Cmd+- then change only the area you are in. Default: off, or on when you had already changed one of those sizes."),
            row("agentManagerZoomPercent", "Sidebar zoom (%)", "Scale the sidebar, from 50% to 200%, when Scale areas separately is on."),
            row("sessionChatZoomPercent", "Chat zoom (%)", "Scale every desktop chat, including messages and the prompt composer, from 70% to 200% in 5% steps, when Scale areas separately is on. Cmd+= and Cmd+- in a chat change it too, and Cmd+0 sets 100%. Default: 100%."),
            row("filesZoomPercent", "Files zoom (%)", "Scale the text of a file open in the Files view (Markdown, text and code, with its line numbers) and the Changes diff, from 70% to 200% in 5% steps, when Scale areas separately is on. Cmd+= and Cmd+- (Ctrl on Windows and Linux) in an open file change it too, and Cmd+0 sets 100%. Default: 100%."),
            row("terminalFontSize", "Terminal font size", "The terminal text size in points, when Scale areas separately is on. Cmd+= and Cmd+- in a terminal change it for that terminal only until it restarts. Default: 13."),
    ];
    if platform == Platform::Linux {
        // CDXC:PlatformSupport 2026-11-08 DECISION: User (issue #503): a Linux-only Display scale row (interface_scale.rs has the full decision).
        // Last on the page, as Q4153 arranged it.
        rows.push(
            row("linuxDisplayScale", "Display scale", "How big the whole app is drawn: the header, menus, dialogs and web pages too. Automatic follows your desktop's display scale; pick a fixed one if Ghostex gets it wrong. UI Scale applies on top. Takes effect when Ghostex restarts. Default: Automatic.").options(crate::interface_scale::LINUX_DISPLAY_SCALE_OPTIONS),
        );
    }
    // Panel sizes, the page's second group (CDXC:Settings 2026-11-19 at [`theming`]).
    rows.extend([
        row("sidebarDefaultWidthPx", "Sidebar width on double-click", "Width restored when double-clicking the sidebar resize handle."),
        row("commandsPanelDefaultHeightPx", "Commands pane height on double-click", "Height used when opening the command pane and when double-clicking its top resize rail."),
    ]);
    section("sizeZoom", "Size & Zoom", rows)
}

/// Settings > Window & Layout (CDXC:Settings 2026-11-18 at [`theming`]).
pub(crate) fn window_layout() -> Section {
    let rows = vec![
            // CDXC:Settings 2026-11-08 DECISION: User: "please add setting under this [Frame the work area] to show traffic lights on macos when sidebar is collapsed (default to yes). only show this sub setting on macos. also please move this whole setting under a new advanced area in the interface page in settings". The Advanced fold is superseded by CDXC:Settings 2026-11-18 at [`theming`]: the row is shown on the Window & Layout page, and Frame the work area is gone (CDXC:Workarea 2026-11-20 DECISION in apps/desktop/src/app/helpers/work_area_frame.rs: always framed).
            row("windowLayout", "Layout", "Floating sidebar draws the sidebar as a card floating over the work area, which runs to the window's edges. Framed work area draws the work area as the card beside the sidebar.").options(WINDOW_LAYOUT_OPTIONS),
            row("showWindowButtonsWhenSidebarCollapsed", "Show the window buttons when the sidebar is collapsed", "Keep the red, yellow and green window buttons in the window's top left while the sidebar is collapsed. Off hides them until you open the floating sidebar, which carries them in its top row. Full screen keeps its own title bar either way."),
            // CDXC:AppModal 2026-11-10 DECISION in apps/desktop/src/app/window/round_close_button.rs: on macOS the close X of dialogs and toasts sits at their top left; this macOS-only row on the Window & Layout page puts it back on the right.
            row("closeButtonsOnRight", "Close buttons on the right", "Put the X that closes dialogs and pop-up messages at their top right. Off keeps it at their top left, where macOS puts a window's close button."),
            row("showActivePaneOutline", "Show active pane outline", "Show an outline around the currently focused pane."),
            row("workspaceActivePaneBorderColor", "Active pane outline colour", "Color of the outline around the currently focused pane."),
            row("panelAnimationSpeed", "Animations", "Set how fast the sidebar, the side panel, the Agents Panel and the bottom or right panel slide open and closed: Off, Slow, Normal or Fast. Reduce Motion in your computer settings always turns it off.").options(PANEL_ANIMATION_SPEED_OPTIONS),
            row("sidebarCollapseAnimationDurationMs", "Animations (fold speed, set with Animations)", "Set how quickly sidebar sections, groups, and projects expand or collapse, and how quickly the floating sidebar and Agents Panel slide in from the window edge. Set to 0 for no animation."),
            row("closeSidePanelWithLastTab", "Close side panel with its last tab", "Close the side panel when you close its last tab, instead of showing the Open a view picker."),
            row("commandsPanelSide", "Commands pane side", "Dock the command pane below the workspace or to its right.").options(COMMANDS_PANEL_SIDE_OPTIONS),
    ];
    section("windowLayout", "Window & Layout", rows)
}

/// `GHOSTTY_THEME_SETTING_OPTIONS` without "Use terminal config file": the light palette always names a theme.
fn ghostty_themes_without_unmanaged() -> Vec<Opt> {
    GHOSTTY_THEME_SETTING_OPTIONS
        .iter()
        .filter(|option| option.value != "__ghostex_ghostty_theme_unmanaged__")
        .copied()
        .collect()
}
