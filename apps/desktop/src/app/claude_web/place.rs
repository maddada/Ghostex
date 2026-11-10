//! The built-in web pages that take the sessions column's place (claude_web/takeover.rs): Claude
//! (claude.ai on the saved Claude accounts' sign-ins, claude_web/accounts.rs) and Grok (grok.com on
//! Ghostex's own Grok logins, claude_web/grok.rs).
//!
//! CDXC:Extensions 2026-11-09 DECISION:
//! User: "let's please add an extension to the app like the claude one ... but this one is to open different grok bot logins". Grok is built the way Claude is, so it reuses Claude's takeover instead of copying it: one of them is on at a time, its sidebar button shows as on, and clicking the other one's button switches to it in place (the layout from before either of them is kept, so turning it off goes back there). Everything else Claude's takeover decided (the hidden panels, the header, opened sessions, showing the sidebar) holds for Grok unchanged.

use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WebPlace {
    Claude,
    Grok,
}

impl WebPlace {
    pub(crate) const ALL: [Self; 2] = [Self::Claude, Self::Grok];

    pub(crate) fn view_id_str(self) -> &'static str {
        match self {
            Self::Claude => CLAUDE_WEB_VIEW_ID,
            Self::Grok => GROK_WEB_VIEW_ID,
        }
    }

    pub(crate) fn view_id(self) -> ExtensionId {
        ExtensionId::new(self.view_id_str()).expect("built-in view ID")
    }

    pub(crate) fn mode(self) -> TitlebarMode {
        TitlebarMode::Extension(self.view_id())
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Grok => "Grok Bot",
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Claude => "agent-icons/claude.svg",
            Self::Grok => "agent-icons/grok-build.svg",
        }
    }

    /// Whether its built-in extension is on (Settings > Extensions).
    pub(crate) fn enabled(self) -> bool {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        match self {
            Self::Claude => settings.claude_web_enabled(),
            Self::Grok => settings.grok_web_enabled(),
        }
    }

    pub(crate) fn from_view_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|place| place.view_id_str() == id)
    }

    pub(crate) fn from_mode(mode: TitlebarMode) -> Option<Self> {
        match mode {
            TitlebarMode::Extension(id) => Self::from_view_id(id.as_str()),
            _ => None,
        }
    }

    /// The name the window's shell state saves it under.
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Grok => "grok",
        }
    }

    pub(crate) fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|place| place.slug() == slug)
    }
}
