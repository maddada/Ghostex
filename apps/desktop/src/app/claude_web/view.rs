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

/// What the Grok page opens on when its start page setting is empty or not a web address.
pub(crate) const GROK_WEB_FALLBACK_URL: &str = "https://grok.com/";

/// The Grok start page (`grokWebStartPage`, Settings > Files & Links > Browser): any http or https
/// address, else grok.com.
///
/// CDXC:Extensions 2026-11-09 DECISION:
/// User: "that url is mine but it should open grok.com usually please". The page opens grok.com unless the user puts a page of their own here (one of their Grok bots, say); a typo or a cleared box falls back to grok.com rather than a blank or broken page.
pub(crate) fn grok_web_start_page(saved: Option<&str>) -> String {
    saved
        .map(str::trim)
        .and_then(|text| url::Url::parse(text).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .map(String::from)
        .unwrap_or_else(|| GROK_WEB_FALLBACK_URL.to_string())
}

/// The Grok view as the built-in view list carries it, opening on the start page.
pub(crate) fn grok_web_view() -> GpuiCustomView {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    let start = grok_web_start_page(
        settings
            .object()
            .get("grokWebStartPage")
            .and_then(serde_json::Value::as_str),
    );
    GpuiCustomView {
        id: ExtensionId::new(GROK_WEB_VIEW_ID).expect("built-in view ID"),
        title: "Grok".into(),
        enabled: settings.grok_web_enabled(),
        url: start,
        definition: json!({"id": GROK_WEB_VIEW_ID, "name": "Grok", "availability": "all"}),
    }
}
