# gpui-bidi API

The complete public API of [`gpui-bidi`](README.md) — every exported item, with
its signature, contract, edge cases, and cost. For the what-and-why (which gpui
lookups are wrong and why), see the [README](README.md).

## Public API at a glance

Three layers. The root works on plain numbers and is testable without a window;
`shaped` reads a gpui line into it; `paragraph` lays out and paints whole
paragraphs.

| Item | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| [`Glyph`](#struct-glyph) | struct | `{ index: usize, x: f32 }` | One glyph: where it came from, where it landed |
| [`VisualMap`](#struct-visualmap) | struct | — | A shaped line's glyphs, readable in both directions |
| [`VisualMap::from_glyphs`](#visualmapfrom_glyphs) | constructor | `fn(impl IntoIterator<Item = Glyph>, f32, usize) -> Self` | Build from glyphs, line width, text length |
| [`VisualMap::with_levels`](#visualmapwith_levels) | builder | `fn(self, &str) -> Self` | Attach the text, for UAX #9 embedding levels |
| [`VisualMap::is_bidi`](#visualmapis_bidi) | method | `fn(&self) -> bool` | Does any glyph sit out of logical order? |
| [`VisualMap::width`](#visualmapwidth) | method | `fn(&self) -> f32` | Total advance width |
| [`VisualMap::is_empty`](#visualmapis_empty) | method | `fn(&self) -> bool` | No glyphs at all |
| [`VisualMap::x_for_index`](#visualmapx_for_index) | method | `fn(&self, usize) -> f32` | Where a caret before a byte belongs |
| [`VisualMap::index_for_x`](#visualmapindex_for_x) | method | `fn(&self, f32) -> usize` | Which byte a click at an x names |
| [`VisualMap::rects_for_range`](#visualmaprects_for_range) | method | `fn(&self, Range<usize>) -> Vec<(f32, f32)>` | The (possibly split) spans a range covers |
| [`VisualMap::step_visual`](#visualmapstep_visual) | method | `fn(&self, usize, bool) -> Option<usize>` | One caret step left/right, visually |
| [`VisualMap::is_rtl_range`](#visualmapis_rtl_range) | method | `fn(&self, Range<usize>) -> bool` | Does a byte range read right-to-left? |
| [`shaped::map_of`](#shapedmap_of) | fn | `fn(&ShapedLine, usize) -> VisualMap` | Read a map out of a shaped line |
| [`shaped::map_of_wrapped`](#shapedmap_of_wrapped) | fn | `fn(&WrappedLine, usize) -> VisualMap` | Same, over a wrapped line's pre-wrap layout |
| [`paragraph::Row`](#struct-row) | struct | — | One laid-out row: span, width, map, runs, shaped line |
| [`paragraph::layout_rows`](#layout_rows) | fn | `fn(&str, &[TextRun], Option<Pixels>, Pixels, &Window) -> Vec<Row>` | Break in logical order, shape each row |
| [`paragraph::paint_row`](#paint_row) | fn | `fn(&Row, Point<Pixels>, Pixels, &mut Window, &mut App)` | Paint one row, styles intact |
| [`paragraph::insert_breaks`](#insert_breaks) | fn | `fn(&str, &[TextRun], &[usize]) -> (SharedString, Vec<TextRun>)` | Inject `\n` at offsets, widening runs |
| [`paragraph::RtlText`](#struct-rtltext) | struct | — | A right-to-left paragraph element |
| [`paragraph::RtlLayout`](#struct-rtllayout) | struct | — | Handle onto the last layout, for hit-testing |

---

## `struct Glyph`

```rust
pub struct Glyph {
    pub index: usize, // byte offset into the shaped text — LOGICAL order
    pub x: f32,       // laid-out leading edge, line-relative — VISUAL order
}
```

The two facts this crate needs about a glyph. Everything else about it — font,
id, size — belongs to the shaper.

---

## `struct VisualMap`

```rust
pub struct VisualMap { /* private */ }
```

A shaped line's glyphs, indexed so logical offsets and visual positions convert
both ways regardless of writing direction. Glyphs are stored in visual
(x-ascending) order with a parallel logical-order table, so neither direction
of the mapping has to scan.

### `VisualMap::from_glyphs`

```rust
pub fn from_glyphs(glyphs: impl IntoIterator<Item = Glyph>, width: f32, len: usize) -> Self
```

`width` is the line's total advance; `len` the byte length of the shaped text
(the caret can sit one past the last glyph, and only the caller knows where
that is).

**Guarantees & edge cases** — `glyphs` may arrive in any order; they are sorted
by x here, so a shaper's runs can be handed over concatenated. Without
[`with_levels`](#visualmapwith_levels), glyph direction is inferred from the
neighbouring indices, which is exact in the middle of a run and **wrong at its
edges** — prefer attaching the text.

**Cost** — two sorts, O(n log n); no allocation beyond the two tables.

### `VisualMap::with_levels`

```rust
pub fn with_levels(mut self, text: &str) -> Self
```

Attach the text the glyphs were shaped from, so direction comes from the UAX #9
embedding levels rather than being inferred.

**Why it matters** — at the edge of an embedded run the glyph visually beside it
belongs to the *other* run and carries a misleading index, so inference reads
"descending, therefore RTL" for the last letter of a Latin word inside Persian.
The caret then sits on that letter's far edge — one glyph out, which a reader
sees as the caret skipping a character.

**Cost** — one `unicode-bidi` pass over the text, plus a `Vec<u8>` per byte.

### `VisualMap::is_bidi`

```rust
pub fn is_bidi(&self) -> bool
```

Whether any glyph sits out of logical order. A pure left-to-right line answers
`false`, and callers can use that to skip to gpui's own (cheaper) lookups.

### `VisualMap::width`

```rust
pub fn width(&self) -> f32
```

The line's total advance width, as handed to the constructor.

### `VisualMap::is_empty`

```rust
pub fn is_empty(&self) -> bool
```

No glyphs — an empty line. Every lookup below is still safe to call.

### `VisualMap::x_for_index`

```rust
pub fn x_for_index(&self, offset: usize) -> f32
```

The x where a caret sitting *before* byte `offset` belongs.

**Guarantees & edge cases** — "before" is in reading order, so for a glyph in an
RTL run that is its **right** edge, not its left. An offset past the end
resolves to the line's trailing edge: the right edge for LTR, x = 0 for a line
ending in RTL. An offset before the first logical glyph gives the leading edge.
Several glyphs can share an index (a ligature, a combining sequence); the first
owns the caret. Empty map → `0.0`.

**Cost** — one binary search.

### `VisualMap::index_for_x`

```rust
pub fn index_for_x(&self, x: f32) -> usize
```

The byte offset a click at `x` should place the caret at.

**Guarantees & edge cases** — picks the nearer edge of whichever glyph `x` lands
on, so clicking a glyph's trailing half puts the caret after it in *reading*
order — past it on the left for RTL, on the right for LTR. Left of every glyph
gives the visually-first glyph's leading edge. Empty map → `0`.

### `VisualMap::rects_for_range`

```rust
pub fn rects_for_range(&self, range: Range<usize>) -> Vec<(f32, f32)>
```

The visual spans covering logical `range`, as `(start x, end x)` pairs.

**Guarantees & edge cases** — a logically contiguous selection can be **visually
split**: selecting across a direction change in `"hello سلام world"` covers two
or three separate stretches, so this returns however many it takes. Adjacent
runs are merged and the result is sorted left to right. Empty range or empty
map → empty vec.

**Cost** — one pass over the glyphs plus a sort of the matches.

### `VisualMap::step_visual`

```rust
pub fn step_visual(&self, offset: usize, right: bool) -> Option<usize>
```

The logical offset one caret step to the visual left or right of `offset`.
`None` when there is no stop that way — the caret is leaving the line, and the
caller should move it to the neighbouring row.

**Why it isn't a byte step** — arrow keys move visually, and inside a line that
is not "this line is RTL, so flip left and right". A Latin word or a URL
embedded in Persian runs the other way, and the caret has to flow *through* it
in its own direction rather than jump to its far end.

**Guarantees & edge cases** — caret stops are ordered by x, ties broken by
offset. The tie-break matters: at a direction change two different offsets sit
at the same x, and ordering by x alone makes one of them unreachable — which
shows up as the caret skipping a character at the edge of an embedded word. An
`offset` that is not itself a caret stop returns `None`.

**Cost** — builds the stop list per call, O(n log n) in the line's glyphs.

### `VisualMap::is_rtl_range`

```rust
pub fn is_rtl_range(&self, range: Range<usize>) -> bool
```

Does the byte range read right-to-left? From the embedding levels when
[`with_levels`](#visualmapwith_levels) supplied them, otherwise from glyph
order (unreliable at run edges — see there).

Used when a slice has to be re-shaped or re-styled on its own and needs its
direction re-asserted; a run of pure neutrals (`[[`, `](`) has no direction of
its own and takes the paragraph's.

---

## `shaped::map_of`

```rust
pub fn map_of(line: &ShapedLine, len: usize) -> VisualMap
```

Build a map from a gpui shaped line, levels attached from the line's own text.
`len` is the byte length of the text that was shaped.

**Note** — `ShapedLine` derefs to `LineLayout`, so `runs` is reachable even
though the `layout` field is `pub(crate)`. That is what lets this work without
forking gpui; worth re-checking whenever the gpui pin moves.

## `shaped::map_of_wrapped`

```rust
pub fn map_of_wrapped(line: &WrappedLine, len: usize) -> VisualMap
```

[`map_of`](#shapedmap_of) over a wrapped line's **pre-wrap** layout.

**Guarantees & edge cases** — a `WrappedLine` keeps one unwrapped glyph list
plus the boundaries it was wrapped at, so the x values run along the single
long line, not within a visual row. That is the right input for a caller that
resolves a row's glyph span itself; a caller wanting per-row coordinates must
subtract the row's start x — or use [`layout_rows`](#layout_rows), which gives
one map per row.

---

## `struct Row`

```rust
pub struct Row {
    pub start: usize,                          // byte offset in the ORIGINAL text
    pub len: usize,                            // byte length of this row's text
    pub width: Pixels,
    pub map: VisualMap,
    pub runs: Vec<(Range<usize>, TextRun)>,    // row-local ranges
    pub line: WrappedLine,                     // shaped, ready to paint
    pub font_size: Pixels,                     // what it was shaped at
}
```

One laid-out row. `start` indexes the **original** text — the row breaks
injected during layout do not exist there — so a caret offset maps straight
through. `font_size` is carried so painting can re-shape without the caller
supplying a size that might not be this row's (a heading is not the body size).

## `layout_rows`

```rust
pub fn layout_rows(
    text: &str,
    runs: &[TextRun],
    wrap_width: Option<Pixels>,
    font_size: Pixels,
    window: &Window,
) -> Vec<Row>
```

Lay `text` out as rows: break it in **logical** order at word boundaries, then
shape each row on its own so the shaper reorders each independently.

`runs` must cover every byte of `text`. `wrap_width` of `None` produces a
single row.

**Guarantees & edge cases** — rows come back in reading order, so row 0 is the
paragraph's first line whichever way the script runs. Word breaking is
space-based (not gpui's `LineWrapper`, whose `is_word_char` does not know
Arabic and so treats every character as a break candidate, splitting words
mid-word); the zero-width non-joiner U+200C is **not** a break, since it sits
inside Persian words. A single word wider than `wrap_width` overflows rather
than being chopped. A line mixing RTL with unspaced CJK wraps poorly — see the
README's Scope.

**Cost** — one `shape_line` per word to measure, then one `shape_text` over the
whole paragraph. gpui's line-layout cache absorbs the repeats across frames.

## `paint_row`

```rust
pub fn paint_row(
    row: &Row,
    origin: Point<Pixels>,
    line_height: Pixels,
    window: &mut Window,
    cx: &mut App,
)
```

Paint one row at `origin`, whose x is the row's **left** edge (right-align by
passing `bounds.right() - row.width`).

**Guarantees & edge cases** — a single-style row goes through gpui's painter
directly. A row with more than one style cannot: `paint_line` walks glyphs in
visual order but pulls decorations from a forward-only iterator keyed on
`glyph.index >= run_end`, and in an RTL row the first glyph carries the highest
index, so the iterator jumps to the last run and never advances — the whole row
paints in the colour of whichever run covers the logically-last character. Such
a row is instead painted once per style, uniformly coloured for that style
(making the collapse harmless) and clipped to the boxes that style occupies.
Only the decoration varies between passes, so every pass lays out exactly as
the row did, and cursive joining survives a style boundary inside a word. Run
backgrounds (an inline-code tint) are painted too.

**Cost** — one paint per distinct style, each over the whole row; the shaping is
cached by gpui.

## `insert_breaks`

```rust
pub fn insert_breaks(
    text: &str,
    runs: &[TextRun],
    breaks: &[usize],
) -> (SharedString, Vec<TextRun>)
```

Split `text` at `breaks` (logical byte offsets) by injecting `\n`, widening
`runs` to cover the injected bytes. Exposed because it is the whole trick
behind [`layout_rows`](#layout_rows): `shape_text` splits on `\n` and shapes
each line separately, in order, so injecting a break at each logical boundary
buys the per-line reordering.

**Guarantees & edge cases** — `shape_text` requires runs to span every byte
including the `\n`s, so a break inside a run lengthens that run rather than
splitting it (the newline inherits the style it interrupts, which paints
nothing either way). Offsets at 0, at `text.len()`, repeated, or mid-codepoint
are ignored — each would produce an empty row or panic the shaper.

---

## `struct RtlText`

```rust
pub struct RtlText { /* private */ }

impl RtlText {
    pub fn new(text: impl Into<SharedString>) -> Self;
    pub fn with_base_rtl(self, rtl: bool) -> Self;
    pub fn with_highlights(self, impl IntoIterator<Item = (Range<usize>, HighlightStyle)>) -> Self;
    pub fn with_pointer_ranges(self, ranges: Vec<Range<usize>>) -> Self;
    pub fn layout(&self) -> &RtlLayout;
}
```

A paragraph element that lays itself out in logical order and paints its rows
right-aligned. Implements `Element`, so it drops into a gpui tree like
`StyledText`.

- **`with_base_rtl`** — defaults `true`. Pass `false` for a left-to-right
  paragraph that merely *contains* right-to-left text: it still needs the
  mapping (or the caret misplaces inside that phrase) but must stay
  left-aligned.
- **`with_highlights`** — styled ranges, exactly as `StyledText` takes them:
  sorted, non-overlapping, on char boundaries.
- **`with_pointer_ranges`** — ranges that show the pointing-hand cursor on
  hover, i.e. links. One hitbox is inserted per visual box, so the hand appears
  over the link's glyphs and nowhere else, and it does not depend on a repaint
  happening as the mouse moves.
- **`layout`** — clone this before the element is consumed by the tree; it is
  how the host hit-tests (see [`RtlLayout`](#struct-rtllayout)).

## `struct RtlLayout`

```rust
pub struct RtlLayout(/* private */);

impl RtlLayout {
    pub fn line_height(&self) -> Pixels;
    pub fn index_for_position(&self, position: Point<Pixels>) -> Result<usize, usize>;
    pub fn position_for_index(&self, offset: usize) -> Option<Point<Pixels>>;
    pub fn rects_for_range(&self, range: Range<usize>) -> Vec<Bounds<Pixels>>;
    pub fn left_edge_of(&self, range: Range<usize>) -> Option<Point<Pixels>>;
}
```

A handle onto the last layout, for mapping between a point on screen and an
offset in the text. Cheap to clone; **empty until the element has been painted
once**, since hit-testing is in window space.

- **`index_for_position`** — `Ok` inside the painted text, `Err` with the
  nearest offset outside, the same contract as gpui's
  `TextLayout::index_for_position`.
- **`rects_for_range`** — one box per visual run per row: a link wrapped across
  two rows gives two, and a range split by a direction change gives one per
  piece.
- **`left_edge_of`** — the top-left of a range's visual box, for seating an
  inline raster (a formula, an image) over the spacer it reserved. Anchoring to
  `range.start` instead would be the **right** edge in an RTL line, painting
  the raster over the neighbouring words.

---

## Threading

`VisualMap` and its lookups are plain data — any thread. `shaped::*`,
`layout_rows`, `paint_row` and the elements need a gpui `Window` and belong on
the UI thread.
