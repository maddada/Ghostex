//! The Claude view: a built-in view with a fixed tab identity that loads claude.ai. It needs no
//! project, so the same page opens from every project.
//!
//! CDXC:Extensions 2026-10-25 DECISION:
//! User: claude.ai in Ghostex "has to be built in extension", not an external Ghostex extension. The view is listed beside Storybook and the project websites as a built-in view (helpers/titlebar/mode_switcher.rs) and exists only while the Claude switch on Settings > Extensions is on (`claudeWebHidden`, packages/settings-catalog/src/data/official_extensions.rs).

use crate::*;
use serde_json::json;

const CLAUDE_WEB_URL: &str = "https://claude.ai/";

/// Whether the Claude built-in extension is on.
pub(crate) fn claude_web_enabled() -> bool {
    shared_settings::shared_sidebar_settings_snapshot().claude_web_enabled()
}

pub(crate) fn claude_web_view_id() -> ExtensionId {
    ExtensionId::new(CLAUDE_WEB_VIEW_ID).expect("built-in view ID")
}

/// The view as the built-in view list carries it. A URL view without a `source`, so every project
/// can show it.
pub(crate) fn claude_web_view() -> GpuiCustomView {
    GpuiCustomView {
        id: claude_web_view_id(),
        title: "Claude".into(),
        enabled: claude_web_enabled(),
        url: CLAUDE_WEB_URL.into(),
        definition: json!({"id": CLAUDE_WEB_VIEW_ID, "name": "Claude", "availability": "all"}),
    }
}
