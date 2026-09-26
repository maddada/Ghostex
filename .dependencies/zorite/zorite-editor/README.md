# zorite-editor

[![crates.io](https://img.shields.io/crates/v/zorite-editor.svg)](https://crates.io/crates/zorite-editor) [![docs.rs](https://docs.rs/zorite-editor/badge.svg)](https://docs.rs/zorite-editor) [![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A multi-line text editor for [GPUI](https://www.gpui.rs/) with live Markdown
preview. It is the editor behind [Zorite](https://github.com/packetThrower/zorite)'s
notes.

It is built directly on GPUI's text primitives: an `EntityInputHandler` for
keyboard and IME input, `shape_line` for per-line shaping, and a custom
`Element` that lays out and paints the lines, caret, and selection. It depends
on `gpui` and `unicode-segmentation`, plus one sibling crate,
[`zorite-markdown`](../zorite-markdown/README.md) with default features off,
for its dependency-free `syntax` module. That module decides what counts as a
link, an alert, or a table style; sharing it keeps the editor and the reading
view in agreement.

The complete API reference, including the seat/commit protocols, is in
[API.md](API.md).

## Overview

- **Auto-grows** to its content height (no inner scrollbar), so a host can stack
  many editors in one scroll view (e.g. a journal feed).
- **Editing:** insert / backspace / delete / newline, arrow + Home/End +
  word-wise navigation, visual-row up/down, copy / cut / paste, IME, undo / redo
  (coalesced), click + drag selection, double-click word / triple-click line.
  **Lists continue on Enter** (an empty item exits the list), and **bold /
  italic / underline / strikethrough / inline-code / highlight** toggles
  (`cmd`/`ctrl`-`b`/`i`/`u`, `-shift-x`, `-e`, `-shift-h`).
- **Soft-wrap** with content-driven height. A line containing right-to-left
  text is instead broken in *logical* order and laid out row by row via
  [`gpui-bidi`](../gpui-bidi/README.md) — gpui's own wrapping slices the
  already-reordered glyph run, which reads bottom-to-top and splits words.
- **Bidirectional text:** caret, selection, click hit-testing and visual arrow
  movement work in Persian/Hebrew prose, table cells and mixed lines, including
  a Latin word embedded in RTL. Direction comes from each line's *content*, so
  markdown markers (`- [x]`, `> [!NOTE]`) don't decide it.
- **Spell-check squiggles:** the host feeds in misspelled byte ranges
  (`Diagnostic`); a right-click menu offers replacements via a lazy provider.
- **Live-preview Markdown ("WYSIWYG"):** with a `SyntaxStyle` installed, the
  editor styles its own content as you type — headings (variable line height),
  bold / italic / strikethrough, inline code, links / wiki-links / tags
  (clickable, emitting `OpenLink` / `OpenWikiLink`), blockquotes, lists,
  clickable task checkboxes, fenced code blocks, thematic rules, footnotes,
  reference links, `==highlights==`, `<u>` — with the raw Markdown markers
  hidden and revealed only around the caret. **GitHub alerts** render with a colored bar
  and title (Obsidian's foldable `> [!NOTE]-`/`+` collapses on a chevron
  click, the flip written back to the source), and **headings fold** — hover
  one for a chevron that collapses its section (view-local state,
  reveal-on-caret while editing).
- **Anchors & properties:** trailing ` ^block-id` markers dim/hide like other
  syntax; `[[Note#Heading]]` / `[[Note#^id]]` targets display as
  `Note → anchor`; and consecutive `key:: value` lines render as a two-column
  **property panel** (per-key icons via `SyntaxStyle::property_icon`, pill
  values) — clicking or arrowing into one emits `EditProperties` so the host
  can seat an in-place property editor.
- **Block widgets:** standalone images, file chips (e.g. PDF embeds), mermaid
  diagrams, `$$…$$` math blocks, and `![[Note]]` **transclusions** (any
  `AnyView` the host resolves, in a reserved gap) render in place via
  host-supplied providers (raw source under the caret). Mid-text images flow
  as small inline thumbnails; a click emits `PreviewImage`.
- **Math:** display `$$…$$` blocks **and** inline `$…$` formulas typeset via the
  math provider; clicking or arrowing into one emits `EditMath` so the host can
  seat its own 2-D structural editor in the spot the editor reserves
  (`set_editing_block` for a block, `set_editing_inline` in-line).
- **Tables:** rendered as a grid and edited *in the cells* — arrow keys move
  cell-to-cell keeping the column and Enter drops to the row below; host-driven
  column alignment, row/column insert/delete, and whole-table delete.
- **Text color and highlights:** colored text and highlights render in place,
  using the HTML Obsidian writes (`<span style="color:…">`,
  `<mark style="background:…">`). With a selection, the right-click menu shows
  two swatch rows under the format bar — text colors and highlights — plus
  clear and a custom-color swatch that emits `PickColor` for a host picker;
  `color_selection` applies the pick.
- **Link hover:** moving onto a link, tag, or property pill (or off every link)
  emits `HoverLink` with the target and its box, so a host can show a preview.

## Adding the dependency

Published on [crates.io](https://crates.io/crates/zorite-editor):

```toml
[dependencies]
zorite-editor = "0.11"
```

> **gpui version:** the crate depends on GPUI as published on crates.io — the
> `gpui-pre` family, consumed under the name `gpui` (`gpui = { package = "gpui-pre",
> version = "0.3" }`). Every `gpui-pre` release is a different Zed snapshot, so your
> app must resolve to the **same** `gpui-pre` version as this crate (one gpui graph);
> pin it in your `Cargo.lock` and move both together.

## Quick start

```rust
use gpui::*;
use zorite_editor::{EditorState, EditorEvent};

// 1. Once at startup, bind the editing keys (scoped to the editor's key context).
zorite_editor::bind_keys(cx);

// 2. Create the editor entity.
let editor = cx.new(|cx| {
    EditorState::new(window, cx)
        .with_placeholder("Type here…")
        .with_text("# Hello\n\nSome **markdown**.")
});

// 3. Focus it to start editing.
editor.update(cx, |ed, cx| ed.focus(window, cx));

// 4. React to edits (e.g. save, re-run spell-check).
cx.subscribe(&editor, |_host, editor, event: &EditorEvent, cx| {
    if let EditorEvent::Changed = event {
        let text = editor.read(cx).text().to_string();
        // …save `text`…
    }
})
.detach();
```

## Rendering

`EditorState` is a GPUI entity that renders itself, so a host just renders it as
a child. The editor has **no chrome of its own** and inherits the ambient text
style (size + color) from its wrapper — set those on the parent:

```rust
div()
    .text_size(px(16.))
    .text_color(rgb(0xe6e6e6))
    .child(editor.clone())
```

## Markdown live preview — opt in at runtime, not compile time

The editor is a **text editor first**: create it, focus it, and it edits plain
text. The whole Markdown/WYSIWYG side is dormant until the host installs a
`SyntaxStyle` — there is deliberately **no cargo feature** for it, because
its only compile-time cost is the dependency-free `zorite_markdown::syntax`
module, and every markdown code path is dead (and dead-code-eliminated) unless
these calls are made:

```rust
editor.update(cx, |ed, cx| {
    // 1. REQUIRED for any live styling: colors + fonts, from your theme.
    ed.set_markdown_style(my_syntax_style(), cx);

    // 2. OPTIONAL, feature by feature — skip any you don't need:
    // standalone image rows render via your image pipeline…
    ed.set_block_image_provider(|src| my_images.get(src));
    // …file chips (e.g. PDF embeds) show a label and click through OpenLink…
    ed.set_block_chip_provider(|src| is_pdf(src).then(|| file_name(src).into()));
    // …standalone ![[Note]] lines render the host's transclusion view in a
    // reserved gap of the given height…
    ed.set_embed_provider(|target| my_embeds.get(target));
    // …```mermaid fences render as diagrams (raster + logical w/h)…
    ed.set_block_mermaid_provider(|src| my_mermaid.get(src));
    // …$$…$$ blocks and inline $…$ render typeset (raster + logical w/h; set
    // the em the rasters were typeset at so inline formulas scale to your text)…
    ed.set_block_math_provider(|src| my_math.get(src));
    ed.set_block_math_em(22.0);
    // …and fenced code with a language tag colors its tokens.
    ed.set_code_highlighter(|lang, code| my_highlighter.highlight(lang, code));
});
```

Then handle the interaction events (see [Events](#events)): `OpenLink` /
`OpenWikiLink` for clicks on links, chips, and bare URLs; `EditMath` /
`MathMenu` if you host a math editor. Alert titles (`> [!NOTE]` …) can show
icons by supplying SVG asset paths in `SyntaxStyle::alert_icons`.

Everything renders **raw under the caret** (move onto a construct to edit its
source), and `clear_markdown_style` reverts the whole surface to plain text at
runtime — e.g. a user's "WYSIWYG off" toggle.

## Key bindings

`bind_keys(cx)` binds these in the editor's `"Editor"` key context (so they don't
shadow the host's shortcuts). `cmd-*` is macOS; `ctrl-*` is the cross-platform
equivalent.

| Keys | Action |
| --- | --- |
| typing, `backspace`, `delete`, `enter` | edit text |
| `←` `→` `↑` `↓`, `home`, `end` | move caret |
| `alt-←` / `alt-→` | word left / right |
| `shift-` + any move | extend selection |
| `cmd-a` | select all |
| `cmd-b` / `cmd-i` / `cmd-u` / `cmd-e` | bold / italic / underline (`<u>`) / inline code (toggle on selection) |
| `cmd-shift-x` / `cmd-shift-h` | strikethrough / `==highlight==` (toggle on selection) |
| `tab` / `shift-tab` | indent / outdent (list-aware) |
| `cmd-c` / `cmd-x` / `cmd-v` | copy / cut / paste |
| `cmd-z` / `cmd-shift-z` (`ctrl-y`) | undo / redo |
| `ctrl-cmd-space` | macOS character palette |
| `escape` | dismiss the right-click suggestions menu |

`tab`/`shift-tab` indent or outdent the caret's list item by `set_tab_indent`
spaces (or insert/remove that many spaces elsewhere), and move between cells when
the caret is in a table. **Enter** continues a list or task (an empty item exits)
and, inside a table, moves to the cell below; the **arrow keys** walk a table
cell-by-cell, keeping your column.

## Events

Subscribe with `cx.subscribe(&editor, …)`. `EditorEvent` is everything the
editor asks the host to do:

| Variant | Meaning |
| --- | --- |
| `Changed` | The text changed via a user edit (not programmatic `set_text`). |
| `OpenLink(src)` | A chip / link / bare URL was clicked — open it. |
| `OpenWikiLink(target)` | A `[[wiki-link]]` / `#tag` / property pill was clicked — navigate. |
| `SelectionChanged` | Caret moved without a text change. |
| `EditMath { … }` | The caret entered a formula — seat a structural editor. |
| `MathMenu { … }` | A formula was right-clicked — show a context menu. |
| `EditProperties { … }` | A property panel was entered — seat a property editor. |
| `PreviewImage(src)` | An inline image thumbnail was clicked — show a preview. |
| `HoverLink(hit)` | The pointer moved onto a link (target + box) or off every link — show or hide a preview. |
| `PickColor { … }` | The selection menu's custom-color swatch was clicked — show a picker, then call `color_selection`. |

The exact fields, host obligations, and the seat/commit protocol behind
`EditMath` / `EditProperties` are in [API.md](API.md#enum-editorevent).

## Demo

A standalone window wired to the real OS spell checker (via the
[`os-spellcheck`](../os-spellcheck) crate), live Markdown styling, a PDF-style
file chip, and styled tables:

```sh
cargo run -p zorite-editor --example demo
```

Type to watch the spell squiggles update; right-click a flagged word for
suggestions.

## Notes & caveats

- **Main thread:** like all GPUI UI, drive the editor on the main thread.
- **No styling without a `SyntaxStyle`:** absent `set_markdown_style`, the editor
  is a plain-text editor (with spell squiggles if diagnostics are fed in).
- The editor caches the **last paint's layout** for hit-testing, `bounds_for_offset`,
  and IME — those return `None`/defaults before the first paint.

## License

MIT. (The Zorite app itself is GPL-3.0-or-later.)
