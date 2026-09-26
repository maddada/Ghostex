//! Zorite's **WYSIWYG** (live-preview) markdown editor — and, without a
//! [`SyntaxStyle`] installed, its **raw**-markdown editor. A from-scratch
//! multi-line text editor for GPUI. (The third view, the read-only
//! **reader**, is the separate `zorite-markdown` crate — the two engines share
//! nothing, so any markdown behavior added here must be checked there and
//! vice versa. See AGENTS.md "The three views".)
//!
//! Host-agnostic — depends only on `gpui` (+ `unicode-segmentation`); no
//! `gpui-component`. Built directly on gpui's text primitives: an
//! [`EntityInputHandler`] for keyboard + IME input, `shape_line` for per-line
//! text shaping, and a custom [`Element`] that lays out + paints the lines,
//! cursor, and selection. The editor **auto-grows** to its content height (no
//! inner scrollbar), so a host can stack many editors in one scroll view.
//! Editing fundamentals: cursor/selection, undo/redo, IME, soft-wrap,
//! clipboard, spell-check diagnostics (squiggles + suggestion menu).
//!
//! WYSIWYG mode is [`EditorState::set_markdown_style`] plus the block
//! providers (`set_block_image_provider` & co). Comments reference its
//! feature milestones by code:
//!
//! - **W1** — inline styling: bold/italic/strike/code/links/wiki-links/tags,
//!   markers dimmed in place (`markdown_syntax::scan_line`).
//! - **W2** — heading font sizes (variable per-line heights).
//! - **W4** — block widgets: **W4a** inline images, **W4b** fenced code
//!   blocks, **W4c** tables (Word-style editing); mermaid + `$$math$$`
//!   rasters ride the same widget path.
//! - **W6** — marker *hiding* with reveal-on-caret: the painted text drops
//!   the syntax markers, and per-row offset maps translate display ↔ source.
//!
//! Usage: create an [`EditorState`] entity and render it; call [`bind_keys`]
//! once at startup so the editing actions resolve while it's focused.

use std::ops::Range;
use std::sync::Arc;

use gpui::{
    App, AvailableSpace, BorderStyle, Bounds, ClipboardItem, Context, Corners, CursorStyle, Edges,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, Font, FontWeight, GlobalElementId, HighlightStyle, Hitbox, HitboxBehavior, Hsla,
    InspectorElementId, InteractiveElement, IntoElement, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, PathBuilder, Pixels,
    Point, Render, RenderImage, ScrollHandle, SharedString, StatefulInteractiveElement, Style,
    Styled, TextRun, UTF16Selection, Window, WrappedLine, actions, div, fill, hsla, point, px,
    relative, rgb, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

mod markdown_syntax;
pub use markdown_syntax::{AlertIcons, MathAlign, PropertyIconFn, SyntaxStyle};

mod tables;
use tables::*;

mod element;
use element::*;

/// Key context the editing actions are scoped to (so they only fire while an
/// editor is focused).
const CONTEXT: &str = "Editor";

actions!(
    zorite_editor,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        Home,
        End,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Newline,
        Paste,
        Copy,
        Cut,
        ShowCharacterPalette,
        Undo,
        Redo,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        Indent,
        Outdent,
        Bold,
        Italic,
        Underline,
        Strike,
        Highlight,
        Code,
        Dismiss,
    ]
);

/// Bind the editor's editing keys. Call once at startup. Bindings are scoped to
/// the editor's key context, so they don't shadow the host's shortcuts.
pub fn bind_keys(cx: &mut App) {
    let ctx = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("shift-up", SelectUp, ctx),
        KeyBinding::new("shift-down", SelectDown, ctx),
        KeyBinding::new("enter", Newline, ctx),
        KeyBinding::new("tab", Indent, ctx),
        KeyBinding::new("shift-tab", Outdent, ctx),
        KeyBinding::new("cmd-a", SelectAll, ctx),
        KeyBinding::new("ctrl-a", SelectAll, ctx),
        KeyBinding::new("cmd-c", Copy, ctx),
        KeyBinding::new("ctrl-c", Copy, ctx),
        KeyBinding::new("cmd-v", Paste, ctx),
        KeyBinding::new("ctrl-v", Paste, ctx),
        KeyBinding::new("cmd-x", Cut, ctx),
        KeyBinding::new("ctrl-x", Cut, ctx),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx),
        KeyBinding::new("cmd-z", Undo, ctx),
        KeyBinding::new("ctrl-z", Undo, ctx),
        KeyBinding::new("cmd-shift-z", Redo, ctx),
        KeyBinding::new("ctrl-shift-z", Redo, ctx),
        KeyBinding::new("ctrl-y", Redo, ctx),
        KeyBinding::new("alt-left", WordLeft, ctx),
        KeyBinding::new("alt-right", WordRight, ctx),
        KeyBinding::new("alt-shift-left", SelectWordLeft, ctx),
        KeyBinding::new("alt-shift-right", SelectWordRight, ctx),
        KeyBinding::new("cmd-b", Bold, ctx),
        KeyBinding::new("ctrl-b", Bold, ctx),
        KeyBinding::new("cmd-i", Italic, ctx),
        KeyBinding::new("ctrl-i", Italic, ctx),
        KeyBinding::new("cmd-e", Code, ctx),
        KeyBinding::new("ctrl-e", Code, ctx),
        KeyBinding::new("cmd-shift-x", Strike, ctx),
        KeyBinding::new("ctrl-shift-x", Strike, ctx),
        KeyBinding::new("cmd-u", Underline, ctx),
        KeyBinding::new("ctrl-u", Underline, ctx),
        KeyBinding::new("cmd-shift-h", Highlight, ctx),
        KeyBinding::new("ctrl-shift-h", Highlight, ctx),
        KeyBinding::new("escape", Dismiss, ctx),
    ]);
}

/// Which inline color [`EditorState::color_selection`] sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorKind {
    /// `<span style="color:…">` around the selection.
    Text,
    /// `<mark style="background:…">` around the selection. (The plain
    /// `==…==` highlight is the theme's tint — the `Highlight` action.)
    Highlight,
}

/// The selection menu's text-color swatches (`0xRRGGBBAA`): mid-tones that
/// read on light and dark themes alike.
pub const TEXT_COLORS: [u32; 7] = [
    0xe5484dff, 0xf76b15ff, 0xd4a017ff, 0x30a46cff, 0x3e63ddff, 0x8e4ec6ff, 0x8b8d98ff,
];

/// The selection menu's highlight swatches (`0xRRGGBBAA`): translucent, so
/// the text under them stays legible on either theme — the same reason the
/// theme's own `mark_bg` carries alpha.
pub const HIGHLIGHT_COLORS: [u32; 7] = [
    0xffd54f80, 0xffab4080, 0xa5d6a780, 0x90caf980, 0xce93d880, 0xf48fb180, 0xbdbdbd80,
];

/// Cap on undo history (full snapshots) to bound memory.
const UNDO_LIMIT: usize = 256;

/// Line height as a multiple of the font size. Derived from the editor's own
/// font (not the ambient `window.line_height()`, which tracks the host's UI text
/// style and would leave the caret/rows mismatched against differently-sized
/// editor text). 1.45 for comfortable reading density while typing (1.25 felt
/// cramped, especially stacking several list rows). Public so a host's scroll
/// math (e.g. Zorite's click-to-edit caret prediction) can mirror row heights.
pub const LINE_HEIGHT_RATIO: f32 = 1.7; // Local change: the Ghostex Docs editor's 1.7 line height.

/// Extra height under each list/task row in WYSIWYG, matching the reader's
/// roomier item gap (its list column uses a 4px inter-item gap) — the one
/// place the reader's look wins over the editor's (see AGENTS.md "Parity
/// direction"). Detected from the raw line, so it's caret-stable.
const LIST_ROW_GAP: f32 = 4.;
/// The gap between a painted bullet/checkbox and its item text — the reader's
/// 8px marker gap, whose roomier indent wins (AGENTS.md "Parity direction").
const LIST_TEXT_GAP: f32 = 8.;
/// Per-space width (px) of one nesting level's indent, matching the reader's
/// `list_indent` sizing (`spaces × 4.5`). A level therefore advances by
/// bullet + gap + this — noticeably wider than the raw source spaces, so the
/// display shifts on reveal-on-caret (the quote inset already set that
/// precedent, just smaller).
const LIST_LEVEL_PER_SPACE: f32 = 4.5;

/// Caret thickness (px) — thin like a native text caret, so it doesn't blend into
/// the first glyph at the start of a line/cell.
const CARET_WIDTH: f32 = 1.0;

/// Horizontal inset (px) of fenced-code-block text from the box's left edge, so
/// code sits inside the padded box rather than flush against it. Mirrors the old
/// renderer's `px(12)` left padding.
const CODE_INSET: f32 = 12.;

/// Vertical padding (px) above the first / below the last line of a fenced code
/// block. Reserved as layout space (a gap in the line tops + total height) so the
/// box doesn't overlap adjacent lines, with no blank line required.
const CODE_PAD: f32 = 8.;

/// Horizontal inset (px) of blockquote text from the editor's left edge, leaving
/// room for the left border (2px) + a gap, matching the reading view's `pl(12)`.
const QUOTE_INSET: f32 = 14.;

/// Vertical padding (px) inside a file chip (e.g. a PDF embed), above + below its
/// label, so the chip box reads as a button rather than a bare line of text.
const CHIP_PAD: f32 = 5.;

/// Total vertical breathing room (px) reserved around an inline image — split
/// above + below — so consecutive images (a bulleted photo list) don't touch.
const IMG_ROW_PAD: f32 = 12.;

/// Extra height (px) a text row gets beyond its tallest inline `$…$` formula, so a fraction
/// has a little breathing room above + below instead of touching the neighbouring rows.
const INLINE_MATH_ROW_PAD: f32 = 6.;

/// Side length (px) of the square drag-to-resize grip painted at an inline
/// image's bottom-right corner (matching the reading view's 14px handle).
const IMG_GRIP: f32 = 14.;

/// Smallest width (px) a drag may shrink an inline image to, so it can't vanish.
const IMG_MIN_W: f32 = 40.;

/// An in-progress drag of an inline image's corner grip: which logical line's
/// `![](src)` is being resized, its display width when the drag began, the
/// pointer x at grab, and the live (preview) width the drag has reached. The
/// image paints at `width` (aspect-preserved) until release writes `{width=N}`.
#[derive(Clone, Copy)]
struct ImageResize {
    line: usize,
    start_width: f32,
    start_x: Pixels,
    width: f32,
}

/// A restorable editor state, for undo/redo. Stores the caret offset (not a
/// selection), so undo/redo place the caret rather than re-selecting text.
#[derive(Clone)]
struct Snapshot {
    content: String,
    caret: usize,
}

/// The last edit's kind, for coalescing a run of edits into one undo step.
/// `Insert(end)` is a single-grapheme insert whose caret ends at `end`.
#[derive(Clone, Copy, PartialEq)]
enum EditKind {
    Insert(usize),
    Delete,
    Other,
}

/// A flagged span (e.g. a misspelling) to underline. The host (e.g. a spell
/// checker) computes these and feeds them in via [`EditorState::set_diagnostics`].
/// Replacement suggestions are fetched lazily when the user right-clicks the
/// span, via the provider set with [`EditorState::on_suggest`] — so detection
/// can stay cheap and run on every edit.
#[derive(Clone)]
pub struct Diagnostic {
    /// Byte range in the document.
    pub range: Range<usize>,
}

/// An open right-click suggestions menu for a diagnostic.
#[derive(Clone)]
struct DiagMenu {
    /// Popup top-left, in window space (rendered on a deferred/anchored layer).
    anchor: Point<Pixels>,
    /// The diagnostic's byte range, replaced when a suggestion is chosen.
    range: Range<usize>,
    suggestions: Vec<SharedString>,
    /// Scroll state of the (capped-height) list, so a thumb can track it.
    scroll: ScrollHandle,
    /// Whether the "Turn into" flyout is open (hover-opened; dies with the menu).
    turn_into: bool,
}

/// A block kind the right-click "Turn into" menu converts between —
/// flat-markdown natural: each conversion is a line-prefix rewrite (fenced
/// kinds wrap/unwrap the block's lines).
#[derive(Clone, Copy, PartialEq, Eq)]
enum TurnKind {
    Text,
    H1,
    H2,
    H3,
    Bullet,
    Numbered,
    Todo,
    Quote,
    Callout,
    Code,
    Math,
}

impl TurnKind {
    const ALL: [TurnKind; 11] = [
        TurnKind::Text,
        TurnKind::H1,
        TurnKind::H2,
        TurnKind::H3,
        TurnKind::Bullet,
        TurnKind::Numbered,
        TurnKind::Todo,
        TurnKind::Quote,
        TurnKind::Callout,
        TurnKind::Code,
        TurnKind::Math,
    ];

    fn label(self, labels: &Labels) -> SharedString {
        match self {
            TurnKind::Text => labels.text.clone(),
            TurnKind::H1 => labels.heading_1.clone(),
            TurnKind::H2 => labels.heading_2.clone(),
            TurnKind::H3 => labels.heading_3.clone(),
            TurnKind::Bullet => labels.bulleted_list.clone(),
            TurnKind::Numbered => labels.numbered_list.clone(),
            TurnKind::Todo => labels.todo.clone(),
            TurnKind::Quote => labels.quote.clone(),
            TurnKind::Callout => labels.callout.clone(),
            TurnKind::Code => labels.code_block.clone(),
            TurnKind::Math => labels.math_block.clone(),
        }
    }
}

/// Canonicalize freshly loaded content: words-attached `$$` forms (mixed
/// lines, words-on-fence-lines) normalize onto their own lines — in memory,
/// persisted on the first edit — so STORED documents render display math
/// exactly like fresh typing and the reader's pre-parse normalization.
/// Unpaired `$$` prose/code is untouched. Both load paths (`with_text`,
/// `set_text`) route through here.
fn normalize_loaded(content: String) -> String {
    if !content.contains("$$") {
        return content;
    }
    match zorite_markdown::syntax::normalize_math_fences(&content) {
        std::borrow::Cow::Owned(n) => n,
        std::borrow::Cow::Borrowed(_) => content,
    }
}

/// If `offset` sits on a collapsed marker line (a table style marker or a
/// math align marker), the offset of the nearest line that reveals nothing
/// when the caret rests there: the table's header, or the line after the
/// math block. Otherwise `offset` unchanged.
fn caret_off_marker_line(content: &str, offset: usize) -> usize {
    let row = content[..offset.min(content.len())].matches('\n').count();
    let line_start = |r: usize| {
        let mut off = 0;
        for (i, l) in content.split('\n').enumerate() {
            if i == r {
                return off;
            }
            off += l.len() + 1;
        }
        content.len()
    };
    if let Some(t) = markdown_syntax::table_regions(content)
        .iter()
        .find(|t| t.marker_line == Some(row))
    {
        return line_start(t.lines.start);
    }
    if let Some(m) = markdown_syntax::math_regions(content)
        .iter()
        .find(|m| m.marker_line == Some(row))
    {
        // Anywhere inside a math region reveals it whole — land after it.
        return line_start(m.range.end);
    }
    offset
}

/// Strip a line's block dressing (heading hashes, list/todo bullet, ordered
/// number, quote `>`), leaving the text a "Turn into" conversion re-prefixes.
fn strip_block_prefix(line: &str) -> &str {
    // Composes the renderer's own recognizers so the strip grammar can't
    // drift from what WYSIWYG classifies (task/list/heading/quote).
    if let Some((p, ..)) = markdown_syntax::task_prefix(line) {
        return &line[p..];
    }
    if let Some((p, ..)) = markdown_syntax::list_prefix(line) {
        return &line[p..];
    }
    if let Some(n) = markdown_syntax::heading_level(line) {
        let after = &line[n as usize..];
        return after.strip_prefix(' ').unwrap_or(after);
    }
    if let Some(p) = markdown_syntax::blockquote_prefix(line) {
        return &line[p..];
    }
    line.trim_start()
}

/// Join `body` lines each carrying the prefix `p(index)` produces — the
/// assembly half of a "Turn into" conversion.
fn prefix_lines(body: &[String], p: impl Fn(usize) -> String) -> String {
    body.iter()
        .enumerate()
        .map(|(i, l)| format!("{}{l}", p(i)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A column edit applied to every row of a table (insert/delete a cell at index).
#[derive(Clone, Copy)]
enum ColEdit {
    Insert(usize),
    Delete(usize),
}

/// An item in the table right-click menu (Word-style table editing).
#[derive(Clone, Copy)]
enum TableMenuAction {
    InsertRowAbove,
    InsertRowBelow,
    DuplicateRow,
    InsertColLeft,
    InsertColRight,
    DeleteRow,
    DeleteColumn,
    AlignLeft,
    AlignCenter,
    AlignRight,
    /// Rewrite the table's `<!-- table:STYLE -->` marker (`None` = the
    /// default Grid, which has no marker).
    SetStyle(Option<&'static str>),
    CopyTable,
    DeleteTable,
}

impl TableMenuAction {
    fn apply(self, editor: &mut EditorState, cx: &mut Context<EditorState>) {
        match self {
            TableMenuAction::InsertRowAbove => editor.insert_table_row(false, cx),
            TableMenuAction::InsertRowBelow => editor.insert_table_row(true, cx),
            TableMenuAction::DuplicateRow => editor.duplicate_table_row(cx),
            TableMenuAction::InsertColLeft => editor.insert_table_column(false, cx),
            TableMenuAction::InsertColRight => editor.insert_table_column(true, cx),
            TableMenuAction::DeleteRow => editor.delete_table_row(cx),
            TableMenuAction::DeleteColumn => editor.delete_table_column(cx),
            TableMenuAction::AlignLeft => editor.set_caret_table_align(CellAlign::Left, cx),
            TableMenuAction::AlignCenter => editor.set_caret_table_align(CellAlign::Center, cx),
            TableMenuAction::AlignRight => editor.set_caret_table_align(CellAlign::Right, cx),
            TableMenuAction::SetStyle(name) => editor.set_table_style(name, cx),
            TableMenuAction::CopyTable => editor.copy_table(cx),
            TableMenuAction::DeleteTable => editor.delete_table(cx),
        }
    }
}

/// Events the editor emits so a host can react. Subscribe with
/// `cx.subscribe(&editor, …)` — e.g. to re-run spell-check after an edit.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorEvent {
    /// The document text changed via a user edit (typing, delete, paste, IME,
    /// applying a suggestion). Not emitted for programmatic `set_text`.
    Changed,
    /// A file chip (e.g. a PDF embed) or an inline `[text](url)` link was
    /// left-clicked — the host opens the `src`/url (http externally, files
    /// via its own resolution). A navigation hint; the text is untouched.
    OpenLink(SharedString),
    /// A `[[wiki-link]]` or `#tag` was left-clicked — the host opens the page
    /// with this title (Logseq semantics, matching the reading view).
    OpenWikiLink(SharedString),
    /// The caret / selection moved without a text change — so a host can update a
    /// caret-anchored affordance (e.g. the table-alignment toolbar).
    SelectionChanged,
    /// The caret entered a `$$…$$` math block (by click, or by arrowing into it): its byte
    /// `range` in the document (covering both fences) and the LaTeX `source` between them, so
    /// the host can open a structural editor and replace the block's text on commit. `at_end`
    /// seats that editor's caret at the formula's end (entered from below/right or by click)
    /// vs its start (from above/left).
    EditMath {
        range: Range<usize>,
        source: SharedString,
        at_end: bool,
        /// `true` for an inline `$…$` span (host splices `$…$` back, seats the editor at the
        /// formula's spot); `false` for a `$$…$$` block (full-width gap).
        inline: bool,
    },
    /// A `$$…$$` math block was right-clicked: the LaTeX source and the window-space click
    /// position, so the host can show a context menu (Copy LaTeX / Export).
    MathMenu {
        source: SharedString,
        position: Point<Pixels>,
    },
    /// A property panel was clicked or arrowed into: the byte `range` of the whole
    /// `key:: value` block and its `source`, so the host can seat an in-place
    /// property editor (via `set_editing_block`) and replace the block's text on
    /// commit — the same seat/commit pattern as [`EditorEvent::EditMath`] for a
    /// `$$` block. `at_end` seats focus on the last field (entered by arrowing up
    /// from below) vs the first (click / arrowing down from above). A click also
    /// carries `row` — the property line's index within the block — so the host
    /// focuses the row the user actually clicked; arrows pass `None`.
    EditProperties {
        range: Range<usize>,
        source: SharedString,
        at_end: bool,
        row: Option<usize>,
    },
    /// An inline `![](src)` image was left-clicked — the host opens a full-size
    /// preview. The text is untouched.
    PreviewImage(SharedString),
    /// The selection menu's "custom color…" (`…`) swatch was clicked, on the
    /// highlight row (`highlight`) or the text-color row; `position` is the
    /// menu's window-space top-left, to anchor a picker at. The host shows its
    /// color picker and hands the pick to [`EditorState::color_selection`];
    /// the selection is untouched meanwhile.
    PickColor {
        highlight: bool,
        position: Point<Pixels>,
    },
    /// The pointer moved onto an inline link (`Some` — the target and the
    /// link's window-space box, from this frame's layout) or off every link
    /// (`None`). Emitted only on change, so a host can show a preview card
    /// anchored to the link.
    HoverLink(Option<(zorite_markdown::syntax::LinkHit, Bounds<Pixels>)>),
}

/// A table column's text alignment, for the host-driven alignment toolbar
/// ([`EditorState::caret_table_align`] / [`EditorState::set_caret_table_align`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CellAlign {
    Left,
    Center,
    Right,
}

/// Provides replacement suggestions for a flagged word (best first); set by the
/// host via [`EditorState::on_suggest`] and consulted on right-click.
type SuggestFn = Box<dyn Fn(&str) -> Vec<String>>;

/// Resolves a standalone image line's `src` to a decoded image so the editor can
/// render it inline (W4). Set by the host via
/// [`EditorState::set_block_image_provider`]; the host owns loading + caching and
/// returns `None` while still decoding / on failure (the line shows raw source).
type BlockImageFn = Box<dyn Fn(&str) -> Option<Arc<RenderImage>>>;

/// Classifies an `![](src)` reference as a file chip (e.g. a PDF) rather than an
/// image, returning its display label. Set via
/// [`EditorState::set_block_chip_provider`]; the editor renders such a line as a
/// clickable chip (left-click emits [`EditorEvent::OpenLink`]).
type BlockChipFn = Box<dyn Fn(&str) -> Option<SharedString>>;

/// Host-supplied clipboard writer for Copy/Cut — receives the markdown text
/// the editor would put on the clipboard, so a host can add flavors gpui's
/// clipboard can't (e.g. rendered HTML beside the plain string). See
/// [`EditorState::set_clipboard_writer`].
pub type ClipboardWriter = std::rc::Rc<dyn Fn(&str, &mut App)>;

/// Resolves a standalone `![[target]]` embed line to the host view that renders
/// the transclusion, plus the row height to reserve for it (the host estimates
/// and caps it; long content scrolls inside the view). `None` falls back to the
/// embed chip.
type EmbedViewFn = Box<dyn Fn(&str) -> Option<(gpui::AnyView, Pixels)>>;

/// Resolves a ` ```mermaid ` block's source to a rendered diagram bitmap plus its
/// **logical** (display) px size — supplied by the host for the same reason as
/// [`BlockMathFn`]. Set via [`EditorState::set_block_mermaid_provider`]; the host
/// renders + caches off-thread (see [`mermaid_sources`] to pre-render).
type BlockMermaidFn = Box<dyn Fn(&str) -> Option<(Arc<RenderImage>, f32, f32)>>;

/// Resolves a `$$…$$` math block's LaTeX to a typeset bitmap plus its **logical**
/// (display) px size, so the editor can render the block as the equation (caret
/// outside) instead of raw source. The host supplies the logical size because it
/// knows the raster's pixel density (e.g. typeset at a fixed 2× DPR); deriving it
/// from texture pixels ÷ window scale factor renders 2× too large on a 1× display
/// (the Linux/X11 bug — the division only cancels on a 2× "Retina" screen). Set
/// via [`EditorState::set_block_math_provider`]; pre-render with [`math_sources`].
type BlockMathFn = Box<dyn Fn(&str) -> Option<(Arc<RenderImage>, f32, f32)>>;

/// Colors a fenced code block's tokens in WYSIWYG: `(language tag, block
/// text) → sorted, non-overlapping styled ranges` (byte offsets into the
/// block). Host-supplied (e.g. a tree-sitter highlighter) so the crate stays
/// engine-free; absent it, code renders in `SyntaxStyle::code`. Set via
/// [`EditorState::set_code_highlighter`].
type CodeHighlightFn = Box<dyn Fn(&str, &str) -> Vec<(Range<usize>, HighlightStyle)>>;

/// Host auto-replace hook, consulted when a word-boundary character (space,
/// punctuation, Enter) completes a word: receives the just-finished line's
/// text up to the boundary and returns the slice range to replace plus its
/// replacement — e.g. wrapping a completed page title as `[[title]]`. The
/// edit is one undo step (⌫Z restores the plain word) and the caret keeps its
/// place after the boundary. Not consulted inside fenced code, and only for
/// single-character insertions (never pastes or IME commits). Set via
/// [`EditorState::set_auto_replace`].
type AutoReplaceFn = Box<dyn Fn(&str) -> Option<(Range<usize>, String)>>;

/// The diagram sources of every ` ```mermaid ` block in `content`, so a host can
/// pre-render them (the editor's mermaid provider then finds the ready bitmap).
pub fn mermaid_sources(content: &str) -> Vec<SharedString> {
    markdown_syntax::mermaid_blocks(content)
        .into_iter()
        .map(|(_, source)| source.into())
        .collect()
}

/// The LaTeX sources of every `$$…$$` math block in `content`, so a host can
/// pre-render them (the editor's math provider then finds the ready bitmap).
pub fn math_sources(content: &str) -> Vec<SharedString> {
    markdown_syntax::math_blocks(content)
        .into_iter()
        .map(|(_, source)| source.into())
        .collect()
}

/// The LaTeX sources of every inline `$…$` formula in `content` (the inner LaTeX, no `$`
/// delimiters), so a host can pre-render them into the same math store the block provider
/// reads. Skips lines inside fenced code blocks, where `$…$` is literal.
pub fn inline_math_sources(content: &str) -> Vec<SharedString> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in content.split('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for span in markdown_syntax::inline_math_spans(line) {
            out.push(markdown_syntax::inline_math_latex(line, &span).into());
        }
    }
    out
}

/// The editor: text + cursor/selection state, an undo/redo history, plus a
/// cached layout (the wrapped lines from the last paint) for hit-testing + IME.
/// Renders the WYSIWYG view when a markdown [`SyntaxStyle`] is installed, the
/// raw-markdown view otherwise.
/// Host-injectable UI labels the editor renders in its context menus and
/// chrome (right-click menu items, the code-block / math `Copy` chips, the
/// table / "Turn into" menus). The crate stays host-agnostic so it never
/// calls `t!()`; the app passes localized strings here via
/// [`EditorState::set_labels`]. The default is English, keeping the crate
/// usable standalone (and existing tests' expectations intact).
#[derive(Clone)]
pub struct Labels {
    /// Text-selection right-click menu.
    pub cut: SharedString,
    pub copy: SharedString,
    pub copy_as_markdown: SharedString,
    pub paste: SharedString,
    /// Code-block / formula chrome.
    pub code_copy: SharedString,
    pub math_copy: SharedString,
    /// "Turn into" block-conversion menu.
    pub turn_into: SharedString,
    pub text: SharedString,
    pub heading_1: SharedString,
    pub heading_2: SharedString,
    pub heading_3: SharedString,
    pub bulleted_list: SharedString,
    pub numbered_list: SharedString,
    pub todo: SharedString,
    pub quote: SharedString,
    pub callout: SharedString,
    pub code_block: SharedString,
    pub math_block: SharedString,
    /// Table right-click menu.
    pub insert_row_above: SharedString,
    pub insert_row_below: SharedString,
    pub duplicate_row: SharedString,
    pub insert_column_left: SharedString,
    pub insert_column_right: SharedString,
    pub align_left: SharedString,
    pub align_center: SharedString,
    pub align_right: SharedString,
    pub grid_style: SharedString,
    pub striped_style: SharedString,
    pub header_style: SharedString,
    pub minimal_style: SharedString,
    pub delete_row: SharedString,
    pub delete_column: SharedString,
    pub delete_table: SharedString,
    /// Property-panel menu.
    pub edit_properties: SharedString,
    pub delete_property: SharedString,
    /// Image menu.
    pub delete_image: SharedString,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            cut: "Cut".into(),
            copy: "Copy".into(),
            copy_as_markdown: "Copy as Markdown".into(),
            paste: "Paste".into(),
            code_copy: "Copy".into(),
            math_copy: "Copy".into(),
            turn_into: "Turn into".into(),
            text: "Text".into(),
            heading_1: "Heading 1".into(),
            heading_2: "Heading 2".into(),
            heading_3: "Heading 3".into(),
            bulleted_list: "Bulleted list".into(),
            numbered_list: "Numbered list".into(),
            todo: "To-do".into(),
            quote: "Quote".into(),
            callout: "Callout".into(),
            code_block: "Code block".into(),
            math_block: "Math block".into(),
            insert_row_above: "Insert row above".into(),
            insert_row_below: "Insert row below".into(),
            duplicate_row: "Duplicate row".into(),
            insert_column_left: "Insert column left".into(),
            insert_column_right: "Insert column right".into(),
            align_left: "Align left".into(),
            align_center: "Align center".into(),
            align_right: "Align right".into(),
            grid_style: "Grid style".into(),
            striped_style: "Striped style".into(),
            header_style: "Header style".into(),
            minimal_style: "Minimal style".into(),
            delete_row: "Delete row".into(),
            delete_column: "Delete column".into(),
            delete_table: "Delete table".into(),
            edit_properties: "Edit properties".into(),
            delete_property: "Delete property".into(),
            delete_image: "Delete image".into(),
        }
    }
}

pub struct EditorState {
    focus_handle: FocusHandle,
    /// The whole document, newline-separated. Byte offsets index into this.
    content: String,
    placeholder: SharedString,
    /// Selection as a byte range; the caret is one end (see [`Self::cursor_offset`]).
    selected_range: Range<usize>,
    selection_reversed: bool,
    /// IME composition range, if any.
    marked_range: Option<Range<usize>>,
    /// Host-supplied clipboard writer for Copy/Cut (e.g. adding an HTML
    /// flavor beside the plain text). `None` = gpui's plain-string copy.
    clipboard_writer: Option<ClipboardWriter>,
    /// Find-in-feed highlights: match byte ranges + the active index, painted
    /// behind the text like the selection. Host-driven ([`Self::set_search`]).
    search: Option<(Vec<Range<usize>>, Option<usize>)>,
    /// Local change (Ghostex Docs): host note highlights, painted behind the text like find
    /// matches: (range, fill, optional strike colour).
    note_highlights: Vec<(Range<usize>, Hsla, Option<Hsla>)>,
    /// Last paint's wrapped lines (one per logical line) and each line's top
    /// offset relative to the editor's top — both used for hit-testing and
    /// cursor/IME positioning.
    wrapped: Vec<WrappedLine>,
    line_tops: Vec<Pixels>,
    /// Per-logical-line wrap-row count (from the last paint). Geometry reads
    /// this, not `wrapped[i].wrap_boundaries()` — a windowed-out line's entry
    /// in `wrapped` is an empty placeholder.
    wrap_rows: Vec<usize>,
    /// Per-logical-line wrap-row height. Variable so a heading (bigger font) gets
    /// a taller row (W2); `line_height` is the base/fallback for the empty doc
    /// and any row without a recorded height.
    line_heights: Vec<Pixels>,
    /// Per-logical-line table-grid row (from the last paint), so a click /
    /// Tab / caret hit-tests against cells instead of the raw source line.
    table_rows: Vec<Option<TableRow>>,
    /// Hover-revealed "+" add-row / add-column strips for each table (issue #16),
    /// each paired with the table row to seat the caret in before inserting. From
    /// the last paint, committed only while the table is hovered; hit-tested on
    /// mouse-down.
    table_row_add_rects: Vec<(Bounds<Pixels>, usize)>,
    table_col_add_rects: Vec<(Bounds<Pixels>, usize, usize)>,
    /// Each table's hover zone (grid + a thin margin) with its header row,
    /// committed every paint so `on_mouse_move` can repaint when the pointer's
    /// table-affordance region changes (the editor otherwise only repaints on
    /// the caret blink) and `on_scroll_wheel` can hit-test the PAINTED table.
    table_hover_zones: Vec<(Bounds<Pixels>, usize)>,
    /// The affordance region the pointer was last in — `(table index, 0 = zone /
    /// 1 = below strip / 2 = right strip)` — so the repaint fires only on change.
    table_hover_region: Option<(usize, u8)>,
    /// Committed delete-handle rects (issue #16): the hovered row's "−" `(bounds,
    /// row)` and the hovered column's "−" `(bounds, row, col)`, hit-tested on click.
    table_row_del: Option<(Bounds<Pixels>, usize)>,
    table_col_del: Option<(Bounds<Pixels>, usize, usize)>,
    /// The table cell `(row, col)` the pointer was last over, so `on_mouse_move`
    /// repaints the delete handles when it changes.
    table_hover_cell: Option<(usize, usize)>,
    /// Per-logical-line flag: this row is painted as an inline image (W4), so a
    /// click on it places the caret at the line start instead of hit-testing
    /// source text. From the last paint.
    widget_rows: Vec<bool>,
    /// Per-logical-line display→source byte map for rows with hidden markers
    /// (W6); `None` when the painted text equals the source. From the last paint.
    offset_maps: Vec<Option<std::rc::Rc<Vec<usize>>>>,
    /// Per-logical-line horizontal text inset (and so the caret/selection/hit-test
    /// inset): non-zero for fenced code blocks and gutter marks (blockquotes,
    /// lists). From the last paint.
    line_insets: Vec<Pixels>,
    /// Per-logical-line right-to-left geometry (#66), `None` on every LTR row
    /// so an LTR document pays nothing. From the last paint. See [`RtlRow`].
    rtl_rows: Vec<Option<RtlRow>>,
    last_bounds: Option<Bounds<Pixels>>,
    line_height: Pixels,
    /// Font size from the last paint. Hit-testing that runs during event
    /// dispatch (e.g. table-cell clicks) must measure at this size — the
    /// window's text-style stack is unwound there, so `window.text_style()`
    /// would report the root size, not the host wrapper's.
    font_size: Pixels,
    /// The font of the last paint, for the same reason: event-time
    /// `text_style()` reports the ROOT font, whose family/metrics can differ
    /// from what the table was painted with (column auto-fit measured with
    /// the wrong glyph widths and wrapped cells).
    paint_font: Option<Font>,
    is_selecting: bool,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    last_edit: EditKind,
    /// Whether the last content edit was a single typed grapheme or a single-char
    /// backspace — the only edits auto-pairing should react to, so programmatic /
    /// structural edits (table ops, etc.) don't trip it.
    last_edit_keystroke: bool,
    /// Spaces inserted per Tab / one list-nesting level (`Indent`/`Outdent`); set
    /// by the host via [`Self::set_tab_indent`] to match its list-indent setting.
    tab_indent: usize,
    /// The target x for vertical (Up/Down) movement, so the caret keeps its
    /// column across short lines. `Some` only during a run of Up/Down.
    goal_x: Option<Pixels>,
    /// Spans to underline (misspellings, etc.), set by the host via
    /// [`Self::set_diagnostics`].
    diagnostics: Vec<Diagnostic>,
    /// Inline-markdown styling palette; `Some` = the WYSIWYG (live-preview)
    /// view (W1), `None` = the raw view (plain text). Set by the host via
    /// [`Self::set_markdown_style`].
    markdown_style: Option<SyntaxStyle>,
    /// Host-injectable UI labels for context menus / chrome; English by
    /// default, localized through [`Self::set_labels`].
    labels: Labels,
    /// The open right-click suggestions menu, if any.
    menu: Option<DiagMenu>,
    /// The open table right-click menu's anchor (window space), if any. Its actions
    /// operate on the caret's table cell.
    table_menu: Option<Point<Pixels>>,
    /// Scroll state for the table menu, so its overflow scrolls + shows a thumb.
    table_menu_scroll: ScrollHandle,
    /// The open image right-click menu, if any: the image's logical line + the
    /// menu's anchor (window space). Offers Word-style object actions (Delete).
    image_menu: Option<(usize, Point<Pixels>)>,
    /// Right-clicked property-panel row: its source line + click position
    /// (anchors the Edit/Delete property menu).
    prop_menu: Option<(usize, Point<Pixels>)>,
    /// Supplies replacement suggestions for a flagged word, fetched lazily when
    /// the user right-clicks it. Set by the host via [`Self::on_suggest`];
    /// without it, the right-click menu has nothing to offer.
    suggest: Option<SuggestFn>,
    /// Resolves a standalone image line's `src` to a decoded image for inline
    /// rendering (W4); set by the host via [`Self::set_block_image_provider`].
    block_image: Option<BlockImageFn>,
    /// Classifies an `![](src)` as a file chip (e.g. a PDF) + its label; set by
    /// the host via [`Self::set_block_chip_provider`].
    block_chip: Option<BlockChipFn>,
    embed_view: Option<EmbedViewFn>,
    /// Resolves a ` ```mermaid ` block's source to a rendered diagram; set by the
    /// host via [`Self::set_block_mermaid_provider`].
    block_mermaid: Option<BlockMermaidFn>,
    /// Resolves a `$$…$$` block's LaTeX to a typeset equation; set by the host via
    /// [`Self::set_block_math_provider`].
    block_math: Option<BlockMathFn>,
    /// Fenced-code syntax highlighter, see [`CodeHighlightFn`].
    code_highlight: Option<CodeHighlightFn>,
    /// Host auto-replace hook, see [`Self::set_auto_replace`].
    auto_replace: Option<AutoReplaceFn>,
    /// What the most recent keystroke edit replaced (the selected text), for
    /// the host's auto-pair logic — a text diff alone can't distinguish
    /// "typed `[` over a selection starting with `[`" from "backspaced inside
    /// a doubled pair". Consumed via [`Self::take_replaced_selection`].
    last_replaced: Option<String>,
    /// The em (px/font-size) the `block_math` provider rasterizes at — set via
    /// [`Self::set_block_math_em`]. Inline `$…$` formulas reuse those rasters scaled by
    /// `text_em / this`, so they sit at text size. `None` disables inline math rendering.
    block_math_em: Option<f32>,
    /// Per-logical-line `src` for rows painted as a file chip (from the last
    /// paint), so a left-click can open it and a right-click can edit it.
    chip_rows: Vec<Option<(SharedString, bool)>>,
    /// Window-space painted bounds of each inline image, with its logical line
    /// index (from the last paint), so a press near a corner can start a resize
    /// and know which `![](src)` line to rewrite. One entry per rendered image.
    image_rects: Vec<(usize, Bounds<Pixels>)>,
    /// Window-space bounds of each painted task checkbox, with its logical line —
    /// so a click on the box toggles `[ ]`↔`[x]` instead of placing the caret.
    checkbox_rects: Vec<(usize, Bounds<Pixels>)>,
    /// Painted code-card chrome bounds from the last frame (lang tag + Copy per
    /// code block, keyed by the opening-fence row) — clicks route here before
    /// caret placement.
    code_chip_rects: Vec<CodeChipHit>,
    /// Full card bounds of each code block from the last paint (`(first body
    /// line, rect)`), for hover tracking — the chrome is hover-revealed.
    code_card_rects: Vec<(usize, Bounds<Pixels>)>,
    /// The hovered code block's first body line, if any (chrome shows there).
    code_chip_hover: Option<usize>,
    /// Open language picker for a code block: `(opening fence row, anchor)`.
    code_lang_menu: Option<(usize, Point<Pixels>)>,
    code_lang_scroll: ScrollHandle,
    /// Languages the host's highlighter supports, offered in the code block's
    /// language picker. Empty (the default) disables the picker.
    code_langs: Vec<SharedString>,
    /// Painted chevron bounds of foldable callouts (`(line, rect)`, from the
    /// last paint) — a click flips the marker's `-`/`+` fold char.
    alert_fold_rects: Vec<(usize, Bounds<Pixels>)>,
    /// The in-progress corner-grip drag, if any (see [`ImageResize`]). While set,
    /// that image paints at the live width and other mouse handling is suppressed.
    image_resize: Option<ImageResize>,
    /// An in-progress table column-border drag (drag-to-resize, issue #16):
    /// the column resizes live; release persists `cols=` into the table's
    /// marker line. `None` = no drag.
    table_col_resize: Option<TableColResize>,
    /// Last-paint column-resize grip bands: `(band, header row, column, width)`.
    table_col_resize_rects: Vec<(Bounds<Pixels>, usize, usize, f32)>,
    /// The band index the pointer is on (repaint-on-change for its accent line).
    table_resize_hover: Option<usize>,
    /// See [`ShapeMemo`] — `RefCell` because the measure closure holds only a
    /// read borrow of the editor.
    shape_memo: std::cell::RefCell<Option<ShapeMemo>>,
    /// See [`ScanData`].
    scan_cache: std::cell::RefCell<Option<(u64, std::rc::Rc<ScanData>)>>,
    /// Local change (Ghostex Docs): [`Self::line_starts`] per (`content_gen`, content length).
    line_starts_cache: std::cell::RefCell<Option<(u64, usize, std::rc::Rc<Vec<usize>>)>>,
    /// See [`ScrollCompensatorFn`].
    scroll_compensator: Option<ScrollCompensatorFn>,
    /// Cross-frame shaping caches — line runs, table column widths, and table
    /// wrap rows (see [`ShapeCaches`]). Capacity-capped in `shape_document`.
    shape_caches: ShapeCaches,
    /// The shaping window (element-local y, quantized), set after each
    /// prepaint from the painted bounds — one frame stale by design, so the
    /// measure pass and prepaint always shape with the SAME band and the
    /// measure→prepaint memo keeps hitting.
    shape_band: std::cell::Cell<Option<(f32, f32)>>,
    /// Latch: the scroll compensator fired since the last paint. Measure can
    /// run several times before a paint commits fresh `line_tops`; without
    /// this, one async height change compensates once per measure call.
    compensated: std::cell::Cell<bool>,
    /// An active gutter block drag (Notion/Cditor-style reorder): the grabbed
    /// block's first + last rows and the current drop boundary (a row index;
    /// `== rows` drops at the document end).
    line_drag: Option<(usize, usize, usize)>,
    /// The row whose gutter grip the pointer hovers (mirrors prepaint's
    /// computation) — tracked so hover changes repaint the grip.
    grip_hover_row: Option<usize>,
    /// Horizontal scroll of each wide table, keyed by its header row — wide
    /// tables keep natural column widths and scroll in place. Keys drift on
    /// edits above a table; entries are clamped at use, so a stale one is a
    /// harmless partial offset. (ponytail: no eviction, the map stays tiny)
    table_scroll_x: std::collections::HashMap<usize, f32>,
    /// Last-paint wide-table scroll thumbs (padded grab rects), so the thumb
    /// is mouse-draggable, not just an indicator.
    table_thumbs: Vec<TableThumb>,
    /// A live thumb drag: `(header row, grab x, scroll offset at grab)`.
    table_thumb_drag: Option<(usize, Pixels, f32)>,
    /// Extra left offset for the drag grip — the host sets its line-number
    /// gutter's width here so the grip sits beside the numbers, not on them.
    grip_inset: Pixels,
    /// `content_gen` as of the last paint — a measure with the SAME generation
    /// but different heights means an async (non-edit) height change, the
    /// scroll-anchoring trigger.
    last_paint_gen: u64,
    /// Bumped on every content mutation — cheap staleness key for caches
    /// (the UTF-16 conversion anchor below; a shape cache later).
    content_gen: u64,
    /// Resume point for UTF-8↔UTF-16 conversion: `(generation, utf8, utf16)`
    /// of the last converted offset. IME composition fires conversions many
    /// times per keystroke, clustered near the caret — resuming from the
    /// anchor makes them O(distance) instead of O(document) (CJK latency
    /// grew with note size; found auditing against Cditor's per-block IME).
    utf16_anchor: std::cell::Cell<(u64, usize, usize)>,
    /// A `$$…$$` block being edited in-line: its byte range + the host-supplied view (the
    /// structural editor) painted in a reserved gap at the block's spot. `None` = none.
    editing_block: Option<EditingBlock>,
    /// Window-space painted bounds of each inline `$…$` formula + its absolute byte range and
    /// inner LaTeX (from the last paint), so a click can open its structural editor and the
    /// seated editor can be positioned at the formula's spot.
    inline_math_rects: Vec<(Range<usize>, SharedString, Bounds<Pixels>)>,
    /// An inline `$…$` formula under structural edit: its byte range + the host's editor view,
    /// overlaid at the formula's spot. `None` = none.
    editing_inline: Option<EditingInline>,
    /// Painted bounds + target of each property-panel pill (from the last paint),
    /// so a left-click opens it (`OpenWikiLink` / `OpenLink`).
    prop_pill_rects: Vec<(Bounds<Pixels>, zorite_markdown::syntax::LinkHit)>,
    /// Inline links' painted boxes + targets from the last paint (the same
    /// geometry as the hand-cursor hitboxes), for hover → `HoverLink`.
    link_rects: Vec<(Bounds<Pixels>, zorite_markdown::syntax::LinkHit)>,
    /// The link under the pointer, if any — `HoverLink` fires on change.
    hovered_link: Option<(zorite_markdown::syntax::LinkHit, Bounds<Pixels>)>,
    /// Painted bounds of each property-panel row (from the last paint), so
    /// `on_mouse_move` repaints when the hovered row changes (the panel's hover
    /// border reads the live pointer during paint).
    /// Each painted property-panel row's bounds + its source line (for
    /// hover borders and the right-click property menu).
    prop_row_rects: Vec<(Bounds<Pixels>, usize)>,
    /// The property row the pointer was last over — drives `on_mouse_move`'s
    /// repaint-on-change (like the table hover).
    prop_hover_row: Option<usize>,
    /// Collapsed headings, keyed by the heading's trimmed source line
    /// (`## Goals`). View-local — markdown has no heading-fold syntax (unlike
    /// callouts' `-`/`+`), so folds live for the editor's lifetime and a key
    /// self-heals by vanishing when its heading text is edited.
    folded_headings: std::collections::HashSet<String>,
    /// Painted chevron bounds of heading folds (`(line, rect)`, from the last
    /// paint) — a click toggles that heading in `folded_headings`.
    heading_fold_rects: Vec<(usize, Bounds<Pixels>)>,
    /// Window-space bounds of every heading's first visual row (from the last
    /// paint) — `on_mouse_move` hit-tests these for the hover chevron.
    heading_row_rects: Vec<(usize, Bounds<Pixels>)>,
    /// The heading line the pointer was last over — its chevron shows on hover
    /// (a fold chevron on every heading would clutter). Drives
    /// `on_mouse_move`'s repaint-on-change, like the property-row hover.
    heading_hover_row: Option<usize>,
}

/// A math block under in-line structural edit: the byte range to overwrite on commit, and
/// the host's editor view to render in the reserved gap.
struct EditingBlock {
    range: Range<usize>,
    view: gpui::AnyView,
    /// The block's displayed height — the gap reserved while editing, so the formula stays
    /// put instead of jumping to a fixed size.
    height: Pixels,
}

/// An inline `$…$` formula under structural edit: the byte range to overwrite on commit, and
/// the host's editor view, overlaid at the formula's painted spot.
struct EditingInline {
    range: Range<usize>,
    view: gpui::AnyView,
    /// Where the view's top-left sits relative to the formula raster's top-left. The
    /// host's editor view pads its raster differently than the display raster (whose
    /// padding was baked at the block em and scaled down), so a zero offset shifts
    /// the glyphs visibly on entering edit.
    offset: Point<Pixels>,
}

impl EditorState {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: String::new(),
            placeholder: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            clipboard_writer: None,
            search: None,
            note_highlights: Vec::new(),
            wrapped: Vec::new(),
            line_tops: Vec::new(),
            line_heights: Vec::new(),
            wrap_rows: Vec::new(),
            widget_rows: Vec::new(),
            offset_maps: Vec::new(),
            line_insets: Vec::new(),
            rtl_rows: Vec::new(),
            table_rows: Vec::new(),
            table_row_add_rects: Vec::new(),
            table_col_add_rects: Vec::new(),
            table_hover_zones: Vec::new(),
            table_hover_region: None,
            table_row_del: None,
            table_col_del: None,
            table_hover_cell: None,
            last_bounds: None,
            line_height: px(20.),
            font_size: px(16.),
            paint_font: None,
            is_selecting: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_edit: EditKind::Other,
            last_edit_keystroke: false,
            tab_indent: 4,
            goal_x: None,
            diagnostics: Vec::new(),
            markdown_style: None,
            labels: Labels::default(),
            menu: None,
            table_menu: None,
            table_menu_scroll: ScrollHandle::new(),
            image_menu: None,
            prop_menu: None,
            suggest: None,
            block_image: None,
            block_chip: None,
            embed_view: None,
            block_mermaid: None,
            block_math: None,
            block_math_em: None,
            code_highlight: None,
            auto_replace: None,
            last_replaced: None,
            chip_rows: Vec::new(),
            image_rects: Vec::new(),
            checkbox_rects: Vec::new(),
            code_chip_rects: Vec::new(),
            code_card_rects: Vec::new(),
            code_chip_hover: None,
            code_lang_menu: None,
            code_lang_scroll: ScrollHandle::new(),
            code_langs: Vec::new(),
            alert_fold_rects: Vec::new(),
            image_resize: None,
            table_col_resize: None,
            table_col_resize_rects: Vec::new(),
            table_resize_hover: None,
            shape_memo: std::cell::RefCell::new(None),
            scan_cache: std::cell::RefCell::new(None),
            line_starts_cache: std::cell::RefCell::new(None),
            scroll_compensator: None,
            last_paint_gen: 0,
            shape_caches: ShapeCaches::default(),
            shape_band: std::cell::Cell::new(None),
            compensated: std::cell::Cell::new(false),
            line_drag: None,
            grip_hover_row: None,
            table_scroll_x: std::collections::HashMap::new(),
            table_thumbs: Vec::new(),
            table_thumb_drag: None,
            grip_inset: px(0.),
            content_gen: 0,
            utf16_anchor: std::cell::Cell::new((0, 0, 0)),
            editing_block: None,
            inline_math_rects: Vec::new(),
            editing_inline: None,
            prop_pill_rects: Vec::new(),
            link_rects: Vec::new(),
            hovered_link: None,
            prop_row_rects: Vec::new(),
            prop_hover_row: None,
            folded_headings: std::collections::HashSet::new(),
            heading_fold_rects: Vec::new(),
            heading_row_rects: Vec::new(),
            heading_hover_row: None,
        }
    }

    /// Builder: start with the given text (caret at the start).
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.content = normalize_loaded(text.into());
        let caret = caret_off_marker_line(&self.content, 0);
        self.selected_range = caret..caret;
        self
    }

    /// Builder: placeholder shown when empty.
    pub fn with_placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// The current document text.
    pub fn text(&self) -> &str {
        &self.content
    }

    /// Replace byte `range` with `text` as ONE recorded (undoable) edit, leaving the caret
    /// after the inserted text. Unlike [`Self::set_text`] this preserves — and extends — the
    /// undo history, so a host writing back a structural edit (e.g. a committed `$$…$$`
    /// formula) lands as a normal undo step rather than clobbering the history.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        // Snap to char boundaries (start down, end up) so a stale/shifted range — e.g. one
        // captured before a prior formula commit moved the bytes — can't panic mid-UTF-8.
        let len = self.content.len();
        let mut start = range.start.min(len);
        while start > 0 && !self.content.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = range.end.clamp(start, len);
        while end < len && !self.content.is_char_boundary(end) {
            end += 1;
        }
        let range = start..end;
        self.record_edit(&range, text);
        self.content.replace_range(range.clone(), text);
        self.remap_diagnostics(&range, text.len());
        let caret = range.start + text.len();
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        // Don't coalesce a following keystroke into this structural replacement.
        self.last_edit = EditKind::Other;
        cx.notify();
    }

    /// Replace the whole document; resets the caret to the start.
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.content_gen += 1;
        self.content = normalize_loaded(text.into());
        // Never park the loaded caret on a collapsed marker line (`<!-- table/
        // math:… -->`): the first focus would reveal it raw mid-interaction.
        // This is the ONE passive parking path — every other caret write is a
        // deliberate placement (which SHOULD reveal markers for editing).
        let caret = caret_off_marker_line(&self.content, 0);
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        // A programmatic load isn't undoable to the prior document.
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.last_edit = EditKind::Other;
        cx.notify();
    }

    /// Replace the set of diagnostics (underlined spans). The host computes these
    /// (e.g. spell-check) and refreshes them as the text changes.
    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>, cx: &mut Context<Self>) {
        self.diagnostics = diagnostics;
        // Diagnostics feed the per-row run keys (they underline spans) — drop
        // the memo so the next shape re-keys affected lines.
        *self.shape_caches.row_keys.borrow_mut() = (None, Vec::new());
        cx.notify();
    }

    /// Turn on WYSIWYG (live-preview) markdown styling with the given
    /// color/font palette (call once at setup). Inline bold/italic/code/link/
    /// tag formatting then renders as you type — markers stay in the text,
    /// dimmed. Without it the editor is the raw view: plain text, spell-check
    /// underlines only.
    /// Languages offered in a code block's language picker (the host's
    /// highlighter set, e.g. its compiled tree-sitter grammars). Empty — the
    /// default — leaves the tag click-inert.
    pub fn set_code_languages(&mut self, langs: Vec<SharedString>) {
        self.code_langs = langs;
    }

    /// Install the scroll-anchoring hook (see [`ScrollCompensatorFn`]): when
    /// an async block render (math/mermaid/image) changes heights above the
    /// window viewport, the host receives the delta and shifts its scroll
    /// offset so the visible content stays put (Cditor's anchor-restore).
    pub fn set_scroll_compensator(&mut self, f: impl Fn(Pixels, &mut Window, &mut App) + 'static) {
        self.scroll_compensator = Some(std::rc::Rc::new(f));
    }

    pub fn set_markdown_style(&mut self, style: SyntaxStyle, cx: &mut Context<Self>) {
        self.markdown_style = Some(style);
        cx.notify();
    }

    /// Set the host-localized labels for the context menus / chrome. Replace
    /// on every language switch so the current editors pick it up.
    pub fn set_labels(&mut self, labels: Labels, cx: &mut Context<Self>) {
        self.labels = labels;
        cx.notify();
    }

    /// Turn off live-preview styling — the editor falls back to plain text
    /// (spell-check underlines only). Used when the host's WYSIWYG setting is
    /// switched off; a no-op if styling was already off.
    pub fn clear_markdown_style(&mut self, cx: &mut Context<Self>) {
        if self.markdown_style.take().is_some() {
            cx.notify();
        }
    }

    /// Install the provider consulted when the user right-clicks a flagged word.
    /// It's handed the offending word and returns replacements (best first).
    /// Kept lazy by design — the OS suggestion call can be slow, so it runs only
    /// on right-click, never in the per-edit detection pass.
    pub fn on_suggest(&mut self, provider: impl Fn(&str) -> Vec<String> + 'static) {
        self.suggest = Some(Box::new(provider));
    }

    /// Install the provider that resolves a standalone image line's `src` to a
    /// decoded image; with it, such lines render inline (W4) when the caret is
    /// elsewhere. Without it (or while an image is still loading), the line shows
    /// its raw `![](src)` source.
    pub fn set_block_image_provider(
        &mut self,
        provider: impl Fn(&str) -> Option<Arc<RenderImage>> + 'static,
    ) {
        self.block_image = Some(Box::new(provider));
    }

    /// Install the provider that classifies an `![](src)` reference as a file chip
    /// (e.g. a PDF) and supplies its label. With it, such lines render as a
    /// clickable chip when the caret is elsewhere; a left-click emits
    /// [`EditorEvent::OpenLink`] and a right-click places the caret to edit.
    pub fn set_block_chip_provider(
        &mut self,
        provider: impl Fn(&str) -> Option<SharedString> + 'static,
    ) {
        self.block_chip = Some(Box::new(provider));
    }

    /// Install the provider that resolves a standalone `![[target]]` line to a
    /// host view rendering the transclusion + the height to reserve for it.
    /// With it, such lines show the embedded content in place (raw on caret);
    /// without (or when it returns `None`) they fall back to a clickable chip.
    pub fn set_embed_provider(
        &mut self,
        provider: impl Fn(&str) -> Option<(gpui::AnyView, Pixels)> + 'static,
    ) {
        self.embed_view = Some(Box::new(provider));
    }

    /// Install the provider that resolves a ` ```mermaid ` block's source to a
    /// rendered diagram: the bitmap plus its logical (display) px size — see
    /// [`BlockMathFn`] for why the host supplies the size. With it, such a block
    /// renders as the diagram when the caret is elsewhere; with the caret inside
    /// (or while it renders) it shows the raw fenced source. Pre-render with
    /// [`mermaid_sources`].
    pub fn set_block_mermaid_provider(
        &mut self,
        provider: impl Fn(&str) -> Option<(Arc<RenderImage>, f32, f32)> + 'static,
    ) {
        self.block_mermaid = Some(Box::new(provider));
    }

    /// Install the provider that resolves a `$$…$$` block's LaTeX to a typeset
    /// equation: the bitmap plus its logical (display) px size — see
    /// [`BlockMathFn`] for why the host supplies the size. With it, such a block
    /// renders as the equation when the caret is elsewhere; with the caret inside
    /// (or while it renders) it shows the raw `$$…$$` source. Pre-render with
    /// [`math_sources`].
    /// Route Copy/Cut through `writer` instead of gpui's plain-string copy —
    /// the host owns the actual clipboard write (and its extra flavors).
    pub fn set_clipboard_writer(&mut self, writer: ClipboardWriter) {
        self.clipboard_writer = Some(writer);
    }

    /// Highlight `matches` (source byte ranges) behind the text — soft yellow,
    /// with `active` in the stronger current-match orange (the reader's
    /// browser-style find colors). Empty clears. Host-driven: a find bar
    /// computes matches (see [`find_in_source`]) and steps `active`.
    pub fn set_search(
        &mut self,
        matches: Vec<Range<usize>>,
        active: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.search = (!matches.is_empty()).then_some((matches, active));
        cx.notify();
    }

    /// Local change (Ghostex Docs): marks source byte ranges behind the text (a note's quote),
    /// each with a fill and an optional 2px line through it. Replaced whole; empty clears.
    pub fn set_note_highlights(
        &mut self,
        highlights: Vec<(Range<usize>, Hsla, Option<Hsla>)>,
        cx: &mut Context<Self>,
    ) {
        if self.note_highlights != highlights {
            self.note_highlights = highlights;
            cx.notify();
        }
    }

    /// The window-space top of the row containing byte `offset` (from the
    /// last layout) — for a host scrolling a find match into view. `None`
    /// before first paint or for an out-of-range offset.
    pub fn offset_screen_top(&self, offset: usize) -> Option<Pixels> {
        let bounds = self.last_bounds?;
        let (row, _) = self.row_col(offset.min(self.content.len()));
        Some(bounds.top() + self.line_tops.get(row).copied()?)
    }

    pub fn set_block_math_provider(
        &mut self,
        provider: impl Fn(&str) -> Option<(Arc<RenderImage>, f32, f32)> + 'static,
    ) {
        self.block_math = Some(Box::new(provider));
    }

    /// Declare the em the `block_math` provider rasterizes at (e.g. the host's display-math
    /// font size). Turns on inline `$…$` rendering: each inline formula reuses the block
    /// raster for the same LaTeX, scaled by `text_em / em` so it sits at text size. Pre-render
    /// inline sources too (see [`inline_math_sources`]).
    pub fn set_block_math_em(&mut self, em: f32) {
        self.block_math_em = (em > 0.).then_some(em);
    }

    /// Set the fenced-code syntax highlighter (see [`CodeHighlightFn`]).
    pub fn set_code_highlighter(
        &mut self,
        f: impl Fn(&str, &str) -> Vec<(Range<usize>, HighlightStyle)> + 'static,
    ) {
        self.code_highlight = Some(Box::new(f));
        let caches = &self.shape_caches;
        caches
            .highlighter_gen
            .set(caches.highlighter_gen.get().wrapping_add(1));
        caches.code_highlights.borrow_mut().take();
    }

    /// The text the most recent keystroke edit replaced (its selection), if
    /// any — consumed (one read per edit). Lets a host's auto-pair logic tell
    /// "opener typed over a selection" from deletions with identical diffs.
    pub fn take_replaced_selection(&mut self) -> Option<String> {
        self.last_replaced.take()
    }

    /// Set the word-completion auto-replace hook (see [`AutoReplaceFn`]).
    pub fn set_auto_replace(
        &mut self,
        f: impl Fn(&str) -> Option<(Range<usize>, String)> + 'static,
    ) {
        self.auto_replace = Some(Box::new(f));
    }

    /// Run the host's auto-replace hook after a boundary character landed at
    /// `boundary` (the byte offset of the char itself). Applies the returned
    /// replacement as its own undo step and shifts the caret by the growth.
    fn apply_auto_replace(&mut self, boundary: usize) {
        let Some(f) = self.auto_replace.as_ref() else {
            return;
        };
        let line_start = self.content[..boundary].rfind('\n').map_or(0, |p| p + 1);
        let line = &self.content[line_start..boundary];
        if line.is_empty() {
            return;
        }
        // Inside a fenced code block, the text is verbatim — never rewrite it.
        // Fence parity comes from the cached scan (this runs on every boundary
        // keystroke; the per-line rescan grew with the document).
        let (row, _) = self.row_col(boundary);
        if *self.scan_data().fence_odd.get(row).unwrap_or(&false)
            || line.trim_start().starts_with("```")
        {
            return;
        }
        let Some((r, replacement)) = f(line) else {
            // No host rule fired — normalize math around the caret instead:
            // words-attached `$$` fences and words-mixed `$$…$$` pairs split
            // onto their own lines (issue #54: the formula renders display,
            // the words stay VISIBLE — nothing is ever hidden). Paragraph-
            // bounded, one recorded edit.
            self.normalize_math_at(row);
            return;
        };
        if r.start >= r.end || r.end > line.len() {
            return;
        }
        let abs = line_start + r.start..line_start + r.end;
        let delta = replacement.len() as isize - abs.len() as isize;
        self.record_edit(&abs, &replacement);
        self.content =
            self.content[..abs.start].to_owned() + &replacement + &self.content[abs.end..];
        self.remap_diagnostics(&abs, replacement.len());
        let caret = (self.selected_range.start as isize + delta) as usize;
        self.selected_range = caret..caret;
    }

    /// Begin an in-line structural edit of the `$$…$$` block at `range`: reserve a gap at
    /// its spot and paint `view` (the host's editor) there. The host focuses `view`.
    pub fn set_editing_block(
        &mut self,
        range: Range<usize>,
        view: gpui::AnyView,
        height: Pixels,
        cx: &mut Context<Self>,
    ) {
        self.editing_block = Some(EditingBlock {
            range,
            view,
            height,
        });
        cx.notify();
    }

    /// The byte range of the block currently being structurally edited (the range handed
    /// to [`Self::set_editing_block`] — the source text is untouched while the edit is
    /// open, so it stays valid). `None` when no block edit is open.
    pub fn editing_block_range(&self) -> Option<Range<usize>> {
        self.editing_block.as_ref().map(|eb| eb.range.clone())
    }

    /// The height reserved for the open block edit (what the host last passed to
    /// [`Self::set_editing_block`]), so a host re-reserving on a resize can skip
    /// a no-op. `None` when no block edit is open.
    pub fn editing_block_height(&self) -> Option<Pixels> {
        self.editing_block.as_ref().map(|eb| eb.height)
    }

    /// End an in-line math edit (the host has committed / cancelled). Returns the block's
    /// byte range, so the host can overwrite it.
    pub fn end_editing_block(&mut self, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let range = self.editing_block.take().map(|eb| eb.range);
        cx.notify();
        range
    }

    /// Begin a structural edit of the inline `$…$` span at `range` (absolute bytes): overlay
    /// `view` (the host's editor) at the formula's painted spot. The host focuses `view`.
    /// `offset` places the view relative to the raster's top-left, letting the host
    /// align the view's glyphs with the displayed formula's (their paddings differ).
    pub fn set_editing_inline(
        &mut self,
        range: Range<usize>,
        view: gpui::AnyView,
        offset: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.editing_inline = Some(EditingInline {
            range,
            view,
            offset,
        });
        cx.notify();
    }

    /// End an inline math edit. Returns the span's byte range, so the host can overwrite it.
    pub fn end_editing_inline(&mut self, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let range = self.editing_inline.take().map(|e| e.range);
        cx.notify();
        range
    }

    /// Whether `range` still bounds an inline `$…$` span (a `$` at each end, content between, no
    /// newline, not a `$$` fence) — guards the inline commit against a stale/shifted range that
    /// would otherwise splice text at the wrong spot.
    pub fn is_inline_math_range(&self, range: &Range<usize>) -> bool {
        range.start < range.end
            && range.end <= self.content.len()
            && self.content.is_char_boundary(range.start)
            && self.content.is_char_boundary(range.end)
            && {
                let s = &self.content[range.clone()];
                s.len() >= 3
                    && s.starts_with('$')
                    && s.ends_with('$')
                    && !s.starts_with("$$")
                    && !s.contains('\n')
            }
    }

    /// The horizontal alignment of the `$$…$$` block whose byte range starts at `block_start`
    /// (its `<!-- math:ALIGN -->` marker, or `Center` by default) — so the host can seed the
    /// in-line editor at the right justification when opening it.
    pub fn math_align(&self, block_start: usize) -> MathAlign {
        let row = self.row_col(block_start).0;
        markdown_syntax::math_regions(&self.content)
            .into_iter()
            .find(|r| r.range.start == row)
            .map_or(MathAlign::default(), |r| r.align)
    }

    /// Compute the recorded edit that writes `align`'s marker for the `$$` block at byte
    /// `block`: the (possibly marker-extended) range to replace, and the marker prefix to
    /// prepend to the rewritten block. Center (default) → no marker (drops any existing one);
    /// left/right → add or replace it. The host appends the block text to the prefix. Folding
    /// the marker into the block's commit edit avoids a separate, range-shifting edit.
    pub fn math_marker_edit(
        &self,
        block: Range<usize>,
        align: MathAlign,
    ) -> (Range<usize>, String) {
        let row = self.row_col(block.start).0;
        let prefix = align.marker().map_or(String::new(), |m| format!("{m}\n"));
        let has_marker =
            row > 0 && markdown_syntax::math_align_marker(self.line_str(row - 1)).is_some();
        let start = if has_marker {
            self.line_starts()[row - 1]
        } else {
            block.start
        };
        (start..block.end, prefix)
    }

    /// Re-find a `$$…$$` block by its exact LaTeX `source`, returned as a BYTE range (nearest
    /// to the now-stale byte `approx` if several match) — so opening/committing one after a
    /// prior formula's commit shifted offsets targets the right block. `math_blocks` yields
    /// LINE ranges, so convert like `math_block_at` does (else the caret jumps to the top).
    pub fn find_math_block(&self, source: &str, approx: usize) -> Option<Range<usize>> {
        let starts = self.line_starts();
        markdown_syntax::math_blocks(&self.content)
            .into_iter()
            .filter(|(_, s)| s == source)
            .map(|(r, _)| starts[r.start]..self.line_end(r.end - 1))
            .min_by_key(|r| r.start.abs_diff(approx))
    }

    /// Re-find an inline `$…$` span by its exact inner LaTeX, as an absolute byte range (nearest
    /// to the now-stale byte `approx` if several match) — the inline counterpart of
    /// [`Self::find_math_block`], so opening/committing after a prior edit shifted offsets
    /// targets the right span.
    pub fn find_inline_math(&self, latex: &str, approx: usize) -> Option<Range<usize>> {
        let mut line_start = 0;
        let mut best: Option<Range<usize>> = None;
        for line in self.content.split('\n') {
            for span in markdown_syntax::inline_math_spans(line) {
                if markdown_syntax::inline_math_latex(line, &span) == latex {
                    let abs = line_start + span.start..line_start + span.end;
                    if best
                        .as_ref()
                        .is_none_or(|b| abs.start.abs_diff(approx) < b.start.abs_diff(approx))
                    {
                        best = Some(abs);
                    }
                }
            }
            line_start += line.len() + 1;
        }
        best
    }

    /// Whether byte `range` (half-open) still starts a `$$…$$` block — a commit guard so a
    /// stale/shifted range can't splice the block into the wrong place and corrupt the doc.
    pub fn is_math_block_range(&self, range: &Range<usize>) -> bool {
        range.end <= self.content.len()
            && range.start <= range.end
            && self.content.is_char_boundary(range.start)
            && self.content[range.start..range.end]
                .trim_start()
                .starts_with("$$")
    }

    /// The text of logical line `row` (without its trailing newline).
    fn line_str(&self, row: usize) -> &str {
        let starts = self.line_starts();
        match starts.get(row) {
            Some(&s) => &self.content[s..self.line_end(row)],
            None => "",
        }
    }

    /// The host-supplied embed views, each positioned in the gap its
    /// `![[target]]` line reserved (from the last paint's line tops) — the
    /// editing-block overlay generalized to N transclusions. Absolute children
    /// of the editor's `relative` root, so they scroll with the content; the
    /// caret's own line shows raw source instead (its gap wasn't reserved).
    fn embed_overlays(&self, window: &Window) -> Vec<gpui::Div> {
        let Some(provider) = &self.embed_view else {
            return Vec::new();
        };
        if self.markdown_style.is_none() {
            return Vec::new();
        }
        let caret_row = self
            .focus_handle
            .is_focused(window)
            .then(|| self.row_col(self.cursor_offset()).0);
        let mut out = Vec::new();
        for (row, line) in self.content.split('\n').enumerate() {
            if caret_row == Some(row) {
                continue;
            }
            let Some(inner) = zorite_markdown::syntax::embed_line(line) else {
                continue;
            };
            let (Some(top), Some((view, height))) = (self.line_tops.get(row), provider(inner))
            else {
                continue;
            };
            out.push(
                div()
                    .absolute()
                    .top(*top)
                    .left(px(0.))
                    .w_full()
                    .h(height)
                    // Clicks/wheel belong to the embed (it may scroll its own
                    // content), not the text layer underneath.
                    .occlude()
                    .child(view),
            );
        }
        out
    }

    /// The host-supplied editor view for an in-line math edit, positioned in the gap its
    /// block reserves (from the last paint's line tops/heights). An absolute child of the
    /// editor's `relative` root, so it scrolls with the content.
    fn editing_block_overlay(&self) -> Option<gpui::Div> {
        let eb = self.editing_block.as_ref()?;
        let row = self.row_col(eb.range.start).0;
        let top = *self.line_tops.get(row)?;
        let height = *self.line_heights.get(row)?;
        Some(
            div()
                .absolute()
                .top(top)
                .left(px(0.))
                .w_full()
                .h(height)
                // Occlude so clicks inside the hosted math editor don't fall through to the
                // text layer below — which would seat the caret on the next line and steal
                // focus, blurring (committing + closing) the structural editor.
                .occlude()
                .child(eb.view.clone()),
        )
    }

    /// The host-supplied editor view for an inline `$…$` edit, overlaid at the formula's last-
    /// painted spot (its window rect, made editor-relative via `content_origin`). Unlike a
    /// `$$` block it doesn't reserve a full-width gap — it floats over the formula, leaving the
    /// surrounding text in place.
    fn editing_inline_overlay(&self) -> Option<gpui::Div> {
        let ei = self.editing_inline.as_ref()?;
        let (_, _, rect) = self
            .inline_math_rects
            .iter()
            .find(|(r, _, _)| *r == ei.range)?;
        let origin = self.last_bounds.map_or(Point::default(), |b| b.origin);
        Some(
            div()
                .absolute()
                .top(rect.origin.y - origin.y + ei.offset.y)
                .left(rect.origin.x - origin.x + ei.offset.x)
                .occlude()
                .child(ei.view.clone()),
        )
    }

    /// Spaces inserted per Tab / list-nesting level (`Indent`/`Outdent`). The host
    /// keeps this in sync with its list-indent setting so nesting is configurable.
    pub fn set_tab_indent(&mut self, spaces: usize) {
        self.tab_indent = spaces.max(1);
    }

    /// The caret's byte offset into [`Self::text`] (the moving end of any
    /// selection). For hosts that drive a menu/completion off the caret position.
    pub fn cursor(&self) -> usize {
        self.cursor_offset()
    }

    /// The current selection as a byte range (empty at the caret). Local addition for the
    /// Ghostex Docs prototype's formatting bar.
    pub fn selected_range(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    /// Select `range` (clamped to char boundaries). Local addition for the Docs prototype.
    pub fn select_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let clamp = |mut offset: usize| {
            offset = offset.min(self.content.len());
            while !self.content.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        self.selected_range = clamp(range.start)..clamp(range.end);
        self.selection_reversed = false;
        cx.notify();
    }

    /// Whether the last content change was a single typed character or single-char
    /// backspace (vs a programmatic / multi-char edit). Hosts gate auto-pairing on
    /// this so structural edits (table row/column ops, paste, …) don't trip it.
    pub fn last_edit_was_keystroke(&self) -> bool {
        self.last_edit_keystroke
    }

    /// Place the caret at `offset` (a byte offset into the document), collapsing
    /// any selection. Clamped to the document and snapped down to a char
    /// boundary, so a host can pass a raw click offset safely — e.g. to enter
    /// edit mode where rendered text was clicked.
    pub fn set_cursor(&mut self, offset: usize, cx: &mut Context<Self>) {
        let mut offset = offset.min(self.content.len());
        while !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        self.move_to(offset, cx);
    }

    /// Per logical line, from the last paint: its top offset within the
    /// editor and its first wrap-row's height — enough for a host-drawn
    /// gutter (line numbers) to align with rows without re-deriving layout.
    /// Empty before the first paint. Rows collapsed by a heading fold show
    /// no vertical advance (the next row's top equals theirs) — a gutter
    /// should skip those.
    pub fn row_layout(&self) -> Vec<(Pixels, Pixels)> {
        self.line_tops
            .iter()
            .enumerate()
            .map(|(row, &top)| (top, self.line_h(row)))
            .collect()
    }

    /// The cached [`ScanData`] for the current content, rebuilding on a
    /// generation mismatch.
    fn scan_data(&self) -> std::rc::Rc<ScanData> {
        if let Some((generation, data)) = self.scan_cache.borrow().as_ref()
            && *generation == self.content_gen
        {
            return data.clone();
        }
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut fence_odd = Vec::with_capacity(lines.len());
        let mut odd = false;
        for l in &lines {
            fence_odd.push(odd);
            if l.trim_start().starts_with("```") {
                odd = !odd;
            }
        }
        let data = std::rc::Rc::new(ScanData {
            generation: self.content_gen,
            ordered: markdown_syntax::ordered_numbers(&lines),
            tables: markdown_syntax::table_regions(&self.content),
            mermaid: markdown_syntax::mermaid_blocks(&self.content),
            math: markdown_syntax::math_regions(&self.content),
            props: markdown_syntax::property_regions(&self.content),
            alert_folds: markdown_syntax::alert_fold_regions(&self.content),
            fence_odd,
        });
        *self.scan_cache.borrow_mut() = Some((self.content_gen, data.clone()));
        data
    }

    /// The wrap-row count of logical line `row` (1 when unrecorded).
    fn row_span(&self, row: usize) -> usize {
        self.wrap_rows.get(row).copied().unwrap_or(1).max(1)
    }

    /// The wrap-row height of logical line `row` (a heading is taller). Falls
    /// back to the base `line_height` for unrecorded rows / the empty document.
    fn line_h(&self, row: usize) -> Pixels {
        self.line_heights
            .get(row)
            .copied()
            .unwrap_or(self.line_height)
    }

    /// Horizontal text inset for logical line `row` (from the last paint): non-zero
    /// for fenced code blocks + gutter marks. Applied to the caret, selection,
    /// hit-test, and text paint so they all stay aligned.
    fn line_inset(&self, row: usize) -> Pixels {
        self.line_insets.get(row).copied().unwrap_or(px(0.))
    }

    /// The right-align shift of an RTL row (zero everywhere else) — see
    /// [`RtlRow::shift`].
    fn rtl_shift(&self, row: usize) -> Pixels {
        self.rtl_rows
            .get(row)
            .and_then(Option::as_ref)
            .and_then(|r| r.shifts.first().copied())
            .unwrap_or(px(0.))
    }

    /// Where logical line `row`'s painted text actually starts: its inset plus
    /// the right-align shift of an RTL row (#66). Everything that positions
    /// against a row's text — caret, click, selection, link boxes — goes
    /// through this, so the two can never drift apart.
    fn row_origin_x(&self, row: usize) -> Pixels {
        self.line_inset(row) + self.rtl_shift(row)
    }

    /// Does the caret's TABLE row read right-to-left? Cells step through their
    /// own stepper, which walks in logical order — so on an RTL table the
    /// visual arrows map to the opposite step.
    fn caret_table_is_rtl(&self) -> bool {
        let (row, _) = self.row_col(self.cursor_offset());
        self.table_rows
            .get(row)
            .and_then(Option::as_ref)
            .is_some_and(|t| t.rtl)
    }

    /// Does the caret's line read right-to-left?
    ///
    /// Arrow keys move VISUALLY — Right steps to the character on the right of
    /// the screen, which every platform does in bidi text and which readers of
    /// Persian expect. On an RTL row that character is the logically PREVIOUS
    /// one, so the two step functions swap.
    fn caret_row_is_rtl(&self) -> bool {
        let (row, _) = self.row_col(self.cursor_offset());
        self.rtl_rows
            .get(row)
            .and_then(Option::as_ref)
            .is_some_and(|r| r.base_rtl)
    }

    /// The offset one step to the visual left/right of the caret, taking the
    /// row's direction into account.
    fn horizontal_step(&self, visual_right: bool) -> usize {
        let off = self.cursor_offset();
        let (row, col) = self.row_col(off);
        // Inside a bidi row, "one step right" is not "one byte forward, maybe
        // flipped". A Latin word or URL embedded in Persian runs the other way,
        // and the caret has to flow THROUGH it rather than jump to its far end
        // — so the step comes from the glyph order, via the row's map.
        if let Some(r) = self.bidi_map(row) {
            let dcol = self.display_col(row, col);
            let (k, local) = r.row_of(dcol);
            if let Some(rr) = r.rows.get(k)
                && let Some(next) = rr.map.step_visual(local, visual_right)
            {
                let target = self.line_starts()[row] + self.source_col(row, rr.start + next);
                // A visual step that doesn't move the caret in the DOCUMENT has
                // landed inside something atomic — an inline formula's spacer,
                // whose every display byte maps back to the span's start. The
                // logical stepper knows how to cross those (and how to hand the
                // formula to its editor), so defer to it rather than sitting
                // still.
                if target != off {
                    // Landing ON a spacer resolves to the span's START, and the
                    // "is the caret inside a formula?" test wants strictly
                    // inside — so approaching a formula from its end side, the
                    // caret stepped to the start, failed the test, and the next
                    // press left the formula behind. Step one byte in so the
                    // formula opens from either side.
                    let line_start = self.line_starts()[row];
                    let here = off.saturating_sub(line_start);
                    let lands_on_a_formula = markdown_syntax::inline_math_spans(self.line_str(row))
                        .into_iter()
                        .any(|s| {
                            line_start + s.start == target && !(s.start < here && here < s.end)
                        });
                    return if lands_on_a_formula {
                        target + 1
                    } else {
                        target
                    };
                }
            }
            // Off the end of this row: fall through to the logical neighbour,
            // which is what carries the caret onto the next row or line.
            return if visual_right != self.caret_row_is_rtl() {
                self.next_visible_boundary(off)
            } else {
                self.prev_visible_boundary(off)
            };
        }
        if visual_right {
            self.next_visible_boundary(off)
        } else {
            let target = self.prev_visible_boundary(off);
            // Same nudge as the bidi branch above: a leftward step over a formula's
            // spacer resolves to the span's START, which fails `left()`'s strictly-
            // inside test — so on plain LTR rows the editor never opened from the
            // right and the caret just seated at the formula's start (#77).
            let (trow, _) = self.row_col(target);
            let line_start = self.line_starts()[trow];
            let here = off.saturating_sub(line_start);
            let lands_on_a_formula = markdown_syntax::inline_math_spans(self.line_str(trow))
                .into_iter()
                .any(|s| line_start + s.start == target && !(s.start < here && here < s.end));
            if lands_on_a_formula {
                target + 1
            } else {
                target
            }
        }
    }

    /// The RTL layout for `row`, if it has one (see [`RtlRow`]).
    fn bidi_map(&self, row: usize) -> Option<&RtlRow> {
        self.rtl_rows.get(row).and_then(Option::as_ref)
    }

    /// Window-space bounds of the caret at `offset`, from the last paint's
    /// layout — for anchoring a popup (e.g. a slash menu) at a document offset.
    /// `None` before the first paint or if `offset`'s row isn't laid out.
    pub fn bounds_for_offset(&self, offset: usize) -> Option<Bounds<Pixels>> {
        let bounds = self.last_bounds?;
        let (row, col) = self.row_col(offset);
        let lh = self.line_h(row);
        let line = self.wrapped.get(row)?;
        let p = line_pos(line, self.bidi_map(row), self.display_col(row, col), lh)?;
        let top = bounds.top() + self.line_tops.get(row).copied().unwrap_or(px(0.)) + p.y;
        let x = bounds.left() + p.x + self.row_origin_x(row);
        Some(Bounds::from_corners(point(x, top), point(x, top + lh)))
    }

    /// The document text as an owned [`SharedString`]; use [`Self::text`] for a
    /// borrowed `&str`.
    pub fn value(&self) -> SharedString {
        self.content.clone().into()
    }

    /// Focus the editor so it receives keyboard input. (`set_cursor` only moves
    /// the caret; call this to enter edit mode, e.g. on a click into rendered text.)
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
    }

    /// Keep diagnostics valid across an edit at `edited` (the replaced byte
    /// range) that inserted `new_len` bytes: spans before the edit are left
    /// alone, spans after it are shifted by the size delta, and spans that
    /// overlap the edited text are dropped (that text changed, so they're
    /// stale). The host still recomputes the edited region on its own schedule —
    /// this just keeps the *other* spans correct so they don't all flicker off
    /// on every keystroke.
    fn remap_diagnostics(&mut self, edited: &Range<usize>, new_len: usize) {
        let delta = new_len as isize - (edited.end - edited.start) as isize;
        self.diagnostics.retain_mut(|d| {
            if d.range.end <= edited.start {
                true
            } else if d.range.start >= edited.end {
                d.range.start = (d.range.start as isize + delta) as usize;
                d.range.end = (d.range.end as isize + delta) as usize;
                true
            } else {
                false
            }
        });
    }

    // --- Cursor movement -----------------------------------------------------

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            // Collapse to the selection's VISUALLY left edge, which on an RTL
            // row is its logical end.
            let to = if self.caret_row_is_rtl() {
                self.selected_range.end
            } else {
                self.selected_range.start
            };
            self.move_to(to, cx);
            return;
        }
        if self.caret_in_table()
            && let Some(off) =
                self.table_move_horizontal(if self.caret_table_is_rtl() { 1 } else { -1 })
        {
            self.move_to(off, cx);
            return;
        }
        let off = self.horizontal_step(false);
        if let Some((range, source)) = self.inline_math_span_at(off) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: true,
                inline: true,
            });
            return;
        }
        if let Some((range, source)) = self.math_block_at(self.row_col(off).0) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: true,
                inline: false,
            });
            return;
        }
        // Left into a property panel opens its editor at the last field.
        if let Some((range, source)) = self.property_block_at(self.row_col(off).0) {
            cx.emit(EditorEvent::EditProperties {
                range,
                source,
                at_end: true,
                row: None,
            });
            return;
        }
        self.move_to(off, cx);
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let to = if self.caret_row_is_rtl() {
                self.selected_range.start
            } else {
                self.selected_range.end
            };
            self.move_to(to, cx);
            return;
        }
        if self.caret_in_table()
            && let Some(off) =
                self.table_move_horizontal(if self.caret_table_is_rtl() { -1 } else { 1 })
        {
            self.move_to(off, cx);
            return;
        }
        let off = self.horizontal_step(true);
        if let Some((range, source)) = self.inline_math_span_at(off) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: false,
                inline: true,
            });
            return;
        }
        if let Some((range, source)) = self.math_block_at(self.row_col(off).0) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: false,
                inline: false,
            });
            return;
        }
        // Right into a property panel opens its editor at the first field.
        if let Some((range, source)) = self.property_block_at(self.row_col(off).0) {
            cx.emit(EditorEvent::EditProperties {
                range,
                source,
                at_end: false,
                row: None,
            });
            return;
        }
        self.move_to(off, cx);
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        // In a table, step cell-to-cell keeping the column; at the table's edge
        // `table_move_vertical` returns `None` and a normal move exits the table.
        if self.caret_in_table()
            && let Some(off) = self.table_move_vertical(-1)
        {
            self.move_to(off, cx);
            return;
        }
        let off = self.move_vertical(-1);
        if let Some((range, source)) = self.inline_math_span_at(off) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: true,
                inline: true,
            });
            return;
        }
        if let Some((range, source)) = self.math_block_at(self.row_col(off).0) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: true,
                inline: false,
            });
            return;
        }
        // Arrowing UP into a property panel opens its editor at the LAST field
        // (entered from below), not the raw source.
        if let Some((range, source)) = self.property_block_at(self.row_col(off).0) {
            cx.emit(EditorEvent::EditProperties {
                range,
                source,
                at_end: true,
                row: None,
            });
            return;
        }
        // Set the caret directly (not via `move_to`) to keep the goal column.
        self.selected_range = off..off;
        self.last_edit = EditKind::Other;
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.caret_in_table()
            && let Some(off) = self.table_move_vertical(1)
        {
            self.move_to(off, cx);
            return;
        }
        let off = self.move_vertical(1);
        if let Some((range, source)) = self.inline_math_span_at(off) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: false,
                inline: true,
            });
            return;
        }
        if let Some((range, source)) = self.math_block_at(self.row_col(off).0) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: false,
                inline: false,
            });
            return;
        }
        // Arrowing DOWN into a property panel opens its editor at the FIRST field.
        if let Some((range, source)) = self.property_block_at(self.row_col(off).0) {
            cx.emit(EditorEvent::EditProperties {
                range,
                source,
                at_end: false,
                row: None,
            });
            return;
        }
        self.selected_range = off..off;
        self.last_edit = EditKind::Other;
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let off = self.horizontal_step(false);
        self.select_to(off, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let off = self.horizontal_step(true);
        self.select_to(off, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let off = self.move_vertical(-1);
        self.select_to(off, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let off = self.move_vertical(1);
        self.select_to(off, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let (row, col) = self.row_col(self.cursor_offset());
        let starts = self.line_starts();
        // Smart Home on a gutter line (list/task/quote): the marker is hidden
        // behind a painted bullet, so land on the first content character —
        // the raw line start would reveal the marker and let typing break it.
        // A second Home (at or inside the prefix) goes to the true start.
        let plen = self.hidden_prefix_len(row);
        let target = if plen > 0 && col > plen {
            starts[row] + plen
        } else {
            starts[row]
        };
        self.move_to(target, cx);
    }

    /// The hidden marker prefix length of logical `row` — list/task/quote
    /// lines draw their marker as a painted gutter and hide the source chars.
    /// 0 when the line has no gutter or markdown styling is off.
    fn hidden_prefix_len(&self, row: usize) -> usize {
        if self.markdown_style.is_none() {
            return 0;
        }
        let Some(&start) = self.line_starts().get(row) else {
            return 0;
        };
        let line = &self.content[start..self.line_end(row)];
        markdown_syntax::task_prefix(line)
            .map(|(l, ..)| l)
            .or_else(|| markdown_syntax::list_prefix(line).map(|(l, ..)| l))
            .or_else(|| markdown_syntax::blockquote_prefix(line))
            .unwrap_or(0)
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let (row, _) = self.row_col(self.cursor_offset());
        self.move_to(self.line_end(row), cx);
    }

    // --- Editing -------------------------------------------------------------

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            // Word-style image deletion: with the caret on an image row — or at
            // the start of the line just below one — remove the whole picture
            // (line + newline) as one edit, never stepping into its hidden
            // markdown character by character.
            let off = self.cursor_offset();
            let (row, col) = self.row_col(off);
            if let Some(range) = self.image_row_range(row).or_else(|| {
                (col == 0 && row > 0)
                    .then(|| self.image_row_range(row - 1))
                    .flatten()
            }) {
                self.replace_range(range, "", cx);
                cx.emit(EditorEvent::Changed);
                return;
            }
            // The same Word-style treatment for math: backspacing onto an
            // inline formula's closing `$` removes the whole formula, and at
            // the start of the line below a `$$` block removes the whole
            // block — never stripping one hidden delimiter and dumping raw
            // LaTeX. A caret strictly INSIDE a span (revealed source) still
            // edits character-wise.
            if let Some((range, _)) = self
                .inline_math_span_at(self.previous_boundary(off))
                .filter(|(r, _)| off == r.end)
            {
                self.replace_range(range, "", cx);
                cx.emit(EditorEvent::Changed);
                return;
            }
            if col == 0
                && row > 0
                && self.math_block_at(row).is_none() // inside = raw editing
                && let Some((range, _)) = self.math_block_at(row - 1)
            {
                let range = self.math_delete_range(range);
                self.replace_range(range, "", cx);
                cx.emit(EditorEvent::Changed);
                return;
            }
            // Backspacing from the line below a property panel joins as
            // usual — but the caret would land inside the panel and reveal
            // its raw `key:: value` source. Seat the in-place form after the
            // join instead (the same landing as arrowing in from below).
            let join_into_props = col == 0
                && row > 0
                && self.property_block_at(row).is_none()
                && self.property_block_at(row - 1).is_some();
            // Cditor-style around hidden formatting markers: delete the
            // previous VISIBLE character (never a marker byte), and take an
            // emptied construct's marker pair with it.
            if let Some(range) = self.fmt_delete_range(off, true) {
                self.replace_range(range, "", cx);
                cx.emit(EditorEvent::Changed);
                return;
            }
            let prev = self.previous_boundary(off);
            if off == prev {
                return;
            }
            self.select_to(prev, cx);
            self.replace_text_in_range(None, "", window, cx);
            if join_into_props {
                self.edit_properties_at_caret(true, cx);
            }
            return;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            // Word-style, mirroring `backspace`: the caret on an image row — or
            // at the end of the line just above one — removes the whole picture.
            let off = self.cursor_offset();
            let (row, _) = self.row_col(off);
            if let Some(range) = self.image_row_range(row).or_else(|| {
                (off == self.line_end(row))
                    .then(|| self.image_row_range(row + 1))
                    .flatten()
            }) {
                self.replace_range(range, "", cx);
                cx.emit(EditorEvent::Changed);
                return;
            }
            // Math mirrors of the backspace guards: deleting onto an inline
            // formula's opening `$` removes the whole formula; at the end of
            // the line above a `$$` block removes the whole block.
            if let Some((range, _)) = self
                .inline_math_span_at(self.next_boundary(off))
                .filter(|(r, _)| off == r.start)
            {
                self.replace_range(range, "", cx);
                cx.emit(EditorEvent::Changed);
                return;
            }
            if off == self.line_end(row)
                && self.math_block_at(row).is_none() // inside = raw editing
                && let Some((range, _)) = self.math_block_at(row + 1)
            {
                let range = self.math_delete_range(range);
                self.replace_range(range, "", cx);
                cx.emit(EditorEvent::Changed);
                return;
            }
            // Mirroring backspace's property join: pulling the panel's first
            // line up would seat a raw caret in the block — open the form.
            let join_into_props = off == self.line_end(row)
                && self.property_block_at(row).is_none()
                && self.property_block_at(row + 1).is_some();
            // Cditor-style around hidden formatting markers (see backspace).
            if let Some(range) = self.fmt_delete_range(off, false) {
                self.replace_range(range, "", cx);
                cx.emit(EditorEvent::Changed);
                return;
            }
            let next = self.next_boundary(off);
            if off == next {
                return;
            }
            self.select_to(next, cx);
            self.replace_text_in_range(None, "", window, cx);
            if join_into_props {
                self.edit_properties_at_caret(false, cx);
            }
            return;
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        // Inside a table, a raw newline would split the row's `| … |` markup.
        // Enter instead moves to the cell directly below (next row, same column,
        // spreadsheet-style); from the last row it exits onto a fresh line below
        // the table.
        if self.caret_in_table() {
            if let Some(off) = self.table_move_vertical(1)
                && self
                    .table_rows
                    .get(self.row_col(off).0)
                    .and_then(Option::as_ref)
                    .is_some_and(|t| !t.is_separator)
            {
                self.move_to(off, cx);
                return;
            }
            let (row, _) = self.row_col(self.cursor_offset());
            let mut last = row;
            while self
                .table_rows
                .get(last + 1)
                .and_then(Option::as_ref)
                .is_some()
            {
                last += 1;
            }
            let starts = self.line_starts();
            let end = starts.get(last + 1).map_or(self.content.len(), |&s| s - 1);
            self.selected_range = end..end;
            self.replace_text_in_range(None, "\n", window, cx);
            return;
        }
        // Inside a property panel a raw newline would split a `key:: value`
        // line. Enter opens the panel's editor instead — the same route as a
        // click or arrow-in (the form's own Enter then commits).
        if self.selected_range.is_empty() {
            let (row, _) = self.row_col(self.cursor_offset());
            if self.property_block_at(row).is_some() {
                self.edit_properties_at_caret(false, cx);
                return;
            }
        }
        // List auto-continuation: Enter on a list/task item opens the next item
        // (same marker + indent; ordered numbers increment); Enter on an *empty*
        // item removes the marker, exiting the list. Only with a collapsed
        // selection — a selection is just replaced by the newline.
        if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            let line_start = self.content[..cursor].rfind('\n').map_or(0, |i| i + 1);
            let line_end = self.content[line_start..]
                .find('\n')
                .map_or(self.content.len(), |i| line_start + i);
            let line = &self.content[line_start..line_end];
            if let Some((prefix_len, indent, ordered, num)) = markdown_syntax::list_prefix(line) {
                let task = markdown_syntax::task_prefix(line);
                let content_start = task.map_or(prefix_len, |(l, ..)| l);
                let empty = line.get(content_start..).unwrap_or("").trim().is_empty();
                let cont = if empty {
                    None
                } else {
                    let ws = &line[..indent];
                    let bullet = line.as_bytes()[indent] as char;
                    Some(if task.is_some() {
                        format!("\n{ws}{bullet} [ ] ")
                    } else if ordered {
                        format!("\n{ws}{}. ", num + 1)
                    } else {
                        format!("\n{ws}{bullet} ")
                    })
                };
                match cont {
                    // Empty item: clear the marker, leaving an empty line.
                    None => {
                        self.selected_range = line_start..line_end;
                        self.replace_text_in_range(None, "", window, cx);
                    }
                    Some(text) => self.replace_text_in_range(None, &text, window, cx),
                }
                return;
            }
        }
        self.replace_text_in_range(None, "\n", window, cx);
    }

    /// Toggle an inline wrapping marker (`**` bold, `*` italic, `` ` `` code)
    /// around the selection — the symmetric case of [`Self::toggle_wrap_pair`].
    fn toggle_wrap(&mut self, marker: &str, cx: &mut Context<Self>) {
        self.toggle_wrap_pair(marker, marker, cx);
    }

    /// Toggle an open/close marker pair (`<u>`/`</u>`, or a symmetric `**`)
    /// around the selection. No-op on an empty selection. Unwraps when the
    /// selection is already wrapped (markers just inside or just outside it),
    /// otherwise wraps — keeping the same text selected so presses toggle.
    fn toggle_wrap_pair(&mut self, open: &str, close: &str, cx: &mut Context<Self>) {
        let sel = self.selected_range.clone();
        if sel.start >= sel.end {
            return;
        }
        let (ol, cl) = (open.len(), close.len());
        let sel_text = &self.content[sel.clone()];
        let (range, new, new_sel) =
            if sel_text.len() >= ol + cl && sel_text.starts_with(open) && sel_text.ends_with(close)
            {
                // `**foo**` selected → strip the markers inside the selection.
                let inner = self.content[sel.start + ol..sel.end - cl].to_string();
                (sel.clone(), inner, sel.start..sel.end - ol - cl)
            } else if self.content[..sel.start].ends_with(open)
                && self.content[sel.end..].starts_with(close)
            {
                // `foo` selected with the markers just outside → strip them.
                (
                    sel.start - ol..sel.end + cl,
                    sel_text.to_string(),
                    sel.start - ol..sel.end - ol,
                )
            } else {
                // Plain → wrap.
                (
                    sel.clone(),
                    format!("{open}{sel_text}{close}"),
                    sel.start + ol..sel.end + ol,
                )
            };
        self.splice(range, new, new_sel);
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// The edit every wrap/unwrap ends in: replace `range` with `new`, select
    /// `new_sel`, keep undo and diagnostics in step. Callers emit `Changed`.
    fn splice(&mut self, range: Range<usize>, new: String, new_sel: Range<usize>) {
        self.record_edit(&range, &new);
        self.content.replace_range(range.clone(), &new);
        self.selected_range = new_sel;
        self.selection_reversed = false;
        self.goal_x = None;
        self.remap_diagnostics(&range, new.len());
    }

    /// Is the selection wrapped by `open`…`close` — the markers just inside or
    /// just outside it (the two shapes [`Self::toggle_wrap_pair`] unwraps)?
    fn wrapped_by(&self, open: &str, close: &str) -> bool {
        let sel = &self.selected_range;
        let text = &self.content[sel.clone()];
        (text.len() >= open.len() + close.len() && text.starts_with(open) && text.ends_with(close))
            || (self.content[..sel.start].ends_with(open)
                && self.content[sel.end..].starts_with(close))
    }

    /// Strip a `kind`-colored tag wrapping the selection (just inside or just
    /// outside it), whatever its color. `true` when one was removed; the
    /// caller emits `Changed`.
    fn strip_color(&mut self, kind: ColorKind) -> bool {
        use zorite_markdown::syntax::{StyledKind, styled_tag};
        let sel = self.selected_range.clone();
        if sel.is_empty() {
            return false;
        }
        let want = match kind {
            ColorKind::Text => StyledKind::Span,
            ColorKind::Highlight => StyledKind::Mark,
        };
        let is_tag = |tag: &str| styled_tag(tag).is_some_and(|t| t.kind == want);
        let close = want.close();
        // Markers just outside: `<span …>` ending at the selection, `</span>` after it.
        let before = &self.content[..sel.start];
        if let Some(lt) = before.rfind('<')
            && is_tag(&before[lt..])
            && self.content[sel.end..].starts_with(close)
        {
            let inner = self.content[sel.clone()].to_string();
            let len = inner.len();
            self.splice(lt..sel.end + close.len(), inner, lt..lt + len);
            return true;
        }
        // Markers just inside: the selection IS `<span …>body</span>`.
        let text = &self.content[sel.clone()];
        if text.starts_with('<')
            && let Some(gt) = text.find('>')
            && is_tag(&text[..=gt])
            && text.ends_with(close)
            && text.len() >= gt + 1 + close.len()
        {
            let inner = text[gt + 1..text.len() - close.len()].to_string();
            let len = inner.len();
            self.splice(sel.clone(), inner, sel.start..sel.start + len);
            return true;
        }
        false
    }

    /// Color the selection. `Some(rgba)` (`0xRRGGBBAA`) wraps it in the tag for
    /// `kind` — `<span style="color:#…">` or `<mark style="background:#…">`,
    /// the spellings Obsidian reads too — replacing any same-kind color
    /// already on it (and, for a highlight, a plain `==…==`); `None` removes
    /// that color. No-op on an empty selection.
    pub fn color_selection(&mut self, kind: ColorKind, rgba: Option<u32>, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            return;
        }
        let stripped = self.strip_color(kind);
        if kind == ColorKind::Highlight && self.wrapped_by("==", "==") {
            self.toggle_wrap_pair("==", "==", cx);
        }
        match rgba {
            Some(c) => {
                let hex = zorite_markdown::syntax::hex_color(c);
                let (open, close) = match kind {
                    ColorKind::Text => (format!("<span style=\"color:{hex}\">"), "</span>"),
                    ColorKind::Highlight => {
                        (format!("<mark style=\"background:{hex}\">"), "</mark>")
                    }
                };
                self.toggle_wrap_pair(&open, close, cx);
            }
            None if stripped => {
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            None => {}
        }
    }

    /// ⌘⇧H: toggle the plain `==…==` highlight on the selection. A colored
    /// highlight already on it gives way to the plain one.
    fn highlight(&mut self, _: &Highlight, _: &mut Window, cx: &mut Context<Self>) {
        self.strip_color(ColorKind::Highlight);
        self.toggle_wrap("==", cx);
    }

    /// One swatch row of the selection menu: `Text` draws an "A" in each
    /// color, `Highlight` a filled box (first the theme tint — the plain
    /// `==…==`); then clear (✕) and custom color (…, → `PickColor`).
    fn color_row(
        kind: ColorKind,
        mark_bg: Option<Hsla>,
        border: Hsla,
        hover: Hsla,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let colors: &[u32] = match kind {
            ColorKind::Text => &TEXT_COLORS,
            ColorKind::Highlight => &HIGHLIGHT_COLORS,
        };
        let tag = match kind {
            ColorKind::Text => "menu-text-color",
            ColorKind::Highlight => "menu-highlight",
        };
        let cell = move |id: (&'static str, usize)| {
            div()
                .id(id)
                .w(px(22.))
                .h(px(22.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
        };
        let box_swatch = move |fill: Hsla| {
            div()
                .w(px(14.))
                .h(px(14.))
                .rounded(px(3.))
                .bg(fill)
                .border_1()
                .border_color(border)
        };
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .px(px(6.))
            .py(px(3.));
        if let Some(bg) = mark_bg {
            row = row.child(cell((tag, 100)).child(box_swatch(bg)).on_mouse_down(
                MouseButton::Left,
                cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    editor.menu = None;
                    editor.highlight(&Highlight, window, cx);
                }),
            ));
        }
        for (i, &c) in colors.iter().enumerate() {
            let fill: Hsla = rgba(c).into();
            let swatch = match kind {
                ColorKind::Text => div()
                    .text_color(fill)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(14.))
                    .child("A"),
                ColorKind::Highlight => box_swatch(fill),
            };
            row = row.child(cell((tag, i)).child(swatch).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    editor.menu = None;
                    editor.color_selection(kind, Some(c), cx);
                }),
            ));
        }
        row.child(
            cell((tag, 200))
                .text_size(px(12.))
                .child("\u{2715}")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        editor.menu = None;
                        editor.color_selection(kind, None, cx);
                    }),
                ),
        )
        .child(
            cell((tag, 201))
                .text_size(px(12.))
                .child("\u{2026}")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let position = editor.menu.take().map(|m| m.anchor).unwrap_or_default();
                        cx.emit(EditorEvent::PickColor {
                            highlight: kind == ColorKind::Highlight,
                            position,
                        });
                    }),
                ),
        )
    }

    fn bold(&mut self, _: &Bold, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_wrap("**", cx);
    }

    fn italic(&mut self, _: &Italic, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_wrap("*", cx);
    }

    fn code(&mut self, _: &Code, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_wrap("`", cx);
    }

    fn strike(&mut self, _: &Strike, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_wrap("~~", cx);
    }

    fn underline(&mut self, _: &Underline, _: &mut Window, cx: &mut Context<Self>) {
        // Markdown has no underline — the `<u>` tag, which both views honor.
        self.toggle_wrap_pair("<u>", "</u>", cx);
    }

    /// Tab: on a list/quote item, indent the whole item one level (`tab_indent`
    /// spaces at the line start, caret shifts with it); elsewhere insert that many
    /// spaces at the caret (replacing any selection).
    fn indent(&mut self, _: &Indent, window: &mut Window, cx: &mut Context<Self>) {
        // In a table, Tab moves to the next cell rather than indenting.
        if self.caret_in_table() {
            if let Some(offset) = self.table_cell_nav(true) {
                self.move_to(offset, cx);
            }
            return;
        }
        let cursor = self.cursor_offset();
        let line_start = self.content[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let line_end = self.content[line_start..]
            .find('\n')
            .map_or(self.content.len(), |i| line_start + i);
        let line = &self.content[line_start..line_end];
        let item = markdown_syntax::list_prefix(line);
        let is_item = item.is_some() || markdown_syntax::blockquote_prefix(line).is_some();
        let indent = " ".repeat(self.tab_indent);
        if !is_item {
            self.replace_text_in_range(None, &indent, window, cx);
            return;
        }
        // Indenting an ordered item starts a NESTED list, so its number
        // becomes the new list's start: rewrite it to 1. (Both views
        // renumber the items after it, so only the start digit matters.)
        let (range, new_text) = match item {
            Some((_, ws, true, _)) => {
                let de = ws + line[ws..].bytes().take_while(u8::is_ascii_digit).count();
                (
                    line_start..line_start + de,
                    format!("{indent}{}1", &line[..ws]),
                )
            }
            _ => (line_start..line_start, indent),
        };
        self.record_edit(&range, &new_text);
        let delta = new_text.len() as isize - (range.end - range.start) as isize;
        self.content.replace_range(range.clone(), &new_text);
        let caret = (cursor as isize + delta).max(line_start as isize) as usize;
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.goal_x = None;
        self.remap_diagnostics(&range, new_text.len());
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Shift+Tab: outdent the caret's line — remove up to `tab_indent` leading
    /// spaces (or one leading tab) from the line start. No-op if there's none.
    fn outdent(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        // In a table, Shift+Tab moves to the previous cell rather than outdenting.
        if self.caret_in_table() {
            if let Some(offset) = self.table_cell_nav(false) {
                self.move_to(offset, cx);
            }
            return;
        }
        let cursor = self.cursor_offset();
        let line_start = self.content[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let line = &self.content[line_start..];
        let removed = if line.starts_with('\t') {
            1
        } else {
            line.bytes()
                .take(self.tab_indent)
                .take_while(|b| *b == b' ')
                .count()
        };
        if removed == 0 {
            return;
        }
        let range = line_start..line_start + removed;
        self.record_edit(&range, "");
        self.content.replace_range(range.clone(), "");
        let caret = cursor.saturating_sub(removed).max(line_start);
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.goal_x = None;
        self.remap_diagnostics(&range, 0);
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let item = cx.read_from_clipboard();
        // A copied FILE also carries its path as text; inserting that string
        // is never what a paste meant. Treat file and image clipboards as
        // not-ours: fall through so a host binding on the same keys
        // (zorite's image/file paste) can run.
        let has_files = item.as_ref().is_some_and(|i| {
            i.entries()
                .iter()
                .any(|e| matches!(e, gpui::ClipboardEntry::ExternalPaths(_)))
        });
        match item.and_then(|i| i.text()).filter(|_| !has_files) {
            Some(text) => {
                // Normalize foreign line endings — a Windows/browser copy
                // carries \r\n (or bare \r), and a literal \r in the buffer
                // garbles rendering + desyncs the \n-based column math.
                let mut text = if text.contains('\r') {
                    text.replace("\r\n", "\n").replace('\r', "\n")
                } else {
                    text
                };
                // Inside a table cell, a raw paste of newlines/pipes would
                // break the `| … |` row markup (Enter is guarded the same
                // way): flatten newlines and escape pipes so the paste stays
                // one cell's content.
                if self.caret_in_table() && text.contains(['\n', '|']) {
                    // Unescape-then-escape so text already carrying `\|`
                    // doesn't double up into `\\|` (an escaped backslash
                    // followed by a live separator).
                    text = text
                        .trim_end_matches('\n')
                        .replace('\n', " ")
                        .replace("\\|", "|")
                        .replace('|', "\\|");
                } else if self.markdown_style.is_some() && text.contains("$$") {
                    // Words-attached `$$` fences / words-mixed pairs in pasted
                    // text split onto their own lines (issue #54) — same
                    // normalization typing gets.
                    if let std::borrow::Cow::Owned(n) =
                        zorite_markdown::syntax::normalize_math_fences(&text)
                    {
                        text = n;
                    }
                }
                self.replace_text_in_range(None, &text, window, cx);
            }
            None => cx.propagate(),
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let range = self.copy_range();
            // Ordered markers copy at their DISPLAYED positions (still digit
            // markdown), so a paste counts the way the screen did.
            let text = if self.markdown_style.is_some() {
                markdown_syntax::renumber_copy(&self.content, range)
            } else {
                self.content[range].to_string()
            };
            self.write_clipboard(text, cx);
        }
    }

    /// Copy the selection as the raw markdown ONLY — no host clipboard
    /// flavors — for pasting literal source into rich surfaces (the context
    /// menu's "Copy as Markdown"). Same selection/renumber rules as `copy`.
    pub fn copy_plain(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let range = self.copy_range();
            let text = if self.markdown_style.is_some() {
                markdown_syntax::renumber_copy(&self.content, range)
            } else {
                self.content[range].to_string()
            };
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Route a Copy/Cut payload through the host's clipboard writer when one
    /// is set (see [`Self::set_clipboard_writer`]), else gpui's plain copy.
    fn write_clipboard(&self, text: String, cx: &mut Context<Self>) {
        match &self.clipboard_writer {
            Some(writer) => writer(&text, cx),
            None => cx.write_to_clipboard(ClipboardItem::new_string(text)),
        }
    }

    /// What a copy takes: the selection — extended back over the first
    /// line's hidden list/task/quote prefix when the selection is multi-line
    /// and starts exactly at that line's body start. With markers painted
    /// (not text), "select the whole list" visually anchors AFTER the first
    /// `1. `, so a verbatim copy dropped the first marker while every other
    /// line kept its own. Raw mode (no markdown style) copies verbatim.
    fn copy_range(&self) -> std::ops::Range<usize> {
        let (start, end) = (
            self.selected_range.start.min(self.selected_range.end),
            self.selected_range.start.max(self.selected_range.end),
        );
        if self.markdown_style.is_none() || !self.content[start..end].contains('\n') {
            return start..end;
        }
        let line_start = self.content[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = self.content[line_start..]
            .find('\n')
            .map_or(self.content.len(), |i| line_start + i);
        let line = &self.content[line_start..line_end];
        let prefix_len = markdown_syntax::task_prefix(line)
            .map(|(l, ..)| l)
            .or_else(|| markdown_syntax::list_prefix(line).map(|(l, ..)| l))
            .or_else(|| markdown_syntax::blockquote_prefix(line));
        match prefix_len {
            Some(plen) if start == line_start + plen => line_start..end,
            _ => start..end,
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.write_clipboard(self.content[self.selected_range.clone()].to_string(), cx);
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // --- Undo / redo ---------------------------------------------------------

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.snapshot());
            self.restore(prev);
            self.last_edit = EditKind::Other;
            cx.notify();
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.snapshot());
            self.restore(next);
            self.last_edit = EditKind::Other;
            cx.notify();
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            content: self.content.clone(),
            // The forward caret (selection end), so undoing a backspace lands the
            // caret after the restored text rather than inside it.
            caret: self.selected_range.end,
        }
    }

    fn restore(&mut self, s: Snapshot) {
        self.content_gen += 1;
        self.content = s.content;
        let caret = s.caret.min(self.content.len());
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    /// Snapshot the pre-edit state for undo, coalescing a run of single-grapheme
    /// inserts (or a run of deletes) into one undo step so typing isn't undone
    /// one character at a time.
    fn record_edit(&mut self, range: &Range<usize>, new_text: &str) {
        self.content_gen += 1;
        let kind = if new_text.is_empty() {
            EditKind::Delete
        } else if range.start == range.end
            && new_text != "\n"
            && new_text.graphemes(true).count() == 1
        {
            EditKind::Insert(range.start + new_text.len())
        } else {
            EditKind::Other
        };
        let coalesce = match (self.last_edit, kind) {
            (EditKind::Insert(end), EditKind::Insert(_)) => end == range.start,
            (EditKind::Delete, EditKind::Delete) => true,
            _ => false,
        };
        if !coalesce {
            self.undo_stack.push(self.snapshot());
            if self.undo_stack.len() > UNDO_LIMIT {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
        self.last_edit = kind;
        // A keystroke is one typed grapheme (incl. typed over a selection — that's
        // an auto-pair "wrap") or a single-char backspace. Multi-char edits (paste,
        // table ops, …) are not, so auto-pairing skips them.
        self.last_edit_keystroke = (new_text != "\n" && new_text.graphemes(true).count() == 1)
            || (new_text.is_empty() && self.content[range.clone()].graphemes(true).count() == 1);
    }

    // --- Mouse ---------------------------------------------------------------

    /// If logical `row` is inside a `$$…$$` block, the block's byte range in the document
    /// (both fences) and the LaTeX between them — so a double-click can hand it to the host's
    /// structural editor.
    fn math_block_at(&self, row: usize) -> Option<(Range<usize>, SharedString)> {
        // The structural LaTeX editor is a WYSIWYG affordance (markdown_style is set only in
        // live-preview mode). In raw-markdown mode the user edits `$$…$$` as plain text, so
        // report no math block here — clicks / arrows / `/math` stay in the text editor.
        self.markdown_style.as_ref()?;
        let starts = self.line_starts();
        let blocks = markdown_syntax::math_blocks(&self.content);
        blocks
            .iter()
            .find(|(r, _)| r.contains(&row))
            .or_else(|| {
                // A `<!-- math:ALIGN -->` marker row belongs to the block directly
                // below it: it's invisible in WYSIWYG, so a caret seated there —
                // e.g. arrow-up returns offset 0 when the block opens the document
                // (#77) — would reveal the raw rows instead of opening the editor.
                markdown_syntax::math_align_marker(self.line_str(row))?;
                blocks.iter().find(|(r, _)| r.start == row + 1)
            })
            .map(|(r, source)| {
                (
                    starts[r.start]..self.line_end(r.end - 1),
                    source.clone().into(),
                )
            })
    }

    /// A `$$` block's byte range grown for deletion: takes in the
    /// `<!-- math:ALIGN -->` marker line directly above (removing the block
    /// alone would orphan it) and the trailing newline.
    fn math_delete_range(&self, range: Range<usize>) -> Range<usize> {
        let mut start = range.start;
        let (row, _) = self.row_col(range.start);
        if row > 0 {
            let prev_start = self.line_starts()[row - 1];
            let prev = &self.content[prev_start..self.line_end(row - 1)];
            if markdown_syntax::math_align_marker(prev).is_some() {
                start = prev_start;
            }
        }
        start..(range.end + 1).min(self.content.len())
    }

    /// Route a landing offset into an atomic construct the way the plain
    /// arrows do: an inline `$…$` span strictly containing it, or a `$$`
    /// block / property-panel row, opens its in-place editor instead of
    /// seating a raw caret (which would reveal hidden source). Returns true
    /// when handled — word-jumps (⌥←/→) stop there.
    fn enter_construct_at(&mut self, off: usize, at_end: bool, cx: &mut Context<Self>) -> bool {
        if let Some((range, source)) = self.inline_math_span_at(off) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end,
                inline: true,
            });
            return true;
        }
        let (row, _) = self.row_col(off);
        if let Some((range, source)) = self.math_block_at(row) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end,
                inline: false,
            });
            return true;
        }
        if let Some((range, source)) = self.property_block_at(row) {
            let block_row = row - self.row_col(range.start).0;
            cx.emit(EditorEvent::EditProperties {
                range,
                source,
                at_end,
                row: Some(block_row),
            });
            return true;
        }
        false
    }

    /// If the caret sits inside a property block, ask the host to seat the
    /// in-place form there (focused on the caret's row; `at_end` = caret at
    /// the value's end) — the recovery for any edit that lands a raw caret in
    /// the panel, mirroring what arrows and clicks do on entry.
    fn edit_properties_at_caret(&mut self, at_end: bool, cx: &mut Context<Self>) {
        let (row, _) = self.row_col(self.cursor_offset());
        if let Some((range, source)) = self.property_block_at(row) {
            let block_row = row - self.row_col(range.start).0;
            cx.emit(EditorEvent::EditProperties {
                range,
                source,
                at_end,
                row: Some(block_row),
            });
        }
    }

    /// The property block whose lines cover `row`, as an absolute byte range +
    /// source — so a click or an arrow into the panel opens the property editor
    /// instead of landing in (and revealing) the raw `key:: value` lines.
    /// WYSIWYG-only, like [`Self::math_block_at`].
    fn property_block_at(&self, row: usize) -> Option<(Range<usize>, SharedString)> {
        self.markdown_style.as_ref()?;
        let region = markdown_syntax::property_regions(&self.content)
            .into_iter()
            .find(|r| r.contains(&row))?;
        let start = *self.line_starts().get(region.start)?;
        let end = self.line_end(region.end - 1);
        Some((start..end, self.content[start..end].to_string().into()))
    }

    /// The inline `$…$` span strictly containing source byte `off` (between the `$` delimiters),
    /// as an absolute byte range + inner LaTeX — so arrowing the caret into a formula opens its
    /// structural editor instead of landing in (and revealing) the raw source. WYSIWYG-only.
    fn inline_math_span_at(&self, off: usize) -> Option<(Range<usize>, SharedString)> {
        self.markdown_style.as_ref()?;
        let (row, _) = self.row_col(off);
        let line_start = *self.line_starts().get(row)?;
        let line = self.line_str(row);
        let col = off.saturating_sub(line_start);
        markdown_syntax::inline_math_spans(line)
            .into_iter()
            .find(|s| s.start < col && col < s.end)
            .map(|s| {
                (
                    line_start + s.start..line_start + s.end,
                    SharedString::from(markdown_syntax::inline_math_latex(line, &s).to_string()),
                )
            })
    }

    /// If the caret sits inside a `$$…$$` block, ask the host to open the structural editor
    /// for it (caret at the formula's start). Lets the host turn a freshly-inserted, empty
    /// math block (the `/math` snippet) straight into a live editor instead of raw source.
    pub fn edit_math_at_caret(&mut self, cx: &mut Context<Self>) {
        let (row, _) = self.row_col(self.cursor_offset());
        if let Some((range, source)) = self.math_block_at(row) {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: false,
                inline: false,
            });
        }
    }

    /// The property block covering the caret's line, if any (WYSIWYG-only, like
    /// [`Self::edit_math_at_caret`]) — so the host can open the property editor
    /// on a freshly-inserted `/property` line instead of leaving raw source.
    pub fn property_block_at_caret(&self) -> Option<(Range<usize>, SharedString)> {
        let (row, _) = self.row_col(self.cursor_offset());
        self.property_block_at(row)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A press on an image's corner grip starts a resize drag — this takes
        // precedence over placing the caret on the image row (which the press
        // would otherwise do). The image keeps its bounds; the drag previews a new
        // width and release writes `{width=N}` (see on_mouse_move / on_mouse_up).
        if let Some((line, width)) = self.grip_at(event.position) {
            self.image_resize = Some(ImageResize {
                line,
                start_width: width,
                start_x: event.position.x,
                width,
            });
            self.is_selecting = false;
            self.menu = None;
            self.table_menu = None;
            self.image_menu = None;
            self.prop_menu = None;
            cx.notify();
            return;
        }
        // A press on a task checkbox toggles it (☐↔☑) instead of placing the
        // caret — the box sits in the gutter, so this never competes with editing
        // the body text. Same length swap, so the caret/selection stay valid.
        if let Some(row) = self.checkbox_at(event.position) {
            let range = self.line_starts()[row]..self.line_end(row);
            if let Some(new_line) =
                markdown_syntax::toggle_task_checkbox(&self.content[range.clone()])
            {
                self.record_edit(&range, &new_line);
                self.content =
                    self.content[..range.start].to_owned() + &new_line + &self.content[range.end..];
                self.remap_diagnostics(&range, new_line.len());
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            return;
        }
        // A press on a code card's chrome: Copy writes the block's body to the
        // clipboard; the language tag opens the picker — neither places the caret.
        if let Some((on_copy, fence_row)) = self.code_chip_at(event.position) {
            if on_copy {
                if let Some((_, body)) = self.code_block_at(fence_row) {
                    let text = self.content[body].to_string();
                    self.write_clipboard(text, cx);
                }
            } else if !self.code_langs.is_empty() {
                self.code_lang_menu = Some((fence_row, event.position));
                cx.notify();
            }
            return;
        }
        // A press on a foldable callout's chevron flips its `-`/`+` fold char
        // (folding/unfolding the body) instead of placing the caret — the same
        // toggle-in-source model as the task checkbox.
        if let Some(row) = self.alert_fold_at(event.position) {
            let start = self.line_starts()[row];
            let line = &self.content[start..self.line_end(row)];
            if let Some((at, folded)) = zorite_markdown::syntax::alert_fold_char(line) {
                let range = start + at..start + at + 1;
                let repl = if folded { "+" } else { "-" };
                self.record_edit(&range, repl);
                self.content.replace_range(range.clone(), repl);
                self.remap_diagnostics(&range, 1);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            return;
        }
        // A press on a heading's fold chevron toggles its section collapsed —
        // view-local state, not an edit (markdown has no heading-fold syntax).
        if let Some(row) = self.heading_fold_at(event.position) {
            let start = self.line_starts()[row];
            let end = self.line_end(row);
            let key = self.content[start..end].trim().to_string();
            if !self.folded_headings.remove(&key) {
                // Folding with the caret inside the section would no-op
                // (reveal-on-caret keeps it open) — seat the caret at the
                // heading's end first.
                let single = std::collections::HashSet::from([key.clone()]);
                let crow = self.row_col(self.cursor_offset()).0;
                if markdown_syntax::heading_fold_regions(&self.content, &single)
                    .iter()
                    .any(|r| crow > r.start && crow < r.end)
                {
                    self.move_to(end, cx);
                }
                self.folded_headings.insert(key);
            }
            cx.notify();
            return;
        }
        // Left-click a file chip (e.g. a PDF embed) opens it rather than editing —
        // the host handles the link. Right-click edits (see on_right_mouse_down).
        if let Some((src, wiki)) = self.chip_at(event.position) {
            cx.emit(if wiki {
                EditorEvent::OpenWikiLink(src)
            } else {
                EditorEvent::OpenLink(src)
            });
            return;
        }
        // Left-click an inline `$…$` formula opens its structural editor at the formula's spot
        // (the host seats it). Shift extends a selection; Control-click is the secondary button.
        if !event.modifiers.shift
            && !event.modifiers.control
            && let Some((range, source)) = self.inline_math_at(event.position)
        {
            cx.emit(EditorEvent::EditMath {
                range,
                source,
                at_end: true,
                inline: true,
            });
            return;
        }
        // Left-click a property-panel pill opens its target — the pill is painted
        // over a collapsed source line, so hit-test the painted bounds directly
        // (not the raw text like `link_at` below).
        if event.click_count == 1
            && !event.modifiers.shift
            && !event.modifiers.control
            && let Some((_, hit)) = self
                .prop_pill_rects
                .iter()
                .find(|(b, _)| b.contains(&event.position))
        {
            match hit {
                zorite_markdown::syntax::LinkHit::Page(t) => {
                    cx.emit(EditorEvent::OpenWikiLink(t.clone().into()))
                }
                zorite_markdown::syntax::LinkHit::BlockRef(id) => {
                    cx.emit(EditorEvent::OpenWikiLink(format!("#^{id}").into()))
                }
                zorite_markdown::syntax::LinkHit::Url(u) => {
                    cx.emit(EditorEvent::OpenLink(u.clone().into()))
                }
            }
            return;
        }
        // Left-click on (or beside) a property panel opens the in-place editor
        // for its whole block — the panel edits its properties, not the raw
        // markdown. Keyed off the ROW the click maps to, not the painted panel
        // rects, so a click in the empty space right of the panel opens the
        // editor too instead of seating the caret in (and revealing) the source.
        if event.click_count == 1 && !event.modifiers.shift && !event.modifiers.control {
            let offset = self.index_for_mouse_position(event.position);
            let row = self.row_col(offset).0;
            if let Some((range, source)) = self.property_block_at(row) {
                // Which property line within the block was clicked — the host
                // focuses that row's field instead of always the first.
                let block_row = row - self.row_col(range.start).0;
                cx.emit(EditorEvent::EditProperties {
                    range,
                    source,
                    at_end: false,
                    row: Some(block_row),
                });
                return;
            }
        }
        // Left-click an inline image opens a full-size preview (host-shown).
        if !event.modifiers.shift
            && !event.modifiers.control
            && let Some(src) = self.inline_image_at(event.position)
        {
            cx.emit(EditorEvent::PreviewImage(src));
            return;
        }
        // Left-click a link navigates, like the reading view: a `[[wiki]]` /
        // `#tag` opens that page, a `[text](url)` opens the url — consistent
        // with chips and inline math above. Only a plain single click: a
        // double-click still selects the word, shift still extends the
        // selection, and the caret goes anywhere else as usual (to edit a
        // link's own text, click beside it and arrow in — reveal-on-caret).
        if event.click_count == 1
            && !event.modifiers.shift
            && !event.modifiers.control
            && self.markdown_style.is_some()
        {
            let offset = self.index_for_mouse_position(event.position);
            let (row, _) = self.row_col(offset);
            let start = self.line_starts()[row];
            let line = &self.content[start..self.line_end(row)];
            match markdown_syntax::link_at(line, offset - start) {
                Some(markdown_syntax::LinkHit::Page(title)) => {
                    cx.emit(EditorEvent::OpenWikiLink(title.into()));
                    return;
                }
                Some(markdown_syntax::LinkHit::BlockRef(id)) => {
                    cx.emit(EditorEvent::OpenWikiLink(format!("#^{id}").into()));
                    return;
                }
                Some(markdown_syntax::LinkHit::Url(url)) => {
                    cx.emit(EditorEvent::OpenLink(url.into()));
                    return;
                }
                None => {}
            }
            // The reference-count badge painted over a hidden ` ^id` anchor:
            // a click on its (replaced) range lists the referencers. Only when
            // the anchor is hidden — with the caret on the line the raw text
            // is revealed for editing and clicks place the caret as usual.
            if self.row_col(self.selected_range.start).0 != row
                && let Some((at, id)) = zorite_markdown::syntax::block_id(line)
                && offset - start >= at
                && self
                    .markdown_style
                    .as_ref()
                    .and_then(|st| st.block_ref_count.as_ref().map(|f| f(id)))
                    .unwrap_or(0)
                    > 0
            {
                cx.emit(EditorEvent::OpenWikiLink(format!("refs:^{id}").into()));
                return;
            }
        }
        // A press on a table's hover "+" strip adds a row (below) or column (right).
        // The insert APIs are caret-driven, so seat the caret in the table to target
        // them — but capture the user's cell first and restore it after, so the
        // caret stays put instead of following the new row/column.
        if let Some(row) = self.table_add_row_at(event.position) {
            let keep = self.caret_table_cell_pos();
            if let Some(off) = self.cell_start_offset(row, 0) {
                self.selected_range = off..off;
                self.insert_table_row(true, cx);
            }
            if let Some((r, c, ic)) = keep {
                let caret = self.caret_pos_for_cell(r, c, ic);
                self.selected_range = caret..caret;
                cx.notify();
            }
            return;
        }
        if let Some((row, col)) = self.table_add_col_at(event.position) {
            let keep = self.caret_table_cell_pos();
            if let Some(off) = self.cell_start_offset(row, col) {
                self.selected_range = off..off;
                self.insert_table_column(true, cx);
            }
            if let Some((r, c, ic)) = keep {
                let caret = self.caret_pos_for_cell(r, c, ic);
                self.selected_range = caret..caret;
                cx.notify();
            }
            return;
        }
        // A press on a row/column delete "−" handle removes that row/column (seat
        // the caret in it, then reuse the caret-driven delete APIs).
        if let Some((rect, row)) = self.table_row_del
            && rect.contains(&event.position)
        {
            if let Some(off) = self.cell_start_offset(row, 0) {
                self.selected_range = off..off;
                self.delete_table_row(cx);
            }
            return;
        }
        if let Some((rect, row, col)) = self.table_col_del
            && rect.contains(&event.position)
        {
            if let Some(off) = self.cell_start_offset(row, col) {
                self.selected_range = off..off;
                self.delete_table_column(cx);
            }
            return;
        }
        // A press on a wide table's scroll thumb starts a thumb drag — the
        // table scrolls with the pointer (see `on_mouse_move`).
        if let Some(&TableThumb { header, .. }) = self
            .table_thumbs
            .iter()
            .find(|t| t.grab.contains(&event.position))
        {
            let sx = self.table_scroll_x.get(&header).copied().unwrap_or(0.);
            self.table_thumb_drag = Some((header, event.position.x, sx));
            self.is_selecting = false;
            cx.notify();
            return;
        }
        // A press on a column border's resize band starts a drag — the column
        // resizes live; release persists the width (issue #16). A DOUBLE-click
        // auto-fits the column to its content instead (the Excel/Sheets
        // convention for a column border).
        if let Some(&(_, header_row, col, width)) = self
            .table_col_resize_rects
            .iter()
            .find(|(band, ..)| band.contains(&event.position))
        {
            if event.click_count == 2 {
                self.autofit_table_col(header_row, col, window, cx);
                return;
            }
            self.table_col_resize = Some(TableColResize {
                header_row,
                col,
                start_x: event.position.x,
                orig: width,
                width,
            });
            self.is_selecting = false;
            cx.notify();
            return;
        }
        // A click on a table cell drops the caret inside the cell, not in the raw
        // `| … |` source.
        let offset = self
            .table_offset_at(event.position, window)
            .unwrap_or_else(|| self.index_for_mouse_position(event.position));
        self.menu = None;
        self.table_menu = None;
        self.image_menu = None;
        self.prop_menu = None;
        self.code_lang_menu = None;
        self.goal_x = None;
        self.last_edit = EditKind::Other;
        match event.click_count {
            // Double-click selects the word under the cursor. On a $$…$$ block
            // or property panel the FIRST click of the pair already opened the
            // in-place editor — word-selecting the hidden source underneath
            // would fight the seated editor, so those clicks are swallowed.
            2 => {
                let (row, _) = self.row_col(offset);
                if self.math_block_at(row).is_some() || self.property_block_at(row).is_some() {
                    return;
                }
                self.is_selecting = false;
                self.selected_range = self.word_range_at(offset).unwrap_or(offset..offset);
                self.selection_reversed = false;
                cx.notify();
            }
            // Triple-click (or more): select the whole logical line — except on
            // a block construct, where it would select the raw hidden fences.
            n if n >= 3 => {
                let (row, _) = self.row_col(offset);
                if self.math_block_at(row).is_some() || self.property_block_at(row).is_some() {
                    return;
                }
                self.is_selecting = false;
                let start = self.line_starts()[row];
                self.selected_range = start..self.line_end(row);
                self.selection_reversed = false;
                cx.notify();
            }
            // Single click: place the caret, or extend the selection with Shift.
            _ => {
                // A single left-click on a $$…$$ block opens the structural editor in
                // place; a Control-click (macOS secondary click, which AppKit delivers as
                // a left button + control modifier, NOT a right button) shows the formula
                // context menu instead. Shift-click still extends the selection.
                if !event.modifiers.shift {
                    let (row, _) = self.row_col(offset);
                    if let Some((range, source)) = self.math_block_at(row) {
                        if event.modifiers.control {
                            self.focus(window, cx);
                            cx.emit(EditorEvent::MathMenu {
                                source,
                                position: event.position,
                            });
                        } else {
                            cx.emit(EditorEvent::EditMath {
                                range,
                                source,
                                at_end: true,
                                inline: false,
                            });
                        }
                        return;
                    }
                }
                self.is_selecting = true;
                if event.modifiers.shift {
                    self.select_to(offset, cx);
                } else {
                    self.move_to(offset, cx);
                }
            }
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        // End a gutter block drag: splice the block at the drop boundary.
        if let Some((bs, be, t)) = self.line_drag.take() {
            self.apply_line_drag(bs, be, t, cx);
            cx.notify();
            return;
        }
        // End an image-resize drag by persisting the rounded width as `{width=N}`
        // in that image's source line (through the normal mutation path, so it
        // joins the undo history + emits Changed); the next paint shows the saved
        // size and the live override clears.
        if let Some(resize) = self.image_resize.take() {
            self.commit_image_resize(resize, cx);
            cx.notify();
            return;
        }
        // End a table scroll-thumb drag (the live offsets are already stored).
        if self.table_thumb_drag.take().is_some() {
            cx.notify();
            return;
        }
        // End a column-border drag by persisting every column's width into the
        // table marker's `cols=` list (one undo step, emits Changed).
        if let Some(resize) = self.table_col_resize.take() {
            self.commit_table_col_widths(resize, cx);
            cx.notify();
            return;
        }
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Link under the pointer → `HoverLink` on change (a host shows a preview
        // card there). Painted boxes from the last frame, like the hand cursor.
        let over_link = self
            .link_rects
            .iter()
            .chain(self.prop_pill_rects.iter())
            .find(|(b, _)| b.contains(&event.position))
            .map(|(b, hit)| (hit.clone(), *b));
        if over_link != self.hovered_link {
            self.hovered_link = over_link.clone();
            cx.emit(EditorEvent::HoverLink(over_link));
        }
        // While dragging an image's grip, track the pointer: the new width is the
        // grab width plus the horizontal travel, floored at `IMG_MIN_W` and capped
        // to the content width left of the image's inset (so a bulleted image's cap
        // matches `block_img`, no snap-back on release, and it can't run off the
        // page). The paint reads this live width for the dragged image (aspect
        // preserved).
        // While dragging a block by its gutter grip, track the drop boundary
        // (snapped out of rendered regions); paint draws the indicator there.
        if let Some((bs, be, t)) = self.line_drag {
            let b = self.snap_drop_boundary(self.drop_boundary_at(event.position));
            if b != t {
                self.line_drag = Some((bs, be, b));
                cx.notify();
            }
            return;
        }
        // While dragging a wide table's scroll thumb, track the pointer: thumb
        // travel maps to content scroll through the committed factor.
        if let Some((header, grab_x, start_sx)) = self.table_thumb_drag {
            let factor = self
                .table_thumbs
                .iter()
                .find(|t| t.header == header)
                .map_or(0., |t| t.factor);
            let sx = start_sx + f32::from(event.position.x - grab_x) * factor;
            // Clamp at use like the wheel path — the paint clamps too, but a
            // sane stored value keeps other consumers simple.
            let sx = sx.max(0.);
            if self.table_scroll_x.get(&header).copied().unwrap_or(0.) != sx {
                self.table_scroll_x.insert(header, sx);
                cx.notify();
            }
            return;
        }
        // While dragging a table column's border, track the pointer: the new
        // width is the grab width plus the travel, floored so the column can't
        // vanish. Shaping applies it live (see `table_column_widths`).
        if let Some(resize) = self.table_col_resize {
            let dx = f32::from(event.position.x - resize.start_x);
            let width = (resize.orig + dx).max(24.);
            if let Some(r) = self.table_col_resize.as_mut() {
                r.width = width;
            }
            cx.notify();
            return;
        }
        if let Some(resize) = self.image_resize {
            let avail = self
                .last_bounds
                .map_or(f32::MAX, |b| f32::from(b.size.width))
                - f32::from(self.line_inset(resize.line));
            let max_w = avail.max(IMG_MIN_W);
            let dx = f32::from(event.position.x - resize.start_x);
            let width = (resize.start_width + dx).clamp(IMG_MIN_W, max_w);
            if let Some(r) = self.image_resize.as_mut() {
                r.width = width;
            }
            cx.notify();
            return;
        }
        if self.is_selecting {
            let offset = self
                .table_offset_at(event.position, window)
                .unwrap_or_else(|| self.index_for_mouse_position(event.position));
            self.select_to(offset, cx);
            return;
        }
        // While the right-click menu is open it owns the pointer — don't let the
        // table hover (highlight/handles) track the mouse behind it.
        if self.table_menu.is_some() {
            return;
        }
        // Repaint table "+" affordances when the pointer's region changes, so the
        // hover fill + cursor track the mouse live (the editor otherwise only
        // repaints on the caret blink).
        let region = self.table_hover_region_at(event.position);
        let cell = self.hovered_table_cell(event.position);
        if region != self.table_hover_region || cell != self.table_hover_cell {
            self.table_hover_region = region;
            self.table_hover_cell = cell;
            cx.notify();
        }
        // Repaint the column-resize border accent as the pointer crosses a band
        // (the cursor comes from the hitbox; the painted line needs a frame).
        let on_band = self
            .table_col_resize_rects
            .iter()
            .position(|(b, ..)| b.contains(&event.position));
        if on_band != self.table_resize_hover {
            self.table_resize_hover = on_band;
            cx.notify();
        }
        // Repaint the property-panel hover border when the pointer moves between
        // rows (the border itself reads the live pointer during paint).
        let prow = self
            .prop_row_rects
            .iter()
            .position(|(b, _)| b.contains(&event.position));
        if prow != self.prop_hover_row {
            self.prop_hover_row = prow;
            cx.notify();
        }
        // Repaint the heading fold chevron when the pointer enters/leaves a
        // heading row (the chevron is hover-revealed).
        let hrow = self
            .heading_row_rects
            .iter()
            .find_map(|(row, b)| b.contains(&event.position).then_some(*row));
        if hrow != self.heading_hover_row {
            self.heading_hover_row = hrow;
            cx.notify();
        }
        // Repaint the code card's chrome (lang tag + Copy) when the pointer
        // enters/leaves a card — it's hover-revealed.
        let ccard = self
            .code_card_rects
            .iter()
            .find_map(|(row, b)| b.contains(&event.position).then_some(*row));
        if ccard != self.code_chip_hover {
            self.code_chip_hover = ccard;
            cx.notify();
        }
    }

    /// Persist a finished grip drag: replace the resized image's source line with
    /// one carrying the rounded `{width=N}`, going through `record_edit` so it's
    /// one undoable edit and emits `Changed`. A no-op if the line vanished or
    /// isn't an image any more (it shaped to an image last paint, but guard
    /// anyway), or if the width didn't actually change.
    fn commit_image_resize(&mut self, resize: ImageResize, cx: &mut Context<Self>) {
        let starts = self.line_starts();
        let Some(&start) = starts.get(resize.line) else {
            return;
        };
        let end = self.line_end(resize.line);
        let line = &self.content[start..end];
        let new_line = set_image_width(line, resize.width.round().max(IMG_MIN_W) as u32);
        if new_line == line {
            return;
        }
        let range = start..end;
        let delta = new_line.len() as isize - (end - start) as isize;
        self.record_edit(&range, &new_line);
        self.content = self.content[..start].to_owned() + &new_line + &self.content[end..];
        self.remap_diagnostics(&range, new_line.len());
        // The line just grew/shrank by `delta` — shift a caret at/after its old
        // end with the text (the drop path parks it on the line below), else its
        // stale offset lands inside the new `{width=N}` tail and reveal-on-caret
        // swaps the freshly resized image for raw source. An offset inside the
        // line clamps to the new line end.
        let remap = |o: usize| {
            if o >= end {
                o.saturating_add_signed(delta)
            } else {
                o.min(start + new_line.len())
            }
        };
        self.selected_range = remap(self.selected_range.start)..remap(self.selected_range.end);
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// If logical line `row` renders as an inline image (a standalone `![](src)`
    /// or list-item image in markdown mode — not a file chip), the byte range of
    /// the whole line plus its trailing newline: the atomic unit Word-style
    /// deletion removes. `None` with styling off (raw mode edits as plain text).
    fn image_row_range(&self, row: usize) -> Option<Range<usize>> {
        self.markdown_style.as_ref()?;
        let start = *self.line_starts().get(row)?;
        let end = self.line_end(row);
        let (src, ..) = markdown_syntax::image_row(&self.content[start..end])?;
        if let Some(chip) = &self.block_chip
            && chip(src).is_some()
        {
            return None; // a chip's line edits as text (reveal-on-caret)
        }
        Some(start..(end + 1).min(self.content.len()))
    }

    /// Delete the `key:: value` line at `row` (+ its newline) — the panel's
    /// right-click "Delete property". One undoable edit; deleting the last
    /// property removes the panel.
    fn delete_property_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(&start) = self.line_starts().get(row) else {
            return;
        };
        let end = (self.line_end(row) + 1).min(self.content.len());
        self.replace_range(start..end, "", cx);
        cx.emit(EditorEvent::Changed);
    }

    /// Delete the image occupying logical line `row` — line + trailing newline,
    /// one undoable edit. Backs the right-click "Delete image" and the
    /// Word-style Backspace/Delete on an image row.
    fn delete_image_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Some(range) = self.image_row_range(row) {
            self.replace_range(range, "", cx);
            cx.emit(EditorEvent::Changed);
        }
    }

    /// Right-click: if the click lands on a flagged word, fetch its suggestions
    /// (lazily, via the provider) and open a menu anchored there; otherwise close
    /// any open menu.
    fn on_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Right-click on an inline image: Word-style object menu (Delete) — the
        // row renders as a picture, there's no text under the pointer to edit.
        // Only for real `![](src)` rows: mermaid/math rasters share the widget
        // type but delete as text (their menu would silently no-op).
        if let Some(&(line, _)) = self
            .image_rects
            .iter()
            .find(|(_, rect)| rect.contains(&event.position))
            .filter(|&&(line, _)| self.image_row_range(line).is_some())
        {
            self.menu = None;
            self.table_menu = None;
            self.focus(window, cx);
            self.image_menu = Some((line, event.position));
            cx.notify();
            return;
        }
        // Right-click a property-panel row: Edit / Delete property menu — the
        // panel renders as a widget, there's no text under the pointer.
        if let Some(&(_, row)) = self
            .prop_row_rects
            .iter()
            .find(|(rect, _)| rect.contains(&event.position))
        {
            self.menu = None;
            self.table_menu = None;
            self.image_menu = None;
            self.focus(window, cx);
            self.prop_menu = Some((row, event.position));
            cx.notify();
            return;
        }
        // Right-click a file chip places the caret to edit its source (the line
        // then reveals raw `![](src)`), instead of opening the spell menu.
        if self.chip_at(event.position).is_some() {
            self.menu = None;
            self.focus(window, cx);
            let offset = self.index_for_mouse_position(event.position);
            self.move_to(offset, cx);
            return;
        }
        // Right-click a $$…$$ block: emit a MathMenu event so the host can show a
        // context menu (Copy LaTeX / Export SVG / PNG). Focus the editor (not the caret
        // move of old) so it stays live after the menu closes.
        {
            let offset = self.index_for_mouse_position(event.position);
            let (row, _) = self.row_col(offset);
            if let Some((_range, source)) = self.math_block_at(row) {
                self.focus(window, cx);
                cx.emit(EditorEvent::MathMenu {
                    source,
                    position: event.position,
                });
                return;
            }
        }
        // Right-click in a table cell: place the caret there + open the table menu
        // (insert/delete rows + columns), instead of the spell menu. INSIDE a
        // selection, keep the selection and show the clipboard menu instead —
        // Cut/Copy act on it, like prose; the structure menu stays a
        // selection-free right-click away.
        if let Some(offset) = self.table_offset_at(event.position, window) {
            self.menu = None;
            self.focus(window, cx);
            let sel = self.selected_range.clone();
            if !sel.is_empty() && offset >= sel.start && offset <= sel.end {
                self.menu = Some(DiagMenu {
                    anchor: event.position,
                    range: offset..offset,
                    suggestions: Vec::new(),
                    scroll: ScrollHandle::new(),
                    turn_into: false,
                });
            } else {
                self.move_to(offset, cx);
                self.table_menu = Some(event.position);
            }
            cx.notify();
            return;
        }
        let offset = self.index_for_mouse_position(event.position);
        // Window-space — the popup renders on a `deferred`/`anchored` layer.
        let anchor = event.position;
        // A right-click outside the selection moves the caret there (so Paste
        // lands under the pointer); inside it, the selection stays put — it's
        // what Cut/Copy act on.
        let sel = self.selected_range.clone();
        let in_selection = !sel.is_empty() && offset >= sel.start && offset <= sel.end;
        if !in_selection {
            self.move_to(offset, cx);
        }
        self.focus(window, cx);
        // Suggestions when the click lands on a flagged word; the clipboard
        // verbs (Cut / Copy / Paste) ride along either way.
        let (range, suggestions) = match self.diagnostic_at(offset).map(|d| d.range.clone()) {
            Some(range) => {
                let word = self.content[range.clone()].to_string();
                let suggestions = self.suggest.as_ref().map(|f| f(&word)).unwrap_or_default();
                (range, suggestions)
            }
            None => (offset..offset, Vec::new()),
        };
        self.menu = Some(DiagMenu {
            anchor,
            range,
            suggestions: suggestions.into_iter().map(SharedString::from).collect(),
            scroll: ScrollHandle::new(),
            turn_into: false,
        });
        cx.notify();
    }

    /// The diagnostic whose range contains `offset`, if any.
    fn diagnostic_at(&self, offset: usize) -> Option<&Diagnostic> {
        self.diagnostics
            .iter()
            .find(|d| d.range.start <= offset && offset < d.range.end)
    }

    /// Close the suggestions menu (Escape, or a click elsewhere).
    fn dismiss(&mut self, _: &Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu.take().is_some()
            || self.table_menu.take().is_some()
            || self.image_menu.take().is_some()
            || self.prop_menu.take().is_some()
            || self.code_lang_menu.take().is_some()
        {
            cx.notify();
        }
    }

    /// Run the shared math-fence normalizer over the paragraph containing
    /// `row` (blank-line bounded), splicing only when it changes something.
    /// One recorded (undoable) edit; the caret shifts with the insertion.
    fn normalize_math_at(&mut self, row: usize) {
        if self.markdown_style.is_none() {
            return;
        }
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let blank = |r: usize| self.content[starts[r]..self.line_end(r)].trim().is_empty();
        let mut first = row;
        while first > 0 && !blank(first - 1) {
            first -= 1;
        }
        let mut last = row;
        while last < last_row && !blank(last + 1) {
            last += 1;
        }
        let span = starts[first]..self.line_end(last);
        let para = &self.content[span.clone()];
        if !para.contains("$$") {
            return;
        }
        let normalized = match zorite_markdown::syntax::normalize_math_fences(para) {
            std::borrow::Cow::Borrowed(_) => return,
            std::borrow::Cow::Owned(s) => s,
        };
        let old_caret = self.selected_range.start;
        let delta = normalized.len() as isize - (span.end - span.start) as isize;
        self.record_edit(&span, &normalized);
        self.content =
            self.content[..span.start].to_owned() + &normalized + &self.content[span.end..];
        self.remap_diagnostics(&span, normalized.len());
        // Typing happens at/after the paragraph's tail — shifting by the whole
        // delta keeps the caret on its text for the common case.
        let caret = if old_caret >= span.start {
            (old_caret as isize + delta).max(0) as usize
        } else {
            old_caret
        };
        let caret = caret.min(self.content.len());
        self.selected_range = caret..caret;
        self.last_edit = EditKind::Other;
    }

    /// Replace `range` with a chosen suggestion and close the menu.
    fn apply_suggestion(
        &mut self,
        range: Range<usize>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.selected_range = range;
        self.selection_reversed = false;
        self.replace_text_in_range(None, text, window, cx);
    }

    /// The fenced code block containing `row`: its opening fence row plus the
    /// closing fence row (`None` when the block runs unclosed to the end).
    /// The single source for turn-into, block drag, and drop snapping.
    fn fence_block_rows(&self, row: usize) -> (usize, Option<usize>) {
        let scan = self.scan_data();
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let open = (0..=row)
            .rev()
            .find(|&r| !scan.fence_odd.get(r).copied().unwrap_or(false))
            .unwrap_or(0);
        let close = ((open + 1)..=last_row).find(|&r| {
            self.content[starts[r]..self.line_end(r)]
                .trim_start()
                .starts_with("```")
        });
        (open, close)
    }

    /// The contiguous blockquote run containing `row` (first row..=last row),
    /// by the renderer's `blockquote_prefix` test.
    fn quote_run_rows(&self, row: usize) -> (usize, usize) {
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let is_q = |r: usize| {
            markdown_syntax::blockquote_prefix(&self.content[starts[r]..self.line_end(r)]).is_some()
        };
        let mut first = row;
        while first > 0 && is_q(first - 1) {
            first -= 1;
        }
        let mut last = row;
        while last < last_row && is_q(last + 1) {
            last += 1;
        }
        (first, last)
    }

    /// The "Turn into" kind of the block containing `row` — what the flyout
    /// shows checked.
    fn block_kind_at(&self, row: usize) -> TurnKind {
        let scan = self.scan_data();
        if scan.math.iter().any(|m| m.range.contains(&row)) {
            return TurnKind::Math;
        }
        let starts = self.line_starts();
        let Some(&start) = starts.get(row) else {
            return TurnKind::Text;
        };
        // The renderer's own recognizers (task/list/heading/quote/alert), so
        // the checked kind always agrees with what WYSIWYG actually renders.
        let line = &self.content[start..self.line_end(row)];
        if scan.fence_odd.get(row).copied().unwrap_or(false) || line.trim_start().starts_with("```")
        {
            return TurnKind::Code;
        }
        if markdown_syntax::task_prefix(line).is_some() {
            return TurnKind::Todo;
        }
        match markdown_syntax::heading_level(line) {
            Some(1) => return TurnKind::H1,
            Some(2) => return TurnKind::H2,
            Some(3) => return TurnKind::H3,
            _ => {}
        }
        if let Some((_, _, ordered, _)) = markdown_syntax::list_prefix(line) {
            return if ordered {
                TurnKind::Numbered
            } else {
                TurnKind::Bullet
            };
        }
        if markdown_syntax::blockquote_prefix(line).is_some() {
            // Callout if the caret's contiguous `>`-run carries a VALID
            // `[!KIND]` marker anywhere (marker line or body line).
            let (first, last) = self.quote_run_rows(row);
            for (r, &start) in starts.iter().enumerate().take(last + 1).skip(first) {
                let l = &self.content[start..self.line_end(r)];
                let p = markdown_syntax::blockquote_prefix(l).unwrap_or(0);
                if markdown_syntax::alert_kind(&l[p..]).is_some() {
                    return TurnKind::Callout;
                }
            }
            return TurnKind::Quote;
        }
        TurnKind::Text
    }

    /// Convert the caret's block to `kind` — the "Turn into" menu action.
    /// One undoable edit rewriting the block's lines; fenced kinds (code,
    /// math) and quote runs convert as whole blocks.
    fn turn_into(&mut self, kind: TurnKind, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.row_col(self.selected_range.start).0;
        let cur = self.block_kind_at(row);
        if cur == kind {
            return;
        }
        let scan = self.scan_data();
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let line_at = |r: usize| self.content[starts[r]..self.line_end(r)].to_string();
        // The block's line span + its content with the current dressing removed.
        let (first, last, mut body): (usize, usize, Vec<String>) = match cur {
            TurnKind::Code => {
                let (open, close) = self.fence_block_rows(row);
                let last = close.unwrap_or(last_row);
                let body_end = close.map(|c| c.saturating_sub(1)).unwrap_or(last_row);
                let body = ((open + 1)..=body_end).map(line_at).collect();
                (open, last, body)
            }
            TurnKind::Math => {
                let Some(reg) = scan.math.iter().find(|m| m.range.contains(&row)) else {
                    return;
                };
                let first = reg.range.start;
                let last = reg.range.end.saturating_sub(1).max(first);
                let body = ((first + 1)..last).map(line_at).collect();
                (first, last, body)
            }
            TurnKind::Quote | TurnKind::Callout => {
                let (first, last) = self.quote_run_rows(row);
                let body = (first..=last)
                    .filter_map(|r| {
                        let line = line_at(r);
                        let stripped = strip_block_prefix(&line);
                        // Drop the `[!KIND]` marker token; keep any title text
                        // after it (and skip the line if that leaves nothing).
                        if let Some(rest) = stripped.trim_start().strip_prefix("[!") {
                            let after = rest.split_once(']').map(|(_, a)| a).unwrap_or("");
                            let after = after.trim_start_matches(['-', '+']).trim();
                            return (!after.is_empty()).then(|| after.to_string());
                        }
                        Some(stripped.to_string())
                    })
                    .collect();
                (first, last, body)
            }
            _ => (
                row,
                row,
                vec![strip_block_prefix(&line_at(row)).to_string()],
            ),
        };
        if body.is_empty() {
            body.push(String::new());
        }
        let text = match kind {
            TurnKind::Text => body.join("\n"),
            TurnKind::H1 => prefix_lines(&body, |_| "# ".into()),
            TurnKind::H2 => prefix_lines(&body, |_| "## ".into()),
            TurnKind::H3 => prefix_lines(&body, |_| "### ".into()),
            TurnKind::Bullet => prefix_lines(&body, |_| "- ".into()),
            TurnKind::Numbered => prefix_lines(&body, |i| format!("{}. ", i + 1)),
            TurnKind::Todo => prefix_lines(&body, |_| "- [ ] ".into()),
            TurnKind::Quote => prefix_lines(&body, |_| "> ".into()),
            TurnKind::Callout => format!("> [!NOTE]\n{}", prefix_lines(&body, |_| "> ".into())),
            TurnKind::Code => format!("```\n{}\n```", body.join("\n")),
            TurnKind::Math => format!("$$\n{}\n$$", body.join("\n")),
        };
        let range = starts[first]..self.line_end(last);
        let range_start = range.start;
        self.selected_range = range;
        self.selection_reversed = false;
        self.replace_text_in_range(None, &text, window, cx);
        // Fenced kinds: the caret parks after the closing fence, revealing the
        // raw markers (reveal-on-caret) — seat it on the body's first line
        // instead, like `set_code_lang`.
        if matches!(kind, TurnKind::Code | TurnKind::Math) {
            let caret =
                (range_start + text.find('\n').map_or(0, |i| i + 1)).min(self.content.len());
            self.selected_range = caret..caret;
        }
    }

    /// Extra left offset for the gutter drag grip (e.g. the host's
    /// line-number gutter width), so the grip clears other gutter chrome.
    pub fn set_grip_inset(&mut self, inset: Pixels) {
        self.grip_inset = inset;
    }

    /// The line span the gutter grip drags as one unit: whole fenced/rendered
    /// regions (code, math, mermaid, tables incl. their style marker,
    /// property panels), a quote/callout run, a list item with its
    /// deeper-indented children — otherwise the single line.
    fn drag_block_rows(&self, row: usize) -> (usize, usize) {
        let scan = self.scan_data();
        let starts = self.line_starts();
        let last_row = starts.len().saturating_sub(1);
        let line_at = |r: usize| &self.content[starts[r]..self.line_end(r)];
        if let Some(m) = scan
            .math
            .iter()
            .find(|m| m.range.contains(&row) || m.marker_line == Some(row))
        {
            let first = m.marker_line.unwrap_or(m.range.start).min(m.range.start);
            return (first, m.range.end.saturating_sub(1).max(m.range.start));
        }
        if let Some((r, _)) = scan.mermaid.iter().find(|(r, _)| r.contains(&row)) {
            return (r.start, r.end.saturating_sub(1).max(r.start));
        }
        if let Some(t) = scan
            .tables
            .iter()
            .find(|t| t.lines.contains(&row) || t.marker_line == Some(row))
        {
            let first = t.marker_line.unwrap_or(t.lines.start).min(t.lines.start);
            return (first, t.lines.end.saturating_sub(1).max(t.lines.start));
        }
        if let Some(p) = scan.props.iter().find(|r| r.contains(&row)) {
            return (p.start, p.end.saturating_sub(1).max(p.start));
        }
        if scan.fence_odd.get(row).copied().unwrap_or(false)
            || line_at(row).trim_start().starts_with("```")
        {
            let (open, close) = self.fence_block_rows(row);
            return (open, close.unwrap_or(last_row));
        }
        if markdown_syntax::blockquote_prefix(line_at(row)).is_some() {
            return self.quote_run_rows(row);
        }
        if markdown_syntax::list_prefix(line_at(row)).is_some() {
            let indent = |r: usize| {
                let l = line_at(r);
                l.len() - l.trim_start().len()
            };
            let base = indent(row);
            let mut last = row;
            while last < last_row && !line_at(last + 1).trim().is_empty() && indent(last + 1) > base
            {
                last += 1;
            }
            return (row, last);
        }
        (row, row)
    }

    /// The row whose gutter grip the pointer would hover (the event-time
    /// mirror of prepaint's grip computation, for repaint change-detection).
    fn grip_hover_row_at(&self, position: Point<Pixels>) -> Option<usize> {
        let bounds = self.last_bounds?;
        if self.markdown_style.is_none()
            || self.line_drag.is_some()
            || position.x < grip_left(bounds.origin.x, self.grip_inset) - px(4.)
            || position.x > bounds.origin.x + bounds.size.width
            || position.y < bounds.origin.y
            || position.y > bounds.origin.y + bounds.size.height
        {
            return None;
        }
        let y = position.y - bounds.origin.y;
        (0..self.line_tops.len()).find(|&i| {
            let h = self.line_h(i) * self.row_span(i) as f32;
            h > px(0.5) && y >= self.line_tops[i] && y < self.line_tops[i] + h
        })
    }

    /// The drop boundary (a between-rows index) nearest the pointer, from the
    /// last paint's committed geometry.
    fn drop_boundary_at(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        let y = position.y - bounds.origin.y;
        for i in 0..self.line_tops.len() {
            let mid = self.line_tops[i] + self.line_h(i) * self.row_span(i) as f32 / 2.;
            if y < mid {
                return i;
            }
        }
        self.line_tops.len()
    }

    /// Clamp a drop boundary out of the interior of any rendered region — a
    /// block can't land inside a table, fence, math block, or property panel.
    fn snap_drop_boundary(&self, mut b: usize) -> usize {
        let scan = self.scan_data();
        let snap = |b: usize, s: usize, e: usize| {
            if b > s && b < e {
                if b - s <= e - b { s } else { e }
            } else {
                b
            }
        };
        for m in scan.math.iter() {
            let s = m.marker_line.unwrap_or(m.range.start).min(m.range.start);
            b = snap(b, s, m.range.end);
        }
        for (r, _) in scan.mermaid.iter() {
            b = snap(b, r.start, r.end);
        }
        for t in scan.tables.iter() {
            let s = t.marker_line.unwrap_or(t.lines.start).min(t.lines.start);
            b = snap(b, s, t.lines.end);
        }
        for p in scan.props.iter() {
            b = snap(b, p.start, p.end);
        }
        // Inside a code fence: snap to the opening fence or past the close.
        if scan.fence_odd.get(b).copied().unwrap_or(false) {
            let (open, close) = self.fence_block_rows(b);
            let close = close.map(|c| c + 1).unwrap_or(self.line_starts().len());
            b = if b - open <= close - b { open } else { close };
        }
        b
    }

    /// Land the grabbed block at boundary `t` — one undoable splice of the
    /// span between the block and the target, caret seated on the block.
    fn apply_line_drag(&mut self, bs: usize, be: usize, t: usize, cx: &mut Context<Self>) {
        let starts = self.line_starts();
        let n = starts.len();
        if bs >= n || be >= n || (t >= bs && t <= be + 1) {
            return; // dropped onto itself (or stale rows) — no-op
        }
        let block_start = starts[bs];
        let block_end = self.line_end(be);
        let block_text = self.content[block_start..block_end].to_string();
        if t > be {
            // Down: [block \n between...] → [between... block (\n)]
            let target_off = if t >= n {
                self.content.len()
            } else {
                starts[t]
            };
            let after_block = (block_end + 1).min(self.content.len());
            let mut new = self.content[after_block..target_off].to_string();
            if !new.is_empty() && !new.ends_with('\n') {
                new.push('\n');
            }
            let rest_len = new.len();
            new.push_str(&block_text);
            if t < n {
                new.push('\n');
            }
            self.replace_range(block_start..target_off, &new, cx);
            let caret = block_start + rest_len;
            self.selected_range = caret..caret;
        } else {
            // Up: [between... block] → [block \n between...]
            let target_off = starts[t];
            let between = &self.content[target_off..block_start];
            let mut new = block_text.clone();
            new.push('\n');
            new.push_str(&between[..between.len().saturating_sub(1)]);
            self.replace_range(target_off..block_end, &new, cx);
            self.selected_range = target_off..target_off;
        }
        cx.emit(EditorEvent::Changed);
    }

    // --- Selection helpers ---------------------------------------------------

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        // A deliberate caret move ends the current typing/deleting run and the
        // vertical-movement goal column.
        self.last_edit = EditKind::Other;
        self.goal_x = None;
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }

    /// Seat the caret on the plain-text line just before (`after = false`) or after
    /// (`after = true`) the math `block`, and focus the editor — the keyboard counterpart to
    /// clicking away, for when the caret flows out of a `$$…$$` formula's structural editor
    /// (so it never lands on the hidden `$$` fence lines, which would reveal raw source).
    pub fn exit_math(
        &mut self,
        block: Range<usize>,
        after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus(window, cx);
        let target = if after {
            let (end_row, _) = self.row_col(block.end.saturating_sub(1));
            match self.line_starts().get(end_row + 1).copied() {
                Some(start) => start,
                // The block ends the document: give the caret a fresh line
                // below. Landing at content-end would park it ON the block's
                // last row, revealing the raw source it just committed.
                None => {
                    let end = self.content.len();
                    self.replace_range(end..end, "\n", cx);
                    cx.emit(EditorEvent::Changed);
                    self.content.len()
                }
            }
        } else {
            let (start_row, _) = self.row_col(block.start);
            // An alignment marker directly above belongs to the block — resting
            // on it reveals the region, so step past it too.
            let mut row = start_row;
            if row > 0 && markdown_syntax::math_align_marker(self.line_str(row - 1)).is_some() {
                row -= 1;
            }
            if row > 0 { self.line_end(row - 1) } else { 0 }
        };
        self.move_to(target, cx);
    }

    /// The caret's bounds in window space (its painted Y range), or `None` before
    /// the first paint. Lets a host scroll the caret into view; computed from the
    /// layout stored at the last paint, so it's valid for caret moves that don't
    /// change the text (arrow keys, click).
    pub fn caret_screen_bounds(&self) -> Option<Bounds<Pixels>> {
        let bounds = self.last_bounds?;
        let (row, col) = self.row_col(self.cursor_offset());
        let lh = self.line_h(row);
        let p = self
            .wrapped
            .get(row)?
            .position_for_index(self.display_col(row, col), lh)?;
        let top = bounds.top() + self.line_tops.get(row).copied().unwrap_or(px(0.)) + p.y;
        Some(Bounds::from_corners(
            point(bounds.left(), top),
            point(bounds.left(), top + lh),
        ))
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.emit(EditorEvent::SelectionChanged);
        cx.notify();
    }

    // --- Line / row-col mapping ---------------------------------------------

    /// Byte offset where each visual line starts (line 0 starts at 0; each line
    /// after a `\n`). Always has at least one entry.
    ///
    /// Local change (Ghostex Docs): cached per content generation. Every
    /// `line_end` / `row_col` rebuilt this by scanning the whole document, and
    /// prepaint calls `line_end` once per line, so each frame of a 2,500-line
    /// document scanned it 2,500 times (about half a second per frame).
    fn line_starts(&self) -> std::rc::Rc<Vec<usize>> {
        let key = (self.content_gen, self.content.len());
        if let Some((generation, len, starts)) = self.line_starts_cache.borrow().as_ref()
            && (*generation, *len) == key
        {
            return starts.clone();
        }
        let mut starts = vec![0];
        for (i, b) in self.content.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        let starts = std::rc::Rc::new(starts);
        *self.line_starts_cache.borrow_mut() = Some((key.0, key.1, starts.clone()));
        starts
    }

    /// The `(row, byte-column)` of a byte offset.
    fn row_col(&self, offset: usize) -> (usize, usize) {
        let starts = self.line_starts();
        let row = starts.partition_point(|&s| s <= offset).saturating_sub(1);
        (row, offset - starts[row])
    }

    /// Byte offset of the end of a row's text (before its `\n`, or the document
    /// end for the last row).
    fn line_end(&self, row: usize) -> usize {
        let starts = self.line_starts();
        starts
            .get(row + 1)
            .map(|&s| s - 1)
            .unwrap_or(self.content.len())
    }

    /// Offset one row up/down from the caret, preserving the byte column where
    /// possible. At the top/bottom edge, jumps to the document start/end.
    fn vertical_offset(&self, dir: i32) -> usize {
        let cursor = self.cursor_offset();
        let starts = self.line_starts();
        let (row, col) = self.row_col(cursor);
        let target = row as i32 + dir;
        if target < 0 {
            return 0;
        }
        if target as usize >= starts.len() {
            return self.content.len();
        }
        let target = target as usize;
        let target_start = starts[target];
        let target_len = self.line_end(target) - target_start;
        let mut new_col = col.min(target_len);
        while new_col > 0 && !self.content.is_char_boundary(target_start + new_col) {
            new_col -= 1;
        }
        target_start + new_col
    }

    /// Offset one *visual* row up/down from the caret, preserving the goal column
    /// (x) across the run. Falls back to logical-line movement before the first
    /// paint (when no wrapped layout is cached yet).
    fn move_vertical(&mut self, dir: i32) -> usize {
        if self.wrapped.is_empty() {
            return self.vertical_offset(dir);
        }
        let (row, col) = self.row_col(self.cursor_offset());
        let cur_lh = self.line_h(row);
        if cur_lh <= px(0.) {
            return self.vertical_offset(dir);
        }
        let Some(cur) = self
            .wrapped
            .get(row)
            .and_then(|l| line_pos(l, self.bidi_map(row), self.display_col(row, col), cur_lh))
        else {
            return self.vertical_offset(dir);
        };
        let global_y = self.line_tops[row] + cur.y;
        // The goal column is the caret's *visual* x, so it carries an RTL row's
        // right-align shift — the target row's shift comes off again below.
        // (Row insets stay out of it, as they always have.)
        let goal = self.goal_x.unwrap_or(cur.x + self.rtl_shift(row));
        self.goal_x = Some(goal);
        // Step to the adjacent visual row. Down: to the bottom of the current
        // row (= the top of the next one). Up: just above the current row's top
        // — robust to the row above having a different height (e.g. a heading),
        // since it doesn't depend on the current row's height.
        let target_y = if dir >= 0 {
            global_y + cur_lh
        } else {
            global_y - px(1.)
        };
        if target_y < px(0.) {
            return 0;
        }
        let last = self.wrapped.len() - 1;
        let total = self.line_tops[last] + self.line_h(last) * self.row_span(last) as f32;
        if target_y >= total {
            // Landing at the very end would park the caret on a trailing
            // collapsed row (a hidden closing ``` fence) and reveal it — clamp
            // to the end of the last VISIBLE row instead.
            let mut r = last;
            while r > 0 && self.line_h(r) <= px(0.) {
                r -= 1;
            }
            return if r == last {
                self.content.len()
            } else {
                self.line_end(r)
            };
        }
        let mut trow = last;
        for i in 0..self.wrapped.len() {
            let h = self.line_h(i) * self.row_span(i) as f32;
            if target_y < self.line_tops[i] + h {
                trow = i;
                break;
            }
        }
        // A reserved gutter gap (a table's top/bottom, a code block's pads) belongs
        // to no row, and the loop assigns it to the row *after* it — right going
        // down, but going up that strands the caret on the far side of the gap (e.g.
        // just below a table). Going up, target the row before the gap instead.
        if dir < 0 && trow > 0 && target_y < self.line_tops[trow] {
            trow -= 1;
        }
        // A table separator (`|---|`) row isn't editable — skip past it (in the
        // direction of travel) so the caret lands on the header/body row rather
        // than dropping the whole table to raw source.
        if self
            .table_rows
            .get(trow)
            .and_then(Option::as_ref)
            .is_some_and(|t| t.is_separator)
        {
            let skip = if dir >= 0 {
                trow + 1
            } else {
                trow.wrapping_sub(1)
            };
            if skip < self.wrapped.len() {
                trow = skip;
            }
        }
        // A collapsed row (a hidden ``` fence, a folded body line) has no visual
        // height — landing there reveals it. Keep stepping in the direction of
        // travel so the caret skips over it; if the document runs out that way,
        // stay put.
        if self.line_h(trow) <= px(0.) {
            let step = |r: usize| {
                if dir >= 0 {
                    (r + 1 < self.wrapped.len()).then_some(r + 1)
                } else {
                    r.checked_sub(1)
                }
            };
            let mut r = trow;
            loop {
                match step(r) {
                    Some(n) if self.line_h(n) <= px(0.) => r = n,
                    Some(n) => {
                        trow = n;
                        break;
                    }
                    None => return self.cursor_offset(),
                }
            }
        }
        let rel = point(
            (goal - self.rtl_shift(trow)).max(px(0.)),
            (target_y - self.line_tops[trow]).max(px(0.)),
        );
        let col = line_index_at(
            &self.wrapped[trow],
            self.bidi_map(trow),
            rel,
            self.line_h(trow),
        );
        self.line_starts()[trow] + self.source_col(trow, col)
    }

    /// The end of the next word at/after `offset` (⌥→ on macOS).
    fn next_word(&self, offset: usize) -> usize {
        self.content
            .unicode_word_indices()
            .map(|(i, w)| i + w.len())
            .find(|&end| end > offset)
            .unwrap_or(self.content.len())
    }

    /// The start of the previous word before `offset` (⌥← on macOS).
    fn prev_word(&self, offset: usize) -> usize {
        self.content
            .unicode_word_indices()
            .map(|(i, _)| i)
            .rfind(|&start| start < offset)
            .unwrap_or(0)
    }

    /// The byte range of the word at `offset` (double-click); `None` in whitespace.
    fn word_range_at(&self, offset: usize) -> Option<Range<usize>> {
        let mut ends_at = None;
        for (i, w) in self.content.unicode_word_indices() {
            let range = i..i + w.len();
            if range.start <= offset && offset < range.end {
                return Some(range);
            }
            if range.end == offset {
                ends_at = Some(range);
            }
        }
        ends_at
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let off = self.prev_word(self.cursor_offset());
        if self.enter_construct_at(off, true, cx) {
            return;
        }
        self.move_to(off, cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        let off = self.next_word(self.cursor_offset());
        if self.enter_construct_at(off, false, cx) {
            return;
        }
        self.move_to(off, cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.prev_word(self.cursor_offset()), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.next_word(self.cursor_offset()), cx);
    }

    /// The `src` of a file chip on the row at window `position`, if that row is a
    /// chip (from the last paint) — left-click opens it, right-click edits.
    fn chip_at(&self, position: Point<Pixels>) -> Option<(SharedString, bool)> {
        if self.wrapped.is_empty() || self.chip_rows.iter().all(Option::is_none) {
            return None;
        }
        let bounds = self.last_bounds.as_ref()?;
        let rel_y = position.y - bounds.top();
        let mut row = self.wrapped.len() - 1;
        for i in 0..self.wrapped.len() {
            let h = self.line_h(i) * self.row_span(i) as f32;
            if rel_y < self.line_tops[i] + h {
                row = i;
                break;
            }
        }
        self.chip_rows.get(row).and_then(Option::clone)
    }

    /// The inline `$…$` formula under `position` (its absolute byte range + inner LaTeX), from
    /// the last paint's window-space `inline_math_rects` — so a click opens its editor.
    fn inline_math_at(&self, position: Point<Pixels>) -> Option<(Range<usize>, SharedString)> {
        self.inline_math_rects
            .iter()
            // Empty latex marks an inline IMAGE sharing this machinery — not a
            // formula, so a click on it shouldn't open the math editor.
            .find(|(_, latex, rect)| !latex.is_empty() && rect.contains(&position))
            .map(|(range, latex, _)| (range.clone(), latex.clone()))
    }

    /// The inline image `src` under `position` (an empty-latex entry in
    /// `inline_math_rects`), parsed from its `![alt](src)` source.
    fn inline_image_at(&self, position: Point<Pixels>) -> Option<SharedString> {
        let (range, _, _) = self
            .inline_math_rects
            .iter()
            .find(|(_, latex, rect)| latex.is_empty() && rect.contains(&position))?;
        let text = self.content.get(range.clone())?;
        let open = text.rfind('(')?;
        let close = text.rfind(')')?;
        (open < close).then(|| text[open + 1..close].to_string().into())
    }

    /// If `position` lands on an inline image's bottom-right resize grip, the
    /// `(logical line, current display width)` of that image — so a press can
    /// start a corner-grip drag. The grip is the `IMG_GRIP`-side square pinned to
    /// each image's painted corner (see [`Self::image_grip`]); checked against the
    /// last paint's window-space `image_rects`.
    fn grip_at(&self, position: Point<Pixels>) -> Option<(usize, f32)> {
        self.image_rects.iter().find_map(|&(line, rect)| {
            Self::image_grip(rect)
                .contains(&position)
                .then_some((line, f32::from(rect.size.width)))
        })
    }

    /// The window-space bounds of an image's corner grip, given the image's
    /// painted `rect`. A small square overhanging the bottom-right corner (its
    /// center on the corner, like the reading view's), so it's easy to grab
    /// without covering much of the image.
    fn image_grip(rect: Bounds<Pixels>) -> Bounds<Pixels> {
        let s = px(IMG_GRIP);
        Bounds::new(
            point(rect.right() - s / 2., rect.bottom() - s / 2.),
            size(s, s),
        )
    }

    /// If `position` lands on a task checkbox painted last frame, the logical line
    /// of that task — so a click can toggle it. The hit area is the box padded a
    /// little, to stay easy to tap without swallowing the body text beside it.
    /// The code block whose opening fence is `fence_row`: its language token
    /// (empty for none) and the byte range of the body between the fences.
    fn code_block_at(&self, fence_row: usize) -> Option<(String, Range<usize>)> {
        let starts = self.line_starts();
        let &start = starts.get(fence_row)?;
        let fence_line = &self.content[start..self.line_end(fence_row)];
        let trimmed = fence_line.trim_start();
        let lang = trimmed.strip_prefix("```")?.trim().to_string();
        let body_start = (self.line_end(fence_row) + 1).min(self.content.len());
        let mut body_end = self.content.len();
        for (row, &row_start) in starts.iter().enumerate().skip(fence_row + 1) {
            if self.content[row_start..self.line_end(row)]
                .trim_start()
                .starts_with("```")
            {
                body_end = row_start.saturating_sub(1).max(body_start);
                break;
            }
        }
        Some((lang, body_start..body_end))
    }

    /// If `position` lands on a code card's chrome painted last frame:
    /// `(on_copy, fence_row)` — `true` = the Copy button, `false` = the
    /// language tag.
    fn code_chip_at(&self, position: Point<Pixels>) -> Option<(bool, usize)> {
        self.code_chip_rects.iter().find_map(|c| {
            if c.copy.contains(&position) {
                Some((true, c.fence_row))
            } else if c.lang.contains(&position) {
                Some((false, c.fence_row))
            } else {
                None
            }
        })
    }

    /// Rewrite the block's opening fence to carry `lang` (one undo step).
    fn set_code_lang(&mut self, fence_row: usize, lang: &str, cx: &mut Context<Self>) {
        let starts = self.line_starts();
        let Some(&start) = starts.get(fence_row) else {
            return;
        };
        let end = self.line_end(fence_row);
        let line = &self.content[start..end];
        let trimmed = line.trim_start();
        if !trimmed.starts_with("```") {
            return;
        }
        let indent = &line[..line.len() - trimmed.len()];
        let new_line = format!("{indent}```{}", if lang == "text" { "" } else { lang });
        self.replace_range(start..end, &new_line, cx);
        // replace_range parks the caret at the fence line's end, which reveals
        // the raw ``` marker (reveal-on-caret). Step onto the body's first
        // line instead so the fence stays hidden.
        let caret = (start + new_line.len() + 1).min(self.content.len());
        self.selected_range = caret..caret;
        cx.emit(EditorEvent::Changed);
    }

    fn checkbox_at(&self, position: Point<Pixels>) -> Option<usize> {
        let pad = px(4.);
        self.checkbox_rects.iter().find_map(|&(line, rect)| {
            Bounds::new(
                point(rect.origin.x - pad, rect.origin.y - pad),
                size(rect.size.width + pad * 2., rect.size.height + pad * 2.),
            )
            .contains(&position)
            .then_some(line)
        })
    }

    /// If `position` lands on a foldable callout's chevron painted last frame,
    /// that marker's logical line — so a click can flip its fold char. Padded
    /// like the task checkbox to stay easy to hit.
    fn alert_fold_at(&self, position: Point<Pixels>) -> Option<usize> {
        let pad = px(4.);
        self.alert_fold_rects.iter().find_map(|&(line, rect)| {
            Bounds::new(
                point(rect.origin.x - pad, rect.origin.y - pad),
                size(rect.size.width + pad * 2., rect.size.height + pad * 2.),
            )
            .contains(&position)
            .then_some(line)
        })
    }

    /// If `position` lands on a heading's fold chevron painted last frame,
    /// that heading's logical line — so a click can toggle its fold. Padded
    /// like the callout chevron.
    fn heading_fold_at(&self, position: Point<Pixels>) -> Option<usize> {
        let pad = px(4.);
        self.heading_fold_rects.iter().find_map(|&(line, rect)| {
            Bounds::new(
                point(rect.origin.x - pad, rect.origin.y - pad),
                size(rect.size.width + pad * 2., rect.size.height + pad * 2.),
            )
            .contains(&position)
            .then_some(line)
        })
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() || self.wrapped.is_empty() {
            return 0;
        }
        let Some(bounds) = self.last_bounds.as_ref() else {
            return 0;
        };
        let rel = point(position.x - bounds.left(), position.y - bounds.top());
        // Which logical line, by the vertical band each occupies (variable height).
        let mut row = self.wrapped.len() - 1;
        for i in 0..self.wrapped.len() {
            let height = self.line_h(i) * self.row_span(i) as f32;
            if rel.y < self.line_tops[i] + height {
                row = i;
                break;
            }
        }
        // An inline-image row: clicking it puts the caret at the line start (the
        // line then shows its source — "raw on caret"), not a text column.
        if self.widget_rows.get(row).copied().unwrap_or(false) {
            return self.line_starts()[row];
        }
        let x = (rel.x - self.row_origin_x(row)).max(px(0.));
        let line_rel = point(x, rel.y - self.line_tops[row]);
        let col = line_index_at(
            &self.wrapped[row],
            self.bidi_map(row),
            line_rel,
            self.line_h(row),
        );
        self.line_starts()[row] + self.source_col(row, col)
    }

    /// Map a display byte column on `row` back to its source column. Identity
    /// unless the row's markers are hidden (W6), where the painted text is
    /// shorter than the source.
    fn source_col(&self, row: usize, display_col: usize) -> usize {
        match self.offset_maps.get(row).and_then(Option::as_ref) {
            Some(map) => map.get(display_col).copied().unwrap_or(display_col),
            None => display_col,
        }
    }

    /// Map a source byte column on `row` to its display column — the inverse of
    /// [`Self::source_col`], for positioning the caret/selection on a row whose
    /// markers are hidden (W6/#5). Uses the last painted map; in-paint code that
    /// has this frame's fresh map should call [`display_col_in`] directly.
    fn display_col(&self, row: usize, source_col: usize) -> usize {
        display_col_in(
            self.offset_maps.get(row).and_then(Option::as_ref),
            source_col,
        )
    }

    // --- UTF-16 + grapheme boundaries (IME / cursor movement) ----------------

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = self.utf16_resume(offset, false);
        for ch in self.content[utf8..].chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        self.utf16_anchor.set((self.content_gen, utf8, utf16));
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = self.utf16_resume(offset, true);
        for ch in self.content[utf8..].chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        self.utf16_anchor.set((self.content_gen, utf8, utf16));
        utf16
    }

    /// Where a conversion can start: the saved anchor when it's from the
    /// current content generation and at/before the target (`by_utf8` picks
    /// which unit the target is in), else the document start. IME composition
    /// converts monotonically-close offsets many times per keystroke, so this
    /// turns O(document) scans into O(distance-from-anchor).
    fn utf16_resume(&self, target: usize, by_utf8: bool) -> (usize, usize) {
        let (generation, utf8, utf16) = self.utf16_anchor.get();
        let anchor_pos = if by_utf8 { utf8 } else { utf16 };
        if generation == self.content_gen && anchor_pos <= target && utf8 <= self.content.len() {
            (utf8, utf16)
        } else {
            (0, 0)
        }
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    /// One VISIBLE position left of `offset`: a grapheme step, extended while
    /// the display column doesn't change — always-hidden formatting markers
    /// (`**`, `~~`, …) are zero-width on screen, so crossing one must not cost
    /// extra keypresses. Rows without a display map (raw/code) step plainly.
    fn prev_visible_boundary(&self, offset: usize) -> usize {
        let (row0, col0) = self.row_col(offset);
        let d0 = self.display_col(row0, col0);
        let mut off = self.previous_boundary(offset);
        loop {
            if off == 0 {
                return off;
            }
            let (row, col) = self.row_col(off);
            if row != row0
                || self.offset_maps.get(row).and_then(Option::as_ref).is_none()
                || self.display_col(row, col) != d0
            {
                return off;
            }
            off = self.previous_boundary(off);
        }
    }

    /// One VISIBLE position right of `offset` — see [`Self::prev_visible_boundary`].
    fn next_visible_boundary(&self, offset: usize) -> usize {
        let (row0, col0) = self.row_col(offset);
        let d0 = self.display_col(row0, col0);
        let mut off = self.next_boundary(offset);
        loop {
            if off >= self.content.len() {
                return self.content.len();
            }
            let (row, col) = self.row_col(off);
            if row != row0
                || self.offset_maps.get(row).and_then(Option::as_ref).is_none()
                || self.display_col(row, col) != d0
            {
                return off;
            }
            off = self.next_boundary(off);
        }
    }

    /// Cditor-style deletion planning around hidden formatting markers: the
    /// range a backspace (`back`) / forward delete should remove at `off`.
    /// Skips the invisible marker bytes to the adjacent VISIBLE character —
    /// and when removing it would empty its construct, the now-empty marker
    /// pair goes too (deleting bold's last char deletes the bold). `None` =
    /// no hidden markers involved (the plain grapheme deletion applies).
    fn fmt_delete_range(&self, off: usize, back: bool) -> Option<Range<usize>> {
        let st = self.markdown_style.as_ref()?;
        let (row, col) = self.row_col(off);
        let line_start = self.line_starts()[row];
        let line_end = self.line_end(row);
        let line = &self.content[line_start..line_end];
        // Inside a fenced code block the text is verbatim — no markers there.
        if *self.scan_data().fence_odd.get(row).unwrap_or(&false)
            || line.trim_start().starts_with("```")
        {
            return None;
        }
        let pairs = markdown_syntax::fmt_marker_pairs(line, st);
        if pairs.is_empty() {
            return None;
        }
        let markers: Vec<&Range<usize>> = pairs.iter().flat_map(|(o, c)| [o, c]).collect();
        // Skip marker bytes in the deletion direction to the visible char.
        let mut edge = col;
        if back {
            while let Some(sp) = markers.iter().find(|sp| sp.end == edge) {
                edge = sp.start;
            }
        } else {
            while let Some(sp) = markers.iter().find(|sp| sp.start == edge) {
                edge = sp.end;
            }
        }
        // The visible grapheme adjacent to the (possibly skipped-to) edge —
        // staying on this line; a line join takes the default path.
        let (del_start, del_end) = if back {
            if edge == 0 {
                return None;
            }
            let p = self.previous_boundary(line_start + edge) - line_start;
            (p, edge)
        } else {
            if edge >= line.len() {
                return None;
            }
            let n = self.next_boundary(line_start + edge).min(line_end) - line_start;
            (edge, n)
        };
        // Would this empty a construct? Only a real PAIR collapses: ITS opener
        // ending at the deletion's start and ITS closer starting at the end
        // (adjacent different constructs must not fuse).
        let emptied = pairs
            .iter()
            .find(|(o, c)| o.end == del_start && c.start == del_end);
        let range = match emptied {
            Some((o, c)) => o.start..c.end,
            None if edge == col => return None, // no markers were involved
            None => del_start..del_end,
        };
        Some(line_start + range.start..line_start + range.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for EditorState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        // Report what this edit replaced (see `last_replaced`) — the fact a
        // diff can't reconstruct when the selection starts with the typed char.
        self.last_replaced =
            (range.start < range.end).then(|| self.content[range.clone()].to_string());
        self.record_edit(&range, new_text);
        self.content =
            self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        let caret = range.start + new_text.len();
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range = None;
        self.goal_x = None;
        // Keep unaffected diagnostics valid across the edit (shift those after
        // it, drop those it overlapped); the host recomputes the edited region.
        self.remap_diagnostics(&range, new_text.len());
        if word_boundary_input(new_text) {
            self.apply_auto_replace(range.start);
        }
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());
        self.content_gen += 1;
        self.content =
            self.content[0..range.start].to_owned() + new_text + &self.content[range.end..];
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|r| r.start + range.start..r.end + range.start)
            .unwrap_or_else(|| {
                let caret = range.start + new_text.len();
                caret..caret
            });
        self.remap_diagnostics(&range, new_text.len());
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let (row, col) = self.row_col(range.start);
        let lh = self.line_h(row);
        let line = self.wrapped.get(row)?;
        let map = self.bidi_map(row);
        let p = line_pos(line, map, self.display_col(row, col), lh)?;
        let top = bounds.top() + self.line_tops.get(row).copied().unwrap_or(px(0.)) + p.y;
        let x = bounds.left() + p.x + self.row_origin_x(row);
        // Span the whole range when it stays on one wrap row (the common IME
        // composition), so the candidate window anchors under the marked TEXT
        // rather than a zero-width bar at its start. Multi-row ranges keep the
        // start-anchored bar.
        let (erow, ecol) = self.row_col(range.end);
        let x2 = if erow == row && range.end > range.start {
            line_pos(line, map, self.display_col(row, ecol), lh)
                .filter(|e| e.y == p.y)
                .map(|e| bounds.left() + e.x + self.row_origin_x(row))
                .unwrap_or(x)
        } else {
            x
        };
        Some(Bounds::from_corners(
            point(x, top),
            point(x2.max(x), top + lh),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(point)))
    }
}

impl Focusable for EditorState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<EditorEvent> for EditorState {}

impl Render for EditorState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            // While a `$$` block OR an inline `$…$` formula is being edited, the hosted math
            // editor is focused but lives *inside* this element — so the editor's own
            // keybindings (arrows, typing, …) would capture keys before they reach it. Drop the
            // key context for the duration so raw keys flow to the math editor's on_key_down.
            .key_context(
                if self.editing_block.is_some() || self.editing_inline.is_some() {
                    ""
                } else {
                    CONTEXT
                },
            )
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::bold))
            .on_action(cx.listener(Self::italic))
            .on_action(cx.listener(Self::underline))
            .on_action(cx.listener(Self::strike))
            .on_action(cx.listener(Self::highlight))
            .on_action(cx.listener(Self::code))
            .on_action(cx.listener(Self::indent))
            .on_action(cx.listener(Self::outdent))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::dismiss))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .child(EditorElement {
                editor: cx.entity(),
            })
            .children(self.embed_overlays(window))
            .children(self.editing_block_overlay())
            .children(self.editing_inline_overlay())
            // Right-click suggestions menu, absolutely positioned over the
            // editor (anchored at the click). `Option`'s `IntoIterator` renders
            // zero or one popup; clicking a row replaces the misspelled span.
            .children(self.menu.clone().map(|menu| {
                let DiagMenu {
                    anchor,
                    range,
                    suggestions,
                    scroll,
                    turn_into: menu_turn_into,
                } = menu;
                let count = suggestions.len();
                // Menu chrome from the host's theme (fallbacks match the former
                // hardcoded dark menu when no markdown style is set).
                let st = self.markdown_style.as_ref();
                let menu_bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
                let menu_border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
                let menu_fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
                let hover = st.map_or(rgba(0x2f6fd628).into(), |s| s.popover_hover);
                let mut thumb_c = st.map_or(rgba(0xffffff66).into(), |s| s.marker);
                thumb_c.a = 0.5;
                // Collected eagerly (not a lazy iterator) so `cx` is only
                // borrowed here and stays free for the menu's own listeners below.
                let rows: Vec<_> = suggestions
                    .into_iter()
                    .enumerate()
                    .map(|(i, sugg)| {
                        let range = range.clone();
                        let replacement = sugg.to_string();
                        div()
                            // A stable per-row id so gpui tracks hover state and
                            // repaints as the pointer moves between rows. Without
                            // an id, the hover style only shows on a forced
                            // repaint (e.g. while scrolling).
                            .id(("suggestion-row", i))
                            // Don't let the scroll container's max-height squeeze
                            // the rows; they keep their height and overflow.
                            .flex_shrink_0()
                            .px(px(10.))
                            .py(px(3.))
                            // Highlight the row under the pointer.
                            .hover(move |s| s.bg(hover))
                            .child(sugg)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |editor, _: &MouseDownEvent, window, cx| {
                                    // Keep the editor's own mouse-down from clearing
                                    // the menu / moving the caret out from under us.
                                    cx.stop_propagation();
                                    editor.apply_suggestion(
                                        range.clone(),
                                        &replacement,
                                        window,
                                        cx,
                                    );
                                }),
                            )
                    })
                    .collect();
                // A thin scrollbar thumb, shown when the list overflows ~6 rows
                // so the scroll affordance is visible. Sized from the row count
                // (known now) and positioned from the live scroll offset — a
                // wheel scroll calls window.refresh(), which re-renders this.
                const ROW_H: f32 = 24.0;
                const PAD: f32 = 4.0;
                const MAX_H: f32 = 180.0;
                let rows_h = count as f32 * ROW_H;
                let view_h = MAX_H - 2.0 * PAD;
                let thumb = (rows_h > view_h).then(|| {
                    let scrolled = (-f32::from(scroll.offset().y)).clamp(0.0, rows_h - view_h);
                    let thumb_h = (view_h * view_h / rows_h).max(24.0);
                    let thumb_top = PAD + scrolled / (rows_h - view_h) * (view_h - thumb_h);
                    div()
                        .absolute()
                        .top(px(thumb_top))
                        .right(px(2.))
                        .w(px(6.))
                        .h(px(thumb_h))
                        .rounded(px(3.))
                        .bg(thumb_c)
                });

                // Cut / Copy need a selection; Paste always applies (the caret
                // was seated at the click when it landed outside the selection).
                let has_sel = !self.selected_range.is_empty();
                let clip_item = |id: &'static str, label: SharedString| {
                    div()
                        .id(id)
                        .flex_shrink_0()
                        .px(px(10.))
                        .py(px(3.))
                        .hover(move |s| s.bg(hover))
                        .child(label)
                };
                let mut clipboard = div().flex().flex_col().py(px(4.));
                if has_sel {
                    clipboard = clipboard
                        .child(
                            clip_item("menu-cut", self.labels.cut.clone()).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    editor.menu = None;
                                    editor.cut(&Cut, window, cx);
                                }),
                            ),
                        )
                        .child(
                            clip_item("menu-copy", self.labels.copy.clone()).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    editor.menu = None;
                                    editor.copy(&Copy, window, cx);
                                }),
                            ),
                        )
                        // Plain-only: the raw markdown with no host flavors —
                        // for pasting literal source into rich surfaces
                        // (email, chat) where Copy's HTML flavor would win.
                        .child(
                            clip_item("menu-copy-md", self.labels.copy_as_markdown.clone())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.copy_plain(window, cx);
                                    }),
                                ),
                        );
                }
                let clipboard = clipboard.child(
                    clip_item("menu-paste", self.labels.paste.clone()).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            editor.menu = None;
                            editor.paste(&Paste, window, cx);
                        }),
                    ),
                );

                // Inline-format bar (Cditor-style): with a selection, a strip of
                // B / I / S / <> buttons across the menu's top — each toggles its
                // markdown wrap on the selection and closes the menu.
                let fmt_btn = |id: &'static str| {
                    div()
                        .id(id)
                        .w(px(28.))
                        .h(px(24.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |s| s.bg(hover))
                };
                let format_bar = has_sel.then(|| {
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(2.))
                        .px(px(6.))
                        .py(px(4.))
                        .child(
                            fmt_btn("menu-fmt-bold")
                                .font_weight(FontWeight::BOLD)
                                .child("B")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.bold(&Bold, window, cx);
                                    }),
                                ),
                        )
                        .child(
                            fmt_btn("menu-fmt-italic")
                                .italic()
                                .child("I")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.italic(&Italic, window, cx);
                                    }),
                                ),
                        )
                        .child(
                            fmt_btn("menu-fmt-underline")
                                .underline()
                                .child("U")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.underline(&Underline, window, cx);
                                    }),
                                ),
                        )
                        .child(
                            fmt_btn("menu-fmt-strike")
                                .line_through()
                                .child("S")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.strike(&Strike, window, cx);
                                    }),
                                ),
                        )
                        .child(
                            fmt_btn("menu-fmt-code")
                                .font_family(
                                    self.markdown_style
                                        .as_ref()
                                        .map(|s| s.mono.family.clone())
                                        .unwrap_or_else(|| "monospace".into()),
                                )
                                .text_size(px(12.))
                                .child("<>")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.code(&Code, window, cx);
                                    }),
                                ),
                        )
                });

                // Color swatches under the format bar: text colors, then
                // highlights (theme tint first, then colored `<mark>`s).
                let mark_bg = st.map_or(rgba(0xFFD60066).into(), |s| s.mark_bg);
                let color_rows = has_sel.then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .child(Self::color_row(
                            ColorKind::Text,
                            None,
                            menu_border,
                            hover,
                            cx,
                        ))
                        .child(Self::color_row(
                            ColorKind::Highlight,
                            Some(mark_bg),
                            menu_border,
                            hover,
                            cx,
                        ))
                });

                // "Turn into" block conversion (Cditor-style): a row whose
                // hover opens a kind-list flyout beside the menu, the caret
                // block's current kind checked.
                let cur_kind = self.block_kind_at(self.row_col(self.selected_range.start).0);
                let turn_row = div()
                    .id("menu-turn-into")
                    .flex_shrink_0()
                    .px(px(10.))
                    .py(px(3.))
                    .hover(move |s| s.bg(hover))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .child(self.labels.turn_into.clone())
                    .child(div().text_size(px(10.)).child("\u{25b8}"))
                    .on_hover(cx.listener(|editor, hovered: &bool, _, cx| {
                        if *hovered
                            && let Some(m) = editor.menu.as_mut()
                            && !m.turn_into
                        {
                            m.turn_into = true;
                            cx.notify();
                        }
                    }));
                let turn_labels = self.labels.clone();
                let turn_flyout = menu_turn_into.then(|| {
                    let rows: Vec<_> = TurnKind::ALL
                        .iter()
                        .enumerate()
                        .map(|(i, &k)| {
                            let checked = k == cur_kind;
                            div()
                                .id(("turn-kind", i))
                                .flex_shrink_0()
                                .px(px(10.))
                                .py(px(3.))
                                .hover(move |s| s.bg(hover))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .child(div().w(px(12.)).flex_shrink_0().child(if checked {
                                    "\u{2713}"
                                } else {
                                    ""
                                }))
                                .child(k.label(&turn_labels))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |editor, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        editor.menu = None;
                                        editor.turn_into(k, window, cx);
                                    }),
                                )
                        })
                        .collect();
                    // An absolute sibling of the (overflow-clipped) menu box —
                    // out of flow, so it can't inflate the anchored bounds and
                    // re-trigger the window snap (the slash-flyout lesson).
                    div()
                        .absolute()
                        .left(gpui::relative(1.))
                        .bottom(px(0.))
                        .ml(px(2.))
                        .occlude()
                        .min_w(px(140.))
                        .cursor(CursorStyle::Arrow)
                        .bg(menu_bg)
                        .border_1()
                        .border_color(menu_border)
                        .rounded(px(6.))
                        .shadow_md()
                        .text_color(menu_fg)
                        .text_size(px(13.))
                        .flex()
                        .flex_col()
                        .py(px(4.))
                        .children(rows)
                });

                // Deferred + anchored to a window-space top layer with `.occlude()`,
                // so it renders above the page chrome and captures the wheel — else a
                // scroll over the popup scrolls the page behind it.
                gpui::deferred(
                    gpui::anchored().position(anchor).snap_to_window().child(
                        div().relative().children(turn_flyout).child(
                            div()
                                .relative()
                                .occlude()
                                .min_w(px(150.))
                                // Override the editor's I-beam — the menu is a normal
                                // pointer surface (children inherit this hitbox's cursor).
                                .cursor(CursorStyle::Arrow)
                                .bg(menu_bg)
                                .border_1()
                                .border_color(menu_border)
                                .rounded(px(6.))
                                .shadow_md()
                                // Clip rows + thumb to the rounded box.
                                .overflow_hidden()
                                .text_color(menu_fg)
                                .text_size(px(13.))
                                // A click anywhere outside the menu dismisses it.
                                .on_mouse_down_out(cx.listener(
                                    |editor, _: &MouseDownEvent, _, cx| {
                                        editor.menu = None;
                                        cx.notify();
                                    },
                                ))
                                .children(format_bar)
                                .children(color_rows)
                                .children(has_sel.then(|| div().h(px(1.)).bg(menu_border)))
                                .children((count > 0).then(|| {
                                    // The scroll viewport: shows ~6 rows, the rest scroll.
                                    div()
                                        .id("suggestion-menu")
                                        .max_h(px(MAX_H))
                                        .overflow_y_scroll()
                                        .track_scroll(&scroll)
                                        .flex()
                                        .flex_col()
                                        .py(px(PAD))
                                        .children(rows)
                                }))
                                .children((count > 0).then(|| div().h(px(1.)).bg(menu_border)))
                                .child(clipboard)
                                .child(div().h(px(1.)).bg(menu_border))
                                .child(div().flex().flex_col().py(px(4.)).child(turn_row))
                                .children(thumb),
                        ),
                    ),
                )
            }))
            // The table right-click menu (Word-style row/column editing), anchored
            // at the click; each row runs its action on the caret's table cell.
            .children(self.table_menu.map(|anchor| {
                // Menu chrome from the host's theme (fallbacks match the former
                // hardcoded dark menu when no markdown style is set).
                let st = self.markdown_style.as_ref();
                let menu_bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
                let menu_border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
                let menu_fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
                let hover = st.map_or(rgba(0x2f6fd628).into(), |s| s.popover_hover);
                let divider = st.map_or(rgba(0xffffff2e).into(), |s| s.popover_divider);
                let mut thumb_c = st.map_or(rgba(0xffffff66).into(), |s| s.marker);
                thumb_c.a = 0.5;
                const ROW_H: f32 = 24.0;
                const DIV_H: f32 = 9.0;
                const PAD: f32 = 4.0;
                const MAX_H: f32 = 480.0;
                // Cditor-style grouped rows: a glyph column, a checkmark on the
                // current align/style, and the destructive group in red.
                let danger = st.map_or(rgb(0xE5484D).into(), |s| s.popover_danger);
                let cur_align = self.caret_table_align();
                let cur_style = self
                    .caret_table_region()
                    .map(|r| r.style)
                    .unwrap_or_default();
                use markdown_syntax::TableStyle as TS;
                enum Row {
                    Div,
                    Item {
                        glyph: &'static str,
                        label: SharedString,
                        action: TableMenuAction,
                        red: bool,
                        checked: bool,
                    },
                }
                let item = |glyph, label, action| Row::Item {
                    glyph,
                    label,
                    action,
                    red: false,
                    checked: false,
                };
                let specs = [
                    item(
                        "↑",
                        self.labels.insert_row_above.clone(),
                        TableMenuAction::InsertRowAbove,
                    ),
                    item(
                        "↓",
                        self.labels.insert_row_below.clone(),
                        TableMenuAction::InsertRowBelow,
                    ),
                    item(
                        "⧉",
                        self.labels.duplicate_row.clone(),
                        TableMenuAction::DuplicateRow,
                    ),
                    Row::Div,
                    item(
                        "←",
                        self.labels.insert_column_left.clone(),
                        TableMenuAction::InsertColLeft,
                    ),
                    item(
                        "→",
                        self.labels.insert_column_right.clone(),
                        TableMenuAction::InsertColRight,
                    ),
                    Row::Div,
                    Row::Item {
                        glyph: "",
                        label: self.labels.align_left.clone(),
                        action: TableMenuAction::AlignLeft,
                        red: false,
                        checked: cur_align == Some(CellAlign::Left),
                    },
                    Row::Item {
                        glyph: "",
                        label: self.labels.align_center.clone(),
                        action: TableMenuAction::AlignCenter,
                        red: false,
                        checked: cur_align == Some(CellAlign::Center),
                    },
                    Row::Item {
                        glyph: "",
                        label: self.labels.align_right.clone(),
                        action: TableMenuAction::AlignRight,
                        red: false,
                        checked: cur_align == Some(CellAlign::Right),
                    },
                    Row::Div,
                    Row::Item {
                        glyph: "▦",
                        label: self.labels.grid_style.clone(),
                        action: TableMenuAction::SetStyle(None),
                        red: false,
                        checked: cur_style == TS::Grid,
                    },
                    Row::Item {
                        glyph: "▤",
                        label: self.labels.striped_style.clone(),
                        action: TableMenuAction::SetStyle(Some("striped")),
                        red: false,
                        checked: cur_style == TS::Striped,
                    },
                    Row::Item {
                        glyph: "▥",
                        label: self.labels.header_style.clone(),
                        action: TableMenuAction::SetStyle(Some("header")),
                        red: false,
                        checked: cur_style == TS::Header,
                    },
                    Row::Item {
                        glyph: "─",
                        label: self.labels.minimal_style.clone(),
                        action: TableMenuAction::SetStyle(Some("minimal")),
                        red: false,
                        checked: cur_style == TS::Minimal,
                    },
                    Row::Div,
                    item(
                        "⊞",
                        self.labels.copy_as_markdown.clone(),
                        TableMenuAction::CopyTable,
                    ),
                    Row::Div,
                    Row::Item {
                        glyph: "✕",
                        label: self.labels.delete_row.clone(),
                        action: TableMenuAction::DeleteRow,
                        red: true,
                        checked: false,
                    },
                    Row::Item {
                        glyph: "✕",
                        label: self.labels.delete_column.clone(),
                        action: TableMenuAction::DeleteColumn,
                        red: true,
                        checked: false,
                    },
                    Row::Item {
                        glyph: "✕",
                        label: self.labels.delete_table.clone(),
                        action: TableMenuAction::DeleteTable,
                        red: true,
                        checked: false,
                    },
                ];
                let n_items = specs
                    .iter()
                    .filter(|r| matches!(r, Row::Item { .. }))
                    .count();
                let n_divs = specs.len() - n_items;
                let mut rows: Vec<gpui::AnyElement> = Vec::new();
                for (i, spec) in specs.into_iter().enumerate() {
                    match spec {
                        Row::Div => rows.push(
                            div()
                                .flex_shrink_0()
                                .h(px(1.))
                                .my(px(4.))
                                .mx(px(8.))
                                .bg(divider)
                                .into_any_element(),
                        ),
                        Row::Item {
                            glyph,
                            label,
                            action,
                            red,
                            checked,
                        } => {
                            let fg = if red { danger } else { menu_fg };
                            let mut glyph_c = fg;
                            glyph_c.a *= 0.7;
                            rows.push(
                                div()
                                    .id(("table-menu-row", i))
                                    .flex_shrink_0()
                                    .px(px(10.))
                                    .py(px(3.))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_color(fg)
                                    .hover(move |s| s.bg(hover))
                                    .child(
                                        div()
                                            .w(px(16.))
                                            .flex_none()
                                            .text_color(glyph_c)
                                            .child(glyph),
                                    )
                                    .child(div().flex_1().child(label))
                                    .children(checked.then(|| div().text_color(glyph_c).child("✓")))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            action.apply(editor, cx);
                                        }),
                                    )
                                    .into_any_element(),
                            );
                        }
                    }
                }
                // Scrollbar thumb, shown when the items overflow the cap — sized from
                // the content height + positioned from the live scroll offset.
                let rows_h = n_items as f32 * ROW_H + n_divs as f32 * DIV_H;
                let view_h = MAX_H - 2.0 * PAD;
                let thumb = (rows_h > view_h).then(|| {
                    let scrolled =
                        (-f32::from(self.table_menu_scroll.offset().y)).clamp(0.0, rows_h - view_h);
                    let thumb_h = (view_h * view_h / rows_h).max(24.0);
                    let thumb_top = PAD + scrolled / (rows_h - view_h) * (view_h - thumb_h);
                    div()
                        .absolute()
                        .top(px(thumb_top))
                        .right(px(2.))
                        .w(px(6.))
                        .h(px(thumb_h))
                        .rounded(px(3.))
                        .bg(thumb_c)
                });
                gpui::deferred(
                    gpui::anchored().position(anchor).snap_to_window().child(
                        div()
                            .relative()
                            .occlude()
                            .min_w(px(190.))
                            .cursor(CursorStyle::Arrow)
                            .bg(menu_bg)
                            .border_1()
                            .border_color(menu_border)
                            .rounded(px(6.))
                            .shadow_md()
                            .overflow_hidden()
                            .text_color(menu_fg)
                            .text_size(px(13.))
                            .on_mouse_down_out(cx.listener(|editor, _: &MouseDownEvent, _, cx| {
                                editor.table_menu = None;
                                cx.notify();
                            }))
                            .child(
                                // Inner scroll viewport: caps the height + scrolls the
                                // overflow (max_h on a separate flex-col div, like the
                                // suggestion menu — combining it with the styled box
                                // above doesn't cap).
                                div()
                                    .id("table-menu")
                                    .max_h(px(MAX_H))
                                    .overflow_y_scroll()
                                    .track_scroll(&self.table_menu_scroll)
                                    .flex()
                                    .flex_col()
                                    .py(px(PAD))
                                    .children(rows),
                            )
                            .children(thumb),
                    ),
                )
            }))
            // The image right-click menu: Word-style object actions on an inline
            // image (Delete), anchored at the click. Chrome matches the table menu.
            .children(self.prop_menu.map(|(row, anchor)| {
                let st = self.markdown_style.as_ref();
                let menu_bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
                let menu_border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
                let menu_fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
                let hover = st.map_or(rgba(0x2f6fd628).into(), |s| s.popover_hover);
                let item = |id: &'static str, label: SharedString| {
                    div()
                        .id(id)
                        .px(px(10.))
                        .py(px(3.))
                        .hover(move |s| s.bg(hover))
                        .child(label)
                };
                gpui::deferred(
                    gpui::anchored().position(anchor).snap_to_window().child(
                        div()
                            .occlude()
                            .min_w(px(160.))
                            .cursor(CursorStyle::Arrow)
                            .bg(menu_bg)
                            .border_1()
                            .border_color(menu_border)
                            .rounded(px(6.))
                            .shadow_md()
                            .overflow_hidden()
                            .text_color(menu_fg)
                            .text_size(px(13.))
                            .py(px(4.))
                            .on_mouse_down_out(cx.listener(|editor, _: &MouseDownEvent, _, cx| {
                                editor.prop_menu = None;
                                cx.notify();
                            }))
                            .child(
                                item("prop-menu-edit", self.labels.edit_properties.clone())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            editor.prop_menu = None;
                                            if let Some((range, source)) =
                                                editor.property_block_at(row)
                                            {
                                                let block_row = row - editor.row_col(range.start).0;
                                                cx.emit(EditorEvent::EditProperties {
                                                    range,
                                                    source,
                                                    at_end: false,
                                                    row: Some(block_row),
                                                });
                                            }
                                        }),
                                    ),
                            )
                            .child(
                                item("prop-menu-delete", self.labels.delete_property.clone())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            editor.prop_menu = None;
                                            editor.delete_property_row(row, cx);
                                        }),
                                    ),
                            ),
                    ),
                )
            }))
            .children(self.image_menu.map(|(line, anchor)| {
                let st = self.markdown_style.as_ref();
                let menu_bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
                let menu_border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
                let menu_fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
                let hover = st.map_or(rgba(0x2f6fd628).into(), |s| s.popover_hover);
                gpui::deferred(
                    gpui::anchored().position(anchor).snap_to_window().child(
                        div()
                            .occlude()
                            .min_w(px(140.))
                            .cursor(CursorStyle::Arrow)
                            .bg(menu_bg)
                            .border_1()
                            .border_color(menu_border)
                            .rounded(px(6.))
                            .shadow_md()
                            .overflow_hidden()
                            .text_color(menu_fg)
                            .text_size(px(13.))
                            .py(px(4.))
                            .on_mouse_down_out(cx.listener(|editor, _: &MouseDownEvent, _, cx| {
                                editor.image_menu = None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("image-menu-delete")
                                    .px(px(10.))
                                    .py(px(3.))
                                    .hover(move |s| s.bg(hover))
                                    .child(self.labels.delete_image.clone())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            editor.image_menu = None;
                                            editor.delete_image_row(line, cx);
                                        }),
                                    ),
                            ),
                    ),
                )
            }))
            .children(self.code_lang_menu.map(|(row, anchor)| {
                // The code block's language picker (Cditor-inspired): the host's
                // highlighter languages, scrollable past the cap, current one
                // checked. Selecting rewrites the opening fence (one undo step).
                let st = self.markdown_style.as_ref();
                let menu_bg = st.map_or(rgb(0x26262b).into(), |s| s.popover_bg);
                let menu_border = st.map_or(rgb(0x45454c).into(), |s| s.popover_border);
                let menu_fg = st.map_or(rgb(0xe6e6e6).into(), |s| s.popover_fg);
                let hover = st.map_or(rgba(0x2f6fd628).into(), |s| s.popover_hover);
                let mut thumb_c = st.map_or(rgba(0xffffff66).into(), |s| s.marker);
                thumb_c.a = 0.5;
                let current = self.code_block_at(row).map(|(l, _)| l).unwrap_or_default();
                const ROW_H: f32 = 22.0;
                const MAX_H: f32 = 260.0;
                const PAD: f32 = 4.0;
                let langs = self.code_langs.clone();
                let rows_h = langs.len() as f32 * ROW_H;
                let view_h = MAX_H - 2.0 * PAD;
                let thumb = (rows_h > view_h).then(|| {
                    let scrolled =
                        (-f32::from(self.code_lang_scroll.offset().y)).clamp(0.0, rows_h - view_h);
                    let thumb_h = (view_h * view_h / rows_h).max(24.0);
                    let thumb_top = PAD + scrolled / (rows_h - view_h) * (view_h - thumb_h);
                    div()
                        .absolute()
                        .top(px(thumb_top))
                        .right(px(2.))
                        .w(px(6.))
                        .h(px(thumb_h))
                        .rounded(px(3.))
                        .bg(thumb_c)
                });
                gpui::deferred(
                    gpui::anchored().position(anchor).snap_to_window().child(
                        div()
                            .relative()
                            .occlude()
                            .min_w(px(140.))
                            .cursor(CursorStyle::Arrow)
                            .bg(menu_bg)
                            .border_1()
                            .border_color(menu_border)
                            .rounded(px(6.))
                            .shadow_md()
                            .overflow_hidden()
                            .text_color(menu_fg)
                            .text_size(px(13.))
                            .py(px(PAD))
                            .on_mouse_down_out(cx.listener(|editor, _: &MouseDownEvent, _, cx| {
                                editor.code_lang_menu = None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("code-lang-list")
                                    .max_h(px(MAX_H - 2.0 * PAD))
                                    .overflow_y_scroll()
                                    .track_scroll(&self.code_lang_scroll)
                                    .children(langs.into_iter().enumerate().map(|(i, lang)| {
                                        let is_current = *lang == current
                                            || (current.is_empty() && *lang == *"text");
                                        let label: SharedString = if is_current {
                                            format!("{lang} ✓").into()
                                        } else {
                                            lang.clone()
                                        };
                                        div()
                                            .id(("code-lang-row", i))
                                            .flex_shrink_0()
                                            .h(px(ROW_H))
                                            .px(px(10.))
                                            .py(px(2.))
                                            .hover(move |s| s.bg(hover))
                                            .child(label)
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(
                                                    move |editor, _: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        editor.code_lang_menu = None;
                                                        editor.set_code_lang(row, &lang, cx);
                                                    },
                                                ),
                                            )
                                            .into_any_element()
                                    })),
                            )
                            .children(thumb),
                    ),
                )
            }))
    }
}

/// The shaped width of `text` at `font_size` — used to inset a gutter line's body
/// to exactly where its (hidden) source prefix ends, so the rendered + raw views
/// line up (and tab/space nesting matches the actual whitespace width).
fn measure_width(window: &mut Window, text: &str, font: &Font, font_size: Pixels) -> Pixels {
    if text.is_empty() {
        return px(0.);
    }
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(
            SharedString::from(text.to_string()),
            font_size,
            &[run],
            None,
        )
        .width()
}

/// Shape `text` with pre-built `runs`, so diagnostics can underline specific
/// spans. The plain-run [`shape_all`] is used for the placeholder + measurement.
fn shape_runs(
    window: &mut Window,
    text: &SharedString,
    font_size: Pixels,
    runs: &[TextRun],
    wrap_width: Option<Pixels>,
) -> Vec<WrappedLine> {
    window
        .text_system()
        .shape_text(text.clone(), font_size, runs, wrap_width, None)
        .map(|lines| lines.into_vec())
        .unwrap_or_default()
}

/// A line currently rendered as an inline image (W4) instead of its source text:
/// the decoded image plus its fit-to-width display size (logical px).
#[derive(Clone)]
struct BlockImg {
    img: Arc<RenderImage>,
    width: Pixels,
    height: Pixels,
    /// Whether to show a corner resize grip. `false` for math (nothing to persist a
    /// `{width=N}` to, and it renders at its natural typeset size); `true` for images.
    resizable: bool,
    /// Horizontal alignment in the content width. `Left` for images; display math sets its
    /// own (centered by default).
    align: MathAlign,
}

/// One inline `$…$` formula painted within a text line. `display_off` is the byte offset of its
/// invisible spacer in the shaped DISPLAY string (resolved to an x via the wrapped line at
/// paint); `source` is the formula's byte range within the *source line* (to hit-test a click
/// back to its edit range); `img`/`width`/`height` are the typeset raster scaled to text size.
#[derive(Clone)]
struct InlineMath {
    display_off: usize,
    /// Byte length of the spacer this raster sits over. On an RTL row
    /// `display_off` is its RIGHT edge, so the whole span is needed to find the
    /// left one — see where it is painted.
    len: usize,
    /// ABSOLUTE byte range of the `$…$` span in the document — to hit-test a click on the
    /// formula back to its edit range and to position the seated editor.
    source: Range<usize>,
    /// The inner LaTeX (no `$` delimiters), to seed the structural editor on click.
    latex: SharedString,
    img: Arc<RenderImage>,
    width: Pixels,
    height: Pixels,
}

/// A line rendered as a block widget instead of its source text: a standalone
/// image, or a clickable file chip (e.g. a PDF — left-click opens it, right-click
/// edits). Shown only while the caret is off the line ("raw on caret").
#[derive(Clone)]
enum Block {
    Image(BlockImg),
    Chip {
        src: SharedString,
        label: SharedString,
        /// Label color (accent, signalling clickable), box fill, box border.
        link: Hsla,
        bg: Hsla,
        border: Hsla,
        height: Pixels,
        /// `src` is a wiki target (an `![[embed]]` chip → OpenWikiLink, which
        /// navigates + jumps to any anchor) vs a file path (→ OpenLink).
        wiki: bool,
    },
    /// A run of `key:: value` properties as a two-column panel (the reader's
    /// `render_property_table` twin). Painted on the region's first line; the
    /// rest of the region's lines collapse. The caret entering the region
    /// reveals the raw source (like a math block).
    Properties(PropPanel),
}

/// A rendered piece of a property value in the panel: plain text, or a colored
/// pill (tag / wiki-link / URL).
#[derive(Clone)]
enum PanelSeg {
    Plain(SharedString),
    Pill {
        text: SharedString,
        color: Hsla,
        target: zorite_markdown::syntax::LinkHit,
    },
}

/// Layout for a WYSIWYG property panel: the measured rows (key + value
/// segments), the column widths + per-row height, and the key/value colors. No
/// grid lines — rows read clean (Obsidian-style); the value's tags and
/// wiki-links render as pills.
#[derive(Clone)]
struct PropPanel {
    /// `(key, icon asset path, value segments)` per property line, in order.
    rows: Vec<(SharedString, Option<SharedString>, Vec<PanelSeg>)>,
    key_w: Pixels,
    /// Panel width (shared by every row) so hover borders align.
    width: Pixels,
    row_h: Pixels,
    height: Pixels,
    /// Icon draw size (0 when the host resolves no icons); the key text is inset
    /// by `key_indent` to leave room for it.
    icon_sz: Pixels,
    key_indent: Pixels,
    key_color: Hsla,
    value_color: Hsla,
    /// The rounded border drawn around the row under the pointer (Obsidian-style
    /// whole-row hover).
    hover_border: Hsla,
}

impl Block {
    fn height(&self) -> Pixels {
        match self {
            Block::Image(i) => i.height,
            Block::Chip { height, .. } => *height,
            Block::Properties(p) => p.height,
        }
    }
}

/// A fenced-code-block line's background (W4b/refinement): the block reads as one
/// rounded, content-fit box (sized to its widest line, like a table — not the
/// full editor width). Each line carries the block color, the shared box width
/// (back-patched once the block's extent is known), and whether it's the
/// first/last visible line (to round the box's top/bottom corners).
/// Last-frame hit rects for one code card's chrome (see `code_chip_rects`).
#[derive(Clone)]
struct CodeChipHit {
    lang: Bounds<Pixels>,
    copy: Bounds<Pixels>,
    fence_row: usize,
}

/// A code card's top-right chrome, laid out in prepaint: the language tag and
/// Copy button (Cditor-inspired, issue #16). Geometry is window-space; paint
/// draws at these bounds and the hitboxes flip the cursor.
struct CodeChip {
    lang_text: SharedString,
    copy_text: SharedString,
    lang_bounds: Bounds<Pixels>,
    copy_bounds: Bounds<Pixels>,
    fence_row: usize,
    /// Card background — the labels sit on an opaque pill of it so they stay
    /// readable over a long first line.
    bg: Hsla,
    fg: Hsla,
    lang_hb: Hitbox,
    copy_hb: Hitbox,
}

#[derive(Clone, Copy)]
struct CodeBg {
    color: Hsla,
    width: Pixels,
    top: bool,
    bottom: bool,
}

/// A table row rendered as a grid (W4c): its cells, per-column alignment, the
/// content-fit per-column widths (shared across the table), header/separator/
/// last-row flags, and the border color. Built only when the caret is outside
/// the table — the caret's table shows source instead ("raw on caret").
#[derive(Clone)]
struct TableRow {
    cells: Vec<SharedString>,
    /// Byte range of each cell's trimmed content within its source line — for
    /// placing the caret inside a cell + hit-testing a click back to a source
    /// offset (in-cell editing).
    cell_ranges: Vec<Range<usize>>,
    aligns: Vec<markdown_syntax::Align>,
    col_widths: Vec<Pixels>,
    is_header: bool,
    is_separator: bool,
    is_last: bool,
    /// 0-based position among the body rows (`None` for header/separator) — drives
    /// striping (shade odd indices) + the rule-under-header (index 0).
    body_index: Option<usize>,
    /// The table's visual style (from its `<!-- table:STYLE -->` marker).
    style: markdown_syntax::TableStyle,
    border: Hsla,
    /// Row-shade color for striped / header-shaded styles (a faint tint).
    shade: Hsla,
    /// The table reads right-to-left (#66) — from its region's
    /// [`markdown_syntax::TableRegion::rtl`], so every row of one table agrees.
    rtl: bool,
}

/// A per-line "gutter" decoration: a left-margin treatment that hides its source
/// marker and renders something in its place, with the body text inset to make
/// Task checkbox edge, as a fraction of the line's font size — shared by the
/// shaping (body inset), the pointer hitbox, and the paint so they agree.
const CHECKBOX_SCALE: f32 = 0.9;

/// room. Covers blockquotes now; list bullets + task checkboxes reuse it.
#[derive(Clone, Copy)]
enum LineMark {
    /// Blockquote: a left border (`bar`); the `>` markers are hidden. `text`
    /// colors the body — `Some` = muted quote tone, `None` = the editor's
    /// normal text color (alert bodies).
    Quote { bar: Hsla, text: Option<Hsla> },
    /// A GitHub alert's marker line (`> [!NOTE]` …): the marker is hidden and
    /// a bold `label` paints in the alert color; any same-line body insets to
    /// `text_inset` (QUOTE_INSET + the label's measured width + a gap) — the
    /// list-bullet pattern. Continuation lines are `Quote` marks with the
    /// alert's bar color.
    Alert {
        bar: Hsla,
        label: &'static str,
        kind: markdown_syntax::AlertKind,
        text_inset: Pixels,
        /// Foldable callout (`[!NOTE]-`/`+`): `Some(true)` = folded. A chevron
        /// paints at `chevron_x` (after the label) and clicking it flips the
        /// fold char in the source.
        fold: Option<bool>,
        chevron_x: Pixels,
    },
    /// List item: a painted bullet (`•`) or number (`N.`) at `bullet_x` (where the
    /// hidden source marker began), muted; the body sits at `text_inset` — the
    /// measured width of the whole source prefix, so the rendered + raw views
    /// line up exactly and tab/space nesting stays in sync.
    List {
        bullet_x: Pixels,
        text_inset: Pixels,
        ordered: bool,
        num: u32,
        /// Structural nesting level (0 = top), for the Word-style marker
        /// scheme (`1.` -> `a.` -> `i.`).
        level: usize,
        color: Hsla,
    },
    /// GFM task item: a painted ☐/☑ box at `bullet_x`, muted; the body sits at
    /// `text_inset` (measured prefix width) like a list item.
    Check {
        bullet_x: Pixels,
        text_inset: Pixels,
        checked: bool,
        color: Hsla,
        /// Fill for a done box (the host's link/accent color; white check on top).
        accent: Hsla,
    },
    /// Thematic break (`---`): a full-width muted divider painted in place of the
    /// source; the line has no body text (reveal-on-caret shows the raw `---`).
    Rule(Hsla),
}

impl LineMark {
    /// Horizontal inset (px) applied to the body text + caret for this mark.
    fn inset(self) -> Pixels {
        match self {
            LineMark::Quote { .. } => px(QUOTE_INSET),
            LineMark::Alert { text_inset, .. } => text_inset,
            LineMark::List { text_inset, .. } | LineMark::Check { text_inset, .. } => text_inset,
            LineMark::Rule(_) => px(0.),
        }
    }
}

/// Per-logical-line shaping output — parallel vecs of equal length: the shaped
/// source line, its row height, an optional inline-image widget, an optional
/// fenced-code-block background, an optional table-row grid, the display→source
/// map, and an optional gutter decoration (blockquote / list / checkbox).
/// One shaped document: per-line parallel channels, all the same length —
/// the per-line loop's normal push and [`ShapedDoc::push_placeholder`] are
/// the only writers, so the lockstep invariant lives here.
#[derive(Default)]
struct ShapedDoc {
    wrapped: Vec<WrappedLine>,
    heights: Vec<Pixels>,
    widgets: Vec<Option<Block>>,
    backgrounds: Vec<Option<CodeBg>>,
    tables: Vec<Option<TableRow>>,
    /// Per-line display→source byte map for lines with markers hidden (W6);
    /// `None` when the displayed text equals the source. Shared with the
    /// line-run cache (a hit re-uses the same allocation across frames).
    maps: Vec<Option<std::rc::Rc<Vec<usize>>>>,
    marks: Vec<Option<LineMark>>,
    /// Per-line inline `$…$` formulas painted over spacers (empty when none).
    inline_maths: Vec<Vec<InlineMath>>,
    /// Per-line wrap-row count. Geometry (line tops, total height) reads THIS,
    /// not `wrap_boundaries()` — a windowed-out line's `WrappedLine` is an
    /// empty placeholder, but its cached count keeps the layout exact. For an
    /// RTL line it is OUR row count, which is not gpui's (#66).
    wrap_rows: Vec<usize>,
    /// Per-line bidi layout: whether the line READS right-to-left, plus the
    /// rows we broke in logical order. `None` for lines with no RTL at all,
    /// which keep gpui's own wrapping. Drives that line's paint, caret,
    /// selection and hit-testing.
    rtl_rows: Vec<Option<(bool, Vec<gpui_bidi::paragraph::Row>)>>,
}

impl ShapedDoc {
    /// Push one line that renders as something other than shaped text — a
    /// widget/collapsed/windowed-out line: an empty placeholder `WrappedLine`,
    /// the given height/widget/mark, and `rows` wrap rows.
    #[allow(clippy::too_many_arguments)]
    fn push_placeholder(
        &mut self,
        window: &mut Window,
        base_font_size: Pixels,
        wrap_width: Option<Pixels>,
        h: Pixels,
        widget: Option<Block>,
        mark: Option<LineMark>,
        rows: usize,
    ) {
        let wl = shape_runs(
            window,
            &SharedString::default(),
            base_font_size,
            &[],
            wrap_width,
        )
        .into_iter()
        .next()
        .expect("a line always shapes to one wrapped line");
        self.wrapped.push(wl);
        self.heights.push(h);
        self.widgets.push(widget);
        self.backgrounds.push(None);
        self.tables.push(None);
        self.maps.push(None);
        self.marks.push(mark);
        self.inline_maths.push(Vec::new());
        self.wrap_rows.push(rows);
        self.rtl_rows.push(None);
    }
}

/// Rewrite an image source `line` to carry an explicit `{width=N}` after the
/// `![alt](src)` (replacing any existing `{width=...}`), preserving a leading
/// list marker and any trailing whitespace. Used to persist a corner-grip resize
/// back into the document. Returns `line` unchanged if it isn't an image row.
fn set_image_width(line: &str, width: u32) -> String {
    let Some((_, _, marker_len)) = markdown_syntax::image_row(line) else {
        return line.to_string();
    };
    // Split off any trailing whitespace so the attr lands right after `)` (or the
    // existing `{width=…}`), with the original trailing run re-appended.
    let trimmed_end = line.trim_end_matches([' ', '\t']);
    let trailing_ws = &line[trimmed_end.len()..];
    // The image body always ends at the first `)` after the list marker; an
    // existing `{width=…}` (only valid right after it) is dropped.
    let close = marker_len + line[marker_len..].find(')').map_or(0, |i| i + 1);
    let body = trimmed_end[..close.min(trimmed_end.len())].trim_end();
    format!("{body}{{width={width}}}{trailing_ws}")
}

/// Invert a display→source offset map: the display column for `source_col`. The
/// map is ascending, so a source column that is hidden (a collapsed marker)
/// snaps to the next visible display column. `None` map → identity (a row shown
/// as full source). The prepaint cursor/selection pass this frame's fresh map
/// (the committed `EditorState::offset_maps` lags a frame); event handlers go
/// through [`EditorState::display_col`], which uses the committed map.
fn display_col_in(map: Option<&std::rc::Rc<Vec<usize>>>, source_col: usize) -> usize {
    match map {
        // The first display byte whose source ≥ `source_col` (a leftmost lower-bound). Unlike
        // `binary_search`, this is deterministic when several display bytes share one source
        // offset — an inline `$…$` spacer maps its whole width to the span start, so the caret
        // just before the formula must land at the spacer's LEFT edge, not somewhere inside it.
        Some(m) => m.partition_point(|&s| s < source_col),
        None => source_col,
    }
}

/// The painted position of display column `dcol` on a shaped line: gpui's own
/// lookup, with the x taken from the row's bidi map when it has one (#66).
///
/// gpui's `x_for_index` returns the first glyph whose `index >= dcol`, and the
/// first glyph of an RTL line carries the HIGHEST index — so every offset in
/// the line collapses onto x = 0. The y (which wrap row) stays gpui's; a row
/// only gets a map while it is ONE visual row, so it is always 0 there.
/// Excludes the row's insets — callers add [`EditorState::row_origin_x`].
fn line_pos(
    line: &WrappedLine,
    rtl: Option<&RtlRow>,
    dcol: usize,
    lh: Pixels,
) -> Option<Point<Pixels>> {
    match rtl {
        // Our own rows: which one holds the offset decides the y, and the
        // row's map the x. gpui's layout is not consulted at all — its rows
        // are not ours.
        Some(r) => {
            let (row, local) = r.row_of(dcol);
            let x = px(r.rows.get(row)?.map.x_for_index(local)) + r.shift_delta(row);
            Some(point(x, lh * row))
        }
        None => line.position_for_index(dcol, lh),
    }
}

/// The display column a click at row-local `p` names — the inverse of
/// [`line_pos`], through the bidi map when the row has one (gpui's
/// `closest_index_for_x` fails on RTL the same way `x_for_index` does).
fn line_index_at(line: &WrappedLine, rtl: Option<&RtlRow>, p: Point<Pixels>, lh: Pixels) -> usize {
    match rtl {
        Some(r) if !r.rows.is_empty() => {
            let row = ((f32::from(p.y) / f32::from(lh).max(1.0)).floor().max(0.) as usize)
                .min(r.rows.len() - 1);
            let x = p.x - r.shift_delta(row);
            let Some(rr) = r.rows.get(row) else {
                return 0;
            };
            rr.start + rr.map.index_for_x(f32::from(x))
        }
        _ => match line.closest_index_for_position(p, lh) {
            Ok(i) | Err(i) => i,
        },
    }
}

/// A right-to-left row's editor-side geometry, built in prepaint (#66).
///
/// Only rows whose source reads RTL ([`zorite_markdown::syntax::base_direction`])
/// get one — the flag *is* `Option::is_some`, so an LTR document allocates
/// nothing and keeps taking gpui's own (cheaper) lookups.
pub(crate) struct RtlRow {
    /// The line's visual rows, broken in LOGICAL order (gpui's own wrapping
    /// slices the reordered glyph run and gets them backwards). Each carries
    /// the map that turns an offset inside it into an x and back.
    rows: Vec<gpui_bidi::paragraph::Row>,
    /// Each row's right-align shift, parallel to `rows`: added to the text
    /// origin so the row right-aligns in the content width (see [`rtl_shift`]).
    /// Per ROW, not per line — a short last row shifts further than a full one.
    /// Kept OUT of `line_insets` on purpose: the list-marker + gutter math
    /// reads that, and must not move with the text.
    shifts: Vec<Pixels>,
    /// Does the line READ right-to-left? A left-to-right line containing a
    /// Persian phrase gets rows and maps too, but stays left-aligned and keeps
    /// left-to-right arrow keys.
    base_rtl: bool,
}

impl RtlRow {
    /// The row holding display column `dcol`, and the column's offset within
    /// it. Rows are in reading order and contiguous, so the last row wins for
    /// an offset at the very end of the line.
    fn row_of(&self, dcol: usize) -> (usize, usize) {
        row_of_spans(self.rows.iter().map(|r| (r.start, r.len)), dcol)
    }

    /// A row's horizontal extent (left, right) relative to the FIRST row's
    /// origin — the coordinate space callers already work in, since they add
    /// `row_origin_x`. Used to band a selection across a row: an RTL row does
    /// not span the content width, so "the whole row" is this, not 0..width.
    pub(crate) fn row_extent(&self, row: usize) -> (Pixels, Pixels) {
        let d = self.shift_delta(row);
        (d, d + self.rows.get(row).map_or(px(0.), |r| r.width))
    }

    /// How many visual rows this line broke into.
    pub(crate) fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// A row's shift relative to the FIRST row's. Callers already add
    /// `row_origin_x`, which carries row 0's shift, so this is the remainder —
    /// that keeps every existing caller correct without threading a wrap-row
    /// index through all of them.
    fn shift_delta(&self, row: usize) -> Pixels {
        self.shifts.get(row).copied().unwrap_or(px(0.))
            - self.shifts.first().copied().unwrap_or(px(0.))
    }
}

/// Which row holds display column `dcol`, and its offset within that row.
///
/// Split out from [`RtlRow::row_of`] so it can be tested without a window: a
/// `Row` carries a shaped line, which needs one. Rows are contiguous and in
/// reading order, so an offset at the very end of the line lands on the last
/// row rather than falling off the end — that is where the caret sits after
/// typing at the end of a paragraph.
fn row_of_spans(rows: impl Iterator<Item = (usize, usize)>, dcol: usize) -> (usize, usize) {
    let mut last = (0, dcol);
    let mut seen = false;
    for (i, (start, len)) in rows.enumerate() {
        seen = true;
        last = (i, dcol.saturating_sub(start));
        if dcol < start + len {
            return (i, dcol - start);
        }
    }
    if seen { last } else { (0, dcol) }
}

/// The x a right-to-left row's text starts at within `content_width`, so its
/// *trailing* edge sits `inset` in from the right — the mirror of the leading
/// `inset` an LTR row gets. Zero once the text no longer fits (it wraps, and
/// every wrap row starts at the left edge).
///
/// Callers add this to the origin they already inset, hence the doubled
/// `inset`: `origin + inset + shift` lands the text `inset` from the right.
fn rtl_shift(content_width: Pixels, inset: Pixels, line_width: Pixels) -> Pixels {
    (content_width - inset * 2. - line_width).max(px(0.))
}

/// Mirror a gutter marker's x (a bullet, a number, a checkbox) to the right
/// edge for an RTL row, so it sits on the side the text now starts at —
/// matching the reader's `flex_row_reverse` list items. Nesting is preserved:
/// a deeper level's larger `marker_x` indents further from the right.
fn rtl_marker_x(content_width: Pixels, marker_x: Pixels, marker_width: Pixels) -> Pixels {
    (content_width - marker_x - marker_width).max(px(0.))
}

/// A task row's checkbox x within the content width, mirrored on an RTL row.
/// One function so the prepaint hitbox (the hand cursor) and the paint (the
/// box, and the rects a click hit-tests) can never land on different sides.
fn checkbox_x(bullet_x: Pixels, size: Pixels, content_width: Pixels, rtl: bool) -> Pixels {
    if rtl {
        rtl_marker_x(content_width, bullet_x, size)
    } else {
        bullet_x
    }
}

/// Case-insensitive occurrences of `query` in `content`, as source byte
/// ranges — the match list a find bar feeds to [`EditorState::set_search`].
/// Unicode-aware (comparison happens on lowercased text through an index map
/// back to original byte offsets). An empty query matches nothing.
pub fn find_in_source(content: &str, query: &str) -> Vec<Range<usize>> {
    let query: String = query.chars().flat_map(char::to_lowercase).collect();
    if query.is_empty() {
        return Vec::new();
    }
    // Lowercased haystack + per-byte map back to original offsets. Boundaries
    // survive: each original char lowercases to >= 1 chars, all of whose bytes
    // map to the original char's start.
    let mut lower = String::with_capacity(content.len());
    let mut map = Vec::with_capacity(content.len() + 1);
    for (off, ch) in content.char_indices() {
        for lc in ch.to_lowercase() {
            lower.push(lc);
            map.resize(lower.len(), off);
        }
    }
    lower
        .match_indices(&query)
        .map(|(i, m)| map[i]..map.get(i + m.len()).copied().unwrap_or(content.len()))
        .collect()
}

/// Paint a flat, line-art document glyph (a page with a folded top-right corner +
/// two text lines) in `color`, the chip's file icon. Drawn with strokes — not a
/// font emoji — so it reads flat and on-theme at the text's size. Public so a
/// host's read-only view can draw the identical icon on its own file chips
/// (cross-view parity).
pub fn paint_doc_icon(
    x: Pixels,
    y: Pixels,
    w: Pixels,
    h: Pixels,
    color: Hsla,
    window: &mut Window,
) {
    let f = w * 0.33; // folded-corner size
    // Page silhouette, with the top-right corner cut away for the fold.
    let mut outline = PathBuilder::stroke(px(1.3));
    outline.move_to(point(x, y));
    outline.line_to(point(x + w - f, y));
    outline.line_to(point(x + w, y + f));
    outline.line_to(point(x + w, y + h));
    outline.line_to(point(x, y + h));
    outline.line_to(point(x, y));
    if let Ok(p) = outline.build() {
        window.paint_path(p, color);
    }
    // The folded corner (dog-ear).
    let mut fold = PathBuilder::stroke(px(1.3));
    fold.move_to(point(x + w - f, y));
    fold.line_to(point(x + w - f, y + f));
    fold.line_to(point(x + w, y + f));
    if let Ok(p) = fold.build() {
        window.paint_path(p, color);
    }
    // Two short text lines below the fold.
    for fy in [0.6_f32, 0.78] {
        let mut ln = PathBuilder::stroke(px(1.));
        ln.move_to(point(x + w * 0.26, y + h * fy));
        ln.line_to(point(x + w * 0.74, y + h * fy));
        if let Ok(p) = ln.build() {
            window.paint_path(p, color);
        }
    }
}

/// How many wrap rows table row `cells` need at `col_widths` — 1 for content
/// that fits; more once a drag-narrowed column forces its text to wrap.
/// The gutter grip's left edge for an editor whose content starts at
/// `bounds_left` — THE grip x formula, shared by prepaint (fresh geometry)
/// and the event-time hover mirror so the two can't drift.
fn grip_left(bounds_left: Pixels, inset: Pixels) -> Pixels {
    bounds_left - px(22.) - inset
}

/// One markdown line's built display + runs, cached across frames (see
/// `EditorState::line_run_cache`). `src` and `line_base` verify a hash hit —
/// the rest of the inputs are folded into the key's hash.
struct CachedLineRuns {
    src: String,
    line_base: Hsla,
    /// Shared payloads: a cache HIT is three refcount bumps, not three deep
    /// clones (this runs per visible line per frame).
    disp: SharedString,
    runs: std::rc::Rc<Vec<TextRun>>,
    map: std::rc::Rc<Vec<usize>>,
}

/// The validity marker + per-row content keys of the run-key memo.
type RowKeys = (Option<u64>, Vec<Option<u64>>);

/// The measured table column widths cache: one keyed entry for the doc.
type RegionCols = Option<(u64, std::rc::Rc<Vec<Vec<Pixels>>>)>;

/// The editor-owned caches `shape_document` reads and writes (interior-
/// mutable — shaping runs under a read borrow of the editor):
/// - `line_runs`: each markdown line's built display + runs (cross-frame).
/// - `region_cols`: the measured table column widths for the WHOLE document,
///   one keyed entry — rebuilt when the tables' source, the wrap width, the
///   font epoch, or a live column drag changes.
/// - `cell_rows`: per table row, how many wrap rows its tallest cell needs.
#[derive(Default)]
struct ShapeCaches {
    line_runs: std::cell::RefCell<std::collections::HashMap<u64, CachedLineRuns>>,
    region_cols: std::cell::RefCell<RegionCols>,
    cell_rows: std::cell::RefCell<std::collections::HashMap<u64, usize>>,
    /// Per-line (row height, wrap rows), keyed by the line-run key ⊕ font
    /// size ⊕ wrap width — the shaping window's exact heights for skipped
    /// offscreen lines.
    line_heights: std::cell::RefCell<std::collections::HashMap<u64, (Pixels, usize)>>,
    /// Per-row CONTENT-part run keys (line bytes + line_base + diags + epoch),
    /// valid for one (scan generation, epoch) pair — so a steady-state frame
    /// hashes three u64s per line instead of every line's bytes. Diagnostics
    /// changes invalidate explicitly (see `set_diagnostics`).
    row_keys: std::cell::RefCell<RowKeys>,
    /// The fenced code blocks' per-line token colors, keyed by (scan generation,
    /// `highlighter_gen`): the host highlighter runs once per edit rather than
    /// for every block of the document on every frame (a scroll repaints).
    code_highlights: std::cell::RefCell<Option<((u64, u64), std::rc::Rc<CodeLineHighlights>)>>,
    /// Bumped by `set_code_highlighter`, retiring `code_highlights`.
    highlighter_gen: std::cell::Cell<u64>,
}

/// Token colors per code line (line index → in-line ranges).
pub(crate) type CodeLineHighlights =
    std::collections::HashMap<usize, Vec<(Range<usize>, HighlightStyle)>>;

/// A hash of the inputs shared by every line's run build (font + palette) —
/// part of the per-line cache key, so a theme or font change misses cleanly.
fn line_run_epoch(font: &Font, st: Option<&SyntaxStyle>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    font.family.hash(&mut h);
    font.weight.0.to_bits().hash(&mut h);
    let hash_hsla = |c: Hsla, h: &mut std::collections::hash_map::DefaultHasher| {
        c.h.to_bits().hash(h);
        c.s.to_bits().hash(h);
        c.l.to_bits().hash(h);
        c.a.to_bits().hash(h);
    };
    if let Some(st) = st {
        for c in [
            st.marker, st.code, st.code_bg, st.link, st.tag, st.quote, st.mark_bg,
        ] {
            hash_hsla(c, &mut h);
        }
        st.mono.family.hash(&mut h);
        st.block_label_gen.hash(&mut h);
    }
    h.finish()
}

/// Host hook for scroll anchoring: called from the measure pass when an
/// ASYNC height change (a math/mermaid/image raster arriving) lands ABOVE
/// the window's viewport, with the height delta — the host shifts its scroll
/// container's offset by it so the content being read doesn't jump.
pub type ScrollCompensatorFn = std::rc::Rc<dyn Fn(Pixels, &mut Window, &mut App)>;

/// Content-derived structural scans — tables, ordered-list numbering,
/// mermaid/math regions, property runs, foldable callouts, and per-line
/// fence parity — cached per [`EditorState::content_gen`]. `shape_document`
/// recomputed all of these on every call (twice a frame before the shape
/// memo); now they rebuild only when the content actually changes, and the
/// caret-driven table ops + auto-replace reuse the same scan.
pub(crate) struct ScanData {
    /// The `content_gen` this scan was built for — cache keys use this
    /// instead of rehashing content.
    generation: u64,
    ordered: Vec<(u32, usize)>,
    tables: Vec<markdown_syntax::TableRegion>,
    mermaid: Vec<(Range<usize>, String)>,
    math: Vec<markdown_syntax::MathRegion>,
    props: Vec<Range<usize>>,
    alert_folds: Vec<(Range<usize>, bool)>,
    /// Whether each line STARTS inside a fenced code block (odd count of ```
    /// fences above it).
    fence_odd: Vec<bool>,
}

/// One frame's shaping, memoized between the measure pass and prepaint —
/// both shape the IDENTICAL inputs, so the measure's result is handed to
/// prepaint instead of shaping the whole document twice per frame. Consumed
/// (taken) by prepaint, so it can never go stale across frames; a key
/// mismatch (e.g. the resolved width differs from the available width)
/// falls back to shaping.
struct ShapeMemo {
    wrap_width: Option<Pixels>,
    caret_row: Option<usize>,
    selection: (usize, usize),
    font_size: Pixels,
    shaped: ShapedDoc,
}

/// Whether a just-typed input completes a word: a single boundary character
/// (space / punctuation / Enter, incl. a list continuation's leading newline)
/// — the moment the host's auto-replace hook (page-title auto-linking) is
/// offered the line. `:` is NOT a boundary, so a `key:: value` property can be
/// typed on a key that happens to name a page — the first `:` must not wrap
/// the key into `[[key]]`.
fn word_boundary_input(new_text: &str) -> bool {
    match new_text.as_bytes() {
        [c] => !c.is_ascii_alphanumeric() && !matches!(c, b'_' | b'-' | b'/' | b'#' | b'[' | b':'),
        [b'\n', ..] => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn caret_off_marker_line_cases() {
        use super::caret_off_marker_line;
        // Table marker at the top: the caret steps to the header line.
        let doc = "<!-- table:grid cols=40,40 -->\n| a | b |\n| --- | --- |\n| 1 | 2 |";
        assert_eq!(caret_off_marker_line(doc, 0), 31);
        // Math align marker: the caret lands after the block.
        let doc = "<!-- math:center -->\n$$\nx^2\n$$\nafter";
        assert_eq!(caret_off_marker_line(doc, 0), 31);
        assert_eq!(&doc[31..], "after");
        // Plain lines pass through untouched.
        assert_eq!(caret_off_marker_line("hello\nworld", 0), 0);
        assert_eq!(caret_off_marker_line("", 0), 0);
    }

    #[test]
    fn strip_block_prefix_cases() {
        use super::strip_block_prefix;
        assert_eq!(strip_block_prefix("# Title"), "Title");
        assert_eq!(strip_block_prefix("### Deep"), "Deep");
        assert_eq!(strip_block_prefix("- [x] done"), "done");
        assert_eq!(strip_block_prefix("- item"), "item");
        assert_eq!(strip_block_prefix("12. nth"), "nth");
        assert_eq!(strip_block_prefix("> quoted"), "quoted");
        assert_eq!(strip_block_prefix(">bare"), "bare");
        assert_eq!(strip_block_prefix("plain"), "plain");
        // Renderer-grammar forms the old hand-rolled version missed.
        assert_eq!(strip_block_prefix("* [ ] star task"), "star task");
        assert_eq!(strip_block_prefix("+ [x] plus task"), "plus task");
        assert_eq!(strip_block_prefix("3) paren"), "paren");
        assert_eq!(strip_block_prefix("#### H4"), "H4");
        // Not block prefixes: mid-word hash runs, `#tag`, a lone dash.
        assert_eq!(strip_block_prefix("#tag"), "#tag");
        assert_eq!(strip_block_prefix("-dash"), "-dash");
    }

    #[test]
    fn find_in_source_cases() {
        use super::find_in_source;
        assert_eq!(find_in_source("aa bb aa", "aa"), vec![0..2, 6..8]);
        // Case-insensitive, unicode-aware.
        assert_eq!(
            find_in_source("Grüße hier", "grüsse"),
            Vec::<std::ops::Range<usize>>::new()
        );
        assert_eq!(find_in_source("Grüße", "grüße"), vec![0..7]);
        assert_eq!(find_in_source("İstanbul", "i̇stanbul"), vec![0..9]);
        assert_eq!(
            find_in_source("abc", ""),
            Vec::<std::ops::Range<usize>>::new()
        );
    }

    use super::{display_col_in, set_image_width, word_boundary_input};

    #[test]
    fn word_boundaries_offer_auto_replace_but_colon_never_does() {
        // Space / punctuation / Enter (incl. a list continuation) complete a word.
        assert!(word_boundary_input(" "));
        assert!(word_boundary_input("."));
        assert!(word_boundary_input("\n"));
        assert!(word_boundary_input("\n- "));
        // Word characters and syntax openers don't.
        assert!(!word_boundary_input("a"));
        assert!(!word_boundary_input("["));
        assert!(!word_boundary_input("#"));
        // `:` must not — typing `key::` on a page-title key would otherwise
        // auto-link the key before the property can form.
        assert!(!word_boundary_input(":"));
    }

    #[test]
    fn display_col_leftmost_for_inline_math_spacer() {
        // An inline `$…$` spacer maps its whole width to the span's start offset (here source 2,
        // repeated across display 2..5). The caret at source 2 must land at the spacer's LEFT
        // edge (display 2), not an arbitrary spot inside it; source 5 (just past the formula)
        // lands at display 5.
        let map = std::rc::Rc::new(vec![0, 1, 2, 2, 2, 5, 6, 7]);
        assert_eq!(display_col_in(Some(&map), 2), 2);
        assert_eq!(display_col_in(Some(&map), 5), 5);
        // A strictly-increasing map (hidden markers) is unaffected.
        let plain = std::rc::Rc::new(vec![0, 1, 2, 3]);
        assert_eq!(display_col_in(Some(&plain), 2), 2);
        assert_eq!(display_col_in(None, 4), 4);
    }

    #[test]
    fn image_width_splice() {
        // No existing attr: append `{width=N}` right after `)`.
        assert_eq!(
            set_image_width("![a](b.png)", 200),
            "![a](b.png){width=200}"
        );
        // Existing `{width=N}` is replaced (not duplicated).
        assert_eq!(
            set_image_width("![a](b.png){width=320}", 200),
            "![a](b.png){width=200}"
        );
        // The `px` unit form is replaced too.
        assert_eq!(
            set_image_width("![a](b.png){width=320px}", 200),
            "![a](b.png){width=200}"
        );
        // List-item image: the leading marker is preserved, attr lands after `)`.
        assert_eq!(
            set_image_width("- ![](x){width=10}", 50),
            "- ![](x){width=50}"
        );
        // Trailing whitespace is preserved (attr lands before it).
        assert_eq!(
            set_image_width("![a](b.png)  ", 80),
            "![a](b.png){width=80}  "
        );
        // Not an image row: returned unchanged.
        assert_eq!(set_image_width("just text", 100), "just text");
    }

    // --- RTL row geometry (#66) ---------------------------------------------

    use super::{checkbox_x, px, row_of_spans, rtl_marker_x, rtl_shift};

    #[test]
    fn a_caret_offset_lands_on_the_row_that_holds_it() {
        // Three rows: "0..5", "5..11", "11..14" — contiguous, reading order.
        let rows = || [(0usize, 5usize), (5, 6), (11, 3)].into_iter();
        assert_eq!(row_of_spans(rows(), 0), (0, 0));
        assert_eq!(row_of_spans(rows(), 4), (0, 4));
        // A boundary belongs to the row that STARTS there, not the one ending.
        assert_eq!(row_of_spans(rows(), 5), (1, 0));
        assert_eq!(row_of_spans(rows(), 12), (2, 1));
        // The caret sits one past the last character after typing at the end
        // of a paragraph: it must stay on the last row, not fall off.
        assert_eq!(row_of_spans(rows(), 14), (2, 3));
        assert_eq!(row_of_spans(rows(), 99), (2, 88));
        // No rows at all (an empty line) is row 0.
        assert_eq!(row_of_spans([].into_iter(), 0), (0, 0));
    }

    #[test]
    fn rtl_shift_mirrors_the_row_inset() {
        // Plain paragraph (no inset): the text's right edge meets the content
        // edge, so the shift is all the slack.
        assert_eq!(rtl_shift(px(500.), px(0.), px(200.)), px(300.));
        // Inset row (a list item / blockquote body): callers add the shift to
        // an origin they already inset, so the doubled inset leaves the SAME
        // gap on the right that an LTR row gets on the left.
        assert_eq!(rtl_shift(px(500.), px(24.), px(200.)), px(252.));
        assert_eq!(px(24.) + rtl_shift(px(500.), px(24.), px(200.)), px(276.));
        // …i.e. text spans 276..476, exactly 24 in from the right edge.
        // Text that fills or overflows the row wraps, and every wrap row starts
        // at the left edge — no shift, never a negative one.
        assert_eq!(rtl_shift(px(500.), px(0.), px(500.)), px(0.));
        assert_eq!(rtl_shift(px(500.), px(0.), px(900.)), px(0.));
        assert_eq!(rtl_shift(px(100.), px(60.), px(50.)), px(0.));
    }

    #[test]
    fn rtl_markers_mirror_to_the_right_edge() {
        // A bullet 8px wide at x=10 lands 10 in from the right edge instead.
        assert_eq!(rtl_marker_x(px(500.), px(10.), px(8.)), px(482.));
        // Nesting is preserved: a deeper level (larger x) indents further FROM
        // THE RIGHT, so the levels keep their order.
        let l1 = rtl_marker_x(px(500.), px(10.), px(8.));
        let l2 = rtl_marker_x(px(500.), px(34.), px(8.));
        assert!(l2 < l1, "level 2 must sit further in from the right");
        assert_eq!(l1 - l2, px(24.), "the indent step survives the mirror");
        // Never off the left edge, however wide the marker.
        assert_eq!(rtl_marker_x(px(20.), px(10.), px(40.)), px(0.));
        // The checkbox shares that math, and an LTR row is untouched.
        assert_eq!(checkbox_x(px(10.), px(12.), px(500.), true), px(478.));
        assert_eq!(checkbox_x(px(10.), px(12.), px(500.), false), px(10.));
    }

    // --- RTL table placement (#66) ------------------------------------------

    use super::tables::{TABLE_GUTTER, table_left_x, table_visible_band};

    /// The note column used throughout: origin 100, width 500 → the LTR band
    /// is 122..600 and the RTL band 100..578, both `TABLE_GUTTER` wide.
    const O: f32 = 100.;
    const W: f32 = 500.;

    #[test]
    fn an_rtl_table_hugs_the_right_edge_with_the_gutter_mirrored() {
        let g = TABLE_GUTTER;
        // A table narrower than the column: LTR starts a gutter in from the
        // left, RTL *ends* a gutter in from the right.
        let ltr = table_left_x(px(O), px(W), px(300.), px(0.), false);
        let rtl = table_left_x(px(O), px(W), px(300.), px(0.), true);
        assert_eq!(ltr, px(O + g));
        assert_eq!(rtl + px(300.), px(O + W - g), "right edge, one gutter in");
        // The two are mirror images about the column's centre.
        assert_eq!(
            f32::from(ltr - px(O)),
            f32::from(px(O + W) - (rtl + px(300.)))
        );
        // Column widths don't move an LTR table but do move an RTL one — it is
        // anchored at its trailing edge.
        assert_eq!(
            table_left_x(px(O), px(W), px(120.), px(0.), false),
            px(O + g)
        );
        assert_eq!(
            table_left_x(px(O), px(W), px(120.), px(0.), true),
            px(O + W - g - 120.)
        );
    }

    #[test]
    fn a_wide_table_scrolls_to_its_own_far_edge_either_way() {
        let g = TABLE_GUTTER;
        let total = px(900.);
        let avail = px(W - g); // what `table_sx` clamps against
        let max = total - avail;
        // Unscrolled, each direction shows its own leading edge at the gutter.
        assert_eq!(table_left_x(px(O), px(W), total, px(0.), false), px(O + g));
        assert_eq!(
            table_left_x(px(O), px(W), total, px(0.), true) + total,
            px(O + W - g)
        );
        // Fully scrolled, each shows its trailing edge at the band's far side:
        // LTR's right edge reaches the column's right, RTL's left edge the left.
        assert_eq!(
            table_left_x(px(O), px(W), total, max, false) + total,
            px(O + W)
        );
        assert_eq!(table_left_x(px(O), px(W), total, max, true), px(O));
        // Scroll moves the content in OPPOSITE directions — why the wheel and
        // the thumb's `factor` invert their sign on an RTL table.
        let step = px(50.);
        assert!(table_left_x(px(O), px(W), total, step, false) < px(O + g));
        assert!(
            table_left_x(px(O), px(W), total, step, true)
                > table_left_x(px(O), px(W), total, px(0.), true)
        );
    }

    #[test]
    fn rtl_mirrors_the_column_order() {
        use super::tables::{cell_span_width, col_offset};
        // Three columns, 10/20/30 wide. Left to right they start at 0/10/30;
        // mirrored, column 0 is the RIGHTMOST, so it starts at 50.
        let w = [px(10.), px(20.), px(30.)];
        assert_eq!(col_offset(&w, 3, 0, false), px(0.));
        assert_eq!(col_offset(&w, 3, 1, false), px(10.));
        assert_eq!(col_offset(&w, 3, 2, false), px(30.));
        assert_eq!(col_offset(&w, 3, 0, true), px(50.));
        assert_eq!(col_offset(&w, 3, 1, true), px(30.));
        assert_eq!(col_offset(&w, 3, 2, true), px(0.));
        // Either way the columns tile the table with no gap or overlap — paint,
        // caret and hit-testing all read this, so a gap is a mis-click.
        for rtl in [false, true] {
            let mut spans: Vec<(f32, f32)> = (0..3)
                .map(|c| {
                    let x = f32::from(col_offset(&w, 3, c, rtl));
                    (x, x + f32::from(cell_span_width(&w, 3, c)))
                })
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            assert_eq!(spans[0].0, 0.0);
            assert_eq!(spans[2].1, 60.0);
            assert!(spans.windows(2).all(|s| s[0].1 == s[1].0), "{spans:?}");
        }
    }

    #[test]
    fn the_visible_band_is_the_column_less_its_gutter() {
        let g = TABLE_GUTTER;
        assert_eq!(
            table_visible_band(px(O), px(W), false),
            (px(O + g), px(O + W))
        );
        assert_eq!(
            table_visible_band(px(O), px(W), true),
            (px(O), px(O + W - g))
        );
        // Both bands are the `avail` width the scroll clamp assumes.
        for rtl in [false, true] {
            let (l, r) = table_visible_band(px(O), px(W), rtl);
            assert_eq!(r - l, px(W - g));
        }
    }
}
