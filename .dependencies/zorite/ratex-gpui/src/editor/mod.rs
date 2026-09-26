//! The structural math editor.
//!
//! **Always built** (the parse/serialize layer the [`crate::render`] path needs):
//! - [`model`] — the edit tree (`Row` / `Atom`) and LaTeX serialization. No gpui.
//! - [`latex`] — LaTeX → `Row` ([`latex::parse_latex`]). No gpui.
//!
//! **Behind the `editor` feature** (on by default; `default-features = false` gives a
//! render-only build that drops these and the [`view::MathEditor`] view):
//! - `geometry` — caret + slot rects from RaTeX's layout. No gpui.
//! - `cursor` — cursor + structural edits (insert / backspace / navigate). No gpui.
//! - `input` — typed-char → edit interpreter ("natural typing") + the `\command` table. No gpui.
//! - `view` — the gpui view: the formula + caret + palette, turning keystrokes into edits.

pub mod latex;
pub mod model;

#[cfg(feature = "editor")]
pub mod cursor;
#[cfg(feature = "editor")]
pub mod geometry;
#[cfg(feature = "editor")]
pub mod input;
#[cfg(feature = "editor")]
pub mod view;
