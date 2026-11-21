//! The Settings catalog: every Settings search row (titles and subtitles are customer copy), the
//! setting defaults, option tables and ranges, and the hotkey catalog.
//!
//! The native Settings window, Quick Access (through gx-core), `ghostex settings` (gxserver) and the
//! Ghostex Help files (`cargo xtask help-generate`) all read this crate, so a row, default or hotkey
//! changes in one place.
//!
//! CDXC:Settings 2026-10-01 DECISION:
//! User (answer 33A, docs/2026-10-01/zero-ts/NEXT-STEPS.md): move the Settings search rows, defaults and hotkeys from TypeScript to Rust, so the native app owns them and the Help files are generated from Rust. This crate is that one source; it supersedes `CDXC:Settings 2026-09-28`, which kept `packages/core-ui/settings-modal/search-catalog.ts` as the source and embedded a generated JSON copy in the desktop.
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/catalog.rs, server/src/ghostex_cli/settings.rs, packages/gx-core/src/quick_access/hotkeys.rs, tooling/xtask/src/help.rs, server/src/ghostex_cli/guide/.
//!
//! To add a setting: give it a default in `data/defaults.rs`, a row in the `general/` or `pages/`
//! file of the page that shows it, and run `cargo xtask help-generate`.

pub mod availability;
pub mod built_in_extensions;
pub mod dark_chrome;
mod data;
pub mod experience;
pub mod file_opening;
pub mod folder_trust;
mod general;
pub mod help;
pub mod hotkey_chords;
mod hotkey_definitions;
pub mod hotkey_label;
pub mod hotkeys;
pub mod interface_scale;
pub mod intros;
pub mod json;
pub mod layout;
pub mod modules;
mod pages;
pub mod platform_text;
pub mod rows;
mod search_keywords;
pub mod spaces;
pub mod theme_colours;
pub mod theme_roles;
pub mod update_checks;
pub use search_keywords::{search_keywords, section_search_keywords};

pub use data::{
    theme_gallery, DEFAULT_LOOK_ID, DEFAULT_LOOK_NAME, MAX_WINDOW_ART_BLUR, MAX_WINDOW_ART_DIM,
};
pub use hotkey_chords::{
    customized_normal_hotkey, hotkey_keystroke, platform_hotkey, terminal_hotkey,
};
pub use hotkey_label::hotkey_label;
pub use hotkeys::{
    default_hotkeys, hotkey_definition, hotkey_definitions, hotkey_has_normal_binding,
    hotkey_has_terminal_binding, terminal_default_key, windows_linux_terminal_default_key,
    HotkeyDefinition, TERMINAL_HOTKEYS_SETTING_KEY, TERMINAL_ONLY_HOTKEY_IDS,
    VIEW_SCOPED_HOTKEY_IDS,
};
pub use intros::page_intro;
pub use json::{Json, ToJson, J};
pub use layout::{general_group, general_navigation, GeneralGroup, NavItem, GENERAL_GROUPS};
pub use rows::{Page, RuntimeOptions, Section, SettingOption, SettingRow};

/// The desktop platform a catalog is built for: shortcut labels and a few rows differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    /// The platform this binary was built for.
    pub const fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// The General page's search sections, in definition order.
pub fn general_sections(platform: Platform) -> Vec<Section> {
    general::sections(platform)
}

/// The searchable pages other than General, Theme and Hotkeys.
pub fn extra_pages(platform: Platform) -> Vec<Page> {
    pages::pages(platform)
}

/// Whether Settings' own search lists a row for `key`: a row of the General (and Theme) sections
/// or of another searchable page. Keys the Help catalog adds without one (its supplemental rows and
/// app state) have none, so searching Settings for their title finds nothing.
pub fn has_search_row(platform: Platform, key: &str) -> bool {
    general_sections(platform)
        .iter()
        .any(|section| section.settings.iter().any(|row| row.key == key))
        || extra_pages(platform).iter().any(|page| {
            page.sections
                .iter()
                .any(|section| section.settings.iter().any(|row| row.key == key))
        })
}

/// Every setting key with its default value, in the order the settings file lists them.
pub fn defaults() -> &'static [(&'static str, J)] {
    data::DEFAULT_GHOSTEX_SETTINGS
}

pub fn default_value(key: &str) -> Option<&'static J> {
    data::DEFAULT_GHOSTEX_SETTINGS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
}

/// Fills in Default to Detailed view (`sessionChatDetailedView`) from the default keys it replaced,
/// for a settings file that never saved it: a saved `sessionChatVerboseMode` true or
/// `sessionChatSimpleMode` false opened new chats in more detail than Simple, which is Detailed now.
/// The old keys are left alone; nothing reads them any more.
///
/// CDXC:SessionChat 2026-11-02 WHY: the user retired Verbose and renamed Simple to Normal (general/chat.rs has their words), and a preference saved under the old keys must not reset.
pub fn migrate_chat_view_default(settings: &mut serde_json::Map<String, serde_json::Value>) {
    if settings.contains_key("sessionChatDetailedView") {
        return;
    }
    let verbose = settings.get("sessionChatVerboseMode") == Some(&serde_json::Value::Bool(true));
    let not_simple = settings.get("sessionChatSimpleMode") == Some(&serde_json::Value::Bool(false));
    if verbose || not_simple {
        settings.insert(
            "sessionChatDetailedView".to_string(),
            serde_json::Value::Bool(true),
        );
    }
}

/// Drops the retired Agent work (`chatAgentWorkStyle`) and Tool call cards (`chatToolCallCards`)
/// settings, and turns Quiet View on for other chats (`chatQuietViewChats`) for someone who had
/// Agent work Reduced or Quiet or Tool call cards on. Returns whether it changed anything; a
/// settings object without the old keys is left alone.
///
/// CDXC:SessionChat 2026-11-13 DECISION: The user answered the chat view modes page, Q2773, Q2774
/// and Q2775 "retire": Agent work Reduced, Agent work Quiet and the Tool call cards preview switch
/// are removed, anyone who had one of them picked lands on Quiet View, and their settings are
/// dropped. Quiet View is on for every chat by default since Q2771, so this matters only for a
/// file that saved `chatQuietViewChats` false; such a person had picked one of the retired looks
/// after that, so the look they wanted wins. The desktop saves this once at start
/// (shared_settings/experimental_switches.rs) and every reader applies it, as for
/// `migrate_experimental_features_switch`.
pub fn migrate_retired_chat_looks(
    settings: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    let work = settings.remove("chatAgentWorkStyle");
    let cards = settings.remove("chatToolCallCards");
    if work.is_none() && cards.is_none() {
        return false;
    }
    let picked = matches!(
        work.as_ref().and_then(serde_json::Value::as_str),
        Some("reduced" | "quiet")
    ) || cards.as_ref().and_then(serde_json::Value::as_bool) == Some(true);
    if picked {
        settings.insert(
            "chatQuietViewChats".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    true
}

/// Which sessions mark their handovers (`markedHandovers`): the coordinators (the default), every
/// session, or none. A coordinator's threads mark their reports under all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkedHandovers {
    Off,
    Coordinators,
    Every,
}

impl MarkedHandovers {
    /// The saved value, read the way `migrate_marked_handovers` turns the old switch into it: true is
    /// Every session, false Coordinators only. Anything else (nothing saved, an unknown word) is the
    /// default, Coordinators only.
    pub fn from_setting(value: Option<&serde_json::Value>) -> Self {
        match value {
            Some(serde_json::Value::String(word)) if word == "off" => Self::Off,
            Some(serde_json::Value::String(word)) if word == "all" => Self::Every,
            Some(serde_json::Value::Bool(true)) => Self::Every,
            _ => Self::Coordinators,
        }
    }

    /// Whether coordinators write their replies as marked sections.
    pub fn coordinators(self) -> bool {
        self != Self::Off
    }

    /// Whether every other agent session Ghostex starts gets the note.
    pub fn every_session(self) -> bool {
        self == Self::Every
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Coordinators => "coordinators",
            Self::Every => "all",
        }
    }
}

/// Turns the old Marked handovers in every session switch (`markedHandovers` true or false) and
/// Coordinators answer without markers (`orchestratorPlainReplies`) into the three-value Marked
/// handovers: true is Every session; false or nothing is Coordinators only, or Off when coordinators
/// answered without markers. Returns whether it changed anything; a file that already holds a word
/// and no old key is left alone, so it runs once per file (and again only for an older computer's
/// synced switch).
///
/// CDXC:MarkedHandovers 2026-11-13 DECISION: User (Q2963 "A"): one setting, Off / Coordinators only /
/// Every session, default Coordinators only; an explicit true becomes Every session, false or unset
/// Coordinators only. The settings file stores every default, so `false` is what almost every file
/// holds and says nothing about a choice. Coordinators answer without markers only ever kept
/// coordinators on prose, so with the switch off it is Off; with the switch on, the explicit Every
/// session wins (the three values have no "everyone but coordinators"). The desktop saves this once
/// at start (shared_settings/experimental_switches.rs) and every reader applies it, as for
/// `migrate_experimental_features_switch`.
pub fn migrate_marked_handovers(settings: &mut serde_json::Map<String, serde_json::Value>) -> bool {
    let plain = settings.remove("orchestratorPlainReplies");
    let current = settings.get("markedHandovers");
    let next = match current {
        Some(serde_json::Value::String(_)) => None,
        Some(serde_json::Value::Bool(true)) => Some(MarkedHandovers::Every),
        Some(_) | None if plain.as_ref().and_then(serde_json::Value::as_bool) == Some(true) => {
            Some(MarkedHandovers::Off)
        }
        Some(_) => Some(MarkedHandovers::Coordinators),
        None => None,
    };
    if let Some(next) = next {
        settings.insert(
            "markedHandovers".to_string(),
            serde_json::Value::String(next.as_str().to_string()),
        );
    }
    plain.is_some() || next.is_some()
}

/// The switches Enable Experimental Features (`showBetaFeatures`) used to open, with the value of
/// each that means on, and whether it was on by default under the old switch.
const EXPERIMENTAL_FEATURE_SWITCHES: &[(&str, bool, bool)] = &[
    ("readAloudHidden", false, false),
    ("connectorsHidden", false, true),
    ("osIntegrationHidden", false, true),
    ("keepAwakeEnabled", true, true),
    ("allAutomationsEnabled", true, true),
];

/// Replaces Enable Experimental Features (`showBetaFeatures`) with the switches it opened: each
/// gets the state it had, on only when the old switch was on and the switch itself was on (or
/// unset and on by default), and the old key is removed. Returns whether it changed anything; a
/// settings object without the old key is left alone, so it runs once per settings file.
///
/// CDXC:Extensions 2026-11-09 DECISION: User (Q2043 on the Extensions tabs page): "remove the
/// 'Experimental features' toggle itself and move toggling each of those individually here
/// instead", with each new switch off by default and someone who had the old switch on keeping
/// everything it showed. The settings file stores every default, so most files carry
/// `showBetaFeatures: false` and `connectorsHidden: false`: the old effective state is computed
/// from both rather than left to the new defaults. Terminal shaders, GBots and Main Hub kept their
/// own switch under the old one and are not touched. The desktop saves this once at start
/// (shared_settings/experimental_switches.rs); gxserver applies it to every settings read and
/// before saving the file itself, so a gxserver that starts before the app, or runs without one,
/// sees the same switches.
pub fn migrate_experimental_features_switch(
    settings: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    let Some(legacy) = settings.remove("showBetaFeatures") else {
        return false;
    };
    let was_on = legacy.as_bool() == Some(true);
    for (key, on_value, on_by_default) in EXPERIMENTAL_FEATURE_SWITCHES {
        let switch_on = settings
            .get(*key)
            .and_then(serde_json::Value::as_bool)
            .map_or(*on_by_default, |value| value == *on_value);
        let now_on = was_on && switch_on;
        settings.insert(
            (*key).to_string(),
            serde_json::Value::Bool(if now_on { *on_value } else { !*on_value }),
        );
    }
    true
}

/// The secret-folder entries that keep agents out of the CLIProxyAPI hub profiles' key files, added
/// to the defaults with the hub (`homeSecretFolders`).
pub const HUB_SECRET_FOLDERS: &[&str] = &[
    "~/.claude-profiles/ghostex-*/.ghostex-hub-key",
    "~/.codex-profiles/ghostex-*/config.toml",
];

/// Adds [`HUB_SECRET_FOLDERS`] to a saved `homeSecretFolders` list that lacks them. Returns whether
/// it changed anything. The desktop runs it once, behind a marker file, so an entry the user
/// removes later stays removed.
///
/// CDXC:Chief 2026-11-19 WHY: the settings file stores every default, so almost everyone's file holds the list as it was before the hub entries existed and the new defaults would never reach them (re-review of the hub work).
pub fn add_hub_secret_folders(settings: &mut serde_json::Map<String, serde_json::Value>) -> bool {
    let Some(list) = settings
        .get_mut("homeSecretFolders")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for entry in HUB_SECRET_FOLDERS {
        if !list.iter().any(|saved| saved.as_str() == Some(*entry)) {
            list.push(serde_json::Value::String((*entry).to_string()));
            changed = true;
        }
    }
    changed
}

/// Switches someone who already had a settings file to the rail when it became the default, and
/// marks their one-time "Meet the rail" pop-up as pending. Returns whether it changed anything; a
/// layout they chose themselves is left alone.
///
/// CDXC:Sidebar 2026-11-20 DECISION: User, for 11.12.0: "On first start of this version, existing users are switched to the Rail automatically (not left on Classic), and the pop-up explains it." The desktop runs this once, behind a marker file, at start-up when the settings file already exists (`migrate_sidebar_rail_default_once`); a new install has no file then, so it starts on Rail without the pop-up. A file a build of the same day set to Classic with its pop-up still pending was never answered, so it is switched too.
pub fn switch_existing_settings_to_rail(
    settings: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    let text = |key: &str| settings.get(key).and_then(serde_json::Value::as_str);
    let unanswered_classic =
        text("sidebarLayout") == Some("classic") && text("sidebarRailIntro") == Some("pending");
    if settings.contains_key("sidebarLayout") && !unanswered_classic {
        return false;
    }
    settings.insert(
        "sidebarLayout".to_string(),
        serde_json::Value::String("rail".to_string()),
    );
    settings.insert(
        "sidebarRailIntro".to_string(),
        serde_json::Value::String("pending".to_string()),
    );
    true
}

/// Takes Notifications off the rail's saved pins (`sidebarRailPins`). Returns whether it changed
/// anything; a file without saved pins, or whose pins lack it, is left alone.
///
/// CDXC:Sidebar 2026-11-20 DECISION:
/// Coordinator, after the user put the bell in the sidebar's top bar with the rail on (Q4318 B): "avoid the double bell for everyone on 11.12.0. Add a one-time step (with its own marker, like the rail switch) that removes Notifications from saved rail pins, since 11.12.0 wrote it from the old defaults; after that, pinning it again from ⋯ sticks." 11.12.0 saved the whole list, defaults included, the first time anyone pinned, unpinned or dragged an icon, so a saved bell pin cannot be told from a chosen one. The desktop runs this once, behind its own marker file, at start-up (`migrate_sidebar_rail_bell_once`); a pin made after that is kept.
pub fn unpin_bell_from_saved_rail_pins(
    settings: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    let Some(serde_json::Value::Array(pins)) = settings.get_mut("sidebarRailPins") else {
        return false;
    };
    let before = pins.len();
    pins.retain(|pin| pin.as_str() != Some("notifications"));
    pins.len() != before
}

/// The keys `sidebarRailPins` held for items that are no longer on the rail: the side panel views
/// and Notifications (gx-core's `RAIL_ITEMS` no longer lists them).
pub const RETIRED_RAIL_PIN_KEYS: &[&str] = &[
    "notifications",
    "files",
    "changes",
    "browser",
    "projectBoard",
    "devServers",
    "resources",
    "extensions",
];

/// Takes the retired items (`RETIRED_RAIL_PIN_KEYS`) off the rail's saved pins. Returns whether it
/// changed anything; a file without saved pins, or whose pins lack them, is left alone.
///
/// CDXC:Sidebar 2026-11-21 DECISION:
/// User: views on the rail "don't make sense", "even when I have a view open in the side panel, don't show it in the left-side rail"; the rail is for agents and hubs. The views and Notifications left the rail and its ⋯ menu, so the rail no longer reads them from the pins; the desktop runs this once, behind its own marker file, at start-up (`migrate_sidebar_rail_views_once`), so the settings file stops carrying them too (coordinator: "A one-time cleanup removes view ids from saved rail pins").
pub fn drop_retired_rail_pins(settings: &mut serde_json::Map<String, serde_json::Value>) -> bool {
    let Some(serde_json::Value::Array(pins)) = settings.get_mut("sidebarRailPins") else {
        return false;
    };
    let before = pins.len();
    pins.retain(|pin| {
        pin.as_str()
            .is_none_or(|key| !RETIRED_RAIL_PIN_KEYS.contains(&key))
    });
    pins.len() != before
}

/// The look settings whose default changed on 2026-11-20 (the user's own dark look became the
/// default), each with the default it had before.
pub const DARK_LOOK_2026_11_20_OLD_DEFAULTS: &[(&str, J)] = &[
    ("darkThemePreset", J::Str("gray")),
    ("windowGlassSidebarOpacityDark", J::Num(94.0)),
    ("windowGlassWorkAreaTintDark", J::Num(94.0)),
    ("windowGlassSidebarOpacityLight", J::Num(97.0)),
    ("windowGlassWorkAreaTintLight", J::Num(97.0)),
    ("windowGlassBlurRadius", J::Num(60.0)),
    ("terminalGhosttyTheme", J::Str("GitHub Dark")),
    ("themeRoles", J::Obj(&[])),
    ("windowGlassLiveStyleDark", J::Str("aurora")),
    ("workspacePaneGap", J::Num(0.0)),
    ("windowGlassMenuBlurRadius", J::Num(20.0)),
];

/// The look settings whose default changed on 2026-11-21 (the user's Windows look, Inkstone's
/// colours on a Custom `#46506a` with Live Ink glass, replaced the Black look), each with the
/// default it had on 2026-11-20. Every key here not also in `DARK_LOOK_2026_11_20_OLD_DEFAULTS`
/// had that same default before 2026-11-20 too.
pub const DARK_LOOK_2026_11_21_OLD_DEFAULTS: &[(&str, J)] = &[
    ("darkThemePreset", J::Str("black")),
    (
        "customSidebarTitlebarBackgroundTintColor",
        J::Str("#808080"),
    ),
    (
        "themeRoles",
        J::Obj(&[(
            "dark",
            J::Obj(&[
                ("userMessage", J::Str("#0000006b")),
                ("border", J::Str("#3e3e3e6b")),
            ]),
        )]),
    ),
    ("workspaceActivePaneBorderColor", J::Str("#3b82f6")),
    ("windowGlassSource", J::Str("desktopAndWindows")),
    ("windowGlassLiveSpeed", J::Num(1.0)),
    ("windowGlassLiveBrightness", J::Num(60.0)),
];

/// The keep-old-look steps, oldest first: the day a dark look became the default and the
/// defaults it replaced. The desktop runs each once, in this order, behind its own marker file
/// (`keep_old_dark_look_once` in apps/desktop/src/shared_settings/dark_look_default.rs), so a
/// file the first step filled is not touched again by the second for the same key.
pub const DARK_LOOK_STEPS: &[(&str, &[(&str, J)])] = &[
    ("2026-11-20", DARK_LOOK_2026_11_20_OLD_DEFAULTS),
    ("2026-11-21", DARK_LOOK_2026_11_21_OLD_DEFAULTS),
];

/// Writes the old default of every look setting in `old_defaults` (one of `DARK_LOOK_STEPS`) that
/// the settings of an existing user do not carry, so their window keeps the look it had. Returns
/// whether it changed anything; a saved value (whatever it is) is left alone.
///
/// CDXC:Theming 2026-11-20 DECISION:
/// User (Q4290), on whether long-time users who never touched these settings should get the new dark look on update: keep their current look. Stored settings leave a key out until it is saved, so without this every existing user with a key missing would switch to Black, 88 tints, a 4 point blur, Kanagawa Dragon, the dark theme colours, Ink, pane gap 6 and menu blur 17 on update. The desktop runs this once, behind a marker file, at start-up and only for a settings file that already exists from a finished setup (`keep_old_dark_look_once` in apps/desktop/src/shared_settings/dark_look_default.rs); a new install, or one whose setup never finished, gets the new defaults.
/// Three keys need more than their old default (Opus review of ce6b86fa1): Same for light and dark is worked out from the dark and light pairs when it was never saved, and the dark tints and Live background this adds would set them apart and turn it off, so its current answer is saved first. A file with no preset whose Custom colour was changed shows that colour (`dark_chrome_raw_controls`), so it gets "custom", not Gray. A work area tint is left out where the retired `windowGlassMainOpacity*` key still decides it (`store_work_area_glass_percent` in window_glass.rs).
/// CDXC:Theming 2026-11-21 WHY:
/// The 2026-11-21 step (the user's Windows look as the default) follows the same rule for the same reason: a long-time user, and anyone set up on the Black look of 2026-11-20, keeps the look they had. Before both steps a Custom colour with no tint saved drew Graphite's #808080, so "changed" is judged against that tint, not today's default.
pub fn keep_old_look_for_existing_settings(
    settings: &mut serde_json::Map<String, serde_json::Value>,
    old_defaults: &[(&str, J)],
) -> bool {
    use crate::json::ToJson;
    let mut changed = false;
    let sets_a_glass_pair = old_defaults.iter().any(|(key, _)| {
        WINDOW_GLASS_LIGHT_DARK_PAIRS
            .iter()
            .any(|(dark, light)| key == dark || key == light)
    });
    if sets_a_glass_pair && !settings.contains_key("windowGlassSameForLightAndDark") {
        let same = window_glass_same_for_light_and_dark(settings);
        settings.insert(
            "windowGlassSameForLightAndDark".to_string(),
            serde_json::Value::Bool(same),
        );
        changed = true;
    }
    let custom_colour_changed = theme_colours::custom_dark_chrome_controls_with_tint(
        settings,
        theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB,
    ) != (
        theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_DARKNESS_PERCENT,
        theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB,
    );
    for (key, old_default) in old_defaults {
        if settings.contains_key(*key) {
            continue;
        }
        let legacy_tint = match *key {
            "windowGlassWorkAreaTintDark" => Some("windowGlassMainOpacityDark"),
            "windowGlassWorkAreaTintLight" => Some("windowGlassMainOpacityLight"),
            _ => None,
        };
        if legacy_tint.is_some_and(|legacy| settings.contains_key(legacy)) {
            continue;
        }
        let value = if *key == "darkThemePreset" && custom_colour_changed {
            serde_json::Value::String("custom".to_string())
        } else {
            old_default.to_json().to_value()
        };
        settings.insert((*key).to_string(), value);
        changed = true;
    }
    changed
}

/// The dark-mode glass keys and their light-mode twins that Same for light and dark folds into one
/// (CDXC:Settings 2026-10-29 DECISION, cleanup Q19, in packages/settings-catalog/src/general/theme.rs).
pub const WINDOW_GLASS_LIGHT_DARK_PAIRS: [(&str, &str); 5] = [
    ("windowGlassImageDark", "windowGlassImageLight"),
    ("windowGlassVideoDark", "windowGlassVideoLight"),
    ("windowGlassLiveStyleDark", "windowGlassLiveStyleLight"),
    (
        "windowGlassSidebarOpacityDark",
        "windowGlassSidebarOpacityLight",
    ),
    (
        "windowGlassWorkAreaTintDark",
        "windowGlassWorkAreaTintLight",
    ),
];

/// Whether light mode uses the dark-mode picture, video, Live background and tints
/// (`windowGlassSameForLightAndDark`). A settings file that never saw the switch counts as on
/// unless its two sides were already set apart: a pair is "the same" when both values are equal or
/// neither was ever saved, so a fresh install starts with one set of rows and a file whose light
/// mode was tuned on purpose keeps both sets.
pub fn window_glass_same_for_light_and_dark(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    if let Some(same) = object
        .get("windowGlassSameForLightAndDark")
        .and_then(serde_json::Value::as_bool)
    {
        return same;
    }
    WINDOW_GLASS_LIGHT_DARK_PAIRS.iter().all(|(dark, light)| {
        match (object.get(*dark), object.get(*light)) {
            (None, None) => true,
            (dark, light) => dark == light,
        }
    })
}

/// The key light mode reads for `light`: its dark twin while Same for light and dark is on.
pub fn window_glass_light_key<'a>(
    object: &serde_json::Map<String, serde_json::Value>,
    light: &'a str,
) -> &'a str {
    if !window_glass_same_for_light_and_dark(object) {
        return light;
    }
    WINDOW_GLASS_LIGHT_DARK_PAIRS
        .iter()
        .find(|(_, twin)| *twin == light)
        .map_or(light, |(dark, _)| *dark)
}

/// Rows a page folds into its collapsed Advanced section (`isAdvancedMainSetting`).
pub fn is_advanced_setting(key: &str) -> bool {
    data::ADVANCED_MAIN_SETTING_KEYS.contains(&key)
}

/// Rows that carry a "Not recommended" label (`NOT_RECOMMENDED_SETTING_KEYS`).
pub fn is_not_recommended_setting(key: &str) -> bool {
    data::NOT_RECOMMENDED_SETTING_KEYS.contains(&key)
}

/// The setting keys each General group renders, for groups whose sections are not listed in `GENERAL_GROUPS`.
pub fn general_group_setting_keys(group: &str) -> &'static [&'static str] {
    data::MAIN_SETTINGS_SECTION_SETTING_KEYS
        .iter()
        .find(|(id, _)| *id == group)
        .map(|(_, keys)| *keys)
        .unwrap_or(&[])
}

#[cfg(test)]
mod chat_view_default_tests {
    use super::migrate_chat_view_default;
    use serde_json::{json, Map, Value};

    fn migrated(value: Value) -> Option<Value> {
        let mut settings: Map<String, Value> = serde_json::from_value(value).unwrap();
        migrate_chat_view_default(&mut settings);
        settings.get("sessionChatDetailedView").cloned()
    }

    #[test]
    fn old_verbose_or_not_simple_defaults_become_detailed() {
        assert_eq!(
            migrated(json!({"sessionChatVerboseMode": true})),
            Some(json!(true))
        );
        assert_eq!(
            migrated(json!({"sessionChatSimpleMode": false})),
            Some(json!(true))
        );
        assert_eq!(migrated(json!({"sessionChatSimpleMode": true})), None);
        assert_eq!(migrated(json!({})), None);
        // A saved choice of the new switch wins over the old keys.
        assert_eq!(
            migrated(json!({"sessionChatDetailedView": false, "sessionChatVerboseMode": true})),
            Some(json!(false))
        );
    }
}

#[cfg(test)]
mod dark_look_tests {
    use super::{
        default_value, keep_old_look_for_existing_settings, theme_colours, window_glass_light_key,
        window_glass_same_for_light_and_dark, ToJson,
    };
    use serde_json::{json, Map, Value};

    fn settings(value: Value) -> Map<String, Value> {
        serde_json::from_value(value).unwrap()
    }

    fn keep_old_dark_look_for_existing_settings(settings: &mut Map<String, Value>) -> bool {
        keep_old_look_for_existing_settings(settings, super::DARK_LOOK_2026_11_20_OLD_DEFAULTS)
    }

    /// Every step in order, as the desktop runs them over one file.
    fn every_step(settings: &mut Map<String, Value>) {
        for (_, old_defaults) in super::DARK_LOOK_STEPS {
            keep_old_look_for_existing_settings(settings, old_defaults);
        }
    }

    /// The tint a key reads as: its saved value, else the catalog default.
    fn tint(object: &Map<String, Value>, key: &str) -> f64 {
        object
            .get(key)
            .cloned()
            .or_else(|| default_value(key).map(|value| value.to_json().to_value()))
            .and_then(|value| value.as_f64())
            .unwrap()
    }

    /// The four tints as the app reads them: light mode reads the dark key while Same for light
    /// and dark is on.
    fn tints(object: &Map<String, Value>) -> [f64; 4] {
        [
            tint(object, "windowGlassSidebarOpacityDark"),
            tint(object, "windowGlassWorkAreaTintDark"),
            tint(
                object,
                window_glass_light_key(object, "windowGlassSidebarOpacityLight"),
            ),
            tint(
                object,
                window_glass_light_key(object, "windowGlassWorkAreaTintLight"),
            ),
        ]
    }

    /// A set-up file with none of the 11 keys keeps what the version before the new dark look
    /// showed: Gray, Same for light and dark on, and 94 everywhere (light mode reads the dark tints).
    #[test]
    fn a_file_without_the_look_keys_keeps_the_old_look() {
        let before = settings(json!({"sidebarWidth": 320}));
        let mut after = before.clone();
        assert!(keep_old_dark_look_for_existing_settings(&mut after));
        let gray =
            theme_colours::dark_chrome_raw_controls(&settings(json!({"darkThemePreset": "gray"})));
        assert_eq!(theme_colours::dark_chrome_raw_controls(&after), gray);
        assert!(window_glass_same_for_light_and_dark(&before));
        assert!(window_glass_same_for_light_and_dark(&after));
        assert_eq!(tints(&after), [94.0; 4]);
        // Runs once: a second pass finds every key saved.
        assert!(!keep_old_dark_look_for_existing_settings(
            &mut after.clone()
        ));
    }

    /// A file with no preset and a changed Custom colour shows that colour, before and after.
    #[test]
    fn a_custom_colour_without_a_preset_is_kept() {
        let before = settings(json!({
            "customSidebarTitlebarBackgroundTintColor": "#46506a",
            "customSidebarTitlebarBackgroundDarknessPercent": 90,
        }));
        let mut after = before.clone();
        keep_old_dark_look_for_existing_settings(&mut after);
        assert_eq!(after.get("darkThemePreset"), Some(&json!("custom")));
        assert_eq!(
            theme_colours::dark_chrome_raw_controls(&after),
            theme_colours::dark_chrome_raw_controls(&before)
        );
        assert_eq!(
            theme_colours::dark_chrome_raw_controls(&after),
            theme_colours::custom_dark_chrome_controls(&before)
        );
    }

    /// A new install (nothing saved) draws the 2026-11-21 look: the Custom #46506a at depth 96.
    #[test]
    fn a_new_install_draws_the_windows_look() {
        assert_eq!(
            theme_colours::dark_chrome_raw_controls(&Map::new()),
            (96.0, 0x46506a)
        );
        assert_eq!(theme_colours::default_custom_dark_tint_rgb(), 0x46506a);
    }

    /// A file set up on the 2026-11-20 Black look (its first step already ran) keeps Black, its
    /// two theme colours, the blue pane outline and Desktop and windows glass.
    #[test]
    fn a_file_from_the_black_look_keeps_it() {
        let mut after = settings(json!({"sidebarWidth": 320}));
        keep_old_look_for_existing_settings(&mut after, super::DARK_LOOK_2026_11_21_OLD_DEFAULTS);
        let black =
            theme_colours::dark_chrome_raw_controls(&settings(json!({"darkThemePreset": "black"})));
        assert_eq!(theme_colours::dark_chrome_raw_controls(&after), black);
        assert_eq!(
            after.get("themeRoles"),
            Some(&json!({"dark": {"userMessage": "#0000006b", "border": "#3e3e3e6b"}}))
        );
        assert_eq!(
            after.get("windowGlassSource"),
            Some(&json!("desktopAndWindows"))
        );
        assert_eq!(after.get("windowGlassLiveBrightness"), Some(&json!(60)));
        assert_eq!(after.get("windowGlassLiveSpeed"), Some(&json!(1)));
        assert_eq!(
            after.get("workspaceActivePaneBorderColor"),
            Some(&json!("#3b82f6"))
        );
        // No glass pair changes, so Same for light and dark is left to be worked out as before.
        assert!(!after.contains_key("windowGlassSameForLightAndDark"));
    }

    /// A file from before 2026-11-20 runs both steps at one start and keeps Gray, no theme
    /// colours and Desktop and windows glass: the first step's values win for the keys both name.
    #[test]
    fn a_file_from_before_both_looks_keeps_gray() {
        let mut after = settings(json!({"sidebarWidth": 320}));
        every_step(&mut after);
        assert_eq!(after.get("darkThemePreset"), Some(&json!("gray")));
        assert_eq!(after.get("themeRoles"), Some(&json!({})));
        assert_eq!(
            after.get("windowGlassSource"),
            Some(&json!("desktopAndWindows"))
        );
        assert_eq!(
            after.get("customSidebarTitlebarBackgroundTintColor"),
            Some(&json!("#808080"))
        );
        assert_eq!(tints(&after), [94.0; 4]);
    }

    /// A Custom colour picked with no tint saved drew Graphite's #808080 before 2026-11-21; it
    /// still does after the steps, and an untouched Custom with no preset is not taken for one.
    #[test]
    fn a_custom_preset_without_a_tint_keeps_graphite() {
        let mut after = settings(json!({"darkThemePreset": "custom"}));
        every_step(&mut after);
        assert_eq!(
            theme_colours::dark_chrome_raw_controls(&after),
            (
                96.0,
                theme_colours::DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_RGB
            )
        );
        let mut untouched = settings(json!({"sidebarWidth": 320}));
        keep_old_look_for_existing_settings(
            &mut untouched,
            super::DARK_LOOK_2026_11_21_OLD_DEFAULTS,
        );
        assert_eq!(untouched.get("darkThemePreset"), Some(&json!("black")));
    }
}

#[cfg(test)]
mod sidebar_layout_tests {
    use super::switch_existing_settings_to_rail;
    use serde_json::{json, Map, Value};

    fn settings(value: Value) -> Map<String, Value> {
        serde_json::from_value(value).unwrap()
    }

    /// A file that never chose a layout goes to Rail and queues Meet the rail, once.
    #[test]
    fn a_file_without_a_layout_goes_to_the_rail_and_queues_the_pop_up() {
        let mut object = settings(json!({"sidebarWidth": 320}));
        assert!(switch_existing_settings_to_rail(&mut object));
        assert_eq!(object.get("sidebarLayout"), Some(&json!("rail")));
        assert_eq!(object.get("sidebarRailIntro"), Some(&json!("pending")));
        assert!(!switch_existing_settings_to_rail(&mut object));
    }

    /// A layout chosen (the pop-up answered, or Settings) is kept.
    #[test]
    fn a_chosen_layout_is_kept() {
        for layout in ["rail", "classic"] {
            let mut object = settings(json!({"sidebarLayout": layout, "sidebarRailIntro": "done"}));
            assert!(!switch_existing_settings_to_rail(&mut object));
            assert_eq!(object.get("sidebarLayout"), Some(&json!(layout)));
        }
    }
}
