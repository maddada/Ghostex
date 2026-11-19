//! The Markdown an agent writes inside a page's `<gx-pick>` rows, rendered to HTML when the page is
//! prepared (`ghostex show`, `ghostex visual check`, an export), so the kit, the stored page and an
//! exported copy only ever hold HTML.
//!
//! CDXC:SessionChat 2026-11-20 DECISION:
//! User, of pickable rows that showed a line or two under each title: "we need to allow writing any markdown as part of those choices". A `<gx-pick>` holds GitHub-flavoured Markdown (paragraphs, lists, code, tables, links, images, quotes) with raw HTML allowed for a mockup, rendered with the `markdown` crate the Files view uses (file_links/pages.rs). The kit's `bindRows` (kit.rs) draws the row around it.

use std::borrow::Cow;

/// Marks a `<gx-pick>` whose content is already HTML, so preparing a prepared page changes nothing.
const RENDERED_MARK: &str = " data-gx-md";

/// Elements whose content is text, not markup: a `<gx-pick>` written there is an example, not a row.
const RAW_TEXT: &[&str] = &["script", "style", "textarea", "pre", "code", "xmp", "title"];

/// The page with each `<gx-pick>`'s Markdown rendered to HTML.
pub(super) fn render_pick_markdown(html: &str) -> Cow<'_, str> {
    if find_ascii_ignore_case(html, 0, "<gx-pick").is_none() {
        return Cow::Borrowed(html);
    }
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() + html.len() / 4);
    let mut copied = 0;
    let mut at = 0;
    while let Some(lt) = html[at..].find('<').map(|found| at + found) {
        let rest = &html[lt..];
        if rest.starts_with("<!--") {
            at = html[lt..]
                .find("-->")
                .map_or(html.len(), |end| lt + end + 3);
            continue;
        }
        let name_end = rest[1..]
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
            .map_or(rest.len(), |end| end + 1);
        let name = rest[1..name_end].to_ascii_lowercase();
        let Some(head_end) = tag_end(bytes, lt) else {
            break;
        };
        if RAW_TEXT.contains(&name.as_str()) {
            let close = format!("</{name}");
            at = find_ascii_ignore_case(html, head_end, &close).unwrap_or(html.len());
            continue;
        }
        if name != "gx-pick" {
            at = head_end;
            continue;
        }
        let head = &html[lt..head_end];
        let Some(close) = find_ascii_ignore_case(html, head_end, "</gx-pick") else {
            break;
        };
        if head.contains(RENDERED_MARK.trim_start()) {
            at = close;
            continue;
        }
        let Some(rendered) = render_markdown(&html[head_end..close]) else {
            at = close;
            continue;
        };
        out.push_str(&html[copied..lt]);
        let head_inner = head.trim_end_matches('>');
        let (start, self_closing) = match head_inner.strip_suffix('/') {
            Some(start) => (start, true),
            None => (head_inner, false),
        };
        out.push_str(start.trim_end());
        out.push_str(RENDERED_MARK);
        out.push('>');
        if !self_closing {
            out.push('\n');
            out.push_str(&rendered);
        }
        copied = close;
        at = close;
    }
    if copied == 0 {
        return Cow::Borrowed(html);
    }
    out.push_str(&html[copied..]);
    Cow::Owned(out)
}

/// The Markdown rendered as GitHub-flavoured HTML, its common indent removed first (an agent indents
/// a row's text under its tag, which Markdown would read as code).
fn render_markdown(source: &str) -> Option<String> {
    let text = dedent(source);
    if text.trim().is_empty() {
        return None;
    }
    let mut options = markdown::Options::gfm();
    options.compile.allow_dangerous_html = true;
    markdown::to_html_with_options(&text, &options).ok()
}

fn dedent(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    let mut text = String::with_capacity(source.len());
    for line in lines {
        text.push_str(line.get(indent..).unwrap_or_else(|| line.trim_start()));
        text.push('\n');
    }
    text.trim_matches('\n').to_string() + "\n"
}

/// Where the start tag that opens at `lt` ends (after its `>`), quotes respected.
fn tag_end(bytes: &[u8], lt: usize) -> Option<usize> {
    let mut quote = None;
    for (index, &byte) in bytes.iter().enumerate().skip(lt + 1) {
        match quote {
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => return Some(index + 1),
            None => {}
        }
    }
    None
}

fn find_ascii_ignore_case(text: &str, from: usize, needle: &str) -> Option<usize> {
    let haystack = text.as_bytes();
    let needle = needle.as_bytes();
    (from..haystack.len().saturating_sub(needle.len() - 1)).find(|&start| {
        haystack[start..start + needle.len()].eq_ignore_ascii_case(needle)
            && haystack
                .get(start + needle.len())
                .is_none_or(|next| !(next.is_ascii_alphanumeric() || *next == b'-'))
    })
}

#[cfg(test)]
mod tests {
    use super::render_pick_markdown;

    #[test]
    fn a_row_s_markdown_becomes_html_once() {
        let page = "<body>\n  <gx-pick value=\"FB-34\" recommended>\n    ##### `FB-34` Edit ticket details\n\n    Agents can't change the email.\n\n    | Where | Today |\n    |---|---|\n    | New ticket | No |\n  </gx-pick>\n</body>";
        let once = render_pick_markdown(page).into_owned();
        assert!(
            once.contains("<gx-pick value=\"FB-34\" recommended data-gx-md>"),
            "{once}"
        );
        assert!(
            once.contains("<h5><code>FB-34</code> Edit ticket details</h5>"),
            "{once}"
        );
        assert!(once.contains("<table>"), "{once}");
        assert_eq!(render_pick_markdown(&once), once);
    }

    #[test]
    fn examples_in_code_and_pages_without_rows_stay_as_written() {
        let page = "<pre><code><gx-pick value=\"a\">\n# Not a row\n</gx-pick></code></pre><gx-pickle>x</gx-pickle>";
        assert_eq!(render_pick_markdown(page), page);
    }
}
