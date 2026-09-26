# gpui-bidi

[![crates.io](https://img.shields.io/crates/v/gpui-bidi.svg)](https://crates.io/crates/gpui-bidi) [![docs.rs](https://docs.rs/gpui-bidi/badge.svg)](https://docs.rs/gpui-bidi) [![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Right-to-left and mixed-direction text for [GPUI](https://www.gpui.rs).

GPUI shapes Arabic, Hebrew, and Persian correctly, but its line-layout lookups
assume that text position and x coordinate grow together, which stops being
true once glyphs are reordered. This crate reads a shaped line the way it
actually is, so the caret, the selection, and a click land on the right
character. It also breaks a paragraph into rows in reading order and paints a
styled row without gpui collapsing its colors.

Built for [Zorite](https://github.com/packetThrower/zorite); it depends on
`gpui` alone.

The complete API reference is in [API.md](API.md).

## Why it exists

The platform shapers already do bidi properly. Shape `"سلام دنیا"` and the
glyphs come back in **visual** order carrying **logical** byte indices — for
that string, indices `15, 13, 11, 9, 8, 6, 2, 0` as x ascends. That is exactly
what UAX #9 asks for, and it means right-to-left text *renders* correctly
today.

What is missing is everything that reads those glyphs back. gpui's lookups
assume byte index and x rise together, which is false the moment a line
contains RTL:

- **`x_for_index`** returns the first glyph whose `index >= target`. The first
  glyph of an RTL line carries the *highest* index, so every offset in the line
  resolves to the same glyph — a caret that will not move, and selection
  rectangles with no width. `index_for_x` and `closest_index_for_x` fail the
  same way.
- **Wrapping** shapes the paragraph as one long line and then slices it into
  rows by ascending x. Glyph order is visual, so the first row gets the
  paragraph's *last* words: a wrapped Persian note reads bottom-to-top. Its
  break candidates come from an `is_word_char` that does not know Arabic, so
  words also split down the middle.
- **Painting** walks glyphs in visual order but pulls decorations from a
  forward-only iterator keyed on `glyph.index >= run_end`. In an RTL row that
  iterator jumps to the last run and never advances, so the whole row paints in
  one colour — a link inside Persian text comes out the colour of the body.

Upstream gpui is not currently taking changes for this, so the fixes live here.

## What it does

- [`VisualMap`](API.md#struct-visualmap) — index↔x in both directions over a
  reordered glyph table, plus visual caret stepping and the rectangles a
  selection needs (a logically contiguous range can be *visually split*).
- [`paragraph::layout_rows`](API.md#layout_rows) — break a paragraph in
  **logical** order at word boundaries, then shape each row on its own, so the
  shaper reorders each independently and the rows come back in reading order.
- [`paragraph::paint_row`](API.md#paint_row) — paint one row, working around
  the decoration collapse without re-shaping (which would lose cursive joining
  across a style boundary).
- [`paragraph::RtlText`](API.md#struct-rtltext) — a ready-made element for
  hosts that just want a right-to-left paragraph, with hit-testing and
  hover-cursor support.

Direction comes from the real UAX #9 embedding levels via `unicode-bidi`, not
from the glyph geometry: at the *edge* of an embedded run the glyph beside it
belongs to the other run and carries a misleading index, so geometry is exact
in the middle of a run and wrong precisely where it matters.

## Adding the dependency

Published on [crates.io](https://crates.io/crates/gpui-bidi):

```toml
[dependencies]
gpui-bidi = "0.1"
```

> **gpui version:** the crate depends on GPUI as published on crates.io — the
> `gpui-pre` family, consumed under the name `gpui` (`gpui = { package = "gpui-pre",
> version = "0.3" }`). Every `gpui-pre` release is a different Zed snapshot, so your
> app must resolve to the **same** `gpui-pre` version as this crate (one gpui graph);
> pin it in your `Cargo.lock` and move both together.

## Quick start

```rust
use gpui_bidi::shaped;

// A shaped line from gpui, plus the text it came from (for the levels).
let map = shaped::map_of(&line, text.len()).with_levels(text);

// Where the caret before byte `offset` belongs — the glyph's RIGHT edge in an
// RTL run, which is what gpui gets wrong.
let x = map.x_for_index(offset);

// And back, for a click.
let offset = map.index_for_x(x);
```

Laying out and painting a paragraph yourself:

```rust
use gpui_bidi::paragraph::{layout_rows, paint_row};

let rows = layout_rows(text, &runs, Some(wrap_width), font_size, window);
for (i, row) in rows.iter().enumerate() {
    let x = bounds.right() - row.width; // right-aligned; use bounds.left() for LTR
    paint_row(row, point(x, bounds.top() + line_height * i), line_height, window, cx);
}
```

## Scope

Everything in the crate root works on plain `(logical byte index, x)` pairs, so
it is unit-testable without a window, a GPU, or a font. Only the `shaped` and
`paragraph` modules touch gpui.

**Paragraph direction** — which side a line starts on — is the caller's, from
the first strong character of its *content*. This crate maps within a line and
lays out rows; it does not decide what a line is.

The word breaking in `layout_rows` is space-based, which suits Arabic and
Hebrew. A line mixing right-to-left text with unspaced CJK would wrap poorly;
lines with no RTL should keep gpui's own wrapping.

## API

Complete reference: [API.md](API.md).

## License

MIT. (The Zorite app itself is GPL-3.0-or-later.)
