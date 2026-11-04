//! The apps a tool-call card knows by their MCP server: which server is which app, the app's
//! name for the row's wording ("Reading SPX-412 in Linear"), its brand colour, and its logo.
//!
//! Every logo is from Simple Icons 16.36.0 (<https://simpleicons.org>,
//! <https://github.com/simple-icons/simple-icons>), CC0-1.0, copied unchanged into
//! `packages/gx-chat-core/visual/tool-apps/<key>.svg` except for its `<title>` and `role`. Only the
//! icons listed here are bundled. An MCP server not listed keeps the generic tool icon.
//!
//! CDXC:SessionChat 2026-10-28 SEE-ALSO: the desktop serves these SVGs as
//! `chat-tool-apps/<key>.svg` (`apps/desktop/src/assets.rs`) for `native_chat/tool_card.rs`.

/// One app a tool-call card draws with its own logo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolApp {
    /// The logo's file name, and the key the renderers ask for it by.
    pub key: &'static str,
    /// How the row's wording names it ("in Linear").
    pub name: &'static str,
    /// The brand colour Simple Icons records, `#rrggbb`.
    pub color: &'static str,
    /// The logo, a one-colour 24x24 SVG.
    pub svg: &'static str,
}

macro_rules! app {
    ($key:literal, $name:literal, $color:literal) => {
        ToolApp {
            key: $key,
            name: $name,
            color: $color,
            svg: include_str!(concat!("../../visual/tool-apps/", $key, ".svg")),
        }
    };
}

/// Each app with the words its MCP server's name may contain, most specific first (Jira and
/// Confluence before Atlassian). No bare "drive" or "calendar": `cua-driver` is no Google Drive.
const APPS: &[(&[&str], ToolApp)] = &[
    (&["linear"], app!("linear", "Linear", "#5E6AD2")),
    (&["github"], app!("github", "GitHub", "#181717")),
    (&["gitlab"], app!("gitlab", "GitLab", "#FC6D26")),
    (&["notion"], app!("notion", "Notion", "#000000")),
    (&["slack"], app!("slack", "Slack", "#4A154B")),
    (&["figma"], app!("figma", "Figma", "#F24E1E")),
    (&["sentry"], app!("sentry", "Sentry", "#362D59")),
    (&["vercel"], app!("vercel", "Vercel", "#000000")),
    (&["supabase"], app!("supabase", "Supabase", "#3FCF8E")),
    (&["stripe"], app!("stripe", "Stripe", "#635BFF")),
    (&["jira"], app!("jira", "Jira", "#0052CC")),
    (&["confluence"], app!("confluence", "Confluence", "#172B4D")),
    (&["atlassian"], app!("atlassian", "Atlassian", "#0052CC")),
    (
        &["googledrive", "gdrive"],
        app!("googledrive", "Google Drive", "#4285F4"),
    ),
    (&["gmail"], app!("gmail", "Gmail", "#EA4335")),
    (
        &["googlecalendar", "gcal"],
        app!("googlecalendar", "Google Calendar", "#4285F4"),
    ),
    (&["asana"], app!("asana", "Asana", "#F06A6A")),
    (&["hubspot"], app!("hubspot", "HubSpot", "#FF7A59")),
    (&["cloudflare"], app!("cloudflare", "Cloudflare", "#F38020")),
    (&["netlify"], app!("netlify", "Netlify", "#00C7B7")),
    (&["discord"], app!("discord", "Discord", "#5865F2")),
    (&["airtable"], app!("airtable", "Airtable", "#18BFFF")),
    (
        &["postgres", "postgresql"],
        app!("postgresql", "PostgreSQL", "#4169E1"),
    ),
];

/// The app an MCP server's name stands for: `linear`, `claude_ai_Linear`, `github-mcp` and
/// `Google-Drive` all count, case and punctuation aside.
pub fn tool_app_for_server(server: &str) -> Option<&'static ToolApp> {
    let normalized: String = server
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect();
    APPS.iter()
        .find(|(needles, _)| needles.iter().any(|needle| normalized.contains(needle)))
        .map(|(_, app)| app)
}

/// The logo for an app key, for the renderers' asset loaders.
pub fn tool_app_svg(key: &str) -> Option<&'static str> {
    APPS.iter()
        .find(|(_, app)| app.key == key)
        .map(|(_, app)| app.svg)
}

/// Every app key, for an asset loader's listing.
pub fn tool_app_keys() -> impl Iterator<Item = &'static str> {
    APPS.iter().map(|(_, app)| app.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_names_find_their_app() {
        assert_eq!(tool_app_for_server("linear").unwrap().name, "Linear");
        assert_eq!(
            tool_app_for_server("claude_ai_Linear").unwrap().key,
            "linear"
        );
        assert_eq!(tool_app_for_server("github-mcp").unwrap().key, "github");
        assert_eq!(
            tool_app_for_server("Google-Drive").unwrap().key,
            "googledrive"
        );
        assert_eq!(
            tool_app_for_server("google_calendar").unwrap().key,
            "googlecalendar"
        );
        assert_eq!(tool_app_for_server("atlassian").unwrap().key, "atlassian");
        assert!(tool_app_for_server("spaceo").is_none());
        assert!(tool_app_for_server("cua-driver").is_none());
        assert!(tool_app_for_server("ghostex-connectors").is_none());
    }

    #[test]
    fn every_logo_is_bundled() {
        for key in tool_app_keys() {
            let svg = tool_app_svg(key).unwrap();
            assert!(svg.starts_with("<svg"), "{key}");
            assert!(svg.contains("<path"), "{key}");
        }
    }
}
