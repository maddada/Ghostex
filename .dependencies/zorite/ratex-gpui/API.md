# ratex-gpui API

The complete public API of [`ratex-gpui`](README.md) — every exported item, with its
signature, parameters, return contract, edge cases, and cost. For the what-and-why
(structural editing, quick start, hosting the editor), see the [README](README.md).

## Public API at a glance

Everything below is the complete public surface — if it isn't listed here, it isn't
public. The **Feature** column says which Cargo feature an item needs: `—` means always
built; `editor` means it needs the default-on **`editor`** feature (a
`default-features = false` build drops it).

The surface splits into three layers, documented in this order:

1. **Rendering** (`render` module) — LaTeX → typeset raster / PNG / SVG. Pure CPU.
2. **The model core** (`editor::model`, `editor::latex`) — the edit tree and its
   LaTeX (de)serialization. GUI-free, always built.
3. **The structural editor** — the GUI-free editing engine (`editor::cursor`,
   `editor::geometry`, `editor::input`) and the gpui view (`editor::view`:
   [`MathEditor`](#struct-matheditor) + its events and theme). All behind `editor`.

### Rendering (`render` module)

| Item | Kind | Feature | Signature | Purpose |
| --- | --- | --- | --- | --- |
| [`render::Rendered`](#struct-rendered) | struct | — | — | A rasterized formula + its logical size |
| [`render::PAD`](#const-pad) | const | — | `const PAD: f32 = 8.0` | Padding (px) the rasterizer reserves around the glyphs |
| [`render::render_latex`](#render_latex) | fn | — | `fn render_latex(latex: &str, font_size: f32, dpr: f32, color: Hsla) -> Option<Rendered>` | LaTeX → tinted `gpui::RenderImage` |
| [`render::render_row`](#render_row) | fn | — | `fn render_row(row: &Row, font_size: f32, dpr: f32, color: Hsla) -> Option<Rendered>` | Same, from an edit-model `Row` |
| [`render::render_latex_to_png`](#render_latex_to_png) | fn | — | `fn render_latex_to_png(latex: &str, font_size: f32, dpr: f32) -> Option<Vec<u8>>` | LaTeX → PNG file bytes (black on white) |
| [`render::render_latex_to_svg`](#render_latex_to_svg) | fn | — | `fn render_latex_to_svg(latex: &str, font_size: f32) -> Option<String>` | LaTeX → self-contained SVG (glyphs embedded) |

### The model core (`editor::model`, `editor::latex`)

| Item | Kind | Feature | Signature | Purpose |
| --- | --- | --- | --- | --- |
| [`editor::model::Row`](#struct-row) | struct | — | `struct Row { pub atoms: Vec<Atom> }` | A horizontal list of atoms — the editable unit ("slot") |
| [`Row::new`](#rownew) | constructor | — | `fn new() -> Self` | An empty row |
| [`Row::is_empty`](#rowis_empty) | method | — | `fn is_empty(&self) -> bool` | Whether the row has no atoms |
| [`Row::syms`](#rowsyms) | constructor | — | `fn syms(s: &str) -> Self` | A row of single-character symbols (test/convenience) |
| [`Row::to_latex`](#rowto_latex) | method | — | `fn to_latex(&self) -> String` | Serialize to LaTeX (empty row → `\square`) |
| `impl Default for Row` | trait impl | — | `fn default() -> Self` | Identical to [`new`](#rownew) (derived) |
| [`editor::model::Atom`](#enum-atom) | enum | — | — | One row element: a leaf symbol or a structure with child rows |
| [`Atom::to_latex`](#atomto_latex) | method | — | `fn to_latex(&self) -> String` | Serialize one atom to LaTeX |
| [`parse_latex`](#parse_latex) | fn | — | `fn parse_latex(latex: &str) -> Row` | LaTeX → `Row` (best-effort, never errors). Re-exported at the crate root; lives in `editor::latex` |

### The structural editor (feature `editor`, on by default)

| Item | Kind | Feature | Signature | Purpose |
| --- | --- | --- | --- | --- |
| [`editor::cursor::Slot`](#enum-slot) | enum | `editor` | — | Which child row of a structural atom a step descends into |
| [`editor::cursor::Step`](#struct-step) | struct | `editor` | `struct Step { pub atom: usize, pub slot: Slot }` | One descent into a structure's slot |
| [`editor::cursor::Cursor`](#struct-cursor) | struct | `editor` | `struct Cursor { pub path: Vec<Step>, pub index: usize }` | A position in the model: descent path + index |
| [`Cursor::start`](#cursorstart) | constructor | `editor` | `fn start() -> Self` | Cursor at the start of the top row |
| [`Cursor::row`](#cursorrow) | method | `editor` | `fn row<'a>(&self, top: &'a Row) -> &'a Row` | The row the cursor is in |
| [`Cursor::insert`](#cursorinsert) | method | `editor` | `fn insert(&mut self, top: &mut Row, atom: Atom)` | Insert an atom (descends into a structure's first slot) |
| [`Cursor::backspace`](#cursorbackspace) | method | `editor` | `fn backspace(&mut self, top: &mut Row)` | Delete before the cursor, or ascend at a slot start |
| [`Cursor::move_right`](#cursormove_right--cursormove_left) | method | `editor` | `fn move_right(&mut self, top: &Row)` | Move right, walking into/through/out of structures |
| [`Cursor::move_left`](#cursormove_right--cursormove_left) | method | `editor` | `fn move_left(&mut self, top: &Row)` | Mirror of `move_right` |
| [`Cursor::move_up`](#cursormove_up--cursormove_down) | method | `editor` | `fn move_up(&mut self, top: &Row)` | To the vertically-stacked sibling slot (den→num, sub→sup, cell up) |
| [`Cursor::move_down`](#cursormove_up--cursormove_down) | method | `editor` | `fn move_down(&mut self, top: &Row)` | Mirror of `move_up` |
| [`Cursor::matrix_add_row`](#matrix-row--column-editing) | method | `editor` | `fn matrix_add_row(&mut self, top: &mut Row)` | Add an empty matrix row below the caret's |
| [`Cursor::matrix_add_col`](#matrix-row--column-editing) | method | `editor` | `fn matrix_add_col(&mut self, top: &mut Row)` | Add an empty matrix column after the caret's |
| [`Cursor::matrix_remove_row`](#matrix-row--column-editing) | method | `editor` | `fn matrix_remove_row(&mut self, top: &mut Row)` | Remove the caret's matrix row (last row is kept) |
| [`Cursor::matrix_remove_col`](#matrix-row--column-editing) | method | `editor` | `fn matrix_remove_col(&mut self, top: &mut Row)` | Remove the caret's matrix column (last column is kept) |
| [`Cursor::delete_range`](#cursordelete_range) | method | `editor` | `fn delete_range(&mut self, top: &mut Row, lo: usize, hi: usize)` | Delete a selection (atom range) in the cursor's row |
| [`Cursor::wrap_delim`](#the-wrap_-methods) | method | `editor` | `fn wrap_delim(&mut self, top: &mut Row, lo: usize, hi: usize, open: &str, close: &str)` | Wrap a selection in `\left…\right` delimiters |
| [`Cursor::wrap_accent`](#the-wrap_-methods) | method | `editor` | `fn wrap_accent(&mut self, top: &mut Row, lo: usize, hi: usize, accent: &str)` | Wrap a selection under an accent (`\hat`, `\bar`, …) |
| [`Cursor::wrap_sqrt`](#the-wrap_-methods) | method | `editor` | `fn wrap_sqrt(&mut self, top: &mut Row, lo: usize, hi: usize)` | Wrap a selection under a square root |
| [`Cursor::wrap_nth_root`](#the-wrap_-methods) | method | `editor` | `fn wrap_nth_root(&mut self, top: &mut Row, lo: usize, hi: usize)` | Wrap under an nth-root; caret into the degree box |
| [`Cursor::wrap_fraction`](#the-wrap_-methods) | method | `editor` | `fn wrap_fraction(&mut self, top: &mut Row, lo: usize, hi: usize)` | Selection → numerator; caret into the denominator |
| [`editor::geometry::Rect`](#struct-rect) | struct | `editor` | `struct Rect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }` | A rectangle in em units, top-left origin |
| [`editor::geometry::layout_row`](#layout_row) | fn | `editor` | `fn layout_row(row: &Row) -> LayoutBox` | Lay a row out to a RaTeX `LayoutBox` |
| [`editor::geometry::caret_rect`](#caret_rect) | fn | `editor` | `fn caret_rect(top: &Row, cursor: &Cursor) -> Option<Rect>` | The caret's rect (em) for a cursor |
| [`editor::geometry::matrix_rect`](#matrix_rect) | fn | `editor` | `fn matrix_rect(top: &Row, cursor: &Cursor) -> Option<Rect>` | The rect of the matrix the cursor is inside |
| [`editor::geometry::cursor_at`](#cursor_at) | fn | `editor` | `fn cursor_at(top: &Row, x: f64, y: f64) -> Cursor` | Hit-test a click to a cursor position |
| [`editor::geometry::span_at`](#span_at) | fn | `editor` | `fn span_at(top: &Row, x: f64, y: f64) -> (Vec<Step>, usize, usize)` | Hit-test to the atom span under a click (double-click unit) |
| [`editor::geometry::row_len_at`](#row_len_at) | fn | `editor` | `fn row_len_at(top: &Row, x: f64, y: f64) -> (Vec<Step>, usize)` | The clicked row's path + length (triple-click unit) |
| [`editor::input::type_char`](#type_char) | fn | `editor` | `fn type_char(top: &mut Row, cursor: &mut Cursor, ch: char)` | Apply one typed character as a structural edit |
| [`editor::input::commit_command`](#commit_command) | fn | `editor` | `fn commit_command(top: &mut Row, cursor: &mut Cursor, name: &str) -> bool` | Commit a `\name` command at the cursor |
| [`editor::input::commit_command_selecting`](#commit_command_selecting) | fn | `editor` | `fn commit_command_selecting(top: &mut Row, cursor: &mut Cursor, name: &str, sel: Option<(usize, usize)>) -> bool` | Same, wrapping a selection when the command can |
| [`editor::input::delim_pair`](#delim_pair) | fn | `editor` | `fn delim_pair(c: char) -> Option<(&'static str, &'static str)>` | The `(open, close)` pair a typed bracket wraps with |
| [`editor::input::command_matches`](#command_matches) | fn | `editor` | `fn command_matches(prefix: &str) -> Vec<&'static str>` | Autocomplete: command names starting with a prefix |
| [`editor::input::PALETTE`](#const-palette) | const | `editor` | `const PALETTE: &[(&str, &str)]` | The 45-button click-to-insert palette (glyph, command) |
| [`MathEditor`](#struct-matheditor) | struct | `editor` | — | The interactive gpui editor view. Re-exported at the crate root; lives in `editor::view` |
| [`MathEditor::new`](#matheditornew) | constructor | `editor` | `fn new(cx: &mut Context<Self>) -> Self` | An empty standalone editor (48 px/em, floating palette) |
| [`MathEditor::from_latex`](#matheditorfrom_latex) | constructor | `editor` | `fn from_latex(latex: &str, font_size: f32, at_end: bool, align: MathAlign, theme: MathTheme, cx: &mut Context<Self>) -> Self` | An in-line editor seeded from LaTeX |
| [`MathEditor::to_latex`](#matheditorto_latex) | method | `editor` | `fn to_latex(&self) -> String` | Serialize the current formula — call on commit |
| [`MathEditor::align`](#matheditoralign--matheditorset_align) | method | `editor` | `fn align(&self) -> MathAlign` | The current horizontal alignment |
| [`MathEditor::set_align`](#matheditoralign--matheditorset_align) | method | `editor` | `fn set_align(&mut self, align: MathAlign, cx: &mut Context<Self>)` | Re-justify the formula live |
| [`MathEditor::focus_handle`](#matheditorfocus_handle) | method | `editor` | `fn focus_handle(&self) -> FocusHandle` | Focus it so keys reach the editor |
| `impl Render for MathEditor` | trait impl | `editor` | — | A normal gpui view: `editor.clone()` drops into any element tree |
| `impl EventEmitter<MathNav> for MathEditor` | trait impl | `editor` | — | Emits [`MathNav`](#enum-mathnav); subscribe with `cx.subscribe` |
| [`MathNav`](#enum-mathnav) | enum | `editor` | — | Events: `Exit { after }`, `ContextMenu { position }`. Re-exported at the crate root |
| [`MathAlign`](#enum-mathalign) | enum | `editor` | — | `Left` / `Center` (default) / `Right`. Re-exported at the crate root |
| [`MathTheme`](#struct-maththeme) | struct | `editor` | — | Host-supplied chrome + glyph colors. Re-exported at the crate root |
| `impl Default for MathTheme` | trait impl | `editor` | `fn default() -> Self` | A light scheme (for the standalone example) |

**Crate-root re-exports:** `ratex_gpui::parse_latex` (always) and
`ratex_gpui::{MathAlign, MathEditor, MathNav, MathTheme}` (with `editor`) are the same
items as their `editor::latex` / `editor::view` module paths — one item each, two paths.

---

## `struct Rendered`

```rust
pub struct Rendered {
    pub image: Arc<RenderImage>,
    pub width: f32,
    pub height: f32,
}
```

A rasterized formula plus its **logical (pre-DPR) size** in px. `image` is a
`gpui::RenderImage` holding one frame of **straight (non-premultiplied) BGRA** pixels;
the pixel dimensions are `width * dpr` × `height * dpr`, so display it at
`w(px(width)).h(px(height))` and the raster is exactly `dpr`× crisp.

The image includes [`PAD`](#const-pad) logical px of padding on every side (part of
`width`/`height`).

---

## `const PAD`

```rust
pub const PAD: f32 = 8.0;
```

Logical padding (px) RaTeX leaves around the formula in every render — the same value
feeds the rasterizer's `RenderOptions::padding`, so a host converting between image
coordinates and formula coordinates (e.g. seating a caret over the raster) offsets by
this much.

---

## `render_latex`

```rust
pub fn render_latex(latex: &str, font_size: f32, dpr: f32, color: Hsla) -> Option<Rendered>
```

Typeset raw LaTeX to a `gpui::RenderImage` — the display path for a `$$…$$` block.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `latex` | `&str` | The formula source (math mode, no `$` delimiters). |
| `font_size` | `f32` | Type size in px per em. |
| `dpr` | `f32` | Device-pixel ratio to rasterize at (`window.scale_factor()`; `2.0` → crisp on retina). |
| `color` | `Hsla` | Glyph tint — typically the host's text color. |

**Returns** — `Some(Rendered)` on success; **`None` if the LaTeX fails to parse, lay
out, or rasterize** — the host falls back to showing the raw source. Glyphs are painted
flat in `color` on a **transparent** background (RaTeX rasters black-on-white; a pixel's
darkness becomes the glyph alpha), so the formula blends into any theme.

**Guarantees & edge cases**

- Never panics — every stage's error is absorbed into `None`.
- Unsupported commands: RaTeX's parser is a KaTeX port (~660 commands), so coverage ≈
  [KaTeX's support table](https://katex.org/docs/support_table); a command it rejects
  makes the whole formula `None`.
- The alpha recolor assumes straight (non-premultiplied) BGRA — which is what
  `gpui::RenderImage` wants; do not premultiply the result.
- The returned `width`/`height` are logical px (pixel dims ÷ `dpr`) and include
  [`PAD`](#const-pad) on all sides.

**Cost & threading** — pure CPU, no `Window`/GPU: safe to call **off the main thread**
and cache. Internally round-trips through a PNG encode + decode (RaTeX only exposes a
PNG encoder), so it's not free — the host typically renders once per formula into a
cache keyed by the LaTeX and reads the bitmap back on paint.

**Example**

```rust
use gpui::{img, px};
use ratex_gpui::render;

match render::render_latex(r"\frac{1}{2}", 22.0, window.scale_factor(), theme.text_color) {
    Some(r) => img(r.image).w(px(r.width)).h(px(r.height)).into_any_element(),
    None => /* fall back to the raw source */ todo!(),
};
```

---

## `render_row`

```rust
pub fn render_row(row: &Row, font_size: f32, dpr: f32, color: Hsla) -> Option<Rendered>
```

Render an edit-model [`Row`](#struct-row) — exactly
[`render_latex`](#render_latex)`(&row.to_latex(), …)`.

**Parameters** — as [`render_latex`](#render_latex), with `row: &Row` in place of the
LaTeX string.

**Returns** — as [`render_latex`](#render_latex).

**Guarantees & edge cases**

- An **empty row serializes to `\square`** (the visible placeholder box) and renders
  successfully — the editor's empty state is never a blank/`None` image.
- Since the LaTeX comes from the model's own serializer, `None` indicates a serializer ↔
  engine mismatch, not user input error.

**Cost & threading** — as [`render_latex`](#render_latex).

---

## `render_latex_to_png`

```rust
pub fn render_latex_to_png(latex: &str, font_size: f32, dpr: f32) -> Option<Vec<u8>>
```

Render raw LaTeX to **PNG file bytes** — for an "Export PNG" action or the clipboard.

**Parameters** — as [`render_latex`](#render_latex), minus `color`.

**Returns** — `Some(bytes)`: a complete PNG file (magic header included), suitable for
writing directly to a `.png`. **Black glyphs on an opaque white background** — no
recoloring, unlike [`render_latex`](#render_latex). `None` on parse/layout/raster
failure.

**Guarantees & edge cases** — never panics; includes [`PAD`](#const-pad) padding;
`dpr` scales the pixel dimensions (use e.g. `4.0` for a high-res export).

**Cost & threading** — pure CPU, safe off-thread. No decode round-trip (the PNG is the
product).

---

## `render_latex_to_svg`

```rust
pub fn render_latex_to_svg(latex: &str, font_size: f32) -> Option<String>
```

Render raw LaTeX to an **SVG document string**.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `latex` | `&str` | The formula source. |
| `font_size` | `f32` | Type size in px per em (vector output — no `dpr`). |

**Returns** — `Some(svg)`: a **self-contained** SVG — glyph outlines are embedded as
`<path>` elements (KaTeX fonts), never `<text>` referencing font families the viewer
lacks, so it renders correctly anywhere. `None` if the LaTeX fails to parse or lay out.

**Cost & threading** — pure CPU, safe off-thread.

---

## `struct Row`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub atoms: Vec<Atom>,
}
```

A horizontal list of [`Atom`](#enum-atom)s — the editable unit (a "slot"). The whole
formula is one top-level `Row`; structural atoms hold child `Row`s, which are the
nested slots. An empty row is a placeholder box (see
[`to_latex`](#rowto_latex)). `Default` is [`new`](#rownew). GUI-free; always built.

### `Row::new`

```rust
pub fn new() -> Self
```

An empty row. **Parameters** — none. **Returns** — `Row { atoms: vec![] }`.

### `Row::is_empty`

```rust
pub fn is_empty(&self) -> bool
```

Whether the row has no atoms. **Returns** — `self.atoms.is_empty()`.

### `Row::syms`

```rust
pub fn syms(s: &str) -> Self
```

Build a row from a string of **single-character** symbols — a test/convenience helper.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `s` | `&str` | Each `char` becomes one `Atom::Sym` (so `"4x"` → two atoms). |

**Returns** — the row. Multi-character commands (`\alpha`) can't be built this way —
push `Atom::Sym("\\alpha".into())` directly.

### `Row::to_latex`

```rust
pub fn to_latex(&self) -> String
```

Serialize to LaTeX.

**Returns** — the LaTeX source. **An empty row emits `\square`** so RaTeX renders a
visible, layout-occupying placeholder box. Atoms are **space-joined** so a command
symbol (`\alpha`) can't fuse with the next token — math mode collapses the spaces, so
the rendered output is unaffected.

**Guarantees & edge cases**

- The output round-trips: [`parse_latex`](#parse_latex) of the result reconstructs an
  equal tree (unit-tested for symbols, fractions, indexed roots, matrices), and RaTeX
  parses + lays it out (tested for every delimiter pair the editor can produce).
- `\square` in the output means an empty slot, and parses back to one.

**Example**

```rust
assert_eq!(Row::new().to_latex(), r"\square");
assert_eq!(Row::syms("4x").to_latex(), "4 x");
```

---

## `enum Atom`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    /// A single symbol, stored as its LaTeX: "x", "4", "+", "\\alpha", "\\int".
    Sym(String),
    /// \frac{num}{den} — a bar with a box above and below.
    Frac { num: Row, den: Row },
    /// A super/subscript attached to the PRECEDING atom in the row (its base).
    SupSub { sup: Option<Row>, sub: Option<Row> },
    /// \sqrt{radicand} or \sqrt[index]{radicand}.
    Sqrt { radicand: Row, index: Option<Row> },
    /// An accent over an editable base: \hat{base}, \bar{base}, \vec{base}, ….
    Accent { accent: String, base: Row },
    /// Auto-growing delimiters: \left<open> body \right<close>.
    Delim { open: String, body: Row, close: String },
    /// A matrix: a rectangular grid of cells (rows[r][c]), rendered with parentheses.
    Matrix { rows: Vec<Vec<Row>> },
}
```

One element of a [`Row`](#struct-row): a leaf symbol, or a structure with child rows.
The unusual one is `SupSub` — MathQuill-style, it is a **postfix** atom whose base is
the preceding row atom (`x^2` is two atoms: `Sym("x")`, then `SupSub`). The base stays
an ordinary editable atom, and the serializer never has to brace it — `\int` followed
by a `SupSub` serializes as `\int _{0}^{1}`, keeping RaTeX's operator-limit layout.

**Guarantees & edge cases**

- `Delim`'s `open`/`close` are raw LaTeX delimiter tokens (`"("`, `"["`, `r"\{"`,
  `"|"`, `r"\|"`, `r"\langle"`, `r"\lfloor"`, `r"\lceil"` + their closers) — every pair
  the editor produces is round-trip tested through RaTeX.
- `Matrix` serializes as `pmatrix` (parentheses); [`parse_latex`](#parse_latex)
  collapses a parsed `\left( <array> \right)` back into `Matrix`.
- A `SupSub` with both scripts serializes sub-before-sup (`_{i}^{2}`), KaTeX's
  canonical order.

### `Atom::to_latex`

```rust
pub fn to_latex(&self) -> String
```

Serialize one atom. Empty child rows emit `\square` (e.g. an empty fraction is
`\frac{\square}{\square}`). A bare `SupSub` serializes without a base — it only makes
sense inside a row, after its base atom.

---

## `parse_latex`

```rust
pub fn parse_latex(latex: &str) -> Row
```

Parse a LaTeX string into the structural [`Row`](#struct-row) model — the inverse of
[`Row::to_latex`](#rowto_latex). Exported both at the crate root
(`ratex_gpui::parse_latex`) and as `editor::latex::parse_latex`. Reuses RaTeX's own
parser (no second LaTeX tokenizer) and walks its AST into `Row`/`Atom`.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `latex` | `&str` | Math-mode LaTeX (no `$` delimiters). |

**Returns** — the parsed `Row`. **Never errors, never panics**: unparseable input (a
RaTeX parse error) yields an **empty row**, and constructs the model doesn't represent
**degrade** rather than fail.

**Guarantees & edge cases**

- Empty input → empty row. `\square` / `□` (the empty-slot placeholder) parses back to
  an **empty** slot, not a literal symbol — so serialize → parse round-trips.
- **Degradation is lossy by design** (this is what makes editing an arbitrary `$$…$$`
  block safe to *open* but not always safe to *commit*):
  - accents (`\hat`, `\vec`, …), `\overline`/`\underline`, fonts (`\mathbb`, …),
    `\text`, `\color`, sizing/styling wrappers → **the inner content, wrapper
    dropped**;
  - `\binom` (a barless fraction) → its two parts, side by side;
  - spacing, kerns, rules, and anything else unrecognized → **dropped**.
  A subsequent `to_latex()` re-emits without the dropped decoration.
- `{…}` groups flatten into the row (the model has no group atom).
- `\left( <array> \right)` is recognized as a `pmatrix` and becomes `Atom::Matrix`;
  other `\left…\right` pairs become `Atom::Delim`.

**Cost & threading** — pure CPU (one RaTeX parse + a tree walk); GUI-free, safe
off-thread.

**Example**

```rust
use ratex_gpui::parse_latex;

let row = parse_latex(r"\frac{1}{2} + \sqrt{x}");
assert!(!row.is_empty());
let back = row.to_latex();          // "\frac{1}{2} + \sqrt{x}" modulo spacing
```

---

## `enum Slot`

*Feature: `editor`.*

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Num,               // fraction numerator
    Den,               // fraction denominator
    Radicand,          // root body
    Index,             // nth-root degree
    Body,              // delimiter body
    Base,              // accent base
    Sup,               // superscript
    Sub,               // subscript
    Cell(usize, usize) // matrix cell at (row, column)
}
```

Which child row of a structural [`Atom`](#enum-atom) a [`Step`](#struct-step) descends
into. Each variant is only valid on the atom kind that has that slot (e.g. `Num` on
`Frac`, `Index` only on a `Sqrt` whose `index` is `Some`).

---

## `struct Step`

*Feature: `editor`.*

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub atom: usize,
    pub slot: Slot,
}
```

One descent: into `slot` of the structural atom at index `atom` in the parent row.
A [`Cursor`](#struct-cursor)'s `path` is a chain of these from the top row down.

---

## `struct Cursor`

*Feature: `editor`.*

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursor {
    pub path: Vec<Step>,
    pub index: usize,
}
```

A position in the model: a descent path from the top row into nested slots, plus an
`index` **between atoms** (0 = before the first atom, `row.atoms.len()` = after the
last) in the target row. All structural edits live here; every method takes the top
`Row` explicitly — the cursor owns no model.

**The validity contract (applies to every method below):** the cursor must be a valid
position **in the `top` you pass** — a path whose steps name existing atoms with
existing slots, as produced by these methods, [`cursor_at`](#cursor_at), or
`Cursor::start()`. A stale or foreign path **panics** (`unreachable!` in the slot
resolver or an index out of bounds). Don't keep a cursor across model edits made by
anything other than the cursor itself.

**Cost & threading** — all methods are pure pointer-chasing over the tree — O(depth +
row length), no layout, no allocation beyond the edit itself. GUI-free.

### `Cursor::start`

```rust
pub fn start() -> Self
```

Cursor at the start of the top-level row (empty path, index 0). Identical to
`Cursor::default()`.

### `Cursor::row`

```rust
pub fn row<'a>(&self, top: &'a Row) -> &'a Row
```

Resolve the path: the row the cursor is currently in (the `top` itself for an empty
path). Panics on an invalid path (see the validity contract).

### `Cursor::insert`

```rust
pub fn insert(&mut self, top: &mut Row, atom: Atom)
```

Insert `atom` at the cursor.

**Guarantees & edge cases**

- A **structure descends** into its first navigation slot: inserting a `Frac` leaves
  the caret in the (empty) numerator; a `Sqrt` with an `index` descends into the
  **degree** (nav order is index-then-radicand); a `Matrix` into cell (0, 0); a
  `SupSub` into its subscript if present, else the superscript.
- A **leaf** (`Sym`) steps past: `index += 1`.

**Example**

```rust
let (mut top, mut cur) = (Row::new(), Cursor::start());
cur.insert(&mut top, Atom::Frac { num: Row::new(), den: Row::new() });
cur.insert(&mut top, Atom::Sym("a".into()));       // lands in the numerator
assert_eq!(top.to_latex(), r"\frac{a}{\square}");
```

### `Cursor::backspace`

```rust
pub fn backspace(&mut self, top: &mut Row)
```

Delete the atom before the cursor. At a **slot start** (index 0 inside a structure), it
**ascends** out to just before the structure instead — the structure itself is not
deleted (deleting an empty structure outright is a noted future refinement). At the
very start of the top row: no-op.

### `Cursor::move_right` / `Cursor::move_left`

```rust
pub fn move_right(&mut self, top: &Row)
pub fn move_left(&mut self, top: &Row)
```

Horizontal navigation, MathQuill-style. Moving right: past a leaf; **into** a
structure's first slot; from a slot's end **on to** the structure's next slot
(numerator → denominator, subscript → superscript, matrix cells in row-major order);
from the last slot **out** to just after the structure. `move_left` is the exact
mirror (entering a structure lands at the *end* of its *last* slot). At the top row's
boundary: no-op — the [`MathEditor`](#struct-matheditor) view detects that
before-and-after position is unchanged and emits [`MathNav::Exit`](#enum-mathnav).

### `Cursor::move_up` / `Cursor::move_down`

```rust
pub fn move_up(&mut self, top: &Row)
pub fn move_down(&mut self, top: &Row)
```

Vertical navigation between **stacked sibling slots**: denominator ↔ numerator,
subscript ↔ superscript, and matrix cells within a column. The index clamps to the
target row's length. No-op when: not inside a structure, the slot has no stacked
sibling (e.g. a radicand), the sibling isn't present (a sup-only script), or a matrix
move would leave the grid.

### Matrix row / column editing

```rust
pub fn matrix_add_row(&mut self, top: &mut Row)
pub fn matrix_add_col(&mut self, top: &mut Row)
pub fn matrix_remove_row(&mut self, top: &mut Row)
pub fn matrix_remove_col(&mut self, top: &mut Row)
```

Grid edits, valid only while the cursor's final step is a matrix `Cell` — otherwise
each is a **no-op**.

**Guarantees & edge cases**

- `matrix_add_row` inserts an empty row **below** the caret's; the caret moves to the
  new row's first cell. `matrix_add_col` inserts an empty column **after** the caret's
  (in every row); the caret moves into it.
- `matrix_remove_row` / `matrix_remove_col` refuse to remove the **last** row/column
  (no-op), and clamp the caret to a surviving cell (index reset to 0).

### `Cursor::delete_range`

```rust
pub fn delete_range(&mut self, top: &mut Row, lo: usize, hi: usize)
```

Delete atoms `lo..hi` of the cursor's **current row** (a selection), leaving the caret
at `lo`.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `lo`, `hi` | `usize` | Atom range in the cursor's row. `hi` is clamped to the row length; `lo >= hi` (after clamping) is a no-op. |

### The `wrap_*` methods

```rust
pub fn wrap_delim(&mut self, top: &mut Row, lo: usize, hi: usize, open: &str, close: &str)
pub fn wrap_accent(&mut self, top: &mut Row, lo: usize, hi: usize, accent: &str)
pub fn wrap_sqrt(&mut self, top: &mut Row, lo: usize, hi: usize)
pub fn wrap_nth_root(&mut self, top: &mut Row, lo: usize, hi: usize)
pub fn wrap_fraction(&mut self, top: &mut Row, lo: usize, hi: usize)
```

Replace atoms `lo..hi` of the cursor's row with **one structure containing them**:

| Method | Selection becomes | Caret lands |
| --- | --- | --- |
| `wrap_delim` | the delimiter's body (`\left<open> … \right<close>`) | just **after** the delimiter |
| `wrap_accent` | the accent's base (`accent` is the command, e.g. `r"\hat"`) | just **after** the accent |
| `wrap_sqrt` | the radicand | just **after** the root |
| `wrap_nth_root` | the radicand (an empty degree box is added) | **inside the degree**, ready to type e.g. `3` |
| `wrap_fraction` | the numerator (denominator starts empty) | **inside the denominator** |

**Guarantees & edge cases**

- An empty (`lo >= hi`) or out-of-bounds (`hi > row.atoms.len()`) range is a **no-op**
  — model and caret untouched.
- `wrap_delim`'s `open`/`close` are raw LaTeX delimiter tokens (see
  [`Atom::Delim`](#enum-atom)); they are not validated here — pass pairs from
  [`delim_pair`](#delim_pair) or the command table to stay within what RaTeX lays out.

**Example**

```rust
// a b c  →  ( a b ) c, caret between the delimiter and c
cur.wrap_delim(&mut top, 0, 2, "(", ")");
assert_eq!(top.to_latex(), r"\left( a b \right) c");
```

---

## `struct Rect`

*Feature: `editor`.*

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
```

A rectangle in **em units** (1 em = the layout font size), origin at the laid-out
formula's **top-left**, y growing downward. The view scales by the render font size
and offsets by the render origin (+ [`PAD`](#const-pad)) to get window px.

---

## `layout_row`

*Feature: `editor`.*

```rust
pub fn layout_row(row: &Row) -> LayoutBox
```

Lay a row's serialized LaTeX out into a RaTeX `LayoutBox`
(`ratex_layout::layout_box::LayoutBox` — a **public-dependency type**; add
`ratex-layout` to use the result).

**Returns** — the laid-out box tree, with default `LayoutOptions`. If the row's LaTeX
somehow fails to parse, the node list defaults to empty and an empty box is returned —
no panic.

**Cost & threading** — serialize + parse + layout of the **whole formula** on every
call — the same is true of every geometry function below. Fine at per-keystroke rates
for single formulas; don't call in a tight per-pixel loop. GUI-free, safe off-thread.

---

## `caret_rect`

*Feature: `editor`.*

```rust
pub fn caret_rect(top: &Row, cursor: &Cursor) -> Option<Rect>
```

The absolute caret rectangle (em, top-left origin) for `cursor` — a zero-width rect
(`w == 0.0`) spanning the target row's height, positioned so it aligns with
[`render_row`](#render_row)'s raster (both use the display list's baseline).

**Returns** — `Some(rect)` normally; **`None`** when the walk can't locate the caret —
a path step whose atom doesn't map to a layout cell (a model ↔ layout mismatch), in
which case the view hides the caret bar for that frame.

**Guarantees & edge cases**

- Descent is handled for fraction slots, super/subscripts (including operator limits),
  delimiter bodies, radicands, nth-root degrees, and matrix cells. A slot the
  positioning doesn't cover falls back to **a caret on the structure itself** (its
  full height), so the bar never silently vanishes for a valid cursor.
- Between a base and its script (`x‸^2`), the caret sits at the base's right edge.
- The cursor validity contract of [`Cursor`](#struct-cursor) applies.

---

## `matrix_rect`

*Feature: `editor`.*

```rust
pub fn matrix_rect(top: &Row, cursor: &Cursor) -> Option<Rect>
```

The rect (em, top-left origin) of the **matrix box the cursor is inside** — the
structure its final path step descends into. Used to dock the matrix toolbar beside
the grid.

**Returns** — `None` when the cursor's final step isn't a matrix `Cell`, or the walk
can't locate the box; otherwise the matrix's bounding rect (including its
parentheses).

---

## `cursor_at`

*Feature: `editor`.*

```rust
pub fn cursor_at(top: &Row, x: f64, y: f64) -> Cursor
```

Hit-test a click to a [`Cursor`](#struct-cursor) — the inverse of
[`caret_rect`](#caret_rect), sharing the same descent walk so click and caret agree.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `top` | `&Row` | The formula. |
| `x`, `y` | `f64` | Click position in **em**, top-left origin — the same space `caret_rect` returns. |

**Returns** — always a valid cursor (never fails): it descends into whichever slot box
contains the point, then places the caret at the **nearest atom gap** in the reached
row (the gap whose cell midpoint is right of the click; past the last cell → the row
end). A click outside every slot lands in the top row; an empty formula yields
`Cursor::start()`.

---

## `span_at`

*Feature: `editor`.*

```rust
pub fn span_at(top: &Row, x: f64, y: f64) -> (Vec<Step>, usize, usize)
```

Hit-test a click to the **atom span of the cell under it** — the unit a double-click
selects: a single glyph, a base + its script (one visual cell = up to two model
atoms), or a whole structure clicked on its non-slot chrome.

**Returns** — `(path, lo, hi)`: the reached row's path plus the half-open atom range
`lo..hi`. `lo == hi` (empty) when the reached row has no atoms. A click past the row's
end returns the last cell's span.

---

## `row_len_at`

*Feature: `editor`.*

```rust
pub fn row_len_at(top: &Row, x: f64, y: f64) -> (Vec<Step>, usize)
```

The row a click lands in and its atom count — the unit a **triple-click** selects
(select `0..len` of that row).

**Returns** — `(path, row.atoms.len())` for the deepest slot containing the point (the
top row when none does).

---

## `type_char`

*Feature: `editor`.*

```rust
pub fn type_char(top: &mut Row, cursor: &mut Cursor, ch: char)
```

Apply one typed character as a structural edit — the "natural typing" interpreter the
view feeds.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `top` | `&mut Row` | The formula. |
| `cursor` | `&mut Cursor` | The caret; updated in place. |
| `ch` | `char` | The typed character. |

**Returns** — nothing; the edit (if any) is applied in place.

**Guarantees & edge cases** — the trigger table:

| `ch` | Effect |
| --- | --- |
| `/` | opens a fraction; caret into the numerator |
| `^` / `_` | opens a super/subscript **on the preceding atom**; at index 0 (no base) the keystroke is **dropped** — no baseless `^{}` |
| `(` | opens a `( )` delimiter pair; caret inside the body |
| `)` | hops **out** of the innermost delimiter body to just after it (no-op outside one) |
| space | exits the current structure — up one level, to just after it (no-op at top level) |
| anything else | inserted as a literal `Atom::Sym` |

Note `[`, `{`, `|` are *not* triggers here — they insert as literal symbols. The
selection-wrapping path for those is [`delim_pair`](#delim_pair) +
[`Cursor::wrap_delim`](#the-wrap_-methods) (which is how the view treats them).

**Example**

```rust
let (mut top, mut cur) = (Row::new(), Cursor::start());
for ch in "x^2".chars() { type_char(&mut top, &mut cur, ch); }
assert_eq!(top.to_latex(), "x ^{2}");
```

---

## `commit_command`

*Feature: `editor`.*

```rust
pub fn commit_command(top: &mut Row, cursor: &mut Cursor, name: &str) -> bool
```

Commit a typed `\name` (without the backslash) at the cursor — the keyboard path to
the symbol/structure long tail.

**Returns** — `true` if `name` is a known command (edit applied); **`false`** if not —
the model and cursor are untouched, and the caller decides the fallback (drop it, or
insert the literal letters).

**Guarantees & edge cases**

- The table has **96 entries** (~95 distinct names): structures (`frac`, `sqrt`,
  `nthroot`, `matrix`), 8 delimiter pairs (`paren` `bracket` `brace` `abs` `norm`
  `angle` `floor` `ceil`), big operators (`int` `iint` `oint` `sum` `prod` → empty
  lower+upper limit boxes, caret in the **lower**; `lim` → lower only), function names
  (`log ln sin cos tan`), ~40 symbols (relations, binary ops, arrows, set/logic,
  misc), and the full Greek alphabet (24 lower + 10 upper).
- Structure commands descend like [`Cursor::insert`](#cursorinsert): `frac` → caret in
  the numerator, `sqrt` → radicand, `nthroot` → the **degree** box, `matrix` → a 2×2
  grid, caret in (0, 0), delimiters → the body.
- An operator with limits inserts **two atoms** (the operator `Sym` + a postfix
  `SupSub`), matching the model's base-then-script shape.
- Lookup is first-match: the name `angle` maps to the ⟨⟩ delimiter pair; the later
  `\angle`-symbol row under that same name is unreachable here.
- Matching is exact and case-sensitive (`Delta` ≠ `delta`).

---

## `commit_command_selecting`

*Feature: `editor`.*

```rust
pub fn commit_command_selecting(
    top: &mut Row,
    cursor: &mut Cursor,
    name: &str,
    sel: Option<(usize, usize)>,
) -> bool
```

[`commit_command`](#commit_command) with selection awareness — the palette / `\command`
commit the view actually calls.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `sel` | `Option<(usize, usize)>` | A selected atom range `(lo, hi)` in the **cursor's row**, or `None`. |

**Returns** — as [`commit_command`](#commit_command).

**Guarantees & edge cases**

- With a selection, a **wrap-capable** command wraps it instead of inserting empty:
  `frac` → [`wrap_fraction`](#the-wrap_-methods), `sqrt` → `wrap_sqrt`, `nthroot` →
  `wrap_nth_root`, any delimiter → `wrap_delim`, any accent → `wrap_accent`. (Returns `true` even if the range was
  empty/out-of-bounds and the wrap no-opped.)
- Every other command — and any command with `sel: None` — is plain
  `commit_command`: it inserts at the caret, leaving the selection's atoms in place
  (the caller clears the selection highlight).

---

## `delim_pair`

*Feature: `editor`.*

```rust
pub fn delim_pair(c: char) -> Option<(&'static str, &'static str)>
```

The `(open, close)` LaTeX pair a typed bracket wraps a selection in — so `[`, `{`, `|`
over a selection behave like `(`.

**Returns** — `Some` for exactly four characters: `'('` → `("(", ")")`, `'['` →
`("[", "]")`, `'{'` → `(r"\{", r"\}")`, `'|'` → `("|", "|")`; `None` for everything
else (including the norm/angle/floor/ceil pairs, which are command-table-only).

---

## `command_matches`

*Feature: `editor`.*

```rust
pub fn command_matches(prefix: &str) -> Vec<&'static str>
```

Autocomplete: the known command names that start with `prefix`, in table order (the
menu order).

**Guarantees & edge cases** — an empty prefix returns the full table (96 entries, in
which `"angle"` appears twice — the delimiter row and the symbol row share the name);
no matches → empty vec. Case-sensitive prefix match.

---

## `const PALETTE`

*Feature: `editor`.*

```rust
pub const PALETTE: &[(&str, &str)]
```

The curated click-to-insert palette: 40 `(display glyph, command name)` pairs —
structures (`x/y` √ ⁿ√ ▦), the 8 delimiter pairs, big operators, and common
symbols/Greek. The command name feeds
[`commit_command_selecting`](#commit_command_selecting), so the palette and `\command`
typing share one source of truth. Exposed so a host can build its own palette UI.

---

## `struct MathEditor`

*Feature: `editor`. Re-exported at the crate root (`ratex_gpui::MathEditor`); lives in
`editor::view`.*

```rust
pub struct MathEditor { /* private */ }

impl Render for MathEditor { /* … */ }
impl EventEmitter<MathNav> for MathEditor {}
```

The interactive structural editor — a normal gpui view. Create it with
[`new`](#matheditornew) / [`from_latex`](#matheditorfrom_latex) inside `cx.new(…)`,
focus it, place `editor.clone()` in your element tree, and subscribe for
[`MathNav`](#enum-mathnav) events. It owns the model, caret, selection, an in-place
undo/redo history (capped at 200 steps), the `\command` autocomplete state, the
floating palette, the matrix toolbar, and a cached raster of the formula.

Once focused it handles typing (via [`type_char`](#type_char) semantics),
`\command` autocomplete (Enter/Tab commits), arrow navigation, Shift-arrow and
mouse-drag selection, double/triple-click selection, selection wrapping, matrix
row/column editing, undo/redo (`Cmd/Ctrl+Z`, `Cmd/Ctrl+Shift+Z`, `Cmd/Ctrl+Y`), Esc
(emits `Exit`), and right-click (emits `ContextMenu`).

**Threading** — a gpui entity: main thread only, like any view.

**Rasterization note** — the editor re-rasterizes the formula after every edit and
frees the previous image's GPU texture. The raster is produced at a **fixed 2.0
device-pixel ratio**, regardless of the window's actual scale factor.

### `MathEditor::new`

```rust
pub fn new(cx: &mut Context<Self>) -> Self
```

An empty **standalone** editor: 48 px/em, centered, default (light)
[`MathTheme`](#struct-maththeme), with the floating palette and panel background shown
— the configuration the crate's `examples/editor.rs` uses. Caret at the start.

### `MathEditor::from_latex`

```rust
pub fn from_latex(
    latex: &str,
    font_size: f32,
    at_end: bool,
    align: MathAlign,
    theme: MathTheme,
    cx: &mut Context<Self>,
) -> Self
```

An **in-line** editor seeded from `latex` — for editing an existing `$$…$$` block at
its displayed size. In-line means: the formula is justified per `align` at its spot,
and the floating palette + white background are hidden, so entering edit doesn't
visually shift or restyle the block.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `latex` | `&str` | The formula source — parsed with [`parse_latex`](#parse_latex), so unrepresented constructs **degrade** (see its contract) and unparseable input opens an empty editor. |
| `font_size` | `f32` | Px per em — match the block's displayed size. |
| `at_end` | `bool` | `true` → caret at the end of the top row (entered from the right/below); `false` → at the start (from the left/above). |
| `align` | `MathAlign` | Match the display block's justification. |
| `theme` | `MathTheme` | The host's colors. |

**Returns** — the editor, already rasterized. Remember the [`parse_latex`
degradation](#parse_latex): committing a formula that used constructs outside the edit
model re-serializes **without** them.

### `MathEditor::to_latex`

```rust
pub fn to_latex(&self) -> String
```

Serialize the current formula ([`Row::to_latex`](#rowto_latex) semantics — an empty
formula yields `\square`). Call on commit: on [`MathNav::Exit`](#enum-mathnav), on
focus-loss, or when the user opens another formula.

### `MathEditor::align` / `MathEditor::set_align`

```rust
pub fn align(&self) -> MathAlign
pub fn set_align(&mut self, align: MathAlign, cx: &mut Context<Self>)
```

Read / live-set the horizontal justification (in-line mode). `set_align` calls
`cx.notify()` — immediate visual feedback; the host persists the choice alongside the
formula on commit (read `align()` then).

### `MathEditor::focus_handle`

```rust
pub fn focus_handle(&self) -> FocusHandle
```

The editor's focus handle (a clone) — `window.focus(&handle, cx)` so keys reach it.
**The host must drop its own key context while the editor is focused**, or the host's
keybindings swallow arrows/typing before they arrive (see the
[README's hosting notes](README.md)).

**Example**

```rust
use ratex_gpui::{MathAlign, MathEditor, MathNav, MathTheme};

let editor = cx.new(|cx| MathEditor::from_latex(
    r"x^2 + 1", 22.0, /* at_end */ true, MathAlign::Center, my_theme(), cx,
));
window.focus(&editor.read(cx).focus_handle(), cx);
cx.subscribe(&editor, |this, editor, ev: &MathNav, cx| match ev {
    MathNav::Exit { after } => { let latex = editor.read(cx).to_latex(); /* commit */ }
    MathNav::ContextMenu { position } => { /* show copy/export menu */ }
});
```

---

## `enum MathNav`

*Feature: `editor`. Re-exported at the crate root.*

```rust
pub enum MathNav {
    Exit { after: bool },
    ContextMenu { position: Point<Pixels> },
}
```

The signals a hosted [`MathEditor`](#struct-matheditor) emits
(`EventEmitter<MathNav>`) so focus can flow back to the host:

- **`Exit { after }`** — the caret tried to move past a boundary of the formula
  (left/up at the start → `after: false`; right/down at the end → `after: true`), or
  Esc was pressed with nothing to cancel (Esc backs out one layer at a time: a pending
  `\command` first, then a selection, and only then exits, always with `after: true`).
  `after == true` → seat the host's text caret after the formula; `false` → before it.
  The host commits `to_latex()` (+ `align()`) and re-focuses its own editor — like
  arrowing out of a table cell.
- **`ContextMenu { position }`** — the formula was right-clicked while being edited.
  The hosted editor occludes the formula, so the host's own right-click handler can't
  fire; this routes it back out. `position` is **window-space** — show the copy-LaTeX
  / export / align menu there.

---

## `enum MathAlign`

*Feature: `editor`. Re-exported at the crate root.*

```rust
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MathAlign {
    Left,
    #[default]
    Center,
    Right,
}
```

Horizontal justification of an in-line display formula, so it matches the surrounding
block and doesn't jump when entered. `Center` is the default (LaTeX display
convention); the host typically persists only a non-default choice.

---

## `struct MathTheme`

*Feature: `editor`. Re-exported at the crate root.*

```rust
#[derive(Clone, Copy)]
pub struct MathTheme {
    pub fg: Hsla,        // formula glyphs + primary text (button labels)
    pub muted: Hsla,     // secondary text — grips, dropdown rows
    pub panel: Hsla,     // panel + button surfaces (palette, toolbar, dropdown)
    pub border: Hsla,    // panel + button borders
    pub accent: Hsla,    // the caret and active/selected highlights
    pub accent_bg: Hsla, // subtle accent fill — hover, selected row, command preview
}
```

All the editor's chrome colors plus the formula glyph color, supplied by the host so
the editor matches the surrounding app. `fg` doubles as the raster tint — the formula
image is recolored to it (see [`render_latex`](#render_latex)).

`Default` is a light slate/blue scheme, intended for the standalone example — a real
host maps its own palette.
