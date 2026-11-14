//! Experience: Chill, Regular and Advanced, named sets of existing settings that decide how many
//! buttons and details each area of Ghostex shows (Settings > Experience). Picking a preset writes
//! its settings; nothing records the pick. Each area shows the preset its settings match, and says
//! Custom when they match none; the Experience row itself selects a preset only while every area
//! matches it, and says Custom otherwise. The desktop, the web build, `ghostex settings` and the
//! Help files read these tables; the phone keeps a copy in `src/settings/experience.ts`.
//!
//! CDXC:Experience 2026-11-15 DECISION:
//! The user answered the presets page (issue #618, Q3280 to Q3289). Q3284 one global choice plus a choice per area: "if i change settings below then it should not select any of the 3 main preset selections and should say "Custom" next to them". Q3285 picking a preset writes the settings and an area that no longer matches says Custom. Q3287 existing users: nothing moves, each area shows the preset its settings match. Q3288 no new syncing: each device keeps its settings. Q3289 the first areas are the chat box, the chat and the sessions list. Q3283, of the sessions list: "don't show number of files changed in advanced. i dont like that. make advanced match exactly my settings that i have set now basically for the stuff these presets touch", so every area's Advanced is the values the user's own computer had on 2026-11-15 (all chat box controls shown; Quiet View for coordinators and chats with the last turn open, no file edit previews, Normal view; the Sidebar's Detailed preset, which shows no changed-file count).
//! SEE-ALSO: apps/desktop/src/app/window/settings_modal/tabs/general/experience.rs (the page), apps/mobile/app/src/settings/experience.ts (the phone's copy and its page), packages/settings-catalog/src/data/presets.rs (`SIDEBAR_SETTINGS_PRESETS`, the sessions list's three).

use crate::json::{ToJson, J};
use serde_json::Value;

/// The three presets, calmest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    Chill,
    Regular,
    Advanced,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Chill, Preset::Regular, Preset::Advanced];

    pub fn id(self) -> &'static str {
        match self {
            Preset::Chill => "chill",
            Preset::Regular => "regular",
            Preset::Advanced => "advanced",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Chill => "Chill",
            Preset::Regular => "Regular",
            Preset::Advanced => "Advanced",
        }
    }

    /// What the preset is for, as the first-run card and the Experience page say it.
    ///
    /// CDXC:Experience 2026-11-15 DECISION:
    /// Q3286, of the first-run card's words: Regular does show the mode picker; Regular hides the terminal button and shows the attach button instead; Advanced is for people who use terminals more.
    pub fn description(self) -> &'static str {
        match self {
            Preset::Chill => "A simple chat box and a calm chat. Good for checking in on your agents.",
            Preset::Regular => "Pick the model, the mode and the effort as you chat. The terminal is in the ••• menu; the chat box shows attach instead.",
            Preset::Advanced => "For people who use terminals more: the terminal button in the chat box and every detail on screen.",
        }
    }

    pub fn from_id(id: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|preset| preset.id() == id)
    }
}

/// The preset a new user starts on: the first-run card preselects it (Q3286 A).
pub const FIRST_RUN_PRESET: Preset = Preset::Regular;

/// The chat box's mode picker (the mode pill on the computer, the glyph in the phone's model pill).
pub const COMPOSER_MODE_PICKER_KEY: &str = "chatComposerModePicker";
/// The terminal button in the chat box's row; off, Terminal View is a row in the ••• menu.
pub const COMPOSER_TERMINAL_BUTTON_KEY: &str = "chatComposerTerminalButton";
/// The context ring (how full the agent's context is) in the chat box's row.
pub const COMPOSER_CONTEXT_RING_KEY: &str = "chatComposerContextRing";
/// The computer's Maximize button over the chat box.
pub const COMPOSER_MAXIMIZE_KEY: &str = "chatComposerMaximize";
/// Chill's Quiet View: the newest turn stays one line with its time, and lines name only reports.
pub const QUIET_VIEW_CALM_KEY: &str = "chatQuietViewCalm";

/// One area of the app and the settings each preset gives it.
#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Chill, Regular, Advanced, in that order; every preset names the same keys.
    pub presets: [&'static [(&'static str, J)]; 3],
}

impl Area {
    pub fn settings(&self, preset: Preset) -> &'static [(&'static str, J)] {
        self.presets[preset as usize]
    }

    /// The keys the area's presets set.
    pub fn keys(&self) -> impl Iterator<Item = &'static str> {
        self.presets[0].iter().map(|(key, _)| *key)
    }
}

/// CDXC:Experience 2026-11-15 DECISION:
/// Q3281 A, Chill's chat box keeps the model name and the paperclip; and "we have to keep effort shown please always, and when mode is hidden in chill just always keep it on auto or bypass permissions based on the setting on that machine basically", so no preset hides the effort, and with the mode picker off the session runs in Auto or Bypass permissions as that computer's Skip permissions setting says. Q3282 A, off Advanced the terminal button leaves the row for a Terminal View row in •••. From the page's table: Regular also drops the context ring, and Chill drops the mode and the computer's Maximize.
const CHAT_BOX: Area = Area {
    id: "chatBox",
    label: "Chat box",
    description: "Chill: the field, the model, the effort, •••, attach, the mic and Send. Regular adds the mode and Maximize. Advanced adds the terminal button and the context ring.",
    presets: [
        &[
            (COMPOSER_MODE_PICKER_KEY, J::Bool(false)),
            (COMPOSER_TERMINAL_BUTTON_KEY, J::Bool(false)),
            (COMPOSER_CONTEXT_RING_KEY, J::Bool(false)),
            (COMPOSER_MAXIMIZE_KEY, J::Bool(false)),
        ],
        &[
            (COMPOSER_MODE_PICKER_KEY, J::Bool(true)),
            (COMPOSER_TERMINAL_BUTTON_KEY, J::Bool(false)),
            (COMPOSER_CONTEXT_RING_KEY, J::Bool(false)),
            (COMPOSER_MAXIMIZE_KEY, J::Bool(true)),
        ],
        &[
            (COMPOSER_MODE_PICKER_KEY, J::Bool(true)),
            (COMPOSER_TERMINAL_BUTTON_KEY, J::Bool(true)),
            (COMPOSER_CONTEXT_RING_KEY, J::Bool(true)),
            (COMPOSER_MAXIMIZE_KEY, J::Bool(true)),
        ],
    ],
};

/// CDXC:Experience 2026-11-15 DECISION:
/// Q3280 B, Chill folds Quiet View further: the newest turn stays one line ("Working · 1m 12s") and lines name only reports; nothing changes for Regular or Advanced. Regular is today's defaults and Advanced the user's own values, which are the same here, so a chat set either way shows the preset the rest of Experience is on.
const CHAT: Area = Area {
    id: "chat",
    label: "Chat",
    description: "Chill: Quiet View keeps the running turn as one line and names only reports. Regular and Advanced: Quiet View with the last turn open.",
    presets: [
        &[
            ("chatQuietViewCoordinators", J::Bool(true)),
            ("chatQuietViewChats", J::Bool(true)),
            ("chatQuietViewExpandLast", J::Bool(false)),
            (QUIET_VIEW_CALM_KEY, J::Bool(true)),
            ("sessionChatDetailedView", J::Bool(false)),
            ("sessionChatFileEditPreviews", J::Bool(false)),
        ],
        &[
            ("chatQuietViewCoordinators", J::Bool(true)),
            ("chatQuietViewChats", J::Bool(true)),
            ("chatQuietViewExpandLast", J::Bool(true)),
            (QUIET_VIEW_CALM_KEY, J::Bool(false)),
            ("sessionChatDetailedView", J::Bool(false)),
            ("sessionChatFileEditPreviews", J::Bool(false)),
        ],
        &[
            ("chatQuietViewCoordinators", J::Bool(true)),
            ("chatQuietViewChats", J::Bool(true)),
            ("chatQuietViewExpandLast", J::Bool(true)),
            (QUIET_VIEW_CALM_KEY, J::Bool(false)),
            ("sessionChatDetailedView", J::Bool(false)),
            ("sessionChatFileEditPreviews", J::Bool(false)),
        ],
    ],
};

/// CDXC:Experience 2026-11-15 DECISION:
/// Q3283 A, the sessions list reuses the Sidebar's presets: Chill is Minimal, Regular is Recommended, Advanced is Detailed (which shows no changed-file count). The Sidebar's own Preset row keeps its four, Codex included.
const SESSIONS_LIST: Area = Area {
    id: "sessionsList",
    label: "Sessions list",
    description:
        "Chill is the Sidebar's Minimal preset, Regular its Recommended and Advanced its Detailed.",
    presets: [
        &[
            ("showProjectIcons", J::Bool(false)),
            ("hideSessionAgentIconUntilHover", J::Bool(true)),
            ("hideBrowserFaviconUntilHover", J::Bool(true)),
            ("hideLastActiveTimeOnSessionCards", J::Bool(true)),
            ("hideProjectHeaderDiffStats", J::Bool(true)),
            ("showProjectEditorDiffFileCount", J::Bool(false)),
        ],
        &[
            ("showProjectIcons", J::Bool(true)),
            ("hideSessionAgentIconUntilHover", J::Bool(false)),
            ("hideBrowserFaviconUntilHover", J::Bool(false)),
            ("hideLastActiveTimeOnSessionCards", J::Bool(true)),
            ("hideProjectHeaderDiffStats", J::Bool(false)),
            ("showProjectEditorDiffFileCount", J::Bool(false)),
        ],
        &[
            ("showProjectIcons", J::Bool(true)),
            ("hideSessionAgentIconUntilHover", J::Bool(false)),
            ("hideBrowserFaviconUntilHover", J::Bool(false)),
            ("hideLastActiveTimeOnSessionCards", J::Bool(false)),
            ("hideProjectHeaderDiffStats", J::Bool(false)),
            ("showProjectEditorDiffFileCount", J::Bool(false)),
        ],
    ],
};

/// The areas Experience covers so far (Q3289), in the order the page lists them.
pub const AREAS: [Area; 3] = [CHAT_BOX, CHAT, SESSIONS_LIST];

pub fn area(id: &str) -> Option<&'static Area> {
    AREAS.iter().find(|area| area.id == id)
}

/// The presets whose every setting `value` (a key's current value, its default when unsaved)
/// matches. Regular and Advanced give the chat the same settings, so a chat can match both.
pub fn area_matches(area: &Area, value: &dyn Fn(&str) -> Value) -> Vec<Preset> {
    Preset::ALL
        .into_iter()
        .filter(|preset| {
            area.settings(*preset)
                .iter()
                .all(|(key, wanted)| wanted.to_json().to_value() == value(key))
        })
        .collect()
}

/// The preset every area matches, which the Experience row selects; `None` is Custom.
pub fn overall(value: &dyn Fn(&str) -> Value) -> Option<Preset> {
    let matches: Vec<Vec<Preset>> = AREAS.iter().map(|area| area_matches(area, value)).collect();
    Preset::ALL
        .into_iter()
        .find(|preset| matches.iter().all(|area| area.contains(preset)))
}

/// The preset an area's row selects; `None` is Custom. An area that matches several presets shows
/// the one the whole of Experience is on, else the one most other areas match, else the calmest.
pub fn shown(area: &Area, value: &dyn Fn(&str) -> Value) -> Option<Preset> {
    let matches = area_matches(area, value);
    if matches.len() <= 1 {
        return matches.first().copied();
    }
    if let Some(preset) = overall(value).filter(|preset| matches.contains(preset)) {
        return Some(preset);
    }
    let votes = |preset: &Preset| {
        AREAS
            .iter()
            .filter(|other| other.id != area.id)
            .filter(|other| area_matches(other, value) == vec![*preset])
            .count()
    };
    let most = matches.iter().map(votes).max().unwrap_or(0);
    matches.into_iter().find(|preset| votes(preset) == most)
}

/// The settings picking `preset` for `area` writes.
pub fn writes(area: &Area, preset: Preset) -> Vec<(&'static str, Value)> {
    area.settings(preset)
        .iter()
        .map(|(key, value)| (*key, value.to_json().to_value()))
        .collect()
}

/// The settings picking `preset` for the whole of Experience writes: every area's.
pub fn writes_all(preset: Preset) -> Vec<(&'static str, Value)> {
    AREAS.iter().flat_map(|area| writes(area, preset)).collect()
}
