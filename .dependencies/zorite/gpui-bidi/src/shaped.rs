//! The gpui adapter: read a [`VisualMap`] out of a shaped line.
//!
//! Kept apart from the core deliberately — everything in the crate root works
//! on plain numbers and is testable without a window, a GPU, or a font. Only
//! this module needs gpui, and all it does is copy each glyph's logical index
//! and laid-out x across.
//!
//! `ShapedLine` derefs to `LineLayout`, so `runs` is reachable even though the
//! `layout` field itself is `pub(crate)` — which is what lets this work
//! without forking gpui. Worth re-checking whenever the gpui pin moves.

use gpui::{ShapedLine, WrappedLine};

use crate::{Glyph, VisualMap};

/// Build a map from a shaped line.
///
/// `len` is the byte length of the text that was shaped — the caret can sit
/// one past the last glyph, and only the caller knows where that is.
pub fn map_of(line: &ShapedLine, len: usize) -> VisualMap {
    let glyphs = line
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|g| Glyph {
            index: g.index,
            x: f32::from(g.position.x),
        });
    VisualMap::from_glyphs(glyphs, f32::from(line.width), len).with_levels(&line.text)
}

/// [`map_of`] for a wrapped line, over its **pre-wrap** layout.
///
/// A `WrappedLine` keeps one unwrapped glyph list plus the boundaries it was
/// wrapped at, so the x values here are positions along the single long line,
/// not within a visual row. That is the right input for a caller that resolves
/// a row's glyph span itself (via `wrap_boundaries`); a caller that wants
/// per-row coordinates must subtract the row's start x.
///
/// No per-row helper is offered yet: getting it right means reasoning about
/// where a bidi run straddles a wrap boundary, and that belongs with the code
/// that actually paints rows rather than being guessed at here.
pub fn map_of_wrapped(line: &WrappedLine, len: usize) -> VisualMap {
    let layout = &line.unwrapped_layout;
    let glyphs = layout
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|g| Glyph {
            index: g.index,
            x: f32::from(g.position.x),
        });
    VisualMap::from_glyphs(glyphs, f32::from(layout.width), len).with_levels(&line.text)
}
