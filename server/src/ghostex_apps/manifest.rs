//! What an app's HTML declares about itself: the powers it may ask for
//! (`<meta name="ghostex-app-permissions" content="send notifications">`) and its icon
//! (`<link rel="icon" href="icon.svg">`). gxserver reads the powers from the stored HTML itself, so
//! what the install dialog and the bridge go by is what the app's file says, not what a caller
//! claims.

/// A power an app may ask for. Each one is off until the user allows it for that app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Power {
    /// Send a message to the chat that made the app (`ghostex.app.send`).
    Send,
    /// System notifications (`ghostex.app.notify`).
    Notifications,
    /// Keep its window above other windows (`ghostex.app.window.setAlwaysOnTop`).
    Ontop,
    /// A menu bar or tray item (`ghostex.app.menubar`).
    Menubar,
    /// A system-wide keyboard shortcut (`ghostex.app.shortcut`).
    Shortcut,
    /// The user's connectors, under the workspace's own Ask / Allow / Never rules.
    Connectors,
}

pub(crate) const ALL_POWERS: [Power; 6] = [
    Power::Send,
    Power::Notifications,
    Power::Ontop,
    Power::Menubar,
    Power::Shortcut,
    Power::Connectors,
];

impl Power {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Power::Send => "send",
            Power::Notifications => "notifications",
            Power::Ontop => "ontop",
            Power::Menubar => "menubar",
            Power::Shortcut => "shortcut",
            Power::Connectors => "connectors",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Power> {
        ALL_POWERS
            .into_iter()
            .find(|power| power.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// The meta tag that lists an app's powers.
pub(crate) const PERMISSIONS_META_NAME: &str = "ghostex-app-permissions";

/// The powers an app's HTML declares, in the order written, without repeats. A name Ghostex does
/// not know is returned in the error, so `ghostex app show` can refuse it with the list of known
/// ones instead of quietly dropping it.
pub(crate) fn declared_powers(html: &str) -> Result<Vec<Power>, String> {
    let mut powers = Vec::new();
    for head in start_tags(html, "meta") {
        if attribute(head, "name").as_deref() != Some(PERMISSIONS_META_NAME) {
            continue;
        }
        let content = attribute(head, "content").unwrap_or_default();
        for word in content.split(|c: char| c.is_whitespace() || c == ',') {
            if word.is_empty() {
                continue;
            }
            let Some(power) = Power::from_name(word) else {
                return Err(format!(
                    "Unknown permission \"{word}\" in <meta name=\"{PERMISSIONS_META_NAME}\">. Use any of: {}.",
                    ALL_POWERS.map(Power::name).join(", ")
                ));
            };
            if !powers.contains(&power) {
                powers.push(power);
            }
        }
    }
    Ok(powers)
}

/// The `href` of the page's `<link rel="icon">`, when it has one.
pub(crate) fn icon_href(html: &str) -> Option<String> {
    start_tags(html, "link").into_iter().find_map(|head| {
        let rel = attribute(head, "rel")?;
        rel.split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("icon"))
            .then(|| attribute(head, "href"))
            .flatten()
            .filter(|href| !href.is_empty())
    })
}

/// The text of every `<name ...>` start tag (without `<name` and `>`) in `html`. Tags inside
/// scripts and styles are not looked for separately: an app that writes a matching tag into a
/// script string only declares what it wrote.
fn start_tags<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let opening = format!("<{name}");
    let lower = html.to_ascii_lowercase();
    let mut tags = Vec::new();
    let mut from = 0;
    while let Some(found) = lower[from..].find(&opening) {
        let start = from + found + opening.len();
        from = start;
        if !html[start..].starts_with(|c: char| c.is_whitespace() || c == '/' || c == '>') {
            continue;
        }
        let end = html[start..]
            .find('>')
            .map_or(html.len(), |end| start + end);
        tags.push(&html[start..end]);
        from = end;
    }
    tags
}

/// An attribute's value in a start tag's text (`name="x"`, `name='x'` or `name=x`), names
/// compared without case.
fn attribute(head: &str, name: &str) -> Option<String> {
    let lower = head.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let before_ok = at == 0
            || head[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let rest = head[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let value = rest[1..].trim_start();
        return match value.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let body = &value[1..];
                Some(body[..body.find(quote)?].trim().to_string())
            }
            Some(_) => Some(
                value
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            ),
            None => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_declared_powers_and_icon() {
        let html = r#"<head><meta charset="utf-8"><META Name="ghostex-app-permissions" content="send, notifications send"><link rel="icon" href="icon.svg"></head>"#;
        assert_eq!(
            declared_powers(html).unwrap(),
            vec![Power::Send, Power::Notifications]
        );
        assert_eq!(icon_href(html).as_deref(), Some("icon.svg"));
    }

    #[test]
    fn refuses_unknown_powers_and_reads_none_without_the_tag() {
        assert!(
            declared_powers(r#"<meta name="ghostex-app-permissions" content="files">"#)
                .unwrap_err()
                .contains("files")
        );
        assert!(
            declared_powers("<meta name=\"description\" content=\"send\">")
                .unwrap()
                .is_empty()
        );
        assert_eq!(icon_href("<link rel=\"stylesheet\" href=\"a.css\">"), None);
    }
}
