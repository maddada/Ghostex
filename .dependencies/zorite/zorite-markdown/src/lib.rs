//! Zorite's **reader** view: a small read-only markdown renderer for GPUI.
//! (Editing — WYSIWYG and raw — is the separate `zorite-editor` crate; the two
//! engines share nothing, so any markdown behavior added here must be checked
//! there and vice versa. See AGENTS.md "The three views".)
//!
//! It parses markdown to an AST (via the `markdown` crate) and renders
//! it with gpui's own `StyledText` / `InteractiveText`, so paragraphs
//! wrap properly and links are clickable through a real callback — not
//! `cx.open_url`, which only opens externally.
//!
//! It is deliberately host-agnostic: styling comes in via [`MarkdownStyle`],
//! and clicking a `[[wiki-link]]` invokes a caller-supplied closure
//! (`on_wiki_link`) rather than knowing anything about the host app.
//! Standard `[text](url)` links open externally via `cx.open_url`.
//!
//! Covers CommonMark + GFM: headings, paragraphs, bold/italic/strikethrough/
//! inline-code, fenced code blocks, ordered/unordered/nested and task lists,
//! blockquotes, thematic breaks, hard breaks, tables, links (inline and
//! reference-style), images (rendered by the host via `on_image`), footnotes,
//! and raw HTML (shown literally, never executed — except `<mark>`, which renders
//! as highlighted text). `[[wiki-links]]` and `#tags` become clickable via caller
//! callbacks.

pub mod syntax;

#[cfg(feature = "view")]
mod view;
#[cfg(feature = "view")]
pub use view::*;
