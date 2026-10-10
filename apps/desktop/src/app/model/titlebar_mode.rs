// C1 wave-3 re-cluster: TitlebarMode (Agents/Source/Browser/Kanban/Automate/Manage) plus the mode switcher item and the titlebar Exit Focus control signature, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

#![allow(unused_imports)]

use crate::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExtensionId(&'static str);

impl ExtensionId {
    pub(crate) fn new(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty()
            || value != value.to_ascii_lowercase()
            || value.starts_with('-')
            || value.ends_with('-')
            || value
                .bytes()
                .any(|byte| !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && byte != b'-')
            || value.as_bytes().windows(2).any(|pair| pair == b"--")
        {
            return None;
        }

        static IDS: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
        let mut ids = IDS.get_or_init(|| Mutex::new(HashMap::new())).lock().ok()?;
        if let Some(value) = ids.get(value) {
            return Some(Self(value));
        }
        let interned = Box::leak(value.to_string().into_boxed_str());
        ids.insert(interned.to_string(), interned);
        Some(Self(interned))
    }

    pub(crate) fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TitlebarMode {
    Agents,
    Source,
    Browser,
    Kanban,
    Automate,
    Manage,
    /// The Terminal view: the Commands pane's second tree, drawn as a view tab. See
    /// `model/command_pane_docks.rs`.
    Terminal,
    /// The Bot automations feed of Hermes cron runs: app-wide, drawn by GPUI, opened from the Bots
    /// sidebar's Automations row. See `app/native_bot_feed/`.
    BotFeed,
    /// The Work page: the tickets, issues and PRs of every work-mode project the window shows,
    /// app-wide, as a first-party web page. See `app/work_view/`.
    Work,
    Extension(ExtensionId),
}

impl TitlebarMode {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "agents" => Some(Self::Agents),
            "source" => Some(Self::Source),
            "browser" => Some(Self::Browser),
            "kanban" => Some(Self::Kanban),
            "automate" => Some(Self::Automate),
            "manage" => Some(Self::Manage),
            "terminal" => Some(Self::Terminal),
            "bot-feed" => Some(Self::BotFeed),
            "work" => Some(Self::Work),
            value if value.starts_with("extension:") => {
                ExtensionId::new(value.trim_start_matches("extension:")).map(Self::Extension)
            }
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> String {
        match self {
            Self::Agents => "agents".to_string(),
            Self::Source => "source".to_string(),
            Self::Browser => "browser".to_string(),
            Self::Kanban => "kanban".to_string(),
            Self::Automate => "automate".to_string(),
            Self::Manage => "manage".to_string(),
            Self::Terminal => "terminal".to_string(),
            Self::BotFeed => "bot-feed".to_string(),
            Self::Work => "work".to_string(),
            Self::Extension(id) => format!("extension:{}", id.as_str()),
        }
    }

    /// CDXC:Docs 2026-09-27 DECISION:
    /// User: "change Docs from Docs to Files instead so it covers more". The view is called Files everywhere a user reads it; internal ids (`Manage`, the `docs` setting values, `docsViewTabHidden`) keep their names so saved settings still work.
    pub(crate) fn display_label(self) -> &'static str {
        match self {
            Self::Agents => "Agents",
            Self::Source => "Code",
            Self::Browser => "Browser",
            Self::Kanban => "Kanban",
            Self::Automate => "Automate",
            Self::Manage => "Files",
            Self::Terminal => "Terminal",
            Self::BotFeed => "Automations",
            Self::Work => "Work",
            Self::Extension(id) => id.as_str(),
        }
    }

    /// The glyph the view panel's tab strip and the `+` menu draw beside a view's name. Extension
    /// and custom views share the puzzle glyph here; the view picker draws an installed
    /// extension's own manifest icon instead.
    pub(crate) fn tab_icon(self) -> &'static str {
        match self {
            mode if mode
                .website_provider()
                .is_some_and(|provider| provider.automatic()) =>
            {
                "titlebar/brand-github.svg"
            }
            mode if mode.website_provider().is_some() => {
                match mode.website_provider().map(|provider| provider.id.as_str()) {
                    Some("linear") => "titlebar/brand-linear.svg",
                    Some("jira") => "titlebar/brand-jira.svg",
                    Some("sentry") => "titlebar/bug.svg",
                    Some("figma") => "titlebar/palette.svg",
                    Some("vercel") => "titlebar/cloud.svg",
                    Some("supabase") => "titlebar/database.svg",
                    Some("github-actions") => "titlebar/player-play.svg",
                    Some("posthog") => "titlebar/chart-bar.svg",
                    _ => TITLEBAR_ICON_WORLD,
                }
            }
            mode if mode.is_storybook() => TITLEBAR_ICON_LAYOUT_BOARD_SPLIT,
            Self::Agents => TITLEBAR_ICON_LAYOUT_COLUMNS,
            Self::Source => TITLEBAR_ICON_CODE,
            Self::Browser => TITLEBAR_ICON_WORLD,
            Self::Kanban => TITLEBAR_ICON_LAYOUT_BOARD_SPLIT,
            Self::Automate => TITLEBAR_ICON_BOLT,
            Self::Manage => TITLEBAR_ICON_FILE_TEXT,
            Self::Terminal => TITLEBAR_ICON_TERMINAL,
            Self::BotFeed => TITLEBAR_ICON_MESSAGES,
            Self::Work => "titlebar/briefcase.svg",
            Self::Extension(_) => TITLEBAR_ICON_EXTENSIONS,
        }
    }

    /// The label a tab, a `+` row or a menu row shows for this view, resolving an extension or custom
    /// view's own title.
    pub(crate) fn tab_label(self) -> String {
        match self {
            Self::Extension(id) => gpui_extension_view_presentation(id)
                .map(|presentation| presentation.title)
                .unwrap_or_else(|| id.as_str().to_string()),
            mode => mode.display_label().to_string(),
        }
    }

    /// A view backed by a CEF page, with the awake/sleep lifecycle that implies. The Terminal view
    /// is GPUI chrome around native terminals: it never sleeps as a view and owns no page.
    pub(crate) fn is_project_editor_mode(self) -> bool {
        matches!(
            self,
            Self::Source
                | Self::Browser
                | Self::Kanban
                | Self::Automate
                | Self::Manage
                | Self::Extension(_)
        )
    }

    pub(crate) fn project_editor_order(self) -> u64 {
        match self {
            Self::Source => 0,
            Self::Browser => 1,
            Self::Kanban => 2,
            Self::Automate => 3,
            Self::Manage => 4,
            Self::Terminal => 5,
            Self::BotFeed => 6,
            Self::Work => 7,
            Self::Extension(_) => 8,
            Self::Agents => 9,
        }
    }

    pub(crate) fn switcher_index(self) -> u64 {
        match self {
            Self::Agents => 0,
            Self::Source => 1,
            Self::Browser => 2,
            Self::Kanban => 3,
            Self::Automate => 4,
            Self::Manage => 5,
            Self::Terminal => 6,
            Self::BotFeed => 7,
            Self::Work => 8,
            Self::Extension(id) => {
                id.as_str()
                    .bytes()
                    .fold(0xcbf29ce484222325_u64, |hash, byte| {
                        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
                    })
                    | (1_u64 << 63)
            }
        }
    }

    pub(crate) fn placeholder_message(self) -> &'static str {
        match self {
            Self::Agents => "",
            // CDXC:Workarea 2026-09-15 DECISION:
            // User: Source must not flash an unavailable-project-context message during startup.
            // This default also covers pending project/runtime restoration; concrete launch states supply their own progress and error messages.
            Self::Source => "",
            Self::Browser => "",
            Self::Kanban => "Kanban is unavailable for the current project context.",
            Self::Automate => "Automate is unavailable for the current project context.",
            Self::Manage => "Files is unavailable for the current project context.",
            Self::Terminal => "",
            Self::BotFeed => "",
            Self::Work => "Work shows the tickets of projects with Work mode on.",
            Self::Extension(_) => "This extension is unavailable for the current project context.",
        }
    }
}

impl TitlebarMode {
    /// Whether `placeholder_message` is an "unavailable for the current project context" notice,
    /// which gives way to the sessions-loading card while the sessions have not loaded.
    pub(crate) fn placeholder_is_unavailable_notice(self) -> bool {
        matches!(
            self,
            Self::Kanban | Self::Automate | Self::Manage | Self::Extension(_)
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct TitlebarModeSwitcherItem {
    pub(crate) mode: TitlebarMode,
    pub(crate) is_available: bool,
    pub(crate) disabled_reason: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiTitlebarExitFocusControlSignature {
    pub(crate) label: &'static str,
    pub(crate) styled_as_active_mode_tab: bool,
    pub(crate) clears_agents_focus_mode: bool,
}

pub(crate) fn gpui_titlebar_exit_focus_control_signature(
    agents_focus_mode_active: bool,
) -> Option<GpuiTitlebarExitFocusControlSignature> {
    /*
    CDXC:FocusRouting 2026-06-27-02:05:
    The titlebar Exit Focus affordance is visible only while the Agents workspace is in pane Focus mode, and it must reuse active mode-tab chrome instead of a separate outlined or icon-button skin. Activating it clears Agents focus mode through the workspace model without changing command-pane focus mode, project-editor focus, terminal content, paths, commands, or renderer state.
    */
    agents_focus_mode_active.then_some(GpuiTitlebarExitFocusControlSignature {
        label: "Exit focus",
        styled_as_active_mode_tab: true,
        clears_agents_focus_mode: true,
    })
}
