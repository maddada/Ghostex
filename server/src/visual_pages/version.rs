//! Which page layout a published page was written for. A page written from today's
//! ghostex-visuals skeleton carries `<meta name="ghostex-page" content="v2">`; a page without it was
//! written for the first layout (an older copy of the skill, or none).
//!
//! CDXC:SessionChat 2026-10-27 DECISION:
//! User, on pages made before the v2 template: "i dont care about old pages … i dont want code to fix old ones just need future created ones to be good", but tell the reader when an opened page is missing "something that indicates whether if this inline html is v2", and "mention it again to his sessions". So nothing repairs a v1 page: serve.rs hands the page `oldLayout` and the page API (theme.rs) shows a one-line notice above it until the reader dismisses it (notice.rs), and `ghostex show` tells the publishing agent when the file it publishes lacks the marker (ghostex_cli/show.rs).
//! SEE-ALSO: skills/ghostex-visuals/SKILL.md (the skeleton that writes the marker), kit.rs (the theme switch only v2 pages get).

/// The marker's `name`.
pub(crate) const PAGE_VERSION_META_NAME: &str = "ghostex-page";
/// The marker's `content` for today's layout.
pub(crate) const PAGE_VERSION_CURRENT: &str = "v2";
/// The tag the skill's skeleton writes, for messages.
pub(crate) const PAGE_VERSION_META_TAG: &str = "<meta name=\"ghostex-page\" content=\"v2\">";

/// Whether the page carries `<meta name="ghostex-page" content="v2">` (any letter case, any
/// attribute order, single, double or no quotes).
pub(crate) fn is_current_layout(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("<meta") {
        let start = from + at + "<meta".len();
        from = start;
        if !lower[start..].starts_with(|c: char| c.is_ascii_whitespace() || c == '/') {
            continue;
        }
        let end = lower[start..]
            .find('>')
            .map_or(lower.len(), |end| start + end);
        let tag = &lower[start..end];
        if attribute(tag, "name").as_deref() == Some(PAGE_VERSION_META_NAME)
            && attribute(tag, "content").as_deref() == Some(PAGE_VERSION_CURRENT)
        {
            return true;
        }
    }
    false
}

/// An attribute's trimmed value in a start tag's text (already lowercased).
fn attribute(tag: &str, name: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut at = 0;
    while let Some(found) = tag[at..].find(name) {
        let start = at + found;
        at = start + name.len();
        if start > 0 && !(bytes[start - 1].is_ascii_whitespace() || bytes[start - 1] == b'/') {
            continue;
        }
        let rest = tag[at..].trim_start();
        let Some(value) = rest.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        return Some(match value.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let inner = &value[1..];
                inner[..inner.find(quote).unwrap_or(inner.len())]
                    .trim()
                    .to_string()
            }
            _ => value
                .split(|c: char| c.is_ascii_whitespace() || c == '/')
                .next()
                .unwrap_or_default()
                .to_string(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skeleton_marker_is_current() {
        assert!(is_current_layout(
            "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"ghostex-page\" content=\"v2\"></head></html>"
        ));
        assert!(is_current_layout(PAGE_VERSION_META_TAG));
    }

    #[test]
    fn letter_case_quotes_and_attribute_order_do_not_matter() {
        assert!(is_current_layout("<META Content='V2' NAME='Ghostex-Page'>"));
        assert!(is_current_layout("<meta name=ghostex-page content=v2>"));
        assert!(is_current_layout(
            "<meta\n  name = \"ghostex-page\"\n  content = \" v2 \" />"
        ));
    }

    #[test]
    fn a_page_without_the_marker_is_old() {
        assert!(!is_current_layout(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>x</title></head><body><gx-questions></gx-questions></body></html>"
        ));
        assert!(!is_current_layout(""));
    }

    #[test]
    fn another_version_or_another_meta_is_not_current() {
        assert!(!is_current_layout(
            "<meta name=\"ghostex-page\" content=\"v1\">"
        ));
        assert!(!is_current_layout(
            "<meta name=\"ghostex-page-x\" content=\"v2\">"
        ));
        assert!(!is_current_layout(
            "<meta name=\"description\" content=\"v2\">"
        ));
        assert!(!is_current_layout(
            "<metadata name=\"ghostex-page\" content=\"v2\">"
        ));
        // `data-name` is not `name`.
        assert!(!is_current_layout(
            "<meta data-name=\"ghostex-page\" content=\"v2\">"
        ));
    }

    #[test]
    fn escaped_markup_in_the_text_is_not_the_marker() {
        assert!(!is_current_layout(
            "<pre>&lt;meta name=\"ghostex-page\" content=\"v2\"&gt;</pre>"
        ));
    }
}
