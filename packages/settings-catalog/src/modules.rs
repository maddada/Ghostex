//! The data exports the native Settings window looks up by name, grouped by area.

use crate::data::*;
use crate::hotkeys::{default_hotkeys, hotkey_definitions};
use crate::json::{Json, ToJson, J};
use crate::platform_text::{
    copy_on_select_description, copy_on_select_options, paste_previewable_images_description,
};
use crate::Platform;

/// The areas `exports` answers for.
pub mod module {
    pub const AGENT_ACCOUNTS: &str = "agent_accounts";
    pub const AGENT_SKILLS: &str = "agent_skills";
    pub const COMPLETION_SOUND: &str = "completion_sound";
    pub const GHOSTTY_CONFIG_ACTIONS: &str = "ghostty_config";
    pub const HOTKEYS: &str = "hotkeys";
    pub const OFFICIAL_EXTENSIONS: &str = "official_extensions";
    pub const OPEN_TARGETS: &str = "open_targets";
    pub const PROJECT_VIEWS: &str = "project_views";
    pub const SEARCH_CATALOG: &str = "search_catalog";
    pub const SESSION_CARD_HOVER_ACTIONS: &str = "session_card_hover_actions";
    pub const SESSION_TAGS: &str = "session_tags";
    pub const SETTINGS: &str = "settings";
    pub const SETTINGS_TYPES: &str = "settings_layout";
    pub const SIDEBAR_AGENTS: &str = "sidebar_agents";
    pub const SIDEBAR_AGENT_ACCEPT_ALL: &str = "agent_accept_all";
    pub const SIDEBAR_COMMANDS: &str = "sidebar_commands";
    pub const TERMINAL_FONT_PRESET: &str = "terminal_fonts";
    pub const TITLEBAR_COLOR: &str = "titlebar_color";
}

/// Every export of one area, by name, for `platform`; empty for an unknown area.
pub fn exports(module: &str, platform: Platform) -> Vec<(&'static str, Json)> {
    match module {
        module::SIDEBAR_AGENT_ACCEPT_ALL => vec![
            (
                "AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS",
                AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS.to_json(),
            ),
            ("AGENT_ACCEPT_ALL_SPECS", AGENT_ACCEPT_ALL_SPECS.to_json()),
        ],
        module::AGENT_ACCOUNTS => vec![(
            "NEW_SESSION_ACCOUNT_RULES",
            NEW_SESSION_ACCOUNT_RULES.to_json(),
        )],
        module::AGENT_SKILLS => vec![
            (
                "BUNDLED_GHOSTEX_AGENT_SKILLS",
                BUNDLED_GHOSTEX_AGENT_SKILLS.to_json(),
            ),
            (
                "GHOSTEX_SPACEO_PRODUCT_NAME",
                GHOSTEX_SPACEO_PRODUCT_NAME.to_json(),
            ),
            (
                "GHOSTEX_TRYCUA_PRODUCT_NAME",
                GHOSTEX_TRYCUA_PRODUCT_NAME.to_json(),
            ),
            (
                "GHOSTEX_TRYCUA_REPOSITORY_LABEL",
                GHOSTEX_TRYCUA_REPOSITORY_LABEL.to_json(),
            ),
            (
                "GHOSTEX_TRYCUA_REPOSITORY_URL",
                GHOSTEX_TRYCUA_REPOSITORY_URL.to_json(),
            ),
            (
                "VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS",
                VISIBLE_BUNDLED_GHOSTEX_AGENT_SKILLS.to_json(),
            ),
        ],
        module::COMPLETION_SOUND => vec![(
            "COMPLETION_SOUND_OPTIONS",
            COMPLETION_SOUND_OPTIONS.to_json(),
        )],
        module::GHOSTTY_CONFIG_ACTIONS => vec![(
            "GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES",
            GHOSTEX_RECOMMENDED_GHOSTTY_CONFIG_LINES.to_json(),
        )],
        module::HOTKEYS => vec![
            (
                "DEFAULT_GHOSTEX_HOTKEYS",
                default_hotkeys().as_slice().to_json(),
            ),
            ("GHOSTEX_HOTKEY_DEFINITIONS", hotkey_definitions().to_json()),
        ],
        module::OFFICIAL_EXTENSIONS => vec![
            (
                "GHOSTEX_OFFICIAL_EXTENSIONS",
                Json::Arr(
                    GHOSTEX_OFFICIAL_EXTENSIONS
                        .as_array()
                        .iter()
                        .filter(|entry| {
                            entry.get("id").and_then(J::as_str).is_none_or(|id| {
                                crate::built_in_extensions::available_on(id, platform)
                            })
                        })
                        .map(ToJson::to_json)
                        .collect(),
                ),
            ),
            (
                "GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES",
                GHOSTEX_OFFICIAL_EXTENSION_CATEGORIES.to_json(),
            ),
        ],
        module::OPEN_TARGETS => vec![
            (
                "BUILT_IN_WORKSPACE_OPEN_TARGETS",
                BUILT_IN_WORKSPACE_OPEN_TARGETS.to_json(),
            ),
            (
                "CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX",
                CUSTOM_WORKSPACE_OPEN_TARGET_ID_PREFIX.to_json(),
            ),
        ],
        module::PROJECT_VIEWS => vec![
            (
                "BUILTIN_PROJECT_VIEW_TEMPLATES",
                BUILTIN_PROJECT_VIEW_TEMPLATES.to_json(),
            ),
            (
                "DEFAULT_PROJECT_VIEW_SOURCE",
                DEFAULT_PROJECT_VIEW_SOURCE.to_json(),
            ),
        ],
        module::SEARCH_CATALOG => vec![
            (
                "COPY_ON_SELECT_DESCRIPTION",
                copy_on_select_description(platform).to_json(),
            ),
            (
                "COPY_ON_SELECT_OPTIONS",
                copy_on_select_options(platform).to_json(),
            ),
            ("IS_WINDOWS_HOST", (platform == Platform::Windows).to_json()),
            (
                "PASTE_PREVIEWABLE_IMAGES_DESCRIPTION",
                paste_previewable_images_description(platform).to_json(),
            ),
        ],
        module::SESSION_CARD_HOVER_ACTIONS => vec![
            (
                "DEFAULT_SESSION_CARD_HOVER_BUTTONS",
                DEFAULT_SESSION_CARD_HOVER_BUTTONS.to_json(),
            ),
            (
                "DEFAULT_UNIFIED_SESSION_CARD_HOVER_BUTTONS",
                DEFAULT_UNIFIED_SESSION_CARD_HOVER_BUTTONS.to_json(),
            ),
            (
                "SESSION_CARD_HOVER_BUTTON_LABELS",
                SESSION_CARD_HOVER_BUTTON_LABELS.to_json(),
            ),
        ],
        module::SESSION_TAGS => vec![
            (
                "DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS",
                DEFAULT_SIDEBAR_SESSION_TAG_LIST_ITEMS.to_json(),
            ),
            (
                "SESSION_TAG_COLOR_PRESETS",
                SESSION_TAG_COLOR_PRESETS.to_json(),
            ),
            ("SIDEBAR_SESSION_TAGS", SIDEBAR_SESSION_TAGS.to_json()),
            (
                "SIDEBAR_SESSION_TAG_LIST_SEPARATOR_IDS",
                SIDEBAR_SESSION_TAG_LIST_SEPARATOR_IDS.to_json(),
            ),
            (
                "SIDEBAR_SESSION_TAG_OPTIONS",
                SIDEBAR_SESSION_TAG_OPTIONS.to_json(),
            ),
        ],
        module::SETTINGS => vec![
            (
                "AGENTBOX_DEFAULT_LOCATION_OPTIONS",
                AGENTBOX_DEFAULT_LOCATION_OPTIONS.to_json(),
            ),
            (
                "AUTO_SLEEP_IDLE_MINUTE_OPTIONS",
                AUTO_SLEEP_IDLE_MINUTE_OPTIONS.to_json(),
            ),
            (
                "COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS",
                COMMANDS_PANEL_AUTO_MINIMIZE_DELAY_OPTIONS.to_json(),
            ),
            (
                "COMMANDS_PANEL_SIDE_OPTIONS",
                COMMANDS_PANEL_SIDE_OPTIONS.to_json(),
            ),
            (
                "DARK_THEME_PRESET_CONTROLS",
                DARK_THEME_PRESET_CONTROLS.to_json(),
            ),
            (
                "DARK_THEME_PRESET_OPTIONS",
                DARK_THEME_PRESET_OPTIONS.to_json(),
            ),
            (
                "DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT",
                DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT.to_json(),
            ),
            (
                "DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT",
                DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT.to_json(),
            ),
            (
                "DEFAULT_EDITOR_COMMAND_OPTIONS",
                DEFAULT_EDITOR_COMMAND_OPTIONS.to_json(),
            ),
            (
                "DEFAULT_GHOSTEX_SETTINGS",
                DEFAULT_GHOSTEX_SETTINGS.to_json(),
            ),
            (
                "DIAGNOSTIC_LOGGING_SCENARIOS",
                DIAGNOSTIC_LOGGING_SCENARIOS.to_json(),
            ),
            (
                "GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS",
                GHOSTTY_CONFIRM_CLOSE_SURFACE_OPTIONS.to_json(),
            ),
            (
                "GHOSTTY_COPY_ON_SELECT_MAC_OPTIONS",
                GHOSTTY_COPY_ON_SELECT_MAC_OPTIONS.to_json(),
            ),
            (
                "GHOSTTY_COPY_ON_SELECT_OPTIONS",
                GHOSTTY_COPY_ON_SELECT_OPTIONS.to_json(),
            ),
            (
                "GHOSTTY_SCROLLBAR_OPTIONS",
                GHOSTTY_SCROLLBAR_OPTIONS.to_json(),
            ),
            (
                "GHOSTTY_THEME_SETTING_OPTIONS",
                GHOSTTY_THEME_SETTING_OPTIONS.to_json(),
            ),
            (
                "KEEP_AWAKE_DURATION_OPTIONS",
                KEEP_AWAKE_DURATION_OPTIONS.to_json(),
            ),
            (
                "LIGHT_THEME_PRESET_CONTROLS",
                LIGHT_THEME_PRESET_CONTROLS.to_json(),
            ),
            (
                "LIGHT_THEME_PRESET_OPTIONS",
                LIGHT_THEME_PRESET_OPTIONS.to_json(),
            ),
            (
                "MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX",
                MAX_COMMANDS_PANEL_DEFAULT_HEIGHT_PX.to_json(),
            ),
            (
                "MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT",
                MAX_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT.to_json(),
            ),
            (
                "MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT",
                MAX_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT.to_json(),
            ),
            (
                "MAX_PROJECT_SWITCH_KEEP_ALIVE_MINUTES",
                MAX_PROJECT_SWITCH_KEEP_ALIVE_MINUTES.to_json(),
            ),
            (
                "MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT",
                MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT.to_json(),
            ),
            (
                "MAX_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT",
                MAX_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT.to_json(),
            ),
            (
                "MAX_SESSION_CHAT_TEXT_STRENGTH_PERCENT",
                MAX_SESSION_CHAT_TEXT_STRENGTH_PERCENT.to_json(),
            ),
            (
                "MAX_SESSION_CHAT_ZOOM_PERCENT",
                MAX_SESSION_CHAT_ZOOM_PERCENT.to_json(),
            ),
            (
                "MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS",
                MAX_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS.to_json(),
            ),
            (
                "MAX_SIDEBAR_DEFAULT_WIDTH_PX",
                MAX_SIDEBAR_DEFAULT_WIDTH_PX.to_json(),
            ),
            (
                "MAX_SIDEBAR_TOOLTIP_DELAY_MS",
                MAX_SIDEBAR_TOOLTIP_DELAY_MS.to_json(),
            ),
            (
                "MAX_TERMINAL_PANE_PADDING_PX",
                MAX_TERMINAL_PANE_PADDING_PX.to_json(),
            ),
            (
                "MAX_TERMINAL_VIEW_WIDTH_PERCENT",
                MAX_TERMINAL_VIEW_WIDTH_PERCENT.to_json(),
            ),
            (
                "MAX_WINDOW_GLASS_BLUR_RADIUS",
                MAX_WINDOW_GLASS_BLUR_RADIUS.to_json(),
            ),
            (
                "MAX_WINDOW_GLASS_LIVE_BRIGHTNESS",
                MAX_WINDOW_GLASS_LIVE_BRIGHTNESS.to_json(),
            ),
            (
                "MAX_WINDOW_GLASS_LIVE_SPEED",
                MAX_WINDOW_GLASS_LIVE_SPEED.to_json(),
            ),
            (
                "MAX_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT",
                MAX_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT.to_json(),
            ),
            (
                "MAX_WINDOW_GLASS_WORK_AREA_TINT_PERCENT",
                MAX_WINDOW_GLASS_WORK_AREA_TINT_PERCENT.to_json(),
            ),
            (
                "FILE_CHANGES_OPEN_TARGET_OPTIONS",
                FILE_CHANGES_OPEN_TARGET_OPTIONS.to_json(),
            ),
            (
                "MEDIA_FILE_OPEN_TARGET_OPTIONS",
                MEDIA_FILE_OPEN_TARGET_OPTIONS.to_json(),
            ),
            (
                "MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX",
                MIN_COMMANDS_PANEL_DEFAULT_HEIGHT_PX.to_json(),
            ),
            (
                "MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT",
                MIN_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT.to_json(),
            ),
            (
                "MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT",
                MIN_CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_LIGHTNESS_PERCENT.to_json(),
            ),
            (
                "MIN_PROJECT_SWITCH_KEEP_ALIVE_MINUTES",
                MIN_PROJECT_SWITCH_KEEP_ALIVE_MINUTES.to_json(),
            ),
            (
                "MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT",
                MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT.to_json(),
            ),
            (
                "MIN_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT",
                MIN_MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT.to_json(),
            ),
            (
                "MIN_SESSION_CHAT_TEXT_STRENGTH_PERCENT",
                MIN_SESSION_CHAT_TEXT_STRENGTH_PERCENT.to_json(),
            ),
            (
                "MIN_SESSION_CHAT_ZOOM_PERCENT",
                MIN_SESSION_CHAT_ZOOM_PERCENT.to_json(),
            ),
            (
                "MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS",
                MIN_SIDEBAR_COLLAPSE_ANIMATION_DURATION_MS.to_json(),
            ),
            (
                "MIN_SIDEBAR_DEFAULT_WIDTH_PX",
                MIN_SIDEBAR_DEFAULT_WIDTH_PX.to_json(),
            ),
            (
                "MIN_SIDEBAR_TOOLTIP_DELAY_MS",
                MIN_SIDEBAR_TOOLTIP_DELAY_MS.to_json(),
            ),
            (
                "MIN_TERMINAL_PANE_PADDING_PX",
                MIN_TERMINAL_PANE_PADDING_PX.to_json(),
            ),
            (
                "MIN_TERMINAL_VIEW_WIDTH_PERCENT",
                MIN_TERMINAL_VIEW_WIDTH_PERCENT.to_json(),
            ),
            (
                "MIN_WINDOW_GLASS_BLUR_RADIUS",
                MIN_WINDOW_GLASS_BLUR_RADIUS.to_json(),
            ),
            (
                "MIN_WINDOW_GLASS_LIVE_BRIGHTNESS",
                MIN_WINDOW_GLASS_LIVE_BRIGHTNESS.to_json(),
            ),
            (
                "MIN_WINDOW_GLASS_LIVE_SPEED",
                MIN_WINDOW_GLASS_LIVE_SPEED.to_json(),
            ),
            (
                "MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT",
                MIN_WINDOW_GLASS_SIDEBAR_OPACITY_PERCENT.to_json(),
            ),
            (
                "MIN_WINDOW_GLASS_WORK_AREA_TINT_PERCENT",
                MIN_WINDOW_GLASS_WORK_AREA_TINT_PERCENT.to_json(),
            ),
            (
                "NEW_SESSION_AGENT_SCOPE_OPTIONS",
                NEW_SESSION_AGENT_SCOPE_OPTIONS.to_json(),
            ),
            (
                "PANEL_ANIMATION_SPEED_OPTIONS",
                PANEL_ANIMATION_SPEED_OPTIONS.to_json(),
            ),
            (
                "PREFERRED_AGENT_INTERFACE_INHERIT_VALUE",
                PREFERRED_AGENT_INTERFACE_INHERIT_VALUE.to_json(),
            ),
            (
                "PREFERRED_AGENT_INTERFACE_OPTIONS",
                PREFERRED_AGENT_INTERFACE_OPTIONS.to_json(),
            ),
            (
                "PROMPT_EDITOR_BACKEND_OPTIONS",
                PROMPT_EDITOR_BACKEND_OPTIONS.to_json(),
            ),
            (
                "SESSION_CHAT_THEME_OPTIONS",
                SESSION_CHAT_THEME_OPTIONS.to_json(),
            ),
            (
                "SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP",
                SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP.to_json(),
            ),
            (
                "MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT_STEP",
                MARKED_HANDOVERS_HEADING_STRENGTH_PERCENT_STEP.to_json(),
            ),
            (
                "SESSION_CHAT_TEXT_STRENGTH_PERCENT_STEP",
                SESSION_CHAT_TEXT_STRENGTH_PERCENT_STEP.to_json(),
            ),
            (
                "SESSION_CHAT_ZOOM_PERCENT_STEP",
                SESSION_CHAT_ZOOM_PERCENT_STEP.to_json(),
            ),
            (
                "SESSION_TITLE_GENERATION_AGENT_OPTIONS",
                SESSION_TITLE_GENERATION_AGENT_OPTIONS.to_json(),
            ),
            (
                "SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER",
                SESSION_TITLE_GENERATION_PROMPT_PLACEHOLDER.to_json(),
            ),
            (
                "SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS",
                SIDEBAR_COLLAPSE_ANIMATION_DURATION_STEP_MS.to_json(),
            ),
            (
                "SIDEBAR_SETTINGS_PRESETS",
                SIDEBAR_SETTINGS_PRESETS.to_json(),
            ),
            (
                "SIDEBAR_SETTINGS_PRESET_KEYS",
                SIDEBAR_SETTINGS_PRESET_KEYS.to_json(),
            ),
            (
                "SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS",
                SIDEBAR_SPACE_SWITCH_BEHAVIOR_OPTIONS.to_json(),
            ),
            ("SIDEBAR_VIEW_OPTIONS", SIDEBAR_VIEW_OPTIONS.to_json()),
            (
                "PINNED_SESSION_ORDER_OPTIONS",
                PINNED_SESSION_ORDER_OPTIONS.to_json(),
            ),
            (
                "SIDEBAR_THEME_SETTING_OPTIONS",
                SIDEBAR_THEME_SETTING_OPTIONS.to_json(),
            ),
            (
                "SIDEBAR_TOOLTIP_DELAY_STEP_MS",
                SIDEBAR_TOOLTIP_DELAY_STEP_MS.to_json(),
            ),
            (
                "SIDEBAR_VISIBILITY_MEMORY_OPTIONS",
                SIDEBAR_VISIBILITY_MEMORY_OPTIONS.to_json(),
            ),
            (
                "TERMINAL_VIEW_WIDTH_MODE_OPTIONS",
                TERMINAL_VIEW_WIDTH_MODE_OPTIONS.to_json(),
            ),
            (
                "TERMINAL_VIEW_WIDTH_PERCENT_STEP",
                TERMINAL_VIEW_WIDTH_PERCENT_STEP.to_json(),
            ),
            (
                "MARKED_HANDOVERS_STYLE_OPTIONS",
                MARKED_HANDOVERS_STYLE_OPTIONS.to_json(),
            ),
            (
                "CHAT_PAGE_OPEN_TARGET_OPTIONS",
                CHAT_PAGE_OPEN_TARGET_OPTIONS.to_json(),
            ),
            (
                "COORDINATOR_BOARD_VIEW_OPTIONS",
                COORDINATOR_BOARD_VIEW_OPTIONS.to_json(),
            ),
            (
                "WEB_LINK_OPEN_TARGET_OPTIONS",
                WEB_LINK_OPEN_TARGET_OPTIONS.to_json(),
            ),
            (
                "AGENT_PREVIEW_BROWSER_OPTIONS",
                AGENT_PREVIEW_BROWSER_OPTIONS.to_json(),
            ),
            (
                "SESSION_LINK_OPEN_TARGET_OPTIONS",
                SESSION_LINK_OPEN_TARGET_OPTIONS.to_json(),
            ),
            (
                "SIDEBAR_SESSION_LINKS_OPTIONS",
                SIDEBAR_SESSION_LINKS_OPTIONS.to_json(),
            ),
            (
                "WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS",
                WINDOW_GLASS_IMAGE_PLACEMENT_OPTIONS.to_json(),
            ),
            (
                "WINDOW_GLASS_LIVE_STYLE_OPTIONS",
                WINDOW_GLASS_LIVE_STYLE_OPTIONS.to_json(),
            ),
            ("WINDOW_GLASS_OPTIONS", WINDOW_GLASS_OPTIONS.to_json()),
            (
                "WINDOW_GLASS_WINDOWS_BLUR_OPTIONS",
                WINDOW_GLASS_WINDOWS_BLUR_OPTIONS.to_json(),
            ),
            (
                "WINDOW_GLASS_SOURCE_OPTIONS",
                WINDOW_GLASS_SOURCE_OPTIONS.to_json(),
            ),
        ],
        module::SETTINGS_TYPES => vec![
            (
                "ADVANCED_MAIN_SETTING_KEYS",
                ADVANCED_MAIN_SETTING_KEYS.to_json(),
            ),
            (
                "AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS",
                AGENT_HOOK_SUPPORTED_DEFAULT_AGENTS.to_json(),
            ),
            (
                "DIAGNOSTIC_LOGGING_DURATION_OPTIONS",
                DIAGNOSTIC_LOGGING_DURATION_OPTIONS.to_json(),
            ),
            ("HOTKEY_SETTINGS_SECTIONS", hotkey_settings_sections()),
            (
                "MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS",
                MAIN_SETTINGS_SCROLL_TARGET_SETTING_KEYS.to_json(),
            ),
            (
                "MAIN_SETTINGS_SECTION_SETTING_KEYS",
                MAIN_SETTINGS_SECTION_SETTING_KEYS.to_json(),
            ),
            (
                "MAIN_SETTINGS_SUBSECTION_NAVIGATION",
                MAIN_SETTINGS_SUBSECTION_NAVIGATION.to_json(),
            ),
            (
                "MAIN_SETTINGS_SUBSECTION_PARENT_IDS",
                MAIN_SETTINGS_SUBSECTION_PARENT_IDS.to_json(),
            ),
            (
                "RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL",
                RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_LABEL.to_json(),
            ),
            (
                "RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE",
                RENAME_SESSION_ON_DOUBLE_CLICK_SETTING_SUBTITLE.to_json(),
            ),
        ],
        module::SIDEBAR_AGENTS => {
            vec![("DEFAULT_SIDEBAR_AGENTS", DEFAULT_SIDEBAR_AGENTS.to_json())]
        }
        module::SIDEBAR_COMMANDS => vec![(
            "DEFAULT_BROWSER_ACTION_URL",
            DEFAULT_BROWSER_ACTION_URL.to_json(),
        )],
        module::TERMINAL_FONT_PRESET => vec![
            (
                "DEFAULT_TERMINAL_FONT_PRESET",
                DEFAULT_TERMINAL_FONT_PRESET.to_json(),
            ),
            (
                "MONOSPACE_TERMINAL_FONT_FAMILY",
                MONOSPACE_TERMINAL_FONT_FAMILY.to_json(),
            ),
            ("TERMINAL_FONT_PRESETS", TERMINAL_FONT_PRESETS.to_json()),
        ],
        module::TITLEBAR_COLOR => vec![
            (
                "CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_COLOR",
                CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_CALIBRATION_COLOR.to_json(),
            ),
            (
                "CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS",
                CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARK_TINTS.to_json(),
            ),
            (
                "CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS",
                CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_LIGHT_TINTS.to_json(),
            ),
            (
                "CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_COLOR",
                CUSTOM_SIDEBAR_TITLEBAR_LIGHT_BACKGROUND_CALIBRATION_COLOR.to_json(),
            ),
        ],
        _ => Vec::new(),
    }
}

/// Every area id `exports` answers for.
pub const MODULES: &[&str] = &[
    module::AGENT_ACCOUNTS,
    module::AGENT_SKILLS,
    module::COMPLETION_SOUND,
    module::GHOSTTY_CONFIG_ACTIONS,
    module::HOTKEYS,
    module::OFFICIAL_EXTENSIONS,
    module::OPEN_TARGETS,
    module::PROJECT_VIEWS,
    module::SEARCH_CATALOG,
    module::SESSION_CARD_HOVER_ACTIONS,
    module::SESSION_TAGS,
    module::SETTINGS,
    module::SETTINGS_TYPES,
    module::SIDEBAR_AGENTS,
    module::SIDEBAR_AGENT_ACCEPT_ALL,
    module::SIDEBAR_COMMANDS,
    module::TERMINAL_FONT_PRESET,
    module::TITLEBAR_COLOR,
];

/// `HOTKEY_SETTINGS_SECTIONS` followed by the Terminal focused section: the terminal-only hotkeys,
/// then every other hotkey with a Terminal focused binding in the order the sections above list
/// them. Its `"terminal": true` makes Settings read and save `terminalHotkeys` for its rows.
fn hotkey_settings_sections() -> Json {
    let Json::Arr(mut sections) = HOTKEY_SETTINGS_SECTIONS.to_json() else {
        unreachable!("HOTKEY_SETTINGS_SECTIONS is an array");
    };
    let mut ids: Vec<&'static str> = crate::hotkeys::TERMINAL_ONLY_HOTKEY_IDS.to_vec();
    for section in HOTKEY_SETTINGS_SECTIONS.as_array() {
        for id in section.get("ids").map(J::as_array).unwrap_or_default() {
            let Some(id) = id.as_str() else { continue };
            if crate::hotkeys::hotkey_definition(id)
                .is_some_and(crate::hotkeys::hotkey_has_terminal_binding)
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
    }
    sections.push(Json::obj([
        ("id", Json::str("terminalFocused")),
        ("title", Json::str("Terminal focused")),
        ("terminal", Json::Bool(true)),
        ("ids", Json::Arr(ids.into_iter().map(Json::str).collect())),
    ]));
    Json::Arr(sections)
}
