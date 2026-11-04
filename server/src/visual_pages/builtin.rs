//! The built-in block every published page carries at the start of its `<head>`: the content
//! policy, the theme, the bootstrap, the page API with its state marker and the question kit.
//! `prepare` writes it between [`BUILTIN_START`] and [`BUILTIN_END`]; `serve` swaps whatever block
//! a stored page was published with for today's.

use super::{
    fit::{FIT_SCRIPT, FIT_STYLE},
    kit::{KIT_SCRIPT, KIT_STYLE},
    prepare::{read_prologue, PAGE_CONTENT_SECURITY_POLICY},
    theme::{PAGE_API, PAGE_BOOTSTRAP, PAGE_STATE_MARKER, THEME_STYLE},
};

/// Where the built-in block starts and ends in a page published since 2026-10-26.
pub(super) const BUILTIN_START: &str = "<!--gx-builtin-->";
pub(super) const BUILTIN_END: &str = "<!--/gx-builtin-->";

/// The tags `prepare` adds before the block when the page has none of its own; they never change,
/// so a stored page keeps the ones it has.
const CHARSET_META: &str = "<meta charset=\"utf-8\">";
const VIEWPORT_META: &str =
    "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">";
const POLICY_META_START: &str = "<meta http-equiv=\"Content-Security-Policy\" content=\"";

/// Today's built-in block, marked so it can be swapped exactly.
pub(super) fn current_block() -> String {
    let mut block = String::with_capacity(
        THEME_STYLE.len()
            + PAGE_API.len()
            + KIT_STYLE.len()
            + KIT_SCRIPT.len()
            + FIT_STYLE.len()
            + FIT_SCRIPT.len()
            + 4096,
    );
    block.push_str(BUILTIN_START);
    block.push_str(POLICY_META_START);
    block.push_str(PAGE_CONTENT_SECURITY_POLICY);
    block.push_str("\">");
    block.push_str(THEME_STYLE);
    block.push_str(PAGE_BOOTSTRAP);
    block.push_str(PAGE_API);
    block.push_str(PAGE_STATE_MARKER);
    block.push_str(KIT_STYLE);
    block.push_str(KIT_SCRIPT);
    block.push_str(FIT_STYLE);
    block.push_str(FIT_SCRIPT);
    block.push_str(BUILTIN_END);
    block
}

/// The stored page with today's built-in block in place of the one it was published with.
///
/// CDXC:SessionChat 2026-10-26 DECISION:
/// User, asked whether pages that are already published should get Ghostex's newest built-in page script whenever they're opened instead of the one they were published with: "Yes, always use the newest script". A page published before a fix (the phone's Send that said "Failed to fetch" and the drafts that stopped landing, theme.rs) kept the broken script because `prepare` writes the block into the stored file, so the block is swapped here on every serve and the stored file is never rewritten.
/// CDXC:SessionChat 2026-10-26 WHY:
/// Only the block `prepare` wrote is replaced, found where `prepare` put it (right after the document's prologue, past the charset and viewport tags it may have added) and never by a search through the page, so the page's own content and scripts are left as they are. A page published since this change has the block between `BUILTIN_START` and `BUILTIN_END`. Older pages have one of four layouts, each a prefix of today's (policy and theme; then the bootstrap; then the page API and its state marker; then the kit's style and script), recognised part by part by openings that never changed: the policy tag, `<style id="ghostex-theme">`, the bootstrap's `var d=document.documentElement;`, the page API's `var st=null` followed by the state marker, `<style id="ghostex-kit">` followed by the kit's script. A built-in style or script never holds `</style>` or `</script>`, so each part ends at the first one. The policy tag is swapped too, because pages published before 2026-10-11 lack the `connect-src 'self'` today's script posts its drafts and answers through. A page whose start matches none of this is served as it was stored.
pub(super) fn with_current_block(html: String) -> String {
    let Some(range) = published_block(&html) else {
        return html;
    };
    let mut out = String::with_capacity(html.len() + 64 * 1024);
    out.push_str(&html[..range.start]);
    out.push_str(&current_block());
    out.push_str(&html[range.end..]);
    out
}

/// Where the block the page was published with lies, when it can be told apart exactly.
fn published_block(html: &str) -> Option<std::ops::Range<usize>> {
    let mut at = read_prologue(html).insert_at;
    for optional in [CHARSET_META, VIEWPORT_META] {
        if html[at..].starts_with(optional) {
            at += optional.len();
        }
    }
    let start = at;
    if html[at..].starts_with(BUILTIN_START) {
        let end = html[at..].find(BUILTIN_END)? + at + BUILTIN_END.len();
        return Some(start..end);
    }
    if !html[at..].starts_with(POLICY_META_START) {
        return None;
    }
    at += html[at..].find("\">")? + 2;
    at = element(html, at, "<style id=\"ghostex-theme\">", "</style>")?;
    let Some(after_bootstrap) = element(
        html,
        at,
        "<script>(function(){var d=document.documentElement;",
        "</script>",
    ) else {
        return Some(start..at);
    };
    at = after_bootstrap;
    let Some(after_api) = element(html, at, "<script>(function(){var st=null", "</script>")
        .filter(|end| html[*end..].starts_with(PAGE_STATE_MARKER))
    else {
        return Some(start..at);
    };
    at = after_api + PAGE_STATE_MARKER.len();
    let Some(after_kit_style) = element(html, at, "<style id=\"ghostex-kit\">", "</style>") else {
        return Some(start..at);
    };
    // The kit's style and script were always added together; a style without its script is not a
    // block this knows.
    let after_kit_script = element(html, after_kit_style, "<script>", "</script>")
        .filter(|end| html[after_kit_style..*end].contains("gx-questions"))?;
    Some(start..after_kit_script)
}

/// The end of the element at `at` that opens with `opening` and closes at the first `closing`.
fn element(html: &str, at: usize, opening: &str, closing: &str) -> Option<usize> {
    if !html[at..].starts_with(opening) {
        return None;
    }
    let body = at + opening.len();
    Some(body + html[body..].find(closing)? + closing.len())
}
