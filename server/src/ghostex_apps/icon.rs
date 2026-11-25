//! The icon an agent draws for its app: a square SVG or PNG file beside the app's HTML, named by
//! `<link rel="icon">`. Checked by `ghostex app show` before it publishes and again by gxserver
//! before it stores it.
//!
//! CDXC:GhostexApps 2026-11-25 DECISION:
//! User picked "the agent draws it" for an installed app's icon (Q4975), not an icon Ghostex makes up and not one the user picks. So an app must carry its own icon: `ghostex app show` refuses an app without a usable one and says how to add it, instead of falling back to a made-up icon. The checks keep it something the Dock, the taskbar and Settings can show: square, small, and an SVG with no scripts, embedded objects or outside references.

/// The two kinds of icon an app may have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconKind {
    Svg,
    Png,
}

impl IconKind {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            IconKind::Svg => "svg",
            IconKind::Png => "png",
        }
    }

    pub(crate) fn mime(self) -> &'static str {
        match self {
            IconKind::Svg => "image/svg+xml",
            IconKind::Png => "image/png",
        }
    }

    pub(crate) fn from_extension(extension: &str) -> Option<IconKind> {
        match extension.to_ascii_lowercase().as_str() {
            "svg" => Some(IconKind::Svg),
            "png" => Some(IconKind::Png),
            _ => None,
        }
    }
}

pub(crate) const SVG_ICON_MAX_BYTES: usize = 64 * 1024;
pub(crate) const PNG_ICON_MAX_BYTES: usize = 512 * 1024;
/// A PNG icon must be at least this wide (and as tall), so the Dock's large sizes stay sharp.
pub(crate) const PNG_ICON_MIN_SIDE: u32 = 256;

/// What an agent is told when its app has no usable icon.
pub(crate) const ICON_HOW_TO: &str = "Draw the app's icon as a square SVG file (a viewBox like \"0 0 512 512\") beside the HTML and name it in the head: <link rel=\"icon\" href=\"icon.svg\">. A square PNG of at least 256 px works too. See `ghostex app guide icon`.";

/// Checks an icon's bytes, and gives the reason when it cannot be used.
pub(crate) fn check_icon(kind: IconKind, bytes: &[u8]) -> Result<(), String> {
    match kind {
        IconKind::Svg => check_svg(bytes),
        IconKind::Png => check_png(bytes),
    }
}

fn check_svg(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > SVG_ICON_MAX_BYTES {
        return Err(format!(
            "The icon is {} KiB; an SVG icon may be at most {} KiB.",
            bytes.len() / 1024,
            SVG_ICON_MAX_BYTES / 1024
        ));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| "The SVG icon is not UTF-8 text.".to_string())?;
    let lower = text.to_ascii_lowercase();
    let Some(open) = lower.find("<svg") else {
        return Err("The icon file has no <svg> element.".to_string());
    };
    for (needle, what) in [
        ("<script", "a script"),
        ("<foreignobject", "a foreignObject"),
        ("<iframe", "an iframe"),
        ("javascript:", "a javascript: link"),
        ("<image", "an embedded image"),
        ("<use", "a <use> reference"),
        ("@import", "an @import"),
        ("<!entity", "an entity declaration"),
    ] {
        if lower.contains(needle) {
            return Err(format!(
                "The SVG icon holds {what}; draw it with shapes, paths, gradients and text only."
            ));
        }
    }
    if has_event_attribute(&lower) {
        return Err(
            "The SVG icon has an event attribute (onload= and the like); remove it.".to_string(),
        );
    }
    if lower.contains("href=\"http") || lower.contains("href='http") || lower.contains("url(http") {
        return Err(
            "The SVG icon refers to an outside address; keep everything inside the file."
                .to_string(),
        );
    }
    let head = &lower[open..lower[open..]
        .find('>')
        .map_or(lower.len(), |end| open + end)];
    let square = view_box(head)
        .map(|(width, height)| (width - height).abs() < 0.01 && width > 0.0)
        .or_else(|| {
            let width = number_attribute(head, "width")?;
            let height = number_attribute(head, "height")?;
            Some((width - height).abs() < 0.01 && width > 0.0)
        });
    match square {
        Some(true) => Ok(()),
        Some(false) => Err(
            "The SVG icon is not square; give it a square viewBox such as \"0 0 512 512\"."
                .to_string(),
        ),
        None => Err(
            "The SVG icon has no viewBox; give it a square one such as \"0 0 512 512\"."
                .to_string(),
        ),
    }
}

/// Whether an attribute such as `onload=` or `onclick =` appears in the lowercased SVG text.
fn has_event_attribute(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    lower.match_indices("on").any(|(at, _)| {
        let before = at.checked_sub(1).map(|index| bytes[index]);
        if !before.is_some_and(|byte| byte.is_ascii_whitespace() || byte == b'"' || byte == b'\'') {
            return false;
        }
        let rest = &lower[at + 2..];
        let name_len = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
        name_len > 0 && rest[name_len..].trim_start().starts_with('=')
    })
}

fn view_box(head: &str) -> Option<(f64, f64)> {
    let at = head.find("viewbox")?;
    let rest = head[at + "viewbox".len()..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let body = &rest[1..];
    let value = &body[..body.find(quote)?];
    let numbers: Vec<f64> = value
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    (numbers.len() == 4).then(|| (numbers[2], numbers[3]))
}

fn number_attribute(head: &str, name: &str) -> Option<f64> {
    let needle = format!(" {name}=");
    let at = head.find(&needle)?;
    let rest = &head[at + needle.len()..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let body = &rest[1..];
    let value = body[..body.find(quote)?].trim_end_matches("px");
    value.trim().parse().ok()
}

fn check_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > PNG_ICON_MAX_BYTES {
        return Err(format!(
            "The icon is {} KiB; a PNG icon may be at most {} KiB.",
            bytes.len() / 1024,
            PNG_ICON_MAX_BYTES / 1024
        ));
    }
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return Err("The icon file is not a PNG image.".to_string());
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width != height {
        return Err(format!(
            "The PNG icon is {width}x{height}; it must be square."
        ));
    }
    if width < PNG_ICON_MIN_SIDE {
        return Err(format!(
            "The PNG icon is {width}x{height}; it must be at least {PNG_ICON_MIN_SIDE}x{PNG_ICON_MIN_SIDE}."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_a_plain_square_svg() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" rx="110" fill="#f4b24c"/></svg>"##;
        assert_eq!(check_icon(IconKind::Svg, svg), Ok(()));
    }

    #[test]
    fn refuses_scripts_events_outside_links_and_odd_shapes() {
        for svg in [
            r#"<svg viewBox="0 0 10 10"><script>alert(1)</script></svg>"#,
            r#"<svg viewBox="0 0 10 10" onload="x()"></svg>"#,
            r#"<svg viewBox="0 0 10 10"><a href="https://x.test">x</a></svg>"#,
            r#"<svg viewBox="0 0 20 10"></svg>"#,
            r#"<svg></svg>"#,
        ] {
            assert!(check_icon(IconKind::Svg, svg.as_bytes()).is_err(), "{svg}");
        }
    }

    #[test]
    fn checks_png_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&512u32.to_be_bytes());
        png.extend_from_slice(&512u32.to_be_bytes());
        assert_eq!(check_icon(IconKind::Png, &png), Ok(()));
        png[20..24].copy_from_slice(&256u32.to_be_bytes());
        assert!(check_icon(IconKind::Png, &png).is_err());
    }
}
