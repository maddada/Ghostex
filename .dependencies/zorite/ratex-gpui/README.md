# ratex-gpui

[![crates.io](https://img.shields.io/crates/v/ratex-gpui.svg)](https://crates.io/crates/ratex-gpui) [![docs.rs](https://docs.rs/ratex-gpui/badge.svg)](https://docs.rs/ratex-gpui) [![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Math for [GPUI](https://www.gpui.rs/), built on the
[RaTeX](https://crates.io/crates/ratex-parser) typesetting engine. RaTeX parses,
lays out, and rasterizes LaTeX; this crate adds the display and editing layer.

Two parts, usable independently:

1. **Rendering** (the `render` module): turn a LaTeX string into a
   `gpui::RenderImage`, or into a PNG or self-contained SVG for export.
2. **A structural editor**: `MathEditor`, a gpui view that edits a formula as a
   two-dimensional structure in the style of MathQuill. A fraction is a stacked
   box and the caret moves into its numerator, rather than along a line of
   LaTeX. It serializes back to LaTeX on demand.

The editing core (`editor::{model, cursor, geometry, input, latex}`) has no GUI
dependency; `editor::view` is the gpui glue on top.

The complete API reference is in [API.md](API.md).

## Features

- **2-D structural editing** — fractions, roots (incl. nth-roots with an editable degree),
  super/subscripts, big operators with limits (∫ ∑ ∏ lim), and matrices (add/remove
  rows + columns)
- **Delimiters** that wrap a selection or insert an empty pair: `()` `[]` `{}` `||` `‖‖`
  `⟨⟩` `⌊⌋` `⌈⌉`
- **`\command` autocomplete** (~100 commands: Greek, relations, operators, arrows,
  set/logic) plus a click-to-insert **palette**
- **Mouse + keyboard** — click-to-place caret, drag / double / triple-click selection,
  arrow navigation that flows *out* of the formula at its edges
- **Undo / redo**, per-formula **alignment**, selection **wrapping** (into a fraction,
  root, or delimiter)
- **LaTeX in, LaTeX out** — seed from a string, serialize back with `to_latex()`
- **Export** — crisp `RenderImage`, PNG, or self-contained SVG (KaTeX glyph paths embedded)
- **Host-themed** — all chrome colors come from a host-supplied `MathTheme`

**Coverage:** rendering ≈ KaTeX (RaTeX's parser is a KaTeX port, ~660 commands), so
anything KaTeX renders, this renders. The 2-D *editor* models a practical subset —
constructs outside it (accents, math fonts, multi-line environments, …) still render
perfectly but **degrade if opened in the structural editor** (the wrapper is dropped on
commit); edit those as raw LaTeX instead. Details in [API.md](API.md) under `parse_latex`.

## Adding the dependency

Published on [crates.io](https://crates.io/crates/ratex-gpui):

```toml
# Full crate (default `editor` feature: MathEditor + the editing modules):
ratex-gpui = { version = "0.4" }

# Render-only build — LaTeX → image / PNG / SVG plus the parse/serialize core,
# without the gpui view + editing machinery:
ratex-gpui = { version = "0.4", default-features = false }
```

## Quick start

### Render a formula to an image

```rust
use gpui::{img, px};
use ratex_gpui::render;

let r = render::render_latex(
    r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
    22.0,                    // font size (px/em)
    window.scale_factor(),   // device-pixel ratio
    theme.text_color,        // tint
);
match r {
    Some(r) => img(r.image).w(px(r.width)).h(px(r.height)).into_any_element(),
    None => /* parse/layout failed — fall back to the raw source */ todo!(),
};
```

Rendering is **pure CPU** (no `Window`/GPU), so it's safe to run off the main thread and
cache — render once per formula, keyed by the LaTeX.

### Host the interactive editor

`MathEditor` is a normal gpui view. Create it, focus it, place it in your tree, and
listen for `MathNav` events:

```rust
use ratex_gpui::{MathAlign, MathEditor, MathNav, MathTheme};

let editor = cx.new(|cx| {
    MathEditor::from_latex(r"x^2 + 1", 22.0, true, MathAlign::Center, my_theme(), cx)
});
window.focus(&editor.read(cx).focus_handle(), cx);

cx.subscribe(&editor, |this, editor, ev: &MathNav, cx| match ev {
    // Arrowing past an edge (or Esc) — commit and take focus back.
    MathNav::Exit { after } => {
        let latex = editor.read(cx).to_latex();
        // splice `latex` back into your document, move your text caret out
    }
    // Right-click while editing — show your copy-LaTeX / export menu.
    MathNav::ContextMenu { position } => { /* … */ }
});
```

## Hosting notes

The crate is a leaf: it edits one formula and tells you (via `MathNav`) when to take
focus back. When hosting:

1. **Drop your key context** while the editor is focused — it lives *inside* your
   element, so your own keybindings would otherwise swallow arrows/typing before they
   reach it.
2. **Commit** on `MathNav::Exit`, on focus-loss (click-away), or when the user opens
   another formula: read `to_latex()` (+ `align()`) and splice it back.
3. On `MathNav::ContextMenu`, show your menu at the given window position;
   `set_align(…)` re-justifies live if you offer alignment there.

This is exactly how its host application (Zorite) drives it for both display `$$…$$`
blocks and inline `$…$` formulas.

## Built on RaTeX

Typesetting comes from the [RaTeX](https://crates.io/crates/ratex-parser) crates —
`ratex-parser` (parse), `ratex-layout` (box layout), `ratex-render` (raster, KaTeX fonts
embedded), and `ratex-svg` (vector export). This crate adds the editing model + cursor +
hit-testing and the gpui view/render adapters on top. GPUI comes from crates.io as the
`gpui-pre` family (`gpui = { package = "gpui-pre", version = "0.3" }`); keep your app on
the same `gpui-pre` version as this crate so there is one gpui graph.

## License

MIT. (The Zorite app itself is GPL-3.0-or-later.)
