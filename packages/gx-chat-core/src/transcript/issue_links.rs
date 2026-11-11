//! `#123` in a chat's Markdown as a link to that issue or pull request in the project's GitHub
//! repository: the one rule for which `#numbers` are links, and the address they link to.
//!
//! CDXC:SessionChat 2026-11-11 DECISION:
//! User, on the phone parity page: "Build it now, moving the rule into the brain (over the air)" for
//! "#123 links to the GitHub issue or pull request". The rule the desktop drew with since
//! 2026-10-09 (`issue_references` in gpui-component's text code, CDXC:SessionChat 2026-10-09 in
//! apps/desktop/src/app/native_chat/markdown_links.rs) lives here now, and both chats draw from it:
//!
//!   * [`native_markdown`](crate::transcript::native_markdown::native_markdown) writes each `#123`
//!     in prose as a link to `ghostex-issue:123` ([`mark_issue_references`]), so the Markdown the
//!     document carries already says which spans are issue links. A `#` inside code, a link, a
//!     heading's own `#` marks, a hex colour (`#fff`, `#ff8800`) and a number inside a word
//!     (`abc#12`, `owner/repo#12`) stay text.
//!   * The address is the project's GitHub `origin` ([`github_issues_url`]), which the host tells
//!     the core (`Event::ProjectOriginChanged`) and the document publishes as `issueLinkBase`; a
//!     project without a GitHub remote has none, and its `ghostex-issue:` links draw as the plain
//!     `#123` they were written as ([`resolve_issue_links`]). The mark stays address-free so a
//!     message's cached projection never depends on the remote, which arrives after it.
//!
//! GitHub forwards `/issues/<n>` to the pull request when the number is one.
//! SEE-ALSO: apps/desktop/src/app/native_chat/rich_markdown.rs (`chat_links`, the desktop),
//! apps/mobile/app/src/chat/native/transcript/markdown/Markdown.tsx (the phone),
//! questions/form_links.rs (the same rule for a question's words and a work item's).

use std::borrow::Cow;
use std::ops::Range;

use markdown::mdast::Node;
use markdown::ParseOptions;

/// The destination [`mark_issue_references`] writes, its number appended.
pub const ISSUE_LINK_SCHEME: &str = "ghostex-issue:";

fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// A bare `#123` in `text` at byte `at`: the end of its digits when it is one. A whole token: a
/// `#` that does not follow a word character, `/`, `#` or `&`, then 1 to 9 decimal digits with no
/// word character, `/` or `-` after them. Six and eight digits are a hex colour (its swatch) and a
/// leading zero names no issue, so neither links; a token with a letter a-f is a colour or nothing.
/// `owner/repo#12` is left alone: its `#` follows a word character.
pub fn issue_reference_at(text: &str, at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.get(at) != Some(&b'#') {
        return None;
    }
    if at > 0 && (word(bytes[at - 1]) || matches!(bytes[at - 1], b'/' | b'#' | b'&')) {
        return None;
    }
    let end = at
        + 1
        + bytes[at + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
    let digits = end - at - 1;
    let closes = end >= bytes.len() || !(word(bytes[end]) || matches!(bytes[end], b'/' | b'-'));
    (closes && (1..=9).contains(&digits) && digits != 6 && digits != 8 && bytes[at + 1] != b'0')
        .then_some(end)
}

/// Every `#123` reference in a run of text, with the number it names.
pub fn issue_references(text: &str) -> Vec<(Range<usize>, &str)> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find('#') {
        let start = at + offset;
        match issue_reference_at(text, start) {
            Some(end) => {
                found.push((start..end, &text[start + 1..end]));
                at = end;
            }
            None => at = start + 1,
        }
    }
    found
}

/// `https://github.com/<owner>/<repo>/issues/` for a GitHub `origin` written as
/// `git@github.com:owner/repo.git`, `ssh://git@github.com/owner/repo.git` or
/// `https://github.com/owner/repo(.git)`; `None` for any other host, or a name GitHub would not
/// give a repository.
pub fn github_issues_url(origin: &str) -> Option<String> {
    let origin = origin.trim();
    let path = match origin.strip_prefix("git@github.com:") {
        Some(path) => path,
        None => {
            let (_, rest) = origin.split_once("://")?;
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit('@').next()?.split(':').next()?;
            if !host.eq_ignore_ascii_case("github.com")
                && !host.eq_ignore_ascii_case("www.github.com")
            {
                return None;
            }
            path
        }
    };
    let path = path.split(['?', '#']).next()?;
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    let owner = segments.next()?;
    let repository = segments.next()?;
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    // The address goes into a Markdown destination as written, so only GitHub's own name characters.
    let named = |name: &str| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    (named(owner) && named(repository))
        .then(|| format!("https://github.com/{owner}/{repository}/issues/"))
}

/// Whether `markdown` has a `#` before a digit 1-9 anywhere, the only text either pass changes.
fn has_issue_evidence(markdown: &str) -> bool {
    markdown
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0] == b'#' && matches!(pair[1], b'1'..=b'9'))
}

/// One source edit: the bytes it replaces and what replaces them.
type Edit = (Range<usize>, String);

/// Walks the parsed Markdown for the edits `edit_text` (an issue reference in prose) and
/// `edit_link` (an existing `ghostex-issue:` link) ask for. Code, links, pictures, raw HTML and
/// definitions are never looked into.
fn collect_edits(
    markdown: &str,
    edit_text: &dyn Fn(Range<usize>, &str) -> Option<String>,
    edit_link: &dyn Fn(&str) -> Option<String>,
) -> Vec<Edit> {
    let Ok(tree) = markdown::to_mdast(markdown, &ParseOptions::default()) else {
        return Vec::new();
    };
    let mut edits = Vec::new();
    fn walk(
        node: &Node,
        markdown: &str,
        edit_text: &dyn Fn(Range<usize>, &str) -> Option<String>,
        edit_link: &dyn Fn(&str) -> Option<String>,
        edits: &mut Vec<Edit>,
    ) {
        match node {
            Node::Text(_) => {
                let Some(position) = node.position() else {
                    return;
                };
                let (start, end) = (position.start.offset, position.end.offset);
                let Some(text) = markdown.get(start..end) else {
                    return;
                };
                let mut at = 0;
                while let Some(offset) = text[at..].find('#') {
                    let hash = start + at + offset;
                    match issue_reference_at(markdown, hash).filter(|&stop| stop <= end) {
                        Some(stop) => {
                            // `\#12` is the same `#12` with its `#` escaped: the link replaces both.
                            let from = if hash > start && markdown.as_bytes()[hash - 1] == b'\\' {
                                hash - 1
                            } else {
                                hash
                            };
                            if let Some(replacement) =
                                edit_text(from..stop, &markdown[hash + 1..stop])
                            {
                                edits.push((from..stop, replacement));
                            }
                            at = stop - start;
                        }
                        None => at = hash - start + 1,
                    }
                }
            }
            Node::Link(link) => {
                if let (Some(number), Some(position)) =
                    (link.url.strip_prefix(ISSUE_LINK_SCHEME), node.position())
                {
                    if let Some(replacement) = edit_link(number) {
                        edits.push((position.start.offset..position.end.offset, replacement));
                    }
                }
            }
            Node::InlineCode(_)
            | Node::Code(_)
            | Node::Html(_)
            | Node::Image(_)
            | Node::ImageReference(_)
            | Node::LinkReference(_)
            | Node::Definition(_)
            | Node::InlineMath(_)
            | Node::Math(_) => {}
            _ => {
                for child in node.children().into_iter().flatten() {
                    walk(child, markdown, edit_text, edit_link, edits);
                }
            }
        }
    }
    walk(&tree, markdown, edit_text, edit_link, &mut edits);
    edits
}

fn apply(markdown: &str, mut edits: Vec<Edit>) -> Cow<'_, str> {
    if edits.is_empty() {
        return Cow::Borrowed(markdown);
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = String::with_capacity(markdown.len() + edits.len() * 24);
    let mut copied = 0;
    for (range, replacement) in edits {
        if range.start < copied {
            continue;
        }
        out.push_str(&markdown[copied..range.start]);
        out.push_str(&replacement);
        copied = range.end;
    }
    out.push_str(&markdown[copied..]);
    Cow::Owned(out)
}

/// Writes every `#123` in `markdown`'s prose as `[#123](ghostex-issue:123)`, the address-free mark
/// both chats read. Markdown with no `#` before a digit comes back as it was, unparsed.
pub fn mark_issue_references(markdown: &str) -> Cow<'_, str> {
    if !has_issue_evidence(markdown) {
        return Cow::Borrowed(markdown);
    }
    let edits = collect_edits(
        markdown,
        &|_, number| Some(format!("[#{number}]({ISSUE_LINK_SCHEME}{number})")),
        &|_| None,
    );
    apply(markdown, edits)
}

/// The Markdown a chat draws: each `ghostex-issue:` link pointing at `base` (the document's
/// `issueLinkBase`), or back to the plain `#123` it was when there is no base; with a base, a
/// `#123` that is not marked yet (text the core never marked, such as a work item's ask) links too.
pub fn resolve_issue_links<'a>(markdown: &'a str, base: Option<&str>) -> Cow<'a, str> {
    if !has_issue_evidence(markdown) {
        return Cow::Borrowed(markdown);
    }
    let edits = collect_edits(
        markdown,
        &|_, number| base.map(|base| format!("[#{number}]({base}{number})")),
        &|number| {
            Some(match base {
                Some(base) => format!("[#{number}]({base}{number})"),
                None => format!("#{number}"),
            })
        },
    );
    apply(markdown, edits)
}
