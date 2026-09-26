//! `ratex-gpui` — a structural, MathQuill-style math editor for GPUI, built on the
//! [RaTeX](https://github.com/erweixin/RaTeX) typesetting engine.
//!
//! RaTeX is the engine (parse → layout → render); this crate is the **editor + display layer**.
//!
//! - [`render`] — display a formula as a `gpui::RenderImage` (or PNG / SVG). Always available.
//! - [`editor`] — the structural editor. The parse/serialize core ([`editor::model`],
//!   [`editor::latex`]) is always built; the interactive editor ([`MathEditor`] + the
//!   `cursor` / `geometry` / `input` / `view` modules) is behind the **`editor`** feature
//!   (enabled by default). For a render-only build, depend with `default-features = false`.

pub mod editor;
pub mod render;

pub use editor::latex::parse_latex;

#[cfg(feature = "editor")]
pub use editor::view::{MathAlign, MathEditor, MathNav, MathTheme};
