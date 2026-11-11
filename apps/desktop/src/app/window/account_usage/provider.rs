//! Which provider a usage popup was opened for, and what that changes: its name, logo and colour,
//! the links in its header, and which sections it has.
//!
//! CDXC:AgentProviders 2026-11-11 DECISION:
//! The popup only knew Claude and Codex, so a Gemini, Cursor or Copilot meter opened "Claude
//! usage" with Claude's logo, empty 5h and 7d rows, an Extra usage row and a "Shared Claude
//! history" section that never loaded. Each provider now names itself. The usage-only providers
//! (server/src/accounts/usage_only/) show the windows gxserver lists, their plan and details, and
//! no history section, because gxserver keeps conversation history only for Claude and Codex.
//! Gemini has no header links: no Google page shows these Antigravity quotas (user, 2026-11-11).
use super::data::text;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UsageProvider {
    Claude,
    Codex,
    Gemini,
    Cursor,
    Copilot,
}

impl UsageProvider {
    pub(super) fn of(account: &Value) -> Self {
        match text(account, "provider") {
            "codex" => Self::Codex,
            "gemini" => Self::Gemini,
            "cursor" => Self::Cursor,
            "copilot" => Self::Copilot,
            _ => Self::Claude,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Gemini => "Gemini",
            Self::Cursor => "Cursor",
            Self::Copilot => "Copilot",
        }
    }

    /// Cursor, Gemini and Copilot only show usage: no resets, Extra usage or shared history.
    pub(super) fn usage_only(self) -> bool {
        matches!(self, Self::Gemini | Self::Cursor | Self::Copilot)
    }

    /// The header's Status and Dashboard pages.
    pub(super) fn links(self) -> Option<[&'static str; 2]> {
        match self {
            Self::Claude => Some([
                "https://status.anthropic.com",
                "https://claude.ai/settings/usage",
            ]),
            Self::Codex => Some([
                "https://status.openai.com",
                "https://chatgpt.com/codex/settings/usage",
            ]),
            Self::Cursor => Some(["https://status.cursor.com", "https://cursor.com/dashboard"]),
            Self::Copilot => Some([
                "https://www.githubstatus.com",
                "https://github.com/settings/copilot",
            ]),
            Self::Gemini => None,
        }
    }

    /// The bars' colour on a dark and a light panel, `None` for a provider whose logo is plain
    /// ink (Cursor, Copilot): their bars take the ink too.
    pub(super) fn accent(self) -> Option<(u32, u32)> {
        match self {
            Self::Claude => Some((0xd97757, 0x9c4328)),
            Self::Codex => Some((0x8cbbe8, 0x285b8c)),
            Self::Gemini => Some((0x8ab4f8, 0x1a5fb4)),
            Self::Cursor | Self::Copilot => None,
        }
    }

    /// The logo in the header, the one the provider's sidebar meter draws.
    pub(super) fn logo(self) -> Arc<gpui::Image> {
        let svg = match self {
            Self::Codex => crate::app::titlebar::account_usage::account_logo_svg(true)
                .replace("#ffffff", "#8cbbe8"),
            Self::Claude => {
                crate::app::titlebar::account_usage::account_logo_svg(false).to_string()
            }
            other => {
                return crate::app::titlebar::account_usage::usage_only_icon(match other {
                    Self::Gemini => "gemini",
                    Self::Cursor => "cursor",
                    _ => "copilot",
                });
            }
        };
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            svg.into_bytes(),
        ))
    }
}

/// A usage-only window's row: "5h limit", "7d limit", "Claude/GPT 5h limit", or "Month limit" for
/// the billing month Cursor and Copilot report (user, 2026-11-11: "Month limit").
pub(super) fn window_limit_label(window: &Value) -> String {
    let span = match window["limitWindowSeconds"].as_f64() {
        Some(seconds) if seconds >= 604_800.0 => "7d".to_string(),
        Some(seconds) if seconds > 0.0 && seconds % 86_400.0 == 0.0 => {
            format!("{}d", (seconds / 86_400.0) as i64)
        }
        Some(seconds) if seconds > 0.0 && seconds % 3_600.0 == 0.0 => {
            format!("{}h", (seconds / 3_600.0) as i64)
        }
        _ if window["id"] == "month" || text(window, "label") == "Mo" => {
            return "Month limit".to_string();
        }
        _ => text(window, "label").to_string(),
    };
    match text(window, "model") {
        "" => format!("{span} limit"),
        model => format!("{model} {span} limit"),
    }
}
