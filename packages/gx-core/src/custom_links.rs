//! The user's own links in the rail's ⋯ menu: what the settings file keeps, how a typed address
//! becomes a page address, and the menu rows the rail's ⋯ menu draws for them.
//!
//! CDXC:Sidebar 2026-11-22 DECISION:
//! User: "add the ability inside the 3 dots in the sidebar for the user to add any link they want. when they add it and click on that link then we show the browser expanded with that link open (almost exactly like we do for grok bot embed extension, only difference is we show the address bar like we do in the browser in the side panel and we don't hide it). useful for someone to add for example a link to their CLIProxyAPI and view it easily from inside ghostex ... not as part of any project since it's more meta". Follow-up same day: "Don't show links here, only in the 3 dots in the rail." The links are the computer's own, not a project's: `customLinks` in the settings file, each `{ "id", "name", "url" }`, in no Settings > Sync category, so a `localhost:8317` link stays on the computer it points at. The rail's ⋯ menu lists them under its own Links heading (favicon or globe, the name or else the address's host) with Add link… under them; the sidebar's ☰ main menu does not show them. A row opens its page in the sessions column's place the way Grok Bot's opens (apps/desktop/src/app/claude_web/takeover.rs), with the Browser toolbar always shown above it; its hover ⋯ and right-click open Edit…, Pin to rail / Unpin from rail, and Remove, the way the Unified filter's rows open their menus (`rowMenu`).
//! SEE-ALSO: apps/desktop/src/app/custom_links/ (opening, the page, its bar), apps/desktop/src/app/window/custom_link_modal.rs (the Add link and Edit link dialog), apps/desktop/src/app/sidebar_rail/menu.rs.

use serde_json::{Map, Value, json};

use crate::sidebar_menu::{MenuCommand, MenuItem};

/// The settings key the links live under.
pub const CUSTOM_LINKS_SETTING: &str = "customLinks";

/// The host command type every link row and its row menu carry: `{ type, op, id? }` with `op` one
/// of `open`, `add`, `edit`, `remove`.
pub const CUSTOM_LINK_COMMAND_TYPE: &str = "customLink";

/// The `rowMenu` kind of a link row (its hover ⋯ and right-click).
pub const CUSTOM_LINK_ROW_MENU_KIND: &str = "customLink";

/// The most links the menu keeps; more are dropped when the setting is read.
pub const CUSTOM_LINKS_MAX: usize = 50;

/// One saved link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomLink {
    pub id: String,
    /// What the user typed as its name; empty for none.
    pub name: String,
    /// A full http or https address (`normalize_custom_link_url`).
    pub url: String,
    /// Optional app icon id (from the app's command/Space icon set). `None` uses the site's
    /// favicon (or the globe until cached).
    pub icon: Option<String>,
}

impl CustomLink {
    /// The row's label: its name, else its address's host (and port).
    pub fn label(&self) -> String {
        let name = self.name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
        url_authority(&self.url)
            .map(str::to_string)
            .unwrap_or_else(|| self.url.clone())
    }
}

/// The links a settings object holds: unique ids, valid addresses, at most `CUSTOM_LINKS_MAX`.
pub fn custom_links_from_settings(settings: &Map<String, Value>) -> Vec<CustomLink> {
    custom_links_from_value(settings.get(CUSTOM_LINKS_SETTING))
}

/// The links a `customLinks` value holds.
pub fn custom_links_from_value(value: Option<&Value>) -> Vec<CustomLink> {
    let mut links: Vec<CustomLink> = Vec::new();
    for entry in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = entry["id"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let Some(url) = entry["url"].as_str().and_then(normalize_custom_link_url) else {
            continue;
        };
        if links.iter().any(|link| link.id == id) {
            continue;
        }
        let icon = entry["icon"]
            .as_str()
            .map(str::trim)
            .filter(|icon| !icon.is_empty())
            .map(str::to_string);
        links.push(CustomLink {
            id: id.to_string(),
            name: entry["name"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string(),
            url,
            icon,
        });
        if links.len() == CUSTOM_LINKS_MAX {
            break;
        }
    }
    links
}

/// `links` as `customLinks` stores them.
pub fn custom_links_value(links: &[CustomLink]) -> Value {
    Value::Array(
        links
            .iter()
            .map(|link| {
                let mut entry = json!({ "id": link.id, "name": link.name, "url": link.url });
                if let Some(icon) = link
                    .icon
                    .as_deref()
                    .map(str::trim)
                    .filter(|i| !i.is_empty())
                {
                    entry["icon"] = json!(icon);
                }
                entry
            })
            .collect(),
    )
}

/// What the user typed as an address, as a page address: `http://` or `https://` kept, and one
/// without a scheme given `http://` when it names this computer, a local network name, an IP
/// address or a port (`localhost:8317`, `192.168.1.4`, `nas.local`, `box:3000`), else `https://`
/// (`example.com/admin`). `None` for anything that is not a web address.
pub fn normalize_custom_link_url(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (scheme, rest) = match text.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.to_ascii_lowercase();
            if scheme != "http" && scheme != "https" {
                return None;
            }
            (Some(scheme), rest)
        }
        None => (None, text),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host_of_authority(authority)?;
    let scheme = scheme.unwrap_or_else(|| {
        if looks_local(host) || authority.len() > host.len() {
            "http".to_string()
        } else {
            "https".to_string()
        }
    });
    let path = &rest[authority.len()..];
    let path = if path.is_empty() { "/" } else { path };
    Some(format!("{scheme}://{authority}{path}"))
}

/// The host of `host[:port]` (an IPv6 address in brackets kept whole), `None` when either part is
/// not one.
fn host_of_authority(authority: &str) -> Option<&str> {
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']')?;
        let host = &authority[..end + 2];
        let after = &rest[end + 1..];
        (host, after.strip_prefix(':'))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    if let Some(port) = port {
        if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if port.parse::<u32>().ok()? > 65_535 {
            return None;
        }
    }
    let bracketed = host.starts_with('[');
    let valid = !host.is_empty()
        && (bracketed
            || host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_')))
        && !host.starts_with('.')
        && !host.ends_with("..");
    valid.then_some(host)
}

/// A host that is this computer or the local network, which seldom serves https.
fn looks_local(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".lan")
        || host.ends_with(".internal")
        || host.starts_with('[')
        || !host.contains('.')
        || host.split('.').all(|part| part.parse::<u8>().is_ok())
}

/// `scheme://host[:port]/…`'s `host[:port]`.
fn url_authority(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    rest.split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
}

fn command(op: &str, id: Option<&str>) -> MenuCommand {
    let mut command = json!({ "type": CUSTOM_LINK_COMMAND_TYPE, "op": op });
    if let Some(id) = id {
        command["id"] = json!(id);
    }
    MenuCommand::host(command)
}

/// The ⋯ menu's rows for `links`: one per link (its chosen app icon if set, else its favicon as a
/// `data:` image where `favicon` has one, else the globe), then Add link….
pub fn custom_link_menu_rows(
    links: &[CustomLink],
    favicon: impl Fn(&CustomLink) -> Option<String>,
) -> Vec<Value> {
    let mut rows: Vec<Value> = links
        .iter()
        .map(|link| {
            let label = link.label();
            let chosen_icon = link.icon.as_deref().filter(|icon| !icon.is_empty());
            let mut row = MenuItem::row(
                &label,
                chosen_icon.unwrap_or("world"),
                command("open", Some(&link.id)),
            )
            .to_json();
            if chosen_icon.is_none() {
                if let Some(image) = favicon(link) {
                    row["imageDataUrl"] = json!(image);
                    if let Some(row) = row.as_object_mut() {
                        row.remove("icon");
                    }
                }
            }
            row["rowMenu"] = json!({
                "kind": CUSTOM_LINK_ROW_MENU_KIND,
                "key": link.id,
                "label": label,
            });
            row
        })
        .collect();
    rows.push(MenuItem::row("Add link…", "plus", command("add", None)).to_json());
    rows
}

/// What a link row's hover ⋯ and right-click open: Edit…, Change icon…, Pin to rail or Unpin from
/// rail (in the rail layout's ⋯ menu, whose host sets `pinned`), and Remove.
pub fn custom_link_row_menu(row_menu: &Value) -> Option<Value> {
    if row_menu["kind"] != CUSTOM_LINK_ROW_MENU_KIND {
        return None;
    }
    let id = row_menu["key"].as_str()?;
    let mut rows = vec![
        MenuItem::row("Edit…", "pencil", command("edit", Some(id))).to_json(),
        MenuItem::row("Change icon…", "sparkles", command("changeIcon", Some(id))).to_json(),
    ];
    if let Some(pinned) = row_menu["pinned"].as_bool() {
        let key = crate::sidebar_rail::rail_link_pin_key(id);
        if pinned {
            rows.push(
                MenuItem::row(
                    "Unpin from rail",
                    "pin-outline",
                    MenuCommand::host(json!({
                        "type": crate::sidebar_rail::RAIL_COMMAND_TYPE,
                        "op": "pin",
                        "key": key,
                        "pinned": false,
                    })),
                )
                .to_json(),
            );
        } else {
            rows.push(
                MenuItem::row(
                    "Pin to rail",
                    "pin-filled",
                    MenuCommand::host(json!({
                        "type": crate::sidebar_rail::RAIL_COMMAND_TYPE,
                        "op": "pin",
                        "key": key,
                        "pinned": true,
                        "section": "top",
                    })),
                )
                .to_json(),
            );
            rows.push(
                MenuItem::row(
                    "Pin to bottom",
                    "pin-filled",
                    MenuCommand::host(json!({
                        "type": crate::sidebar_rail::RAIL_COMMAND_TYPE,
                        "op": "pin",
                        "key": key,
                        "pinned": true,
                        "section": "bottom",
                    })),
                )
                .to_json(),
            );
        }
    }
    rows.push(
        MenuItem::row("Remove", "trash", command("remove", Some(id)))
            .with_danger()
            .to_json(),
    );
    Some(Value::Array(rows))
}
