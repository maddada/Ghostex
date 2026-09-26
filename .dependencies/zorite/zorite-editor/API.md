# zorite-editor API

The complete public API of [`zorite-editor`](README.md) — every exported item,
with its signature, parameters, return contract, edge cases, and cost. For the
what-and-why (the three views, quick start, the WYSIWYG opt-in recipe), see
the [README](README.md).

## Public API at a glance

Everything below is the complete public surface — if it isn't listed here, it
isn't public. (`SyntaxStyle`, `AlertIcons`, `MathAlign`, and `PropertyIconFn`
are defined in this crate's `markdown_syntax` module and re-exported at the
crate root; nothing from `zorite-markdown` is re-exported.)

| Item | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| [`bind_keys`](#fn-bind_keys) | fn | `fn bind_keys(cx: &mut App)` | Bind the editing keys (once, at startup) |
| [`LINE_HEIGHT_RATIO`](#const-line_height_ratio) | const | `const LINE_HEIGHT_RATIO: f32 = 1.45` | Row height as a multiple of the font size |
| [`EditorState::set_clipboard_writer`](#editorstateset_clipboard_writer) | method | `fn set_clipboard_writer(&mut self, writer: ClipboardWriter)` | Host-owned Copy/Cut clipboard write |
| [`EditorState::copy_plain`](#editorstatecopy_plain) | method | `fn copy_plain(&mut self, window: &mut Window, cx: &mut Context<Self>)` | Copy the selection as raw markdown only |
| [`EditorState::set_search`](#editorstateset_search) | method | `fn set_search(&mut self, matches: Vec<Range<usize>>, active: Option<usize>, cx)` | Find-match highlights behind the text |
| [`EditorState::offset_screen_top`](#editorstateoffset_screen_top) | method | `fn offset_screen_top(&self, offset: usize) -> Option<Pixels>` | Window-space top of a byte offset's row |
| [`find_in_source`](#fn-find_in_source) | fn | `fn find_in_source(content: &str, query: &str) -> Vec<Range<usize>>` | Case-insensitive source match ranges |
| [`ClipboardWriter`](#type-clipboardwriter) | type alias | `Rc<dyn Fn(&str, &mut App)>` | The Copy/Cut write hook |
| [`EditorState::row_layout`](#editorstaterow_layout) | method | `fn row_layout(&self) -> Vec<(Pixels, Pixels)>` | Per-line (top, first-wrap-row height) for host gutters |
| [`mermaid_sources`](#fn-mermaid_sources) | fn | `fn mermaid_sources(content: &str) -> Vec<SharedString>` | Every ` ```mermaid ` block's source, for pre-rendering |
| [`paint_doc_icon`](#fn-paint_doc_icon) | fn | `fn paint_doc_icon(x, y, w, h: Pixels, color: Hsla, window: &mut Window)` | The file chips' line-art document glyph |
| [`math_sources`](#fn-math_sources) | fn | `fn math_sources(content: &str) -> Vec<SharedString>` | Every `$$…$$` block's LaTeX, for pre-rendering |
| [`inline_math_sources`](#fn-inline_math_sources) | fn | `fn inline_math_sources(content: &str) -> Vec<SharedString>` | Every inline `$…$` formula's LaTeX, for pre-rendering |
| [`EditorState`](#struct-editorstate) | struct | — | The editor entity (text, caret, undo, layout) |
| [`EditorState::new`](#new) | constructor | `fn new(window: &mut Window, cx: &mut Context<Self>) -> Self` | An empty editor |
| [`EditorState::with_text`](#with_text) | builder | `fn with_text(self, text: impl Into<String>) -> Self` | Start with text (caret at start) |
| [`EditorState::with_placeholder`](#with_placeholder) | builder | `fn with_placeholder(self, text: impl Into<SharedString>) -> Self` | Placeholder shown when empty |
| [`EditorState::text`](#text) | method | `fn text(&self) -> &str` | The document, borrowed |
| [`EditorState::value`](#value) | method | `fn value(&self) -> SharedString` | The document, owned |
| [`EditorState::set_text`](#set_text) | method | `fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>)` | Replace the whole document (programmatic load) |
| [`EditorState::replace_range`](#replace_range) | method | `fn replace_range(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>)` | Splice a range as one undoable edit |
| [`EditorState::cursor`](#cursor) | method | `fn cursor(&self) -> usize` | Caret byte offset |
| [`EditorState::set_cursor`](#set_cursor) | method | `fn set_cursor(&mut self, offset: usize, cx: &mut Context<Self>)` | Place the caret (clamped, snapped) |
| [`EditorState::focus`](#focus) | method | `fn focus(&self, window: &mut Window, cx: &mut Context<Self>)` | Take keyboard focus |
| [`EditorState::bounds_for_offset`](#bounds_for_offset) | method | `fn bounds_for_offset(&self, offset: usize) -> Option<Bounds<Pixels>>` | Window-space caret box at an offset |
| [`EditorState::caret_screen_bounds`](#caret_screen_bounds) | method | `fn caret_screen_bounds(&self) -> Option<Bounds<Pixels>>` | The caret's painted Y range, for scroll-into-view |
| [`EditorState::last_edit_was_keystroke`](#last_edit_was_keystroke) | method | `fn last_edit_was_keystroke(&self) -> bool` | Gate host auto-pairing on this |
| [`EditorState::set_diagnostics`](#set_diagnostics) | method | `fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>, cx: &mut Context<Self>)` | Replace the underlined spans |
| [`EditorState::on_suggest`](#on_suggest) | method | `fn on_suggest(&mut self, provider: impl Fn(&str) -> Vec<String> + 'static)` | Lazy right-click suggestion provider |
| [`EditorState::set_markdown_style`](#set_markdown_style) | method | `fn set_markdown_style(&mut self, style: SyntaxStyle, cx: &mut Context<Self>)` | Turn on WYSIWYG styling |
| [`EditorState::set_code_languages`](#set_code_languages) | method | `fn set_code_languages(&mut self, langs: Vec<SharedString>)` | Languages for the code card's picker |
| [`EditorState::set_scroll_compensator`](#set_scroll_compensator) | method | `fn set_scroll_compensator(&mut self, f: impl Fn(Pixels, &mut Window, &mut App) + 'static)` | Scroll anchoring for async block renders |
| `EditorState::set_grip_inset` | method | `fn set_grip_inset(&mut self, inset: Pixels)` | Shift the block-drag gutter grip left of host gutter chrome (e.g. line numbers) |
| [`ScrollCompensatorFn`](#set_scroll_compensator) | type alias | `Rc<dyn Fn(Pixels, &mut Window, &mut App)>` | The installed compensator's shape |
| [`EditorState::clear_markdown_style`](#clear_markdown_style) | method | `fn clear_markdown_style(&mut self, cx: &mut Context<Self>)` | Back to plain text at runtime |
| [`EditorState::set_block_image_provider`](#set_block_image_provider) | method | `fn set_block_image_provider(&mut self, impl Fn(&str) -> Option<Arc<RenderImage>> + 'static)` | Standalone `![](src)` → decoded image |
| [`EditorState::set_block_chip_provider`](#set_block_chip_provider) | method | `fn set_block_chip_provider(&mut self, impl Fn(&str) -> Option<SharedString> + 'static)` | Classify `![](src)` as a file chip + label |
| [`EditorState::set_embed_provider`](#set_embed_provider) | method | `fn set_embed_provider(&mut self, impl Fn(&str) -> Option<(AnyView, Pixels)> + 'static)` | Standalone `![[target]]` → host view + reserved height |
| [`EditorState::set_block_mermaid_provider`](#set_block_mermaid_provider) | method | `fn set_block_mermaid_provider(&mut self, impl Fn(&str) -> Option<(Arc<RenderImage>, f32, f32)> + 'static)` | ` ```mermaid ` → diagram bitmap + logical w/h |
| [`EditorState::set_block_math_provider`](#set_block_math_provider) | method | `fn set_block_math_provider(&mut self, impl Fn(&str) -> Option<(Arc<RenderImage>, f32, f32)> + 'static)` | `$$…$$` LaTeX → typeset bitmap + logical w/h |
| [`EditorState::set_block_math_em`](#set_block_math_em) | method | `fn set_block_math_em(&mut self, em: f32)` | Provider's em size; enables inline `$…$` |
| [`EditorState::set_code_highlighter`](#set_code_highlighter) | method | `fn set_code_highlighter(&mut self, impl Fn(&str, &str) -> Vec<(Range<usize>, HighlightStyle)> + 'static)` | Token colors for fenced code |
| [`EditorState::set_editing_block`](#set_editing_block) | method | `fn set_editing_block(&mut self, range: Range<usize>, view: AnyView, height: Pixels, cx: &mut Context<Self>)` | Seat a host editor in a block's gap |
| [`EditorState::editing_block_height`](#editing_block_height) | method | `fn editing_block_height(&self) -> Option<Pixels>` | The height reserved for the open block seat |
| [`EditorState::end_editing_block`](#end_editing_block) | method | `fn end_editing_block(&mut self, cx: &mut Context<Self>) -> Option<Range<usize>>` | Unseat it; returns the range to overwrite |
| [`EditorState::set_editing_inline`](#set_editing_inline) | method | `fn set_editing_inline(&mut self, range: Range<usize>, view: AnyView, cx: &mut Context<Self>)` | Seat a host editor over an inline `$…$` span |
| [`EditorState::end_editing_inline`](#end_editing_inline) | method | `fn end_editing_inline(&mut self, cx: &mut Context<Self>) -> Option<Range<usize>>` | Unseat it; returns the range to overwrite |
| [`EditorState::is_math_block_range`](#is_math_block_range) | method | `fn is_math_block_range(&self, range: &Range<usize>) -> bool` | Commit guard: range still starts a `$$` block |
| [`EditorState::is_inline_math_range`](#is_inline_math_range) | method | `fn is_inline_math_range(&self, range: &Range<usize>) -> bool` | Commit guard: range still bounds a `$…$` span |
| [`EditorState::find_math_block`](#find_math_block) | method | `fn find_math_block(&self, source: &str, approx: usize) -> Option<Range<usize>>` | Re-find a `$$` block by LaTeX after offsets shifted |
| [`EditorState::find_inline_math`](#find_inline_math) | method | `fn find_inline_math(&self, latex: &str, approx: usize) -> Option<Range<usize>>` | Re-find an inline `$…$` span by LaTeX |
| [`EditorState::math_align`](#math_align) | method | `fn math_align(&self, block_start: usize) -> MathAlign` | A `$$` block's alignment marker |
| [`EditorState::math_marker_edit`](#math_marker_edit) | method | `fn math_marker_edit(&self, block: Range<usize>, align: MathAlign) -> (Range<usize>, String)` | Fold an alignment change into a commit edit |
| [`EditorState::edit_math_at_caret`](#edit_math_at_caret) | method | `fn edit_math_at_caret(&mut self, cx: &mut Context<Self>)` | Emit `EditMath` for the caret's `$$` block |
| [`EditorState::property_block_at_caret`](#property_block_at_caret) | method | `fn property_block_at_caret(&self) -> Option<(Range<usize>, SharedString)>` | The property block at the caret, for the host's `/property` flow |
| [`EditorState::exit_math`](#exit_math) | method | `fn exit_math(&mut self, block: Range<usize>, after: bool, window: &mut Window, cx: &mut Context<Self>)` | Seat the caret just outside a math block |
| [`EditorState::set_auto_replace`](#set_auto_replace) | method | `fn set_auto_replace(&mut self, impl Fn(&str) -> Option<(Range<usize>, String)> + 'static)` | Word-completion rewrite hook |
| [`EditorState::take_replaced_selection`](#take_replaced_selection) | method | `fn take_replaced_selection(&mut self) -> Option<String>` | Text the last keystroke typed over (consumed) |
| [`EditorState::set_tab_indent`](#set_tab_indent) | method | `fn set_tab_indent(&mut self, spaces: usize)` | Spaces per Tab / list-nesting level |
| [`EditorState::caret_table_align`](#caret_table_align) | method | `fn caret_table_align(&self) -> Option<CellAlign>` | Caret column's alignment (header row only) |
| [`EditorState::set_caret_table_align`](#set_caret_table_align) | method | `fn set_caret_table_align(&mut self, align: CellAlign, cx: &mut Context<Self>)` | Rewrite the `\|---\|` separator |
| [`EditorState::insert_table_row`](#insert_table_row) | method | `fn insert_table_row(&mut self, below: bool, cx: &mut Context<Self>)` | Empty row above/below the caret's |
| [`EditorState::delete_table_row`](#delete_table_row) | method | `fn delete_table_row(&mut self, cx: &mut Context<Self>)` | Delete the caret's row (body only) |
| [`EditorState::insert_table_column`](#insert_table_column) | method | `fn insert_table_column(&mut self, right: bool, cx: &mut Context<Self>)` | Empty column left/right of the caret's |
| [`EditorState::delete_table_column`](#delete_table_column) | method | `fn delete_table_column(&mut self, cx: &mut Context<Self>)` | Delete the caret's column (not the last) |
| [`EditorState::delete_table`](#delete_table) | method | `fn delete_table(&mut self, cx: &mut Context<Self>)` | Delete the whole table block |
| [`EditorState::duplicate_table_row`](#duplicate_table_row) | method | `fn duplicate_table_row(&mut self, cx: &mut Context<Self>)` | Copy the caret's row below itself |
| [`EditorState::copy_table`](#copy_table) | method | `fn copy_table(&mut self, cx: &mut Context<Self>)` | Whole table to the clipboard (markdown) |
| [`EditorState::set_table_style`](#set_table_style) | method | `fn set_table_style(&mut self, name: Option<&'static str>, cx: &mut Context<Self>)` | Rewrite the table's style marker |
| [`EditorState::color_selection`](#inline-color) | method | `fn color_selection(&mut self, kind: ColorKind, rgba: Option<u32>, cx: &mut Context<Self>)` | Color / highlight the selection (or clear it) |
| [`EditorEvent`](#enum-editorevent) | enum | 10 variants | Everything the editor asks the host to do |
| [`SyntaxStyle`](#struct-syntaxstyle) | struct | 25 public fields | Colors + fonts + icon hooks for WYSIWYG — incl. `block_label: Option<Rc<dyn Fn(&str, &str) -> Option<String>>>` (block-link display resolver) + `block_label_gen: u64` (bumped when labels change, re-keys the line cache) + `block_ref_count: Option<BlockRefCountFn>` (id → referencing-page count; >0 paints a superscript badge over the hidden ` ^id` anchor whose click emits `OpenWikiLink("refs:^id")`) |
| [`AlertIcons`](#struct-alerticons) | struct | 5 public fields | SVG asset paths for alert title icons |
| [`Diagnostic`](#struct-diagnostic) | struct | `pub range: Range<usize>` | A flagged (underlined) span |
| [`CellAlign`](#enum-cellalign) | enum | `Left · Center · Right` | A table column's alignment |
| [`MathAlign`](#enum-mathalign) | enum | `Left · Center (default) · Right` | A `$$` block's horizontal alignment |
| [`PropertyIconFn`](#type-propertyiconfn) | type | `Rc<dyn Fn(&str) -> Option<SharedString>>` | Property key → icon asset path |
| [`ColorKind`](#inline-color) | enum | `Text · Highlight` | Which color `color_selection` sets |
| [`TEXT_COLORS`](#inline-color) / [`HIGHLIGHT_COLORS`](#inline-color) | const | `[u32; 7]` (`0xRRGGBBAA`) | The selection menu's swatches |

`EditorState` also implements `Render`, `Focusable`,
`EventEmitter<EditorEvent>`, and `EntityInputHandler` (the IME plumbing) — so
it is a self-rendering GPUI entity: render it as a child, subscribe with
`cx.subscribe(&editor, …)`.

---

## `fn bind_keys`

```rust
pub fn bind_keys(cx: &mut App)
```

Bind the editor's editing keys. Call **once at startup**. All bindings are
scoped to the `"Editor"` key context, so they resolve only while an editor is
focused and never shadow the host's shortcuts. Without this call the editor
still accepts plain typing and mouse input (those arrive via
`EntityInputHandler`), but every action key — arrows, backspace, enter,
copy/paste, undo, bold/italic — is dead.

The bound set (`cmd-*` and its `ctrl-*` twin are both bound, so the same table
works on macOS, Windows, and Linux): backspace/delete/enter; arrows,
home/end; `shift-` + any movement to extend the selection;
`alt-left`/`alt-right` (± `shift`) word-wise; `cmd-a` select all;
`cmd-c`/`cmd-x`/`cmd-v`; `cmd-z` undo, `cmd-shift-z`/`ctrl-y` redo;
`tab`/`shift-tab` indent/outdent; `cmd-b`/`cmd-i`/`cmd-u`/`cmd-e`/
`cmd-shift-x` bold/italic/underline (`<u>`)/inline-code/strikethrough;
`cmd-shift-h` the `==highlight==`;
`ctrl-cmd-space` character palette; `escape` dismisses the built-in
right-click menus.

---

## `const LINE_HEIGHT_RATIO`

```rust
pub const LINE_HEIGHT_RATIO: f32 = 1.45;
```

Row height as a multiple of the editor's font size. The editor derives its row
height from its **own** font (not the ambient `window.line_height()`, which
tracks the host's UI text style and would leave caret and rows mismatched
against differently-sized editor text). Public so a host's scroll math (e.g.
click-to-edit caret prediction) can mirror the editor's row heights exactly.

---

## `fn paint_doc_icon`

```rust
pub fn paint_doc_icon(x: Pixels, y: Pixels, w: Pixels, h: Pixels, color: Hsla, window: &mut Window)
```

Paint the flat, line-art document glyph (a page with a folded top-right corner
and two text lines) the editor draws on its file chips, in `color` at the
given bounds. Stroke-drawn — not a font emoji — so it reads flat and on-theme
at any text size. Public so a host's read-only view can draw the identical
icon on its own file chips (cross-view parity); the editor sizes it
`h = font_size × 0.92`, `w = h × 0.74`.

---

## `EditorState::set_clipboard_writer`

```rust
pub fn set_clipboard_writer(&mut self, writer: ClipboardWriter)
```

Route Copy/Cut through `writer` instead of gpui's plain-string copy. The
writer receives the markdown text the editor would have written (Copy's
renumbered ordered-list form included), so a host can add clipboard flavors
gpui can't express — e.g. rendered HTML beside the plain string. Paste is
unaffected (it still reads gpui's clipboard).

## `EditorState::set_search`

```rust
pub fn set_search(&mut self, matches: Vec<Range<usize>>, active: Option<usize>, cx: &mut Context<Self>)
```

Highlight `matches` (source byte ranges) behind the text — soft yellow, with
`active` in the stronger current-match orange (the reader's browser-style find
colors). The quads reuse the selection's multi-wrap-row geometry, tables
included. An empty vec clears. Host-driven: a find bar computes matches (see
[`find_in_source`](#fn-find_in_source)) and steps `active`.

## `EditorState::offset_screen_top`

```rust
pub fn offset_screen_top(&self, offset: usize) -> Option<Pixels>
```

The window-space top of the row containing byte `offset`, from the last
layout — what a find bar needs to scroll a match into view. `None` before the
first paint.

## `fn find_in_source`

```rust
pub fn find_in_source(content: &str, query: &str) -> Vec<Range<usize>>
```

Case-insensitive occurrences of `query` in `content`, as source byte ranges —
Unicode-aware (comparison happens on lowercased text through an index map back
to original offsets, so `Grüße` matches `grüße`). An empty query matches
nothing.

## `EditorState::copy_plain`

```rust
pub fn copy_plain(&mut self, window: &mut Window, cx: &mut Context<Self>)
```

Copy the selection as the raw markdown **only** — bypassing any
[`ClipboardWriter`](#type-clipboardwriter) — for pasting literal source into
rich surfaces where Copy's extra flavors would win. Same selection extension
and ordered-list renumbering as Copy; the context menu's "Copy as Markdown".

## `type ClipboardWriter`

```rust
pub type ClipboardWriter = Rc<dyn Fn(&str, &mut App)>;
```

The Copy/Cut write hook — `(markdown_text, app)`.

---

## `EditorState::row_layout`

```rust
pub fn row_layout(&self) -> Vec<(Pixels, Pixels)>
```

Per logical line, from the last paint: its top offset within the editor and
its first wrap-row's height — enough for a host-drawn gutter (line numbers)
to align with rows without re-deriving layout. Empty before the first paint.
Rows collapsed by a heading fold show no vertical advance (the next row's top
equals theirs) — a gutter should skip those.

---

## `fn mermaid_sources`

```rust
pub fn mermaid_sources(content: &str) -> Vec<SharedString>
```

The diagram source of every ` ```mermaid ` block in `content`, in document
order. Lets a host pre-render diagrams off-thread **before** the editor's
mermaid provider is consulted, so the provider finds a ready bitmap instead of
returning `None` (raw source) on first paint. Free function — needs no
`EditorState`.

---

## `fn math_sources`

```rust
pub fn math_sources(content: &str) -> Vec<SharedString>
```

The LaTeX source of every display `$$…$$` block in `content` (the text between
the fences), in document order — the pre-render counterpart of
[`mermaid_sources`](#fn-mermaid_sources) for the math provider.

---

## `fn inline_math_sources`

```rust
pub fn inline_math_sources(content: &str) -> Vec<SharedString>
```

The LaTeX of every inline `$…$` formula in `content` — the **inner** LaTeX,
without the `$` delimiters — so a host can pre-render them into the same
LaTeX-keyed store its block math provider reads (inline rendering reuses block
rasters; see [`set_block_math_em`](#set_block_math_em)).

**Guarantees & edge cases** — lines inside fenced code blocks are skipped
(`$…$` is literal there). `$$` fences are not inline formulas.

---

## `struct EditorState`

```rust
pub struct EditorState { /* private */ }
```

The editor: the document text, caret/selection state, undo/redo history, and a
cached layout (the wrapped lines from the last paint) for hit-testing, IME,
and the geometry queries. Renders the **WYSIWYG** view when a
[`SyntaxStyle`](#struct-syntaxstyle) is installed, the **raw** view otherwise.

- **Entity:** create with `cx.new(|cx| EditorState::new(window, cx))`; it
  implements `Render` (render it as a child — it auto-grows to content height,
  no inner scrollbar) and `Focusable`.
- **Events:** `EventEmitter<EditorEvent>` — subscribe with
  `cx.subscribe(&editor, …)`; see [`EditorEvent`](#enum-editorevent).
- **Offsets:** every offset/range in this API is a **byte offset into
  [`text`](#text)**. Mutating methods snap incoming offsets to `char`
  boundaries, so a stale or mid-character offset can't panic.
- **Threading:** a GPUI entity — main thread only, like all GPUI UI.
- **Layout-dependent queries** ([`bounds_for_offset`](#bounds_for_offset),
  [`caret_screen_bounds`](#caret_screen_bounds)) read the **last paint's**
  layout and return `None` before the first paint.

### Construction

#### `new`

```rust
pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self
```

An empty editor: no text, no placeholder, tab indent 4, no markdown style, no
providers. (The `window` parameter is currently unused — kept for the
conventional GPUI constructor shape.) Infallible.

#### `with_text`

```rust
pub fn with_text(self, text: impl Into<String>) -> Self
```

Builder: start with `text`, caret at offset 0. Does not touch undo history
(there is none yet).

#### `with_placeholder`

```rust
pub fn with_placeholder(self, text: impl Into<SharedString>) -> Self
```

Builder: the dimmed placeholder shown while the document is empty.

### Content

#### `text`

```rust
pub fn text(&self) -> &str
```

The whole document, borrowed. Newline-separated; every byte offset in this API
indexes into it.

#### `value`

```rust
pub fn value(&self) -> SharedString
```

The document as an owned `SharedString` (clones the content). Use
[`text`](#text) when a borrow suffices.

#### `set_text`

```rust
pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>)
```

Replace the whole document — a **programmatic load**, not a user edit.

**Guarantees & edge cases**

- Caret resets to offset 0; any selection and IME marked text are dropped.
- **Clears the undo/redo history** — the load isn't undoable to the prior
  document. Use [`replace_range`](#replace_range) to write back an edit that
  should be undoable.
- Does **not** emit [`EditorEvent::Changed`](#changed) (the host initiated the
  change, so echoing it back would loop a save-on-change host).
- Existing diagnostics are *not* cleared — feed a fresh
  [`set_diagnostics`](#set_diagnostics) after loading.

#### `replace_range`

```rust
pub fn replace_range(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>)
```

Replace byte `range` with `text` as **one recorded (undoable) edit**, leaving
the caret after the inserted text. The programmatic-edit counterpart to
[`set_text`](#set_text): it preserves — and extends — the undo history, so a
host writing back a structural edit (e.g. a committed `$$…$$` formula) lands
as a normal undo step instead of clobbering history.

**Parameters**

| Name | Type | Description |
| --- | --- | --- |
| `range` | `Range<usize>` | Byte range to replace. Clamped to the document; snapped to `char` boundaries (start down, end up), so a stale/shifted range can't panic mid-UTF-8. |
| `text` | `&str` | Replacement text (may be empty = deletion). |

**Guarantees & edge cases**

- One undo step; a following keystroke is **not** coalesced into it.
- Diagnostics are remapped (spans after the edit shift; overlapping spans
  drop).
- Does **not** emit `Changed` — the host made the edit.
- Caret lands at `range.start + text.len()`; selection collapses.

### Inline color

```rust
pub enum ColorKind { Text, Highlight }
pub const TEXT_COLORS: [u32; 7];
pub const HIGHLIGHT_COLORS: [u32; 7];
pub fn color_selection(&mut self, kind: ColorKind, rgba: Option<u32>, cx: &mut Context<Self>)
```

Colors the selection with the spellings Obsidian reads: `Some(rgba)`
(`0xRRGGBBAA`) wraps it in `<span style="color:#…">` (`Text`) or
`<mark style="background:#…">` (`Highlight`), replacing any same-kind color
already on it (and, for a highlight, a plain `==…==`); `None` removes that
color. No-op on an empty selection. The two palettes are the swatches the
selection menu shows; its `…` swatch emits [`PickColor`](#pickcolor--highlight-bool-position-pointpixels-)
for a host picker.

The `Highlight` action (⌘⇧H / Ctrl+Shift+H) toggles the plain `==…==`
highlight, like `Bold`/`Italic`/`Strike` toggle theirs.

### Caret & geometry

#### `cursor`

```rust
pub fn cursor(&self) -> usize
```

The caret's byte offset (the moving end of any selection). For hosts that
drive a menu/completion off the caret position.

#### `set_cursor`

```rust
pub fn set_cursor(&mut self, offset: usize, cx: &mut Context<Self>)
```

Place the caret at `offset`, collapsing any selection. Clamped to the document
length and snapped **down** to a `char` boundary, so a raw click offset is
safe to pass. Emits [`SelectionChanged`](#selectionchanged). Only moves the
caret — call [`focus`](#focus) to actually receive keyboard input (e.g. when
entering edit mode from clicked rendered text).

#### `focus`

```rust
pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>)
```

Focus the editor so it receives keyboard input (delegates to its
`FocusHandle`). The complement of `set_cursor` for click-to-edit flows.

#### `bounds_for_offset`

```rust
pub fn bounds_for_offset(&self, offset: usize) -> Option<Bounds<Pixels>>
```

The **window-space** caret box at `offset` (zero-width; the row's height) —
for anchoring a popup (slash menu, toolbar) at a document position.

**Returns** — `None` before the first paint, or if `offset`'s row isn't in the
last paint's layout. Reads cached layout only — cheap; safe to call per frame.

#### `caret_screen_bounds`

```rust
pub fn caret_screen_bounds(&self) -> Option<Bounds<Pixels>>
```

The caret's own painted Y range in window space (anchored at the editor's left
edge), or `None` before the first paint — for a host to scroll the caret into
view. Computed from the layout stored at the last paint, so it's valid for
caret moves that don't change the text (arrow keys, click).

#### `last_edit_was_keystroke`

```rust
pub fn last_edit_was_keystroke(&self) -> bool
```

`true` only when the last content change was a **single typed character or a
single-character backspace** — not a programmatic or multi-char edit (table
ops, paste, [`replace_range`](#replace_range), auto-replace). A host doing its
own auto-pairing gates on this so structural edits don't trip it. See also
[`take_replaced_selection`](#take_replaced_selection).

### Diagnostics & spell-check

#### `set_diagnostics`

```rust
pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>, cx: &mut Context<Self>)
```

Replace the set of flagged spans, each painted with a red wavy underline. The
host computes them (e.g. with [`os-spellcheck`](../os-spellcheck)) and
refreshes on its own schedule — typically per edit; detection is designed to
be the cheap half.

**Guarantees & edge cases** — between refreshes the editor keeps the spans
valid across edits itself: spans after an edit shift by the size delta, spans
overlapping the edited text are dropped (stale), so squiggles don't flicker
off on every keystroke. Out-of-bounds spans are ignored at paint.

#### `on_suggest`

```rust
pub fn on_suggest(&mut self, provider: impl Fn(&str) -> Vec<String> + 'static)
```

Install the replacement-suggestion provider, consulted **only when the user
right-clicks a flagged word** — never in the per-edit pass, because the OS
suggestion call can be slow (a synchronous XPC round-trip on macOS). It
receives the offending word and returns candidates best-first; the editor
shows them in its built-in popup and applies the chosen one as a normal
(undoable, `Changed`-emitting) edit.

### Live Markdown styling

#### `set_markdown_style`

```rust
pub fn set_markdown_style(&mut self, style: SyntaxStyle, cx: &mut Context<Self>)
```

Turn on WYSIWYG (live-preview) markdown styling with the given color/font
palette — call once at setup (or again to re-theme). Everything markdown then
renders live: headings sized, inline styles applied, markers hidden and
revealed only around the caret, lists/quotes/alerts/rules drawn, tables
gridded. Without it the editor is the **raw** view: plain text, spell
squiggles only. The block providers below are each independently optional on
top of this.

#### `set_code_languages`

```rust
pub fn set_code_languages(&mut self, langs: Vec<SharedString>)
```

The languages offered in a code block's language picker — the tag at the
card's top-right (next to its Copy button) opens a scrollable menu of these;
selecting one rewrites the opening fence (` ```lang `) as one undo step.
Supply the host highlighter's grammar set (a `"text"` entry maps to no
language). Empty — the default — leaves the tag click-inert. The Copy button
needs no setup: it writes the block's body through the clipboard writer.

#### `set_scroll_compensator`

```rust
pub fn set_scroll_compensator(&mut self, f: impl Fn(Pixels, &mut Window, &mut App) + 'static)
```

Scroll anchoring: when an ASYNC height change — a math/mermaid/image raster
arriving and collapsing raw source lines into a rendered block — lands above
the window's viewport, the hook receives the height delta so the host can
shift its scroll container's offset by it (`offset.y -= delta`) and the
content being read stays put. Detected in the measure pass (same content
generation as the last paint but different heights ⇒ no edit was involved),
and called before the scroll container places its children, so the
compensation applies in the same frame. Edits never trigger it.

#### `clear_markdown_style`

```rust
pub fn clear_markdown_style(&mut self, cx: &mut Context<Self>)
```

Turn live-preview styling off at runtime — back to plain text (squiggles
only), e.g. the host's "WYSIWYG off" toggle. No-op if styling was already off.
Installed providers stay; they're dormant without a style.

### Block widgets & providers

All providers are host-supplied closures: the **host owns loading, caching,
and rendering**; the editor calls the provider during paint (so it must be
fast — return `None` while something is still loading, and pre-render with the
[`*_sources`](#fn-mermaid_sources) helpers). Every block widget renders only
when the caret is elsewhere — the caret's own row shows raw source for
editing (reveal-on-caret). Providers only take effect while a markdown style
is installed.

#### `set_block_image_provider`

```rust
pub fn set_block_image_provider(
    &mut self,
    provider: impl Fn(&str) -> Option<Arc<RenderImage>> + 'static,
)
```

Resolve a standalone `![](src)` line's `src` to a decoded image; the line then
renders as the image. `None` (still decoding / failed) → the line shows its
raw source. Mid-text images flow as small inline thumbnails through the same
provider; clicking one emits [`PreviewImage`](#previewimage). A rendered
image's corner grip drag-resizes it (writes `{width=N}` back to the source).

#### `set_block_chip_provider`

```rust
pub fn set_block_chip_provider(
    &mut self,
    provider: impl Fn(&str) -> Option<SharedString> + 'static,
)
```

Classify an `![](src)` reference as a **file chip** (e.g. a PDF) and supply
its display label. Such lines render as a clickable chip: left-click emits
[`OpenLink(src)`](#openlink), right-click places the caret to edit. Consulted
before the image provider — `None` means "not a chip, try an image".

#### `set_embed_provider`

```rust
pub fn set_embed_provider(
    &mut self,
    provider: impl Fn(&str) -> Option<(AnyView, Pixels)> + 'static,
)
```

Resolve a standalone `![[target]]` line (Obsidian transclusion) to the host
view that renders the embedded content, plus the **row height to reserve**.
The editor reserves that gap in its layout and paints the `AnyView` there as
an absolute overlay (skipped on the caret's row, where the raw `![[…]]` shows
for editing). The host owns resolution — fetch the target page, render it
(typically with [`zorite-markdown`](../zorite-markdown), whose
`syntax::embed_targets` / `extract_block` / `extract_section` slice `#^id` /
`#Heading` anchors) — and refreshing views when a source page changes; it
estimates and caps the height (long content scrolls inside the view).
`None` → a compact `⧉ target` chip for an unresolved target.

#### `set_block_mermaid_provider`

```rust
pub fn set_block_mermaid_provider(
    &mut self,
    provider: impl Fn(&str) -> Option<(Arc<RenderImage>, f32, f32)> + 'static,
)
```

Resolve a ` ```mermaid ` block's source to a rendered diagram: the bitmap plus
its **logical (display) width and height in px**. The host supplies the
logical size because only it knows the raster's pixel density (see the math
provider below). Pre-render with [`mermaid_sources`](#fn-mermaid_sources);
`None` while rendering → the block shows its raw fenced source.

#### `set_block_math_provider`

```rust
pub fn set_block_math_provider(
    &mut self,
    provider: impl Fn(&str) -> Option<(Arc<RenderImage>, f32, f32)> + 'static,
)
```

Resolve a `$$…$$` block's LaTeX to a typeset equation: the bitmap plus its
**logical (display) width and height in px**. The host supplies the logical
size because it knows the density the raster was typeset at (e.g. a fixed 2×
DPR) — deriving it from texture pixels ÷ window scale factor renders 2× too
large on a 1× display. Pre-render with [`math_sources`](#fn-math_sources).
Editing is the host's, via the [math editing hooks](#math-editing-hooks) and
the [`EditMath`](#editmath) event.

#### `set_block_math_em`

```rust
pub fn set_block_math_em(&mut self, em: f32)
```

Declare the em (font size) the math provider rasterizes at. This **turns on
inline `$…$` rendering**: each inline formula reuses the block raster for the
same LaTeX, scaled by `text_em / em` so it sits at text size, painted over a
reserved gap in the line. Pre-render inline sources with
[`inline_math_sources`](#fn-inline_math_sources). `em <= 0` turns inline
rendering back off.

#### `set_code_highlighter`

```rust
pub fn set_code_highlighter(
    &mut self,
    f: impl Fn(&str, &str) -> Vec<(Range<usize>, HighlightStyle)> + 'static,
)
```

Token colors for fenced code blocks in WYSIWYG: `(language tag, block text) →`
sorted, non-overlapping styled ranges (byte offsets into the block text).
Host-supplied (e.g. a tree-sitter highlighter) so the crate stays engine-free;
absent it, code renders uniformly in `SyntaxStyle::code`.

### Math editing hooks

The seat/commit protocol for hosting a structural (2-D) math editor — and,
via the same block seat, an in-place property editor. Flow: the editor emits
[`EditMath`](#editmath) (or [`EditProperties`](#editproperties)) → the host
builds its editor view and **seats** it (`set_editing_block` /
`set_editing_inline`) → on commit the host **unseats** it (`end_editing_*`),
validates the returned range with a guard (`is_math_block_range` /
`is_inline_math_range`, re-finding via `find_*` if stale), and overwrites the
range with [`replace_range`](#replace_range).

#### `set_editing_block`

```rust
pub fn set_editing_block(
    &mut self,
    range: Range<usize>,
    view: AnyView,
    height: Pixels,
    cx: &mut Context<Self>,
)
```

Begin an in-line structural edit of the block at `range`: the editor reserves
a gap of `height` at the block's spot and paints `view` (the host's editor)
there. Pass the block's currently displayed height so the formula stays put
instead of jumping to a fixed size. The host focuses `view` itself. One block
seat at a time — a second call replaces the first, which is also how a host
resizes the gap when its view grows or shrinks (same `range`, new `height`).

#### `editing_block_height`

```rust
pub fn editing_block_height(&self) -> Option<Pixels>
```

The height last passed to [`set_editing_block`](#set_editing_block) for the
open seat — `None` when nothing is seated. Lets a host that re-reserves on
every change of its view skip the no-op calls.

#### `end_editing_block`

```rust
pub fn end_editing_block(&mut self, cx: &mut Context<Self>) -> Option<Range<usize>>
```

End the block edit (the host committed or cancelled). Returns the seated
block's byte range so the host can overwrite it — `None` if nothing was
seated. Validate with [`is_math_block_range`](#is_math_block_range) before
splicing: another edit may have shifted offsets.

#### `set_editing_inline`

```rust
pub fn set_editing_inline(
    &mut self,
    range: Range<usize>,
    view: AnyView,
    cx: &mut Context<Self>,
)
```

Begin a structural edit of the inline `$…$` span at `range` (absolute bytes):
`view` is overlaid at the formula's painted spot (no gap is reserved — the
formula's own space is reused). The host focuses `view`.

#### `end_editing_inline`

```rust
pub fn end_editing_inline(&mut self, cx: &mut Context<Self>) -> Option<Range<usize>>
```

End the inline edit; returns the span's byte range to overwrite (`None` if
nothing was seated). Validate with
[`is_inline_math_range`](#is_inline_math_range).

#### `is_math_block_range`

```rust
pub fn is_math_block_range(&self, range: &Range<usize>) -> bool
```

Whether byte `range` (half-open) still starts a `$$…$$` block — a commit guard
so a stale/shifted range can't splice the block into the wrong place and
corrupt the document. `false` for out-of-bounds or non-boundary ranges (never
panics).

#### `is_inline_math_range`

```rust
pub fn is_inline_math_range(&self, range: &Range<usize>) -> bool
```

Whether `range` still bounds an inline `$…$` span: a `$` at each end, content
between, no newline, and not a `$$` fence. The inline commit guard. Never
panics on bad ranges.

#### `find_math_block`

```rust
pub fn find_math_block(&self, source: &str, approx: usize) -> Option<Range<usize>>
```

Re-find a `$$…$$` block by its **exact** LaTeX `source`, returned as the byte
range covering both fences — the one nearest to the now-stale byte `approx`
if several blocks share the LaTeX. For re-targeting after a prior formula's
commit shifted offsets. `None` if no block has that source.

#### `find_inline_math`

```rust
pub fn find_inline_math(&self, latex: &str, approx: usize) -> Option<Range<usize>>
```

The inline counterpart of [`find_math_block`](#find_math_block): re-find a
`$…$` span by its exact inner LaTeX (no delimiters), as the absolute byte
range (including the `$`s) nearest `approx`.

#### `math_align`

```rust
pub fn math_align(&self, block_start: usize) -> MathAlign
```

The horizontal alignment of the `$$…$$` block whose byte range starts at
`block_start` — from its `<!-- math:ALIGN -->` marker comment on the line
above, or `MathAlign::Center` (the default, stored as no marker). Lets the
host seed its math editor at the right justification.

#### `math_marker_edit`

```rust
pub fn math_marker_edit(&self, block: Range<usize>, align: MathAlign) -> (Range<usize>, String)
```

Compute the single edit that writes `align`'s marker for the `$$` block at
byte range `block`: the (possibly marker-extended) range to replace, and the
marker **prefix** to prepend to the rewritten block text. The host appends the
committed block text to the prefix and applies one
[`replace_range`](#replace_range) — folding the marker into the commit edit
avoids a separate, range-shifting edit. `Center` → empty prefix (and the
returned range swallows any existing marker line, dropping it).

#### `edit_math_at_caret`

```rust
pub fn edit_math_at_caret(&mut self, cx: &mut Context<Self>)
```

If the caret sits inside a `$$…$$` block, emit
[`EditMath`](#editmath) for it (`at_end: false`, `inline: false`) — so a host
can turn a freshly inserted, empty math block (e.g. a `/math` snippet)
straight into a live editor instead of raw source. No-op outside a block.

#### `property_block_at_caret`

```rust
pub fn property_block_at_caret(&self) -> Option<(Range<usize>, SharedString)>
```

The `key:: value` property block covering the caret's line, as its absolute
byte range + source — `None` outside a block or when no markdown style is
installed (WYSIWYG-only, like [`edit_math_at_caret`](#edit_math_at_caret)).
Lets a host open the property editor on a freshly inserted `/property` line
instead of leaving raw source.

#### `exit_math`

```rust
pub fn exit_math(
    &mut self,
    block: Range<usize>,
    after: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
)
```

Seat the caret on the plain-text line just **before** (`after = false`) or
**after** (`after = true`) the math `block`, and focus the editor — the
keyboard counterpart to clicking away, for when the caret flows out of a
formula's structural editor. Skips the hidden `$$` fence lines (landing on one
would reveal raw source). Emits [`SelectionChanged`](#selectionchanged).

### Auto-replace

#### `set_auto_replace`

```rust
pub fn set_auto_replace(
    &mut self,
    f: impl Fn(&str) -> Option<(Range<usize>, String)> + 'static,
)
```

Install the word-completion rewrite hook, consulted when a **word-boundary
character** (space, punctuation, Enter) completes a word. It receives the
just-finished line's text **up to the boundary** and returns the slice range
to replace plus its replacement — e.g. wrapping a completed page title as
`[[title]]` — or `None` to leave the text alone.

**Guarantees & edge cases**

- The rewrite is its own undo step (one ⌫Z restores the plain word); the caret
  keeps its place after the boundary.
- Never consulted inside fenced code blocks (text there is verbatim), and only
  for single-character insertions — never pastes or IME commits.
- A returned range that is empty or out of the line's bounds is ignored.

#### `take_replaced_selection`

```rust
pub fn take_replaced_selection(&mut self) -> Option<String>
```

The text the most recent keystroke edit **typed over** (its selection), if
any — consumed on read (one read per edit; subsequent calls return `None`).
Lets a host's auto-pair logic tell "opener typed over a selection" (wrap it)
apart from deletions with identical text diffs. Pair with
[`last_edit_was_keystroke`](#last_edit_was_keystroke).

### Indentation

#### `set_tab_indent`

```rust
pub fn set_tab_indent(&mut self, spaces: usize)
```

Spaces inserted per Tab / list-nesting level (the `Indent`/`Outdent` actions).
Clamped to a minimum of 1. Default 4. The host keeps this in sync with its
list-indent setting so nesting is configurable.

### Table editing

The editor renders GFM tables as a grid and edits **inside the cells** (arrows
walk cell-to-cell keeping the column; Enter drops to the cell below; the
built-in right-click menu offers all of the below). These methods let a host
drive the same structural edits from its own UI. Common contract: each is a
**no-op when the caret isn't in a table**; each structural edit is one undo
step, keeps the caret in (or near) its cell, remaps diagnostics, and — unlike
the content setters — **emits [`Changed`](#changed)** (the editor itself
changed the text, so the host must hear about it).

#### `caret_table_align`

```rust
pub fn caret_table_align(&self) -> Option<CellAlign>
```

The alignment of the table column the caret sits in — but only while the caret
is in the table's **header row** (alignment is a per-column property, set once
from the header). `None` otherwise, so it doubles as "should I show the
alignment control?".

#### `set_caret_table_align`

```rust
pub fn set_caret_table_align(&mut self, align: CellAlign, cx: &mut Context<Self>)
```

Set the caret column's alignment by rewriting the table's `|---|` separator
row (`:--` / `:-:` / `--:`); the caret stays put. No-op outside a table or on
the separator row itself.

#### `duplicate_table_row`

```rust
pub fn duplicate_table_row(&mut self, cx: &mut Context<Self>)
```

Duplicate the caret's row directly below itself (a duplicated header lands
below the separator, as the first body row). The caret lands in the copy —
same cell, same in-cell offset. No-op on the separator or outside a table.

#### `copy_table`

```rust
pub fn copy_table(&mut self, cx: &mut Context<Self>)
```

Copy the caret's whole table — its grid source plus any
`<!-- table:STYLE -->` marker line — to the clipboard through the installed
clipboard writer (plain markdown, pasteable anywhere). No-op outside a table.

#### `set_table_style`

```rust
pub fn set_table_style(&mut self, name: Option<&'static str>, cx: &mut Context<Self>)
```

Set the caret table's visual style by rewriting the `<!-- table:STYLE -->`
marker line above its header: `Some("striped" | "header" | "minimal")`
writes/replaces the marker, `None` (Grid — the default style) removes it. One
undo step; the caret stays in its cell. Offered in the built-in right-click
table menu with the current style checked.

#### `insert_table_row`

```rust
pub fn insert_table_row(&mut self, below: bool, cx: &mut Context<Self>)
```

Insert an empty row above (`false`) / below (`true`) the caret's row; the
caret moves into the new row's first cell. From the header row a new row
always lands below the separator (the first body position).

#### `delete_table_row`

```rust
pub fn delete_table_row(&mut self, cx: &mut Context<Self>)
```

Delete the caret's row — **body rows only** (the header and separator stay; a
no-op on them). The caret keeps its cell and in-cell offset, landing on the
row that takes the deleted row's place (or the header if no body rows remain).

#### `insert_table_column`

```rust
pub fn insert_table_column(&mut self, right: bool, cx: &mut Context<Self>)
```

Insert an empty column left (`false`) / right (`true`) of the caret's column —
a cell added to every row, the separator getting a default-left marker. The
caret stays in its cell.

#### `delete_table_column`

```rust
pub fn delete_table_column(&mut self, cx: &mut Context<Self>)
```

Delete the caret's column from every row; the caret stays near where the
column was. No-op on the last remaining column (a table keeps ≥ 1 column).

#### `delete_table`

```rust
pub fn delete_table(&mut self, cx: &mut Context<Self>)
```

Delete the whole table the caret is in — its grid lines plus an optional
`<!-- table:STYLE -->` marker line directly above — joining the surrounding
text. The caret lands where the table was.

---

## `enum EditorEvent`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorEvent { /* 8 variants below */ }
```

Everything the editor asks the host to do. Subscribe with
`cx.subscribe(&editor, |host, editor, event: &EditorEvent, cx| …)`.

### `Changed`

The document text changed via a **user** edit: typing, delete, paste, IME,
applying a spell suggestion, toggling a checkbox or alert fold, an image
resize, or a table structural edit (including the host-driven
[table methods](#table-editing)). **Not** emitted for programmatic
[`set_text`](#set_text) / [`replace_range`](#replace_range) — the host made
those, so echoing would loop a save-on-change host.

**Host obligation:** persist / re-derive whatever tracks the text (save,
re-run spell-check, refresh pre-rendered math).

### `OpenLink(SharedString)`

A file chip, `[text](url)` link, or bare `http(s)://` URL was left-clicked;
the payload is the `src`/URL. A navigation hint — the text is untouched.

**Host obligation:** open it — http(s) externally, files via the host's own
resolution.

### `OpenWikiLink(SharedString)`

A `[[wiki-link]]`, `#tag`, wiki file chip, or property-panel pill was
left-clicked; the payload is the target page title. It may carry a
`#Heading` / `#^id` anchor (split with `zorite_markdown::syntax`'s
`split_heading_anchor` / `split_block_anchor`).

**Host obligation:** navigate to that page (and scroll to the anchor).

### `SelectionChanged`

The caret or selection moved **without** a text change (arrows, click,
[`set_cursor`](#set_cursor), [`exit_math`](#exit_math)).

**Host obligation:** update any caret-anchored affordance — e.g. a
table-alignment toolbar driven by
[`caret_table_align`](#caret_table_align) + [`bounds_for_offset`](#bounds_for_offset).

### `EditMath { range, source, at_end, inline }`

The caret entered a math formula (click, or arrowing into it).

| Field | Type | Meaning |
| --- | --- | --- |
| `range` | `Range<usize>` | The formula's byte range — a `$$…$$` block including both fences, or an inline span including both `$`s. |
| `source` | `SharedString` | The LaTeX between the delimiters, to seed the host's editor. |
| `at_end` | `bool` | Seat the structural editor's caret at the formula's end (entered from below/right or by click) vs its start (from above/left). |
| `inline` | `bool` | `true` for an inline `$…$` span (seat with [`set_editing_inline`](#set_editing_inline), splice `$…$` back); `false` for a `$$…$$` block (full-width gap via [`set_editing_block`](#set_editing_block)). |

**Host obligation:** open a structural editor seeded from `source`, seat it,
and on commit overwrite `range` (guarded — see
[Math editing hooks](#math-editing-hooks)) via
[`replace_range`](#replace_range).

### `MathMenu { source, position }`

A rendered formula was right-clicked. `source` is its LaTeX; `position` the
window-space click point.

**Host obligation:** show a context menu (Copy LaTeX / Export / …) at
`position`.

### `EditProperties { range, source, at_end, row }`

A `key:: value` property panel was clicked, arrowed, word-jumped, or edited
into (Enter inside the block, a backspace/delete join at its edge).

| Field | Type | Meaning |
| --- | --- | --- |
| `range` | `Range<usize>` | Byte range of the whole consecutive `key:: value` block. |
| `source` | `SharedString` | The block's text, to seed the host's property editor. |
| `at_end` | `bool` | `true` = entered from below (focus the last field, caret at the value's end); `false` = from above (focus the first). |
| `row` | `Option<usize>` | The property line's index within the block, when the entry targeted a specific row (a click, Enter, a join) — the host focuses that row. `None` for plain arrow entry (`at_end` decides). |

**Host obligation:** seat an in-place property editor with
[`set_editing_block`](#set_editing_block) and overwrite `range` on commit —
the same seat/commit pattern as [`EditMath`](#editmath) for a block.

### `PreviewImage(SharedString)`

An inline (mid-text) image thumbnail was left-clicked; the payload is its
`src`. The text is untouched.

**Host obligation:** open a full-size preview.

### `HoverLink(Option<(LinkHit, Bounds<Pixels>)>)`

The pointer moved onto an inline link or a property-panel pill (`Some` — the
`zorite_markdown::syntax::LinkHit` and the link's window-space box, from the
last paint's layout) or off every link (`None`). Emitted only when the hovered
link changes, so it is cheap to subscribe to.

**Host obligation:** none. Zorite anchors a hover preview card to the box.

### `PickColor { highlight: bool, position: Point<Pixels> }`

The selection menu's "custom color…" (`…`) swatch was clicked — on the
highlight row (`highlight: true`) or the text-color row; `position` is the
menu's window-space top-left, to anchor a picker at. The selection is
untouched meanwhile.

**Host obligation:** show a color picker and hand the pick to
[`EditorState::color_selection`](#inline-color) with the matching
[`ColorKind`](#inline-color).

---

## `struct SyntaxStyle`

```rust
#[derive(Clone)]
pub struct SyntaxStyle { /* 21 public fields below */ }
```

Colors + monospace font + icon hooks for the live-preview styling, supplied by
the host ([`set_markdown_style`](#set_markdown_style)) so the editor stays
theme- and asset-agnostic. No `Default` — the host builds it from its theme,
every field explicit. All fields are `gpui::Hsla` unless noted.

| Field | Type | Styles |
| --- | --- | --- |
| `marker` | `Hsla` | dimmed syntax markers (`**`, `` ` ``, `[`, `](…)`, `^id`, …) |
| `code` | `Hsla` | inline `` `code` `` (and unhighlighted fenced-code) text |
| `code_bg` | `Hsla` | inline-code background (also the table row-shade tint) |
| `link` | `Hsla` | `[text](url)`, `[[wiki-links]]`, footnote/reference refs |
| `tag` | `Hsla` | `#tags` |
| `quote` | `Hsla` | blockquote text + left border (a muted tone) |
| `alert_note` | `Hsla` | `> [!NOTE]` bar + title |
| `alert_tip` | `Hsla` | `> [!TIP]` bar + title |
| `alert_important` | `Hsla` | `> [!IMPORTANT]` bar + title |
| `alert_warning` | `Hsla` | `> [!WARNING]` bar + title |
| `alert_caution` | `Hsla` | `> [!CAUTION]` bar + title |
| `alert_icons` | `Option<AlertIcons>` | SVG asset paths for the alert title icons; `None` = bold label only |
| `rule` | `Hsla` | thematic break (`---`) divider |
| `mark_bg` | `Hsla` | Highlight background for `==text==` and a bare `<mark>` (a `<mark style="background:…">` uses its own color) |
| `popover_bg` | `Hsla` | built-in menu (table ops, spell suggestions) surface |
| `popover_border` | `Hsla` | menu border |
| `popover_fg` | `Hsla` | menu text |
| `popover_hover` | `Hsla` | menu hovered-row background |
| `popover_divider` | `Hsla` | menu group divider |
| `popover_danger` | `Hsla` | menu destructive rows (Delete …) |
| `mono` | `gpui::Font` | monospace font for inline code + code blocks |
| `property_icon` | `Option<PropertyIconFn>` | property key → icon asset path for the property panel; `None` = no icons |

---

## `struct AlertIcons`

```rust
#[derive(Clone)]
pub struct AlertIcons {
    pub note: SharedString,
    pub tip: SharedString,
    pub important: SharedString,
    pub warning: SharedString,
    pub caution: SharedString,
}
```

Per-kind SVG asset paths for the GitHub-alert title icons, resolved through
the host's `AssetSource` — the crate itself ships no assets. Installed via
`SyntaxStyle::alert_icons`.

---

## `struct Diagnostic`

```rust
#[derive(Clone)]
pub struct Diagnostic {
    pub range: Range<usize>,   // byte range in the document
}
```

A flagged span to underline (red squiggle); `&text[range]` is the offending
word. Fed in via [`set_diagnostics`](#set_diagnostics); suggestions fetched
lazily via [`on_suggest`](#on_suggest).

---

## `enum CellAlign`

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CellAlign { Left, Center, Right }
```

A table column's text alignment, for
[`caret_table_align`](#caret_table_align) /
[`set_caret_table_align`](#set_caret_table_align).

---

## `enum MathAlign`

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MathAlign { Left, #[default] Center, Right }
```

Horizontal alignment of a display `$$…$$` block, chosen per block via a
`<!-- math:left -->` / `<!-- math:right -->` marker comment on the line
directly above it. `Center` is the default (stored as **no** marker, matching
LaTeX display math); standard Markdown viewers ignore the comment. Read with
[`math_align`](#math_align), written via
[`math_marker_edit`](#math_marker_edit).

---

## `type PropertyIconFn`

```rust
pub type PropertyIconFn = std::rc::Rc<dyn Fn(&str) -> Option<gpui::SharedString>>;
```

Maps a property key (`tags`, `status`, …) to an icon asset path the host
serves, or `None` for no icon — host-provided (`SyntaxStyle::property_icon`)
so the crate makes no assumption about which assets exist. `Rc`, so one
resolver is cheaply shared across many editors.
