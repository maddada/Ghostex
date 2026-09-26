//! The reader view itself — everything behind the default-on `view` feature:
//! [`MarkdownView`], its styles, and the render tree. `lib.rs` holds the
//! crate docs; `syntax` (always compiled, dependency-free) holds the shared
//! construct recognition.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Corners, ElementId, FontStyle, FontWeight,
    HighlightStyle, Hsla, InteractiveElement, InteractiveText, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, RenderImage, RenderOnce,
    ScrollHandle, SharedString, StatefulInteractiveElement, StrikethroughStyle, Styled, StyledText,
    TextLayout, TextRun, Window, canvas, div, point, prelude::FluentBuilder, px, relative, rgb,
    rgba, size, svg,
};
use markdown::mdast;

use crate::syntax::{
    AlertKind, TableStyle, alert_marker, heading_scale, table_col_widths, table_style_marker,
};

/// Visual configuration for the renderer. The host fills this from its
/// own theme; defaults are a neutral dark palette.
#[derive(Clone)]
pub struct MarkdownStyle {
    /// Resolves a block link's display text: `(page, id)` → the target
    /// block's text, host-supplied. `None` (or a `None` return) keeps the
    /// `Page → id` form.
    #[allow(clippy::type_complexity)]
    pub block_label: Option<Rc<dyn Fn(&str, &str) -> Option<String>>>,
    /// How many pages reference a block id — a non-zero count renders a small
    /// clickable badge in place of the hidden ` ^id` anchor (Logseq-style),
    /// linked through the `refs:^id` target the host resolves. `None` (or 0)
    /// keeps the anchor invisible.
    pub block_ref_count: Option<BlockRefCountFn>,
    pub text_color: Hsla,
    pub text_size: Pixels,
    /// Body line height as a multiple of `text_size`. Hosts with an editor
    /// match it to the editor's ratio so reading and editing line up (Zorite
    /// passes zorite-editor's 1.45); the default follows suit.
    pub line_height: f32,
    pub heading_color: Hsla,
    pub link_color: Hsla,
    pub tag_color: Hsla,
    pub code_color: Hsla,
    pub code_bg: Hsla,
    pub muted_color: Hsla,
    /// Thematic break (`---`) divider color.
    pub rule_color: Hsla,
    /// Nested-list indent guide — a hairline, fainter than `rule_color`.
    pub guide_color: Hsla,
    /// Background for `<mark>…</mark>` highlighted text. Translucent so the body text
    /// stays readable over it in any theme.
    pub mark_bg: Hsla,
    /// Backgrounds for in-page search: every match (`search_bg`) and the current /
    /// active one (`search_current_bg`). Painted only when a query is set via
    /// [`MarkdownView::search`]; the host owns the find UI. Translucent so text
    /// stays readable.
    pub search_bg: Hsla,
    pub search_current_bg: Hsla,
    /// Horizontal indent per nested list level. The host sizes this to match its
    /// editor's literal indent (so reading + editing line up).
    pub list_indent: Pixels,
    /// Monospace font family for code blocks + inline code. The host picks one that
    /// exists on the platform; an unknown family just falls back to the default font.
    pub mono_font: SharedString,
    /// GitHub-style alert (`> [!NOTE]` …) border + title colors.
    pub alerts: AlertColors,
    /// SVG asset paths for the alert title icons, resolved through the host's
    /// `AssetSource`. `None` (the default) renders the title without an icon,
    /// keeping the crate asset-free.
    pub alert_icons: Option<AlertIcons>,
    /// Resolves a property key (`tags`, `status`, …) to an icon shown before it
    /// in the property panel. Host-provided (asset path through the host's
    /// `AssetSource`) so the crate stays asset-agnostic; `None` = no icons.
    pub property_icon: Option<PropertyIconFn>,
}

/// Maps a property key to an icon asset path the host serves, or `None` for no
/// icon. Host-provided so the crate makes no assumption about which assets exist.
pub type PropertyIconFn = Rc<dyn Fn(&str) -> Option<SharedString>>;

/// Maps a block id to how many pages reference it (0 = no badge).
pub type BlockRefCountFn = Rc<dyn Fn(&str) -> usize>;

/// Per-kind SVG asset paths for the alert title icons.
#[derive(Clone)]
pub struct AlertIcons {
    pub note: SharedString,
    pub tip: SharedString,
    pub important: SharedString,
    pub warning: SharedString,
    pub caution: SharedString,
}

impl Default for MarkdownStyle {
    fn default() -> Self {
        Self {
            block_label: None,
            block_ref_count: None,
            text_color: rgb(0xE6E6E6).into(),
            text_size: px(15.0),
            line_height: 1.45,
            heading_color: rgb(0xFFFFFF).into(),
            link_color: rgb(0x4C9EFF).into(),
            tag_color: rgb(0x9D7CD8).into(),
            code_color: rgb(0xD7BA7D).into(),
            code_bg: rgba(0xFFFFFF14).into(),
            muted_color: rgb(0x9AA0A6).into(),
            rule_color: rgba(0xFFFFFF22).into(),
            guide_color: rgba(0xFFFFFF14).into(),
            mark_bg: rgba(0xFFD60066).into(),
            search_bg: rgba(0xFFD60055).into(),
            search_current_bg: rgba(0xFF9500DD).into(),
            list_indent: px(18.0),
            mono_font: "monospace".into(),
            alerts: AlertColors::default(),
            alert_icons: None,
            property_icon: None,
        }
    }
}

/// Border + title colors for the five GitHub-style alerts (`> [!NOTE]` …).
/// Defaults are GitHub's dark palette; the host overlays its theme.
#[derive(Clone, Copy)]
pub struct AlertColors {
    pub note: Hsla,
    pub tip: Hsla,
    pub important: Hsla,
    pub warning: Hsla,
    pub caution: Hsla,
}

impl Default for AlertColors {
    fn default() -> Self {
        Self {
            note: rgb(0x4493F8).into(),
            tip: rgb(0x3FB950).into(),
            important: rgb(0xAB7DF8).into(),
            warning: rgb(0xD29922).into(),
            caution: rgb(0xF85149).into(),
        }
    }
}

/// View-side styling for the shared [`syntax::AlertKind`].
trait AlertKindExt {
    fn color(self, c: &AlertColors) -> Hsla;
    fn icon(self, i: &AlertIcons) -> SharedString;
}

impl AlertKindExt for AlertKind {
    fn color(self, c: &AlertColors) -> Hsla {
        match self {
            Self::Note => c.note,
            Self::Tip => c.tip,
            Self::Important => c.important,
            Self::Warning => c.warning,
            Self::Caution => c.caution,
        }
    }

    fn icon(self, i: &AlertIcons) -> SharedString {
        match self {
            Self::Note => i.note.clone(),
            Self::Tip => i.tip.clone(),
            Self::Important => i.important.clone(),
            Self::Warning => i.warning.clone(),
            Self::Caution => i.caution.clone(),
        }
    }
}

/// If blockquote `b` is a GitHub alert, return its kind and a copy of its
/// children with the marker stripped (the first text's source offset advances
/// by the stripped length, so the rendered→source click map stays aligned).
/// Public so other renderers of the same construct (e.g. a PDF exporter)
/// share the exact recognition.
pub fn alert_children(b: &mdast::Blockquote) -> Option<(AlertKind, Vec<mdast::Node>)> {
    alert_parts(b).map(|(kind, _, _, children)| (kind, children))
}

/// [`alert_children`] plus the callout's fold state (`Some(true)` = folded)
/// and the marker's source byte offset — what a foldable callout's chevron
/// click reports so the host can flip the `-`/`+` in the source.
pub fn alert_parts(
    b: &mdast::Blockquote,
) -> Option<(AlertKind, Option<bool>, usize, Vec<mdast::Node>)> {
    let Some(mdast::Node::Paragraph(p)) = b.children.first() else {
        return None;
    };
    let Some(mdast::Node::Text(t)) = p.children.first() else {
        return None;
    };
    let marker_offset = t.position.as_ref().map_or(0, |p| p.start.offset);
    let (kind, strip, fold) = alert_marker(&t.value)?;
    let mut children = b.children.clone();
    if let Some(mdast::Node::Paragraph(p)) = children.first_mut() {
        if strip >= t.value.len() {
            // The marker was the whole text node: drop it (and a following
            // hard Break, if the marker line ended with one).
            p.children.remove(0);
            if matches!(p.children.first(), Some(mdast::Node::Break(_))) {
                p.children.remove(0);
            }
            if p.children.is_empty() {
                children.remove(0);
            }
        } else if let Some(mdast::Node::Text(t)) = p.children.first_mut() {
            t.value.drain(..strip);
            if let Some(pos) = &mut t.position {
                pos.start.offset += strip;
            }
        }
    }
    Some((kind, fold, marker_offset, children))
}

/// An authoring snippet for a markdown construct: a label, the text to
/// insert, and the caret offset (bytes) within that text. Exposed so a
/// host's command palette can offer markdown commands without re-deriving
/// the syntax. Pure data — no rendering involved.
pub struct Snippet {
    pub label: &'static str,
    pub snippet: &'static str,
    pub caret: usize,
}

/// Built-in markdown authoring snippets (for a `/` command palette).
pub const SNIPPETS: &[Snippet] = &[
    // Blocks
    Snippet {
        label: "Heading 1",
        snippet: "# ",
        caret: 2,
    },
    Snippet {
        label: "Heading 2",
        snippet: "## ",
        caret: 3,
    },
    Snippet {
        label: "Heading 3",
        snippet: "### ",
        caret: 4,
    },
    Snippet {
        label: "Bullet list",
        snippet: "- ",
        caret: 2,
    },
    Snippet {
        label: "Numbered list",
        snippet: "1. ",
        caret: 3,
    },
    Snippet {
        label: "To-do",
        snippet: "- [ ] ",
        caret: 6,
    },
    Snippet {
        label: "Quote",
        snippet: "> ",
        caret: 2,
    },
    Snippet {
        label: "Note alert",
        snippet: "> [!NOTE] ",
        caret: 10,
    },
    Snippet {
        label: "Tip alert",
        snippet: "> [!TIP] ",
        caret: 9,
    },
    Snippet {
        label: "Important alert",
        snippet: "> [!IMPORTANT] ",
        caret: 15,
    },
    Snippet {
        label: "Warning alert",
        snippet: "> [!WARNING] ",
        caret: 13,
    },
    Snippet {
        label: "Caution alert",
        snippet: "> [!CAUTION] ",
        caret: 13,
    },
    Snippet {
        label: "Code block",
        snippet: "```\n\n```",
        caret: 4,
    },
    Snippet {
        label: "Mermaid diagram",
        snippet: "```mermaid\n\n```",
        caret: 11,
    },
    Snippet {
        label: "Math",
        snippet: "$$\n\n$$",
        caret: 3,
    },
    Snippet {
        label: "Table",
        snippet: "|  |  |\n| --- | --- |\n|  |  |\n",
        caret: 2,
    },
    Snippet {
        label: "Divider",
        snippet: "---\n",
        caret: 4,
    },
    // Inline (caret lands between the markers)
    Snippet {
        label: "Bold",
        snippet: "****",
        caret: 2,
    },
    Snippet {
        label: "Italic",
        snippet: "**",
        caret: 1,
    },
    Snippet {
        label: "Strikethrough",
        snippet: "~~~~",
        caret: 2,
    },
    Snippet {
        label: "Inline code",
        snippet: "``",
        caret: 1,
    },
    Snippet {
        label: "Inline math",
        snippet: "$$",
        caret: 1,
    },
    Snippet {
        label: "Highlight",
        snippet: "<mark></mark>",
        caret: 6,
    },
    Snippet {
        label: "Underline",
        snippet: "<u></u>",
        caret: 3,
    },
    Snippet {
        label: "Link",
        snippet: "[]()",
        caret: 1,
    },
    Snippet {
        label: "Wiki link",
        snippet: "[[]]",
        caret: 2,
    },
    Snippet {
        label: "Image",
        snippet: "![]()",
        caret: 4,
    },
];

/// Called when a `[[wiki-link]]` is clicked, with the trimmed title.
pub type WikiLinkHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// Resolves a standalone `![[target]]` embed to `(source label, content)` —
/// the host pre-resolves targets from its database (a page, a `#^id` block, or
/// a `#Heading` section) since render-time providers can't query. `None` (or a
/// missing target) falls back to rendering the line as text.
pub type EmbedProvider = Rc<dyn Fn(&str) -> Option<(SharedString, SharedString)>>;

/// A standalone image (a paragraph that is just `![alt](src)`, optionally
/// followed by a `{width=N}` attribute). Handed to the host's [`ImageRenderer`]
/// so it can render a real, possibly interactive, image element.
pub struct ImageInfo {
    /// The image URL/path as written in the markdown.
    pub src: SharedString,
    /// The alt text (may be empty).
    pub alt: SharedString,
    /// An explicit width in pixels from a `{width=N}` attribute, if present.
    pub width: Option<f32>,
    /// The byte range in the source to replace with `{width=N}` when resizing:
    /// an empty range (just after the image) when there's no attribute yet, or
    /// the existing attribute's span when there is one.
    pub attr_target: Range<usize>,
}

/// Renders a standalone image. The element's event handlers run later (with
/// their own context), so building it needs no window/app — letting the host
/// supply a stateful, draggable image while this crate stays host-agnostic.
pub type ImageRenderer = Rc<dyn Fn(ImageInfo) -> AnyElement>;

/// Renders a ` ```mermaid ` code block as a diagram, given the block's source. The
/// host owns the (expensive, async) render — this crate just detects the fence and
/// hands the source over, staying renderer-agnostic. Set via
/// [`MarkdownView::on_mermaid`].
pub type MermaidRenderer = Rc<dyn Fn(SharedString) -> AnyElement>;

/// Colors a fenced code block's tokens: `(language tag, code) → sorted,
/// non-overlapping styled ranges` (byte offsets into the code). Supplied by
/// the host (e.g. a tree-sitter highlighter) so the crate stays engine-free;
/// absent it, code renders in the single `code_color`.
pub type CodeHighlighter = Rc<dyn Fn(&str, &str) -> Vec<(Range<usize>, HighlightStyle)>>;

/// Renders a `$$…$$` math block as a typeset image, given the block's LaTeX. Like
/// [`MermaidRenderer`], the host owns the (cached, off-thread) render — this crate just
/// detects the block and hands over the source. Set via [`MarkdownView::on_math`].
pub type MathRenderer = Rc<dyn Fn(SharedString) -> AnyElement>;

/// Resolves an inline `$…$` formula's LaTeX to its typeset raster + logical (display) px size
/// at text size, so the renderer can reserve a gap in the line and paint the image over it. The
/// host owns the (cached, off-thread) render; `None` while it's still rasterizing (the raw
/// `$…$` shows until then). Set via [`MarkdownView::on_inline_math`].
pub type InlineMathRenderer = Rc<dyn Fn(SharedString) -> Option<(Arc<RenderImage>, f32, f32)>>;

/// A renderer for an INLINE image (`![](src)` amid text): its src → the raster
/// plus logical `(w, h)` sized to flow at the text's line height. Like inline
/// math, the caller reserves a spacer in the line and paints the image over
/// it; `None` (still decoding / no renderer) falls back to a clickable label.
pub type InlineImageRenderer = Rc<dyn Fn(SharedString) -> Option<(Arc<RenderImage>, f32, f32)>>;

/// Called when the rendered text is clicked (outside a link), with the **source**
/// byte offset nearest the click and the click's window **y** — so the host can
/// place its editor caret there and keep it under the cursor when switching into
/// edit mode. Set via [`MarkdownView::on_click_source`].
pub type ClickSourceHandler = Rc<dyn Fn(usize, Pixels, &mut Window, &mut App)>;

/// A click on an inline image reports its `src` so the host can open a
/// full-size preview. Set via [`MarkdownView::on_image_preview`].
pub type ImagePreviewHandler = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;

/// Host callback as the pointer moves onto a link (`Some` — its target and its
/// window-space box) or off every link (`None`), so the host can show a
/// preview card anchored to it. Set via [`MarkdownView::on_link_hover`].
pub type LinkHoverHandler =
    Rc<dyn Fn(Option<(crate::syntax::LinkHit, Bounds<Pixels>)>, &mut Window, &mut App)>;

/// Toggle the task checkbox of a clicked list item — the argument is the source
/// byte offset of that task item (feed it to [`toggle_task_at`]). Set via
/// [`MarkdownView::on_task_toggle`].
pub type TaskToggleHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// Called when a heading's fold chevron is clicked, with the heading's fold
/// key — its trimmed source line (`## Goals`). The host owns the fold set
/// (this view is rebuilt every frame) and passes it back via
/// [`MarkdownView::folded_headings`].
pub type HeadingToggleHandler = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// A rendered markdown document element — the reader view of a note.
///
/// Host-injectable UI labels the reader renders (the code-card `Copy`
/// button). The crate stays host-agnostic so it never calls `t!()`; the app
/// passes localized strings via [`MarkdownView::set_labels`]. The default is
/// English, keeping the crate usable standalone.
#[derive(Clone)]
pub struct Labels {
    /// The code-card `Copy` button.
    pub code_copy: SharedString,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            code_copy: "Copy".into(),
        }
    }
}

/// A rendered markdown document element — the reader view of a note.
#[derive(IntoElement)]
pub struct MarkdownView {
    id_base: SharedString,
    source: SharedString,
    style: MarkdownStyle,
    /// Host-injectable UI labels (the code-card `Copy` button); English by
    /// default, localized through [`Self::set_labels`] — the crate stays
    /// host-agnostic so it never calls `t!()`.
    labels: Labels,
    on_wiki_link: Option<WikiLinkHandler>,
    on_image: Option<ImageRenderer>,
    on_mermaid: Option<MermaidRenderer>,
    on_highlight: Option<CodeHighlighter>,
    on_math: Option<MathRenderer>,
    on_inline_math: Option<InlineMathRenderer>,
    on_inline_image: Option<InlineImageRenderer>,
    /// In-page search query (non-empty when `Some`) + the active match index.
    query: Option<SharedString>,
    current_match: usize,
    /// When set, the block column is `track_scroll`ed with this handle, so the host
    /// can read each block's bounds (`bounds_for_item`) to scroll a match into view.
    block_scroll: Option<ScrollHandle>,
    /// Click-to-caret: maps a click on the rendered text to its source offset.
    on_click_source: Option<ClickSourceHandler>,
    on_image_preview: Option<ImagePreviewHandler>,
    on_link_hover: Option<LinkHoverHandler>,
    /// Click a task checkbox to toggle it (the host applies + persists).
    on_task_toggle: Option<TaskToggleHandler>,
    on_alert_toggle: Option<TaskToggleHandler>,
    on_embed: Option<EmbedProvider>,
    on_embed_image: Option<ImageRenderer>,
    /// Collapsed headings (trimmed source lines, e.g. `## Goals`) — their
    /// sections don't render. Host-owned state; see [`HeadingToggleHandler`].
    folded_headings: HashSet<String>,
    on_heading_toggle: Option<HeadingToggleHandler>,
}

impl MarkdownView {
    /// `id_base` must be unique per rendered document (used to derive
    /// element ids for clickable paragraphs).
    pub fn new(id_base: impl Into<SharedString>, source: impl Into<SharedString>) -> Self {
        Self {
            id_base: id_base.into(),
            source: source.into(),
            style: MarkdownStyle::default(),
            on_wiki_link: None,
            on_image: None,
            on_mermaid: None,
            on_highlight: None,
            on_math: None,
            on_inline_math: None,
            on_inline_image: None,
            query: None,
            current_match: 0,
            block_scroll: None,
            on_click_source: None,
            on_link_hover: None,
            on_image_preview: None,
            on_task_toggle: None,
            on_alert_toggle: None,
            on_embed: None,
            on_embed_image: None,
            folded_headings: HashSet::new(),
            on_heading_toggle: None,
            labels: Labels::default(),
        }
    }

    /// Set the host-localized labels (code-card `Copy` etc.).
    pub fn set_labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }

    pub fn style(mut self, style: MarkdownStyle) -> Self {
        self.style = style;
        self
    }

    pub fn on_wiki_link(mut self, handler: WikiLinkHandler) -> Self {
        self.on_wiki_link = Some(handler);
        self
    }

    /// Supply a renderer for standalone images. Without one, images fall back
    /// to a clickable "🖼 alt" text label.
    pub fn on_image(mut self, handler: ImageRenderer) -> Self {
        self.on_image = Some(handler);
        self
    }

    /// Supply a renderer for ` ```mermaid ` code blocks. Without one, a mermaid
    /// block renders as a plain code block.
    pub fn on_mermaid(mut self, handler: MermaidRenderer) -> Self {
        self.on_mermaid = Some(handler);
        self
    }

    /// Set the fenced-code syntax highlighter (see [`CodeHighlighter`]).
    pub fn on_highlight(mut self, handler: CodeHighlighter) -> Self {
        self.on_highlight = Some(handler);
        self
    }

    /// Supply a renderer for inline `$…$` formulas (raster + size). Without one, inline math
    /// stays literal `$…$` text.
    pub fn on_inline_math(mut self, handler: InlineMathRenderer) -> Self {
        self.on_inline_math = Some(handler);
        self
    }

    /// Supply a renderer for inline `![](src)` images (raster + size). Without
    /// one, an inline image stays a clickable label.
    pub fn on_inline_image(mut self, handler: InlineImageRenderer) -> Self {
        self.on_inline_image = Some(handler);
        self
    }

    /// Supply a renderer for `$$…$$` math blocks. Without one, a math block renders as
    /// its raw LaTeX in a code block.
    pub fn on_math(mut self, handler: MathRenderer) -> Self {
        self.on_math = Some(handler);
        self
    }

    /// Highlight case-insensitive occurrences of `query` in the rendered (visible)
    /// text, emphasizing the `current`-th match (0-based, document order). An empty
    /// query highlights nothing. The host owns the find bar and the match index +
    /// total — pair this with [`match_count`] to size "n of m" and bound `current`.
    /// zorite-markdown only paints: no I/O, no storage, just the source string.
    pub fn search(mut self, query: impl Into<SharedString>, current: usize) -> Self {
        let q = query.into();
        self.query = (!q.is_empty()).then_some(q);
        self.current_match = current;
        self
    }

    /// Track-scroll the block column with `handle` so the host can read each block's
    /// laid-out bounds via [`ScrollHandle::bounds_for_item`] — indexed exactly as
    /// [`find_matches`] reports — and scroll a match into view. Pair with [`search`].
    ///
    /// [`search`]: MarkdownView::search
    pub fn track_blocks(mut self, handle: ScrollHandle) -> Self {
        self.block_scroll = Some(handle);
        self
    }

    /// Report the **source** byte offset nearest a click on the rendered text
    /// (outside a link), so the host can place its editor's caret there. Maps the
    /// click through gpui's text layout + a source-offset map built while rendering.
    pub fn on_click_source(mut self, handler: ClickSourceHandler) -> Self {
        self.on_click_source = Some(handler);
        self
    }

    /// Supply a handler for a click on an inline image (opens a preview).
    /// Report the link under the pointer (and its box) as the mouse moves, for
    /// a host-drawn preview card. Both text directions; property pills and
    /// table-cell links don't report yet.
    pub fn on_link_hover(mut self, handler: LinkHoverHandler) -> Self {
        self.on_link_hover = Some(handler);
        self
    }

    pub fn on_image_preview(mut self, handler: ImagePreviewHandler) -> Self {
        self.on_image_preview = Some(handler);
        self
    }

    /// Make task checkboxes clickable: clicking a `☐`/`☑` calls `handler` with the
    /// task item's source byte offset, so the host can flip it (see [`toggle_task_at`])
    /// and persist. Without this, checkboxes render but aren't interactive.
    pub fn on_task_toggle(mut self, handler: TaskToggleHandler) -> Self {
        self.on_task_toggle = Some(handler);
        self
    }

    /// Called when a foldable callout's title is clicked, with the marker's
    /// source byte offset — the host flips the `-`/`+` fold char (see
    /// [`crate::syntax::toggle_alert_fold_at`]) and persists, like a task
    /// checkbox toggle.
    pub fn on_alert_toggle(mut self, handler: TaskToggleHandler) -> Self {
        self.on_alert_toggle = Some(handler);
        self
    }

    /// Install the embed resolver: a standalone `![[target]]` line renders the
    /// target's content in a bordered box (see [`EmbedProvider`]).
    pub fn on_embed(mut self, provider: EmbedProvider) -> Self {
        self.on_embed = Some(provider);
        self
    }

    /// The image renderer used INSIDE embeds, replacing [`Self::on_image`]
    /// there — hosts supply a read-only variant, since an embedded image's
    /// source range belongs to the embedded page and a resize written through
    /// the embedding page's handler would corrupt it. Unset = no images in
    /// embeds.
    pub fn on_embed_image(mut self, renderer: ImageRenderer) -> Self {
        self.on_embed_image = Some(renderer);
        self
    }

    /// The collapsed headings (trimmed source lines) — their sections are
    /// skipped and their chevrons render pointing right.
    pub fn folded_headings(mut self, folded: HashSet<String>) -> Self {
        self.folded_headings = folded;
        self
    }

    /// Called when a heading's fold chevron is clicked; the host toggles the
    /// key in its fold set and re-renders. Without a handler headings show no
    /// chevron (embeds and other read-only surfaces).
    pub fn on_heading_toggle(mut self, handler: HeadingToggleHandler) -> Self {
        self.on_heading_toggle = Some(handler);
        self
    }
}

/// Content-keyed parse cache. The host's journal feed re-renders every
/// visible day on any interaction, and re-running `to_mdast` for every
/// non-editing day was the dominant per-frame cost (O(days × content)).
/// Keyed by the exact source string (no hash collisions to reason about;
/// the cap bounds memory to ~a few dozen notes of text) and LRU-evicted.
/// Process-global, not thread-local: a background [`warm_parse`] has to be
/// able to fill what the render thread later reads (issue #60).
const PARSE_CACHE_CAP: usize = 64;

/// The cache entries plus the LRU clock, bumped on every lookup and insert.
type ParseCache = (HashMap<String, (Arc<mdast::Node>, u64)>, u64);

static PARSE_CACHE: LazyLock<Mutex<ParseCache>> = LazyLock::new(|| Mutex::new((HashMap::new(), 0)));

/// The cache guard. Nothing but map lookups runs under this lock — never a
/// parse — so a poisoned lock means an unrelated panic and the entries are
/// still sound to reuse.
fn parse_cache() -> MutexGuard<'static, ParseCache> {
    PARSE_CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Alignment named by a `<!-- math:ALIGN -->` marker line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MathMarkerAlign {
    Left,
    Right,
}

/// The `<!-- math:ALIGN -->` marker on the line **directly above** the block
/// starting at byte `start`, if any — the same adjacency rule the editor's
/// `math_regions` uses, so a marker separated by a blank line steers neither.
/// The single `$$…$$` math node among a paragraph's children, if there is
/// EXACTLY one: its index, node, and source offset. Two-plus display pairs
/// (or none) return `None` and render inline as before.
fn only_display_math<'a>(
    children: &'a [mdast::Node],
    source: &str,
) -> Option<(usize, &'a mdast::InlineMath, usize)> {
    let mut found = None;
    for (ix, node) in children.iter().enumerate() {
        if let mdast::Node::InlineMath(m) = node
            && let Some(start) = m.position.as_ref().map(|pos| pos.start.offset)
            && source.get(start..).is_some_and(|s| s.starts_with("$$"))
        {
            if found.is_some() {
                return None;
            }
            found = Some((ix, m, start));
        }
    }
    found
}

fn preceding_math_align(source: &str, start: Option<usize>) -> Option<MathMarkerAlign> {
    let start = start?.min(source.len());
    let before = source[..start].strip_suffix('\n')?;
    let line = before.rsplit('\n').next().unwrap_or(before).trim();
    let inner = line.strip_prefix("<!--")?.strip_suffix("-->")?.trim();
    match inner.strip_prefix("math:")?.trim() {
        "left" => Some(MathMarkerAlign::Left),
        "right" => Some(MathMarkerAlign::Right),
        _ => None,
    }
}

/// A cache hit for `source`, refreshing its LRU stamp. Never parses.
fn cache_get(source: &str) -> Option<Arc<mdast::Node>> {
    let mut cache = parse_cache();
    cache.1 += 1;
    let tick = cache.1;
    let (node, last_used) = cache.0.get_mut(source)?;
    *last_used = tick;
    Some(node.clone())
}

/// Parse `source` with the view's options (GFM + `$…$`/`$$…$$` math),
/// memoized. `None` when the parser errors (not cached — the caller falls
/// back to plain text). Callers on a render thread go through
/// [`parse_for_render`] instead, which won't parse an expensive shape inline.
fn parse_cached(source: &str) -> Option<Arc<mdast::Node>> {
    if let Some(node) = cache_get(source) {
        return Some(node);
    }
    // Enable block math (`$$…$$` -> a Math node) and inline `$…$`
    // (`math_text` -> an InlineMath node). markdown's `math_text` already
    // follows the sensible rules (a `$` followed/preceded by a non-space,
    // etc.), so prose like "it cost $5" stays literal.
    let mut opts = markdown::ParseOptions::gfm();
    opts.constructs.math_flow = true;
    opts.constructs.math_text = true;
    // Parsed with the lock RELEASED: this is the expensive step (seconds on
    // the shapes `is_cheap_to_parse` screens for), and a background warm must
    // never stall a render thread that only wants a lookup.
    let node = Arc::new(markdown::to_mdast(source, &opts).ok()?);
    let mut cache = parse_cache();
    cache.1 += 1;
    let tick = cache.1;
    if cache.0.len() >= PARSE_CACHE_CAP {
        // Evict the least-recently-used entry (the cap is small; a linear
        // scan is cheaper than an ordered structure).
        if let Some(oldest) = cache
            .0
            .iter()
            .min_by_key(|(_, (_, last))| *last)
            .map(|(k, _)| k.clone())
        {
            cache.0.remove(&oldest);
        }
    }
    cache.0.insert(source.to_string(), (node.clone(), tick));
    Some(node)
}

// The four thresholds below are calibrated against timings of the pinned
// `markdown` crate, release build, on the shapes from issue #60. Crossing one
// costs a single placeholder frame, never a refusal — so each sits far above
// anything a written note reaches and far below where the parser falls over.

/// Source bigger than this never parses on the render thread. Linear shapes
/// stay fast well past it (measured: 129 KB of link-and-emphasis prose parses
/// in 36 ms — a dropped frame, not a freeze), so this is only a backstop for
/// pathological shapes the scan below doesn't recognize.
const INLINE_PARSE_MAX: usize = 128 * 1024;

/// Blockquote nesting deeper than this defers. Depth is the trigger for the
/// worst shape in the issue: one line 25 000 `>` deep is 48 KB and 6.0 s,
/// 8 000 deep is 475 ms, 2 000 deep is 35 ms. Written notes nest 1–3 (a GFM
/// alert `> [!NOTE]` is 1, a quoted reply chain rarely 3), so 8 is orders of
/// magnitude clear of both ends.
const MAX_QUOTE_DEPTH: usize = 8;

/// More `[` in the document than this defers. `[[[[[…` is the O(n²) bracket
/// shape (378 ms at 50 KB); the cost tracks the TOTAL count, not the longest
/// run — 8 brackets on each of 8 000 lines is 662 ms while a run of 8 on 500
/// lines is 2.9 ms — so counting is the honest proxy. Measured: 4 000
/// brackets is 2–6 ms, 16 000 is 37–88 ms. A note with 4 000 links doesn't
/// happen; a note that dumps unmatched brackets does.
const MAX_BRACKETS: usize = 4_000;

/// More rows in a single table than this defers. GFM table parsing is
/// superlinear in rows
/// (the realistic accident, a pasted CSV: 1 000 five-column rows is 53 ms,
/// 2 000 narrow rows 40 ms, and the issue measured 9.2 s at 200 KB), while
/// 200 rows is 1–3 ms. Past that it's a pasted dataset, not a written table.
const MAX_TABLE_ROWS: usize = 200;

/// More `*`/`_` in the document than this defers. CommonMark's emphasis
/// matcher is quadratic in unresolved delimiters, and this is the one
/// pathological shape that carries no other tell — `*_*_*_…` is 58 KB of
/// plain-looking text with no brackets, quotes or tables (2.6 s). Measured on
/// that shape: 8 000 markers is 53 ms, 16 000 is 186 ms, 32 000 is 746 ms.
/// Emphasis that actually pairs stays linear (8 000 markers of ordinary
/// `*bold*` prose is 8.5 ms), so this only bites text that never resolves.
const MAX_EMPHASIS: usize = 8_000;

/// Linear pre-scan: can `source` be parsed on the render thread without a
/// user-visible stall? Size alone does not predict cost — 200 KB of prose is
/// fine while 48 KB of nested blockquotes is 6 s — so this also looks for the
/// shapes that make `to_mdast` superlinear. It runs on every render of an
/// uncached document, so it stays one cheap pass over the lines and bails at
/// the first threshold crossed.
fn is_cheap_to_parse(source: &str) -> bool {
    if source.len() > INLINE_PARSE_MAX {
        return false;
    }
    let mut rows = 0usize;
    let mut brackets = 0usize;
    let mut emphasis = 0usize;
    for line in source.lines() {
        let line = line.trim_start();
        if line.starts_with('|') {
            // Per-table, not per-document: the superlinear cost is inside one
            // table's parse, so a note with several small tables is cheap even
            // though their rows add up. A blank line (or any non-row) ends it.
            rows += 1;
            if rows > MAX_TABLE_ROWS {
                return false;
            }
        } else {
            rows = 0;
        }
        // Blockquote depth: the leading run of `>` markers (spaces between
        // them are the usual `> > >` spelling). Stops at the first content
        // byte, so it costs nothing on an ordinary line.
        let mut depth = 0usize;
        for b in line.bytes() {
            match b {
                b'>' => depth += 1,
                b' ' | b'\t' => {}
                _ => break,
            }
        }
        if depth > MAX_QUOTE_DEPTH {
            return false;
        }
        for b in line.bytes() {
            match b {
                b'[' => brackets += 1,
                b'*' | b'_' => emphasis += 1,
                _ => {}
            }
        }
        if brackets > MAX_BRACKETS || emphasis > MAX_EMPHASIS {
            return false;
        }
    }
    true
}

/// The parse a render pass is allowed to do. A cheap-shaped source parses
/// inline as it always has; an expensive-shaped one renders only if something
/// warmed the cache first — otherwise the caller falls back to plain text for
/// this frame instead of freezing the window for seconds (issue #60), and the
/// host's background [`warm_parse`] makes the next frame the real thing.
fn parse_for_render(source: &str) -> Option<Arc<mdast::Node>> {
    // Cache first. `is_cheap_to_parse` is a linear pass, and while it is cheap
    // (byte scanning, early-bail), it would otherwise run on EVERY frame of
    // every visible document — the journal feed renders several at once. Once
    // a tree is cached the classification cannot change the answer, so skip it.
    if let Some(node) = cache_get(source) {
        return Some(node);
    }
    if is_cheap_to_parse(source) {
        parse_cached(source)
    } else {
        // Expensive and not cached: render plain text for this frame and let
        // the host's background warm fill the cache.
        None
    }
}

/// Parse `source` into the shared cache unless it's already there. Pure and
/// gpui-free by design — call it from a background thread (the host uses
/// `cx.background_executor()`), because that is the only place a superlinear
/// parse can run without wedging the UI.
pub fn warm_parse(source: &str) {
    // Warms the key `MarkdownView::render` will look up, normalization and all.
    let _ = parse_cached(&crate::syntax::normalize_math_fences(source));
}

/// Would rendering `source` block on a parse — i.e. is it expensive-shaped AND
/// not cached yet? Hosts check this before spawning a warm task, so ordinary
/// notes (which parse inline in microseconds) spawn nothing at all.
pub fn needs_warm(source: &str) -> bool {
    let source = crate::syntax::normalize_math_fences(source);
    !is_cheap_to_parse(&source) && cache_get(&source).is_none()
}

impl RenderOnce for MarkdownView {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // Words-attached `$$` fences (`wer $$` … `$$ wer`) normalize onto
        // their own lines before parsing, so they render as display math
        // instead of degenerate paragraphs — matching WYSIWYG's forgiving
        // scanner (issue #54 follow-up).
        let source = self.source;
        let source: SharedString = match crate::syntax::normalize_math_fences(&source) {
            std::borrow::Cow::Borrowed(_) => source,
            std::borrow::Cow::Owned(s) => s.into(),
        };
        let block_scroll = self.block_scroll;
        let root_id: SharedString = format!("{}-md-root", self.id_base).into();
        let mut ctx = Ctx {
            source: source.clone(),
            style: self.style,
            on_wiki_link: self.on_wiki_link,
            on_image: self.on_image,
            on_mermaid: self.on_mermaid,
            on_highlight: self.on_highlight,
            on_math: self.on_math,
            on_inline_math: self.on_inline_math,
            on_inline_image: self.on_inline_image,
            id_base: self.id_base,
            counter: 0,
            definitions: HashMap::new(),
            query: self.query,
            current_match: self.current_match,
            match_ix: 0,
            on_click_source: self.on_click_source,
            on_image_preview: self.on_image_preview,
            on_link_hover: self.on_link_hover,
            on_task_toggle: self.on_task_toggle,
            on_alert_toggle: self.on_alert_toggle,
            on_embed: self.on_embed,
            on_embed_image: self.on_embed_image,
            folded_headings: self.folded_headings,
            on_heading_toggle: self.on_heading_toggle,
            code_copy: self.labels.code_copy,
            suppress_heading_top: false,
            strike_done: false,
            embed_depth: 0,
        };

        let mut col = div()
            .id(root_id)
            .flex()
            .flex_col()
            .gap(px(10.0))
            .text_color(ctx.style.text_color)
            .text_size(ctx.style.text_size)
            // Body line height matches the host's editor (gpui's default is
            // the taller phi), so reading and editing space text identically.
            .line_height(relative(ctx.style.line_height));

        let parsed = parse_for_render(&source);
        match parsed.as_deref() {
            Some(mdast::Node::Root(root)) => {
                for node in &root.children {
                    collect_definitions(node, &mut ctx.definitions);
                }
                // A `<!-- table:STYLE -->` comment styles the next table and is
                // itself hidden; everything else renders normally.
                let mut pending_style = None;
                // A folded heading's section is skipped: everything after it
                // until the next heading at its level or higher.
                let mut fold_below: Option<u8> = None;
                for node in &root.children {
                    if let mdast::Node::Heading(h) = node
                        && fold_below.is_some_and(|l| h.depth <= l)
                    {
                        fold_below = None;
                    }
                    if fold_below.is_some() {
                        continue;
                    }
                    if let mdast::Node::Html(h) = node
                        && let Some(style) = table_style_marker(&h.value)
                    {
                        pending_style = Some((style, table_col_widths(&h.value)));
                        continue;
                    }
                    if let mdast::Node::Table(t) = node {
                        let (style, widths) = pending_style.take().unwrap_or_default();
                        col = col.child(render_table(t, &mut ctx, style, widths, window));
                        continue;
                    }
                    pending_style = None;
                    if let Some(el) = render_block(node, &mut ctx, window) {
                        col = col.child(el);
                    }
                    if let mdast::Node::Heading(h) = node
                        && heading_fold_key(h, &source)
                            .is_some_and(|k| ctx.folded_headings.contains(&k))
                    {
                        fold_below = Some(h.depth);
                    }
                }
            }
            _ => col = col.child(StyledText::new(source)),
        }
        // Track each block's bounds so the host can scroll a search match into view.
        if let Some(handle) = &block_scroll {
            col = col.track_scroll(handle);
        }
        col
    }
}

struct Ctx {
    /// The full markdown source — property blocks slice it by node position to
    /// recover each `key:: value` line and its precise click-to-source offset.
    source: SharedString,
    style: MarkdownStyle,
    on_wiki_link: Option<WikiLinkHandler>,
    on_image: Option<ImageRenderer>,
    on_mermaid: Option<MermaidRenderer>,
    on_highlight: Option<CodeHighlighter>,
    on_math: Option<MathRenderer>,
    on_inline_math: Option<InlineMathRenderer>,
    on_inline_image: Option<InlineImageRenderer>,
    id_base: SharedString,
    counter: usize,
    /// `[id] -> url` from reference definitions (`[id]: url`), collected up
    /// front so `[text][id]` references resolve regardless of definition order.
    definitions: HashMap<String, String>,
    /// In-page search: the active query (non-empty when `Some`), the current/active
    /// match index, and a running counter that assigns each match its document-order
    /// index as blocks render — so it stays in step with [`match_count`].
    query: Option<SharedString>,
    current_match: usize,
    match_ix: usize,
    on_click_source: Option<ClickSourceHandler>,
    on_image_preview: Option<ImagePreviewHandler>,
    on_link_hover: Option<LinkHoverHandler>,
    on_task_toggle: Option<TaskToggleHandler>,
    on_alert_toggle: Option<TaskToggleHandler>,
    on_embed: Option<EmbedProvider>,
    on_embed_image: Option<ImageRenderer>,
    /// Collapsed headings (trimmed source lines) — the root walk skips their
    /// sections; the heading arm renders their chevron pointing right.
    folded_headings: HashSet<String>,
    /// Fold chevron click → the host toggles the key. `None` = no chevrons
    /// (embeds, read-only surfaces).
    on_heading_toggle: Option<HeadingToggleHandler>,
    /// The code-card `Copy` button label, injected by the host.
    code_copy: SharedString,
    /// Set while rendering a list item's first block: drops a leading heading's
    /// top margin so the bullet marker lines up with the heading text instead of
    /// floating above it.
    suppress_heading_top: bool,
    /// Set while rendering a checked task item's content: inline text renders
    /// struck through + muted (Notion-style done state).
    strike_done: bool,
    /// Transclusion nesting level — embeds inside embeds stop at a small cap,
    /// which also breaks A-embeds-B-embeds-A cycles.
    embed_depth: usize,
}

// --- Block rendering ---

fn render_block(node: &mdast::Node, ctx: &mut Ctx, window: &mut Window) -> Option<AnyElement> {
    match node {
        mdast::Node::Paragraph(p) => {
            // A standalone `![[target]]` line transcludes the target: the host
            // pre-resolves it to content and a box renders it in place. An
            // unresolved target (or no provider) falls through to plain text,
            // and a depth cap keeps embed cycles finite.
            if let Some(target) = embed_target(p)
                && ctx.embed_depth < 3
                && let Some((label, content)) = ctx.on_embed.as_ref().and_then(|f| f(&target))
            {
                return Some(render_embed(&target, label, content, ctx, window));
            }
            // A paragraph containing exactly one `$$…$$` math node is display
            // math (issue #54): the formula renders on its own centered row —
            // any text around it gets rows of its own — steered by an
            // adjacent `<!-- math:… -->` marker, matching WYSIWYG. Lone
            // single-`$` spans stay inline-sized; the promotion requires the
            // `$$` delimiters in the source.
            if let Some(renderer) = ctx.on_math.clone()
                && let Some((ix, m, start)) = only_display_math(&p.children, &ctx.source)
            {
                let row = div().w_full().flex();
                let row = match preceding_math_align(&ctx.source, Some(start)) {
                    Some(MathMarkerAlign::Left) => row.justify_start(),
                    Some(MathMarkerAlign::Right) => row.justify_end(),
                    _ => row.justify_center(),
                };
                let formula = row.child(renderer(m.value.clone().into()));
                let (before, after) = (&p.children[..ix], &p.children[ix + 1..]);
                if before.is_empty() && after.is_empty() {
                    return Some(formula.into_any_element());
                }
                let mut column = div().flex().flex_col().gap(px(6.0));
                if !before.is_empty() {
                    column = column.child(inline_element(before, ctx));
                }
                column = column.child(formula);
                if !after.is_empty() {
                    column = column.child(inline_element(after, ctx));
                }
                return Some(column.into_any_element());
            }
            // A block of `key:: value` lines renders as a property panel.
            if let Some(rows) = property_rows(p, &ctx.source.clone()) {
                ctx.counter += 1;
                return Some(render_property_table(rows, ctx, ctx.counter, window));
            }
            // A MIXED paragraph — prose lines with `key:: value` lines among
            // them (the Logseq shape: props right under a block's first
            // line) — renders each property run as a panel between the prose
            // rows, matching WYSIWYG's line-based property regions.
            if let Some(el) = render_mixed_properties(p, ctx, window) {
                return Some(el);
            }
            // A paragraph that *starts* with `![alt](src)` (optionally
            // `{width=N}`) renders as a real image via the host. Any text that
            // follows on the same line (a caption typed right under it) renders
            // below the image rather than reverting the whole thing to text.
            if let Some(renderer) = ctx.on_image.clone()
                && let Some((info, rest)) = leading_image(&p.children)
            {
                let image = renderer(info);
                if rest.is_empty() {
                    return Some(image);
                }
                return Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(image)
                        .child(inline_element(&rest, ctx))
                        .into_any_element(),
                );
            }
            Some(inline_element(&p.children, ctx))
        }
        mdast::Node::Heading(h) => {
            let scale = heading_scale(h.depth);
            let size = px(f32::from(ctx.style.text_size) * scale);
            let color = ctx.style.heading_color;
            // Extra room above a heading so a new section separates from the text
            // before it (on top of the inter-block gap); bigger headings get more.
            let top = if ctx.suppress_heading_top {
                px(0.0)
            } else {
                px(match h.depth {
                    1 => 16.0,
                    2 => 12.0,
                    3 => 8.0,
                    _ => 6.0,
                })
            };
            // An RTL heading right-aligns like any other block (#66), and its
            // fold chevron moves to the left so it stays on the text's
            // trailing edge.
            let h_rtl = h
                .position
                .as_ref()
                .and_then(|p| ctx.source.get(p.start.offset..p.end.offset))
                .is_some_and(|s| crate::syntax::content_direction(s).is_rtl());
            let text = div()
                // Full width, else `inline_element`'s right-alignment has
                // nothing to align WITHIN: a content-sized div sits left
                // whatever its text alignment says.
                .w_full()
                .flex_1()
                .min_w_0()
                .text_size(size)
                .text_color(color)
                .font_weight(FontWeight::BOLD)
                .child(inline_element(&h.children, ctx));
            // Fold chevron: hover-revealed (always shown when folded — the
            // hidden section needs a visible sign), clicking toggles via the
            // host's handler. The section skip itself lives in the root walk.
            if let Some(toggle) = ctx.on_heading_toggle.clone()
                && let Some(key) = heading_fold_key(h, &ctx.source)
            {
                let folded = ctx.folded_headings.contains(&key);
                ctx.counter += 1;
                let group: SharedString = format!("{}-hfold-{}", ctx.id_base, ctx.counter).into();
                let mut muted = ctx.style.muted_color;
                muted.a *= 0.8;
                let chevron = div()
                    .id(ElementId::Name(format!("{group}-btn").into()))
                    .cursor_pointer()
                    .text_size(ctx.style.text_size)
                    .text_color(muted)
                    .when(!folded, |d| {
                        d.invisible().group_hover(group.clone(), |s| s.visible())
                    })
                    // Mouse-down + stop_propagation (the callout chevron's
                    // pattern) so the click can't fall through to the
                    // surrounding click-to-edit surface.
                    .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, app| {
                        app.stop_propagation();
                        toggle(&key, window, app);
                    })
                    .child(if folded { "▸" } else { "▾" });
                return Some(
                    div()
                        .group(group)
                        .mt(top)
                        .flex()
                        .when(h_rtl, |d| d.flex_row_reverse())
                        .when(!h_rtl, |d| d.flex_row())
                        .items_center()
                        .gap(px(8.0))
                        .child(text)
                        .child(chevron)
                        .into_any_element(),
                );
            }
            Some(div().mt(top).child(text).into_any_element())
        }
        mdast::Node::List(list) => Some(render_list(list, ctx, 0, window)),
        mdast::Node::Code(c) => {
            // A ```mermaid fence renders as a diagram when the host supplies a
            // renderer; otherwise it falls through to a normal code block.
            if c.lang.as_deref() == Some("mermaid")
                && let Some(renderer) = ctx.on_mermaid.clone()
            {
                return Some(renderer(c.value.clone().into()));
            }
            let bg = ctx.style.code_bg;
            let color = ctx.style.code_color;
            // With a host highlighter and a language tag, color the tokens
            // (the ranges come back sorted + non-overlapping, as
            // `with_highlights` requires).
            let text = match (&ctx.on_highlight, c.lang.as_deref()) {
                (Some(hl), Some(lang)) if !lang.is_empty() => {
                    StyledText::new(c.value.clone()).with_highlights(hl(lang, &c.value))
                }
                _ => StyledText::new(c.value.clone()),
            };
            // Size the card to its widest line, like WYSIWYG's code box (a
            // full-width card left most of the page as empty gray). Capped to
            // the column; a longer line wraps inside.
            let mut mono = window.text_style().font();
            mono.family = ctx.style.mono_font.clone();
            // Measure at bold: highlighted keywords render bold, and a
            // regular-weight measurement under-sizes the card into wrapping.
            mono.weight = FontWeight::BOLD;
            let widest = c
                .value
                .lines()
                .map(|l| {
                    let run = TextRun {
                        len: l.len(),
                        font: mono.clone(),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .shape_line(
                            SharedString::from(l.to_string()),
                            ctx.style.text_size,
                            &[run],
                            None,
                        )
                        .width()
                })
                .fold(px(0.0), Pixels::max);
            // Hover chrome at the card's top-right (Cditor-inspired): the
            // language tag and a Copy button (writes the raw code to the
            // clipboard). The WYSIWYG editor paints the same chip.
            ctx.counter += 1;
            let group: SharedString = format!("{}-code-{}", ctx.id_base, ctx.counter).into();
            let code_text: SharedString = c.value.clone().into();
            let mut chip_fg = ctx.style.muted_color;
            chip_fg.a *= 0.9;
            let chrome = div()
                .absolute()
                .top(px(4.0))
                .right(px(6.0))
                .invisible()
                .group_hover(group.clone(), |s| s.visible())
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .text_size(px(13.0))
                .text_color(chip_fg)
                .children(
                    c.lang
                        .clone()
                        .filter(|l| !l.is_empty())
                        .map(|l| div().child(SharedString::from(l))),
                )
                .child(
                    div()
                        .id(ElementId::Name(format!("{group}-copy").into()))
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(ctx.style.mark_bg))
                        .on_mouse_down(MouseButton::Left, {
                            let code_text = code_text.clone();
                            move |_: &MouseDownEvent, _window, app| {
                                app.stop_propagation();
                                app.write_to_clipboard(ClipboardItem::new_string(
                                    code_text.to_string(),
                                ));
                            }
                        })
                        .child(ctx.code_copy.clone()),
                );
            Some(
                div()
                    .w(widest + px(26.0))
                    .max_w_full()
                    .rounded(px(6.0))
                    .bg(bg)
                    .px(px(12.0))
                    .py(px(8.0))
                    .font_family(ctx.style.mono_font.clone())
                    .text_color(color)
                    .relative()
                    .group(group)
                    .child(text)
                    .child(chrome)
                    .into_any_element(),
            )
        }
        mdast::Node::Math(m) => {
            // A `$$…$$` block renders as a typeset image when the host supplies a
            // renderer; otherwise it falls back to its raw LaTeX in a code block.
            // Display math centers by default (LaTeX convention); an adjacent
            // `<!-- math:left/right -->` marker line — written by the editor's
            // alignment menu — steers it, matching WYSIWYG and the PDF export.
            if let Some(renderer) = ctx.on_math.clone() {
                let start = m.position.as_ref().map(|p| p.start.offset);
                let row = div().w_full().flex();
                let row = match preceding_math_align(&ctx.source, start) {
                    Some(MathMarkerAlign::Left) => row.justify_start(),
                    Some(MathMarkerAlign::Right) => row.justify_end(),
                    _ => row.justify_center(),
                };
                return Some(
                    row.child(renderer(m.value.clone().into()))
                        .into_any_element(),
                );
            }
            let bg = ctx.style.code_bg;
            let color = ctx.style.code_color;
            Some(
                div()
                    .w_full()
                    .rounded(px(6.0))
                    .bg(bg)
                    .px(px(12.0))
                    .py(px(8.0))
                    .font_family(ctx.style.mono_font.clone())
                    .text_color(color)
                    .child(StyledText::new(m.value.clone()))
                    .into_any_element(),
            )
        }
        mdast::Node::Blockquote(b) => {
            // A GitHub alert (`> [!NOTE]` …): colored border + bold title, body
            // in the normal text color (unlike a plain quote's muted tone). An
            // Obsidian fold char (`[!NOTE]-`/`+`) makes it a foldable callout:
            // a chevron joins the title, clicking flips the `-`/`+` in the
            // source (via the host's handler), and a folded callout shows only
            // its title.
            // Direction from the quote's source, so the `>` markers and
            // whitespace (neutral under UAX #9) don't skew it. Shared by the
            // alert branch and the plain quote below.
            let rtl = b
                .position
                .as_ref()
                .and_then(|p| ctx.source.get(p.start.offset..p.end.offset))
                .is_some_and(|s| crate::syntax::content_direction(s).is_rtl());
            if let Some((kind, fold, marker_offset, children)) = alert_parts(b) {
                let color = kind.color(&ctx.style.alerts);
                let mut title = div()
                    .flex()
                    // The icon leads the label, so it swaps sides with the text.
                    .when(rtl, |d| d.flex_row_reverse())
                    .when(!rtl, |d| d.flex_row())
                    .items_center()
                    .gap(px(6.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(color);
                if let Some(icons) = &ctx.style.alert_icons {
                    let sz = px(f32::from(ctx.style.text_size));
                    title = title.child(
                        svg()
                            .path(kind.icon(icons))
                            .text_color(color)
                            .w(sz)
                            .h(sz)
                            .flex_shrink_0(),
                    );
                }
                title = title.child(kind.label());
                let folded = fold == Some(true);
                let mut title = title.into_any_element();
                if fold.is_some() {
                    let chevron = if folded { "▸" } else { "▾" };
                    ctx.counter += 1;
                    let mut row = div()
                        .id(ElementId::Name(
                            format!("{}-fold-{}", ctx.id_base, ctx.counter).into(),
                        ))
                        .flex()
                        .when(rtl, |d| d.flex_row_reverse())
                        .when(!rtl, |d| d.flex_row())
                        .items_center()
                        .gap(px(6.0))
                        .child(title)
                        .child(div().text_color(color).child(chevron));
                    if let Some(toggle) = ctx.on_alert_toggle.clone() {
                        row = row.cursor_pointer().on_mouse_down(
                            MouseButton::Left,
                            move |_: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                toggle(marker_offset, window, cx);
                            },
                        );
                    }
                    title = row.into_any_element();
                }
                let mut q = div()
                    .border_color(color)
                    // The rule goes on the side the text starts from.
                    .when(rtl, |d| d.border_r_2().pr(px(12.0)))
                    .when(!rtl, |d| d.border_l_2().pl(px(12.0)))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(title);
                if !folded {
                    for child in &children {
                        if let Some(el) = render_block(child, ctx, window) {
                            q = q.child(el);
                        }
                    }
                }
                return Some(q.into_any_element());
            }
            let muted = ctx.style.muted_color;
            // An RTL quote's rule belongs on the right, where the text starts.
            let mut q = div()
                .border_color(muted)
                .when(rtl, |d| d.border_r_2().pr(px(12.0)))
                .when(!rtl, |d| d.border_l_2().pl(px(12.0)))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .text_color(muted);
            for child in &b.children {
                if let Some(el) = render_block(child, ctx, window) {
                    q = q.child(el);
                }
            }
            Some(q.into_any_element())
        }
        mdast::Node::ThematicBreak(_) => Some(
            div()
                .w_full()
                .h(px(1.0))
                .my(px(6.0))
                .bg(ctx.style.rule_color)
                .into_any_element(),
        ),
        // Top-level tables get their style from a preceding marker (see the root
        // loop); a nested/standalone table here renders as the default Grid.
        mdast::Node::Table(t) => Some(render_table(t, ctx, TableStyle::default(), None, window)),
        // A footnote definition: `[label] <content>`, rendered muted/smaller
        // where it sits (authors put these at the bottom).
        mdast::Node::FootnoteDefinition(f) => {
            let label = f.label.clone().unwrap_or_else(|| f.identifier.clone());
            let muted = ctx.style.muted_color;
            let mut body = div().flex().flex_col().gap(px(4.0));
            for child in &f.children {
                if let Some(el) = render_block(child, ctx, window) {
                    body = body.child(el);
                }
            }
            Some(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(6.0))
                    .text_size(px(f32::from(ctx.style.text_size) * 0.9))
                    .text_color(muted)
                    .child(div().flex_shrink_0().child(format!("[{label}]")))
                    .child(body)
                    .into_any_element(),
            )
        }
        // Raw HTML block: show the literal source (muted), never executed.
        // Comments never render — they carry control markers (math alignment,
        // table styles) and are invisible in every view.
        mdast::Node::Html(h) if h.value.trim_start().starts_with("<!--") => None,
        mdast::Node::Html(h) => Some(
            div()
                .text_color(ctx.style.muted_color)
                .child(StyledText::new(h.value.clone()))
                .into_any_element(),
        ),
        // Stray inline content at block level, or unsupported blocks:
        // render whatever text we can.
        mdast::Node::Text(t) => Some(StyledText::new(t.value.clone()).into_any_element()),
        _ => None,
    }
}

/// If a paragraph *begins* with an image (ignoring leading whitespace), return
/// it as an [`ImageInfo`] — picking up a `{width=N}` attribute typed right after
/// it — along with the remaining inline nodes (e.g. a caption on the next line)
/// to render below the image. `None` if the paragraph doesn't start with an
/// image.
fn leading_image(children: &[mdast::Node]) -> Option<(ImageInfo, Vec<mdast::Node>)> {
    let mut iter = children.iter();
    let mut first = iter.next()?;
    // Skip a purely-whitespace leading text node.
    if let mdast::Node::Text(t) = first
        && t.value.trim().is_empty()
    {
        first = iter.next()?;
    }
    let mdast::Node::Image(img) = first else {
        return None;
    };
    let img_end = img.position.as_ref()?.end.offset;

    let rest: Vec<&mdast::Node> = iter.collect();
    let mut width = None;
    let mut attr_end = img_end;
    let mut out: Vec<mdast::Node> = Vec::new();

    // The text immediately after the image may begin with `{width=N}`.
    if let Some((mdast::Node::Text(t), tail)) = rest.split_first() {
        let remainder = if let Some((w, after)) = parse_leading_width(&t.value) {
            width = Some(w);
            let consumed = t.value.len() - after.len();
            attr_end = t
                .position
                .as_ref()
                .map_or(img_end, |p| p.start.offset + consumed);
            after
        } else {
            &t.value
        };
        let remainder = remainder.trim_start();
        if !remainder.is_empty() {
            out.push(text_node(remainder));
        }
        out.extend(tail.iter().map(|n| (*n).clone()));
    } else {
        out.extend(rest.iter().map(|n| (*n).clone()));
    }

    let attr_target = if width.is_some() {
        img_end..attr_end
    } else {
        img_end..img_end
    };
    Some((
        ImageInfo {
            src: img.url.clone().into(),
            alt: img.alt.clone().into(),
            width,
            attr_target,
        },
        out,
    ))
}

/// Every standalone image in `source` (a paragraph or list item that begins with
/// `![alt](src)`), in document order — each with its parsed `{width=N}` (if any)
/// and the `attr_target` byte range to overwrite to set or replace that width.
/// Mirrors how the renderer detects block images, so the offsets line up with
/// what's on screen. Pure: parses the markdown, no I/O or storage.
pub fn images(source: &str) -> Vec<ImageInfo> {
    let mut out = Vec::new();
    if let Ok(mdast::Node::Root(root)) = markdown::to_mdast(source, &markdown::ParseOptions::gfm())
    {
        collect_images(&root.children, &mut out);
    }
    out
}

/// Every image `src` in `source` — block (leading) AND inline — so the host
/// can pre-decode them all (inline images render as rasters too, not just
/// leading ones). Pure: parses the markdown, no I/O.
pub fn all_image_srcs(source: &str) -> Vec<SharedString> {
    let mut out = Vec::new();
    if let Ok(mdast::Node::Root(root)) = markdown::to_mdast(source, &markdown::ParseOptions::gfm())
    {
        collect_all_image_srcs(&root.children, &mut out);
    }
    out
}

fn collect_all_image_srcs(nodes: &[mdast::Node], out: &mut Vec<SharedString>) {
    use mdast::Node::*;
    for node in nodes {
        match node {
            Image(img) => out.push(img.url.clone().into()),
            Paragraph(n) => collect_all_image_srcs(&n.children, out),
            Heading(n) => collect_all_image_srcs(&n.children, out),
            Blockquote(n) => collect_all_image_srcs(&n.children, out),
            List(n) => collect_all_image_srcs(&n.children, out),
            ListItem(n) => collect_all_image_srcs(&n.children, out),
            Emphasis(n) => collect_all_image_srcs(&n.children, out),
            Strong(n) => collect_all_image_srcs(&n.children, out),
            Delete(n) => collect_all_image_srcs(&n.children, out),
            Link(n) => collect_all_image_srcs(&n.children, out),
            LinkReference(n) => collect_all_image_srcs(&n.children, out),
            Table(n) => collect_all_image_srcs(&n.children, out),
            TableRow(n) => collect_all_image_srcs(&n.children, out),
            TableCell(n) => collect_all_image_srcs(&n.children, out),
            _ => {}
        }
    }
}

/// Recurse paragraphs and list items, pushing each leading-image's [`ImageInfo`].
fn collect_images(nodes: &[mdast::Node], out: &mut Vec<ImageInfo>) {
    for node in nodes {
        match node {
            mdast::Node::Paragraph(p) => {
                if let Some((info, _rest)) = leading_image(&p.children) {
                    out.push(info);
                }
            }
            mdast::Node::List(l) => collect_images(&l.children, out),
            mdast::Node::ListItem(li) => collect_images(&li.children, out),
            _ => {}
        }
    }
}

/// Parse a leading `{width=320}` / `{width=320px}` from `s`, returning the width
/// and the text after the closing `}`.
fn parse_leading_width(s: &str) -> Option<(f32, &str)> {
    let rest = s.strip_prefix("{width=")?;
    let close = rest.find('}')?;
    let num = rest[..close].strip_suffix("px").unwrap_or(&rest[..close]);
    let w = num.trim().parse::<f32>().ok().filter(|w| *w > 0.0)?;
    Some((w, &rest[close + 1..]))
}

fn text_node(value: &str) -> mdast::Node {
    mdast::Node::Text(mdast::Text {
        value: value.to_string(),
        position: None,
    })
}

fn render_list(list: &mdast::List, ctx: &mut Ctx, depth: usize, window: &mut Window) -> AnyElement {
    let nested = depth > 0;
    let mut col = div().flex().flex_col().gap(px(4.0)).pl(if nested {
        ctx.style.list_indent
    } else {
        px(2.0)
    });
    for (i, item) in list.children.iter().enumerate() {
        let mdast::Node::ListItem(li) = item else {
            continue;
        };
        // GFM task items (`- [ ]` / `- [x]`) carry `checked`; a drawn box
        // renders instead of a bullet/number (see below).
        let marker = if li.checked.is_some() {
            String::new()
        } else if list.ordered {
            // Word-style depth markers, counted from 1 — the WYSIWYG editor
            // numbers the same way, and source digits are display-irrelevant.
            crate::syntax::ordered_marker(depth, i as u32 + 1)
        } else {
            "•".to_string()
        };

        let mut content = div().flex().flex_col().gap(px(4.0));
        for (ci, child) in li.children.iter().enumerate() {
            match child {
                mdast::Node::List(sub) => {
                    content = content.child(render_list(sub, ctx, depth + 1, window))
                }
                other => {
                    // Drop a leading heading's top margin so the bullet lines up
                    // with the heading; later blocks in the item keep theirs.
                    let prev = ctx.suppress_heading_top;
                    let prev_strike = ctx.strike_done;
                    ctx.suppress_heading_top = ci == 0;
                    ctx.strike_done = li.checked == Some(true);
                    if let Some(el) = render_block(other, ctx, window) {
                        content = content.child(el);
                    }
                    ctx.suppress_heading_top = prev;
                    ctx.strike_done = prev_strike;
                }
            }
            // A folded heading inside a list item hides the item's remaining
            // children (its nested sub-list) — the outliner twin of the root
            // walk's section skip. Siblings are separate topics and stay.
            if let mdast::Node::Heading(h) = child
                && heading_fold_key(h, &ctx.source)
                    .is_some_and(|k| ctx.folded_headings.contains(&k))
            {
                break;
            }
        }

        // If the item leads with a heading, nudge the bullet down to the
        // heading's optical center. Both lines use gpui's default phi line
        // height, so the heading's line is taller by base * phi * (scale - 1);
        // half that gap re-centers the (top-aligned) bullet on the heading.
        let lead_scale = match li.children.first() {
            Some(mdast::Node::Heading(h)) => heading_scale(h.depth),
            _ => 1.0,
        };
        let marker_top = px(f32::from(ctx.style.text_size) * (lead_scale - 1.0) * 1.618_034 / 2.0);

        // The marker is a plain glyph, except a task's box, which is drawn (a
        // rounded square, accent-filled with a white check when done — the
        // editor paints the same) and clickable: a click calls back with the
        // item's source offset so the host can flip + persist.
        let mut marker_el = if let Some(done) = li.checked {
            let sz = px(f32::from(ctx.style.text_size) * 0.9);
            let line_h = px(f32::from(ctx.style.text_size) * ctx.style.line_height);
            let bx = if done {
                div()
                    .size(sz)
                    .rounded(px(3.0))
                    .bg(ctx.style.link_color)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(gpui::white())
                    .text_size(sz * 0.72)
                    .child("✓")
            } else {
                div()
                    .size(sz)
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(ctx.style.muted_color)
            };
            // Center the box on the item's first text line.
            div()
                .flex_shrink_0()
                .pt(marker_top)
                .h(line_h)
                .flex()
                .items_center()
                .child(bx)
        } else {
            div()
                .flex_shrink_0()
                .pt(marker_top)
                .text_color(ctx.style.muted_color)
                .child(marker)
        };
        if li.checked.is_some()
            && let (Some(off), Some(toggle)) = (
                li.position.as_ref().map(|p| p.start.offset),
                ctx.on_task_toggle.clone(),
            )
        {
            marker_el = marker_el.cursor_pointer().on_mouse_down(
                MouseButton::Left,
                move |_: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    toggle(off, window, cx);
                },
            );
        }
        // An item with a nested list hangs a faint vertical guide from its
        // bullet down the sub-items (Logseq-style) — under the bullet itself,
        // not at the nested block's edge.
        let has_sub = li
            .children
            .iter()
            .any(|c| matches!(c, mdast::Node::List(_)));
        let marker_col = if has_sub {
            div()
                .flex()
                .flex_col()
                .items_center()
                .flex_shrink_0()
                .child(marker_el)
                .child(div().w(px(1.0)).flex_1().bg(ctx.style.guide_color))
                .into_any_element()
        } else {
            marker_el.into_any_element()
        };
        // An RTL item puts its bullet/number on the right (#66). Direction
        // comes from the item's SOURCE slice — the list marker, digits and
        // whitespace are all neutral under UAX #9, so `- سلام` reads RTL and
        // `- hello` reads LTR.
        let item_rtl = li
            .position
            .as_ref()
            .and_then(|p| ctx.source.get(p.start.offset..p.end.offset))
            .is_some_and(|s| crate::syntax::content_direction(s).is_rtl());
        col = col.child(
            div()
                .flex()
                .when(item_rtl, |d| d.flex_row_reverse())
                .when(!item_rtl, |d| d.flex_row())
                .gap(px(8.0))
                .child(marker_col)
                .child(div().flex_1().min_w_0().child(content)),
        );
    }
    col.into_any_element()
}

// --- Inline rendering ---

#[derive(Clone)]
enum LinkTarget {
    Wiki(SharedString),
    Url(SharedString),
}

/// The text layout of whichever paragraph element got built.
///
/// RTL blocks lay themselves out (`gpui_bidi::paragraph`), because gpui wraps
/// by slicing the reordered glyph run and gets the rows backwards — so the two
/// directions carry different layouts. Everything downstream (link
/// hit-testing, click-to-caret, seating an inline formula over its spacer)
/// only ever asks these three questions, so it asks them here.
enum TextHandle {
    Ltr(TextLayout),
    Rtl(gpui_bidi::paragraph::RtlLayout),
}

impl TextHandle {
    fn index_for_position(&self, position: Point<Pixels>) -> Result<usize, usize> {
        match self {
            Self::Ltr(l) => l.index_for_position(position),
            Self::Rtl(l) => l.index_for_position(position),
        }
    }

    fn line_height(&self) -> Pixels {
        match self {
            Self::Ltr(l) => l.line_height(),
            Self::Rtl(l) => l.line_height(),
        }
    }

    /// Top-left corner of the spacer occupying `range`, where an inline raster
    /// gets painted.
    ///
    /// In an LTR line that's just the start offset's position. In an RTL line
    /// the start offset is the spacer's RIGHT edge — painting from there would
    /// cover the words beside it — so the whole range's visual box is measured
    /// and its left edge used.
    fn raster_origin(&self, range: Range<usize>) -> Option<Point<Pixels>> {
        match self {
            Self::Ltr(l) => l.position_for_index(range.start),
            Self::Rtl(l) => l.left_edge_of(range),
        }
    }
}

/// Open a link target. Shared by both directions: LTR goes through
/// `InteractiveText`, RTL hit-tests through its own layout, but what a click
/// DOES must not depend on which.
/// A paragraph link as the shared recognition type the hover callback reports.
fn link_hit(target: &LinkTarget) -> crate::syntax::LinkHit {
    match target {
        LinkTarget::Wiki(t) => crate::syntax::LinkHit::Page(t.to_string()),
        LinkTarget::Url(u) => crate::syntax::LinkHit::Url(u.to_string()),
    }
}

fn follow_link(
    target: Option<&LinkTarget>,
    on_wiki: &Option<WikiLinkHandler>,
    window: &mut Window,
    cx: &mut App,
) {
    match target {
        Some(LinkTarget::Wiki(title)) => {
            if let Some(handler) = on_wiki {
                handler(title.clone(), window, cx);
            }
        }
        // Only http(s) reaches the OS opener — the markdown is untrusted, and
        // `open_url` runs the scheme's handler.
        Some(LinkTarget::Url(url)) if crate::syntax::is_safe_external_url(url) => cx.open_url(url),
        Some(LinkTarget::Url(_)) | None => {}
    }
}

/// An inline raster spliced into a paragraph's text: `(display byte offset of
/// the spacer, spacer byte length, raster, logical w, h, image src)`. `src` is
/// `Some` for an image (clickable → preview), `None` for a `$…$` formula.
///
/// The spacer's LENGTH is carried, not just where it starts, because an RTL
/// line runs the other way: the first byte of the spacer sits at its RIGHT
/// edge, and a raster painted from there covers the text beside it instead of
/// the gap. With the range, the raster is placed by the spacer's actual
/// visual box, which is correct whichever way the line runs.
type InlineRaster = (
    usize,
    usize,
    Arc<RenderImage>,
    f32,
    f32,
    Option<SharedString>,
);

/// Window-space bounds of each painted inline image + its src, for click
/// hit-testing (image click → preview).
type ImageHits = Rc<RefCell<Vec<(Bounds<Pixels>, SharedString)>>>;

#[derive(Default)]
struct Inline {
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    links: Vec<(Range<usize>, LinkTarget)>,
    /// `(rendered byte offset, source byte offset)` checkpoints, in increasing
    /// rendered order, recorded as text is appended — so a click on the rendered
    /// text maps back to a source offset (see [`map_to_source`]).
    source_map: Vec<(usize, usize)>,
    /// Inline rasters — `$…$` formulas AND inline `![](src)` images — as
    /// `(rendered byte offset of the spacer, raster, logical w, h)`. The spacer
    /// (non-breaking spaces) reserves the width in the text; a canvas paints
    /// the raster over it at the laid-out position (see [`inline_element`]).
    math: Vec<InlineRaster>,
    /// Source offsets of this paragraph's `==` highlight markers (hidden; see
    /// [`highlight_marks`]), whether the text being appended sits inside one,
    /// and the tint it gets.
    marks: Vec<usize>,
    mark_on: bool,
    mark_bg: Hsla,
}

impl Inline {
    /// Record that the text appended next maps to source byte offset `src`.
    fn map(&mut self, src: usize) {
        self.source_map.push((self.text.len(), src));
    }
}

fn inline_element(nodes: &[mdast::Node], ctx: &mut Ctx) -> AnyElement {
    let mut inl = Inline {
        mark_bg: ctx.style.mark_bg,
        marks: highlight_marks(nodes, &ctx.source),
        ..Default::default()
    };
    // A checked task's text renders struck through + muted (Notion-style).
    let mut base = HighlightStyle::default();
    if ctx.strike_done {
        base.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.0),
            color: None,
        });
        base.color = Some(ctx.style.muted_color);
    }
    build_inline(
        nodes,
        base,
        &ctx.style,
        &ctx.definitions,
        ctx.on_inline_math.as_ref(),
        ctx.on_inline_image.as_ref(),
        &mut inl,
    );

    // In-page search: overlay a background on each match in this block's visible
    // text (document order, a stronger colour for the active match), merged into
    // the existing formatting runs so the result stays sorted + non-overlapping
    // (which `with_highlights` / `compute_runs` requires).
    let highlights = if let Some(query) = ctx.query.clone() {
        let search: Vec<(Range<usize>, Hsla)> = scan_matches(&inl.text, &query)
            .into_iter()
            .map(|r| {
                let bg = if ctx.match_ix == ctx.current_match {
                    ctx.style.search_current_bg
                } else {
                    ctx.style.search_bg
                };
                ctx.match_ix += 1;
                (r, bg)
            })
            .collect();
        overlay_search(inl.highlights, &search)
    } else {
        inl.highlights
    };

    // Writing direction for this block (#66). Detected from the RENDERED text
    // — what the reader actually sees, markers already stripped — so a
    // `- سلام` item and a `## سلام` heading both come out RTL. Every block
    // that shows prose funnels through here (paragraphs, headings, list-item
    // content, quotes, table cells), so this is the one place it's decided.
    //
    // Reader-only by design: the platform shapers already reorder RTL glyphs
    // correctly, so alignment is all that's missing here. The EDITOR needs a
    // logical↔visual index map before it can do the same — gpui's
    // `x_for_index` collapses every caret offset in an RTL line onto x=0 (see
    // the bidi note in TODO.md), and a right-aligned line with a broken caret
    // would be worse than none.
    let dir = crate::syntax::base_direction(&inl.text);
    let align = move |el: AnyElement| -> AnyElement {
        if dir.is_rtl() {
            div().w_full().text_right().child(el).into_any_element()
        } else {
            el
        }
    };

    let math = std::mem::take(&mut inl.math);

    // Rendered-text ranges of this block's links, so the click-to-caret handler
    // below can ignore a click that lands on a link. A link's own `on_click`
    // fires on mouse-*up*; the caret handler fires on mouse-*down*, so without
    // this it would enter the editor first and swallow the link click.
    let link_ranges: Vec<Range<usize>> = inl.links.iter().map(|(r, _)| r.clone()).collect();
    let targets: Vec<LinkTarget> = inl.links.iter().map(|(_, t)| t.clone()).collect();

    // #66: an RTL paragraph cannot use gpui's wrapping. gpui shapes the whole
    // paragraph as one line and slices it into rows by ascending x, and glyph
    // order is visual — so the first row gets the paragraph's LAST words and a
    // wrapped Persian note reads bottom-to-top. `RtlText` breaks in logical
    // order first (UAX #9) and paints the rows itself.
    //
    // Everything past this point is direction-agnostic: it works off
    // `TextHandle`, so links, click-to-caret and inline math behave the same
    // either way.
    // Any paragraph CONTAINING right-to-left text needs the mapping, not just
    // one that reads that way: an English sentence quoting a Persian name
    // misplaces the caret and its link hitboxes inside that phrase otherwise.
    // `with_base_rtl` keeps such a paragraph left-aligned (#66).
    let (inner, layout) = if crate::syntax::contains_rtl(&inl.text) {
        let rtl = gpui_bidi::paragraph::RtlText::new(inl.text)
            .with_base_rtl(dir.is_rtl())
            .with_highlights(highlights)
            .with_pointer_ranges(link_ranges.clone());
        let layout = TextHandle::Rtl(rtl.layout().clone());
        let inner = if link_ranges.is_empty() {
            rtl.into_any_element()
        } else {
            // `InteractiveText` hit-tests through gpui's own layout, which is
            // the thing that's wrong for RTL — so the click lands via the same
            // handle that painted the rows. Mouse-DOWN, and consumed, so the
            // click-to-caret wrapper below doesn't also fire.
            let TextHandle::Rtl(hit) = &layout else {
                unreachable!("just built as Rtl")
            };
            let hit = hit.clone();
            let ranges = link_ranges.clone();
            let targets = targets.clone();
            let on_wiki = ctx.on_wiki_link.clone();
            let hover_hit = hit.clone();
            let hover_ranges = link_ranges.clone();
            let hover_targets = targets.clone();
            let on_hover = ctx.on_link_hover.clone();
            div()
                .child(rtl)
                .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
                    let Ok(ix) = hit.index_for_position(ev.position) else {
                        return;
                    };
                    if let Some(k) = ranges.iter().position(|r| r.contains(&ix)) {
                        cx.stop_propagation();
                        follow_link(targets.get(k), &on_wiki, window, cx);
                    }
                })
                .when_some(on_hover, |el, on_hover| {
                    el.on_mouse_move(move |ev: &MouseMoveEvent, window, cx| {
                        // Same lookup as the click: the row layout that painted the
                        // glyphs maps the pointer to a byte, then to a link's boxes.
                        let hit = hover_hit
                            .index_for_position(ev.position)
                            .ok()
                            .and_then(|ix| hover_ranges.iter().position(|r| r.contains(&ix)))
                            .and_then(|k| {
                                let b = hover_hit
                                    .rects_for_range(hover_ranges[k].clone())
                                    .into_iter()
                                    .next()?;
                                Some((link_hit(hover_targets.get(k)?), b))
                            });
                        on_hover(hit, window, cx);
                    })
                })
                .into_any_element()
        };
        (inner, layout)
    } else {
        let styled = StyledText::new(inl.text).with_highlights(highlights);
        // Capture the text layout (a shared handle, populated on paint) so a click can
        // be mapped to a rendered byte index, then to a source offset (and so a canvas can paint
        // inline formulas over their spacers).
        let layout = TextHandle::Ltr(styled.layout().clone());
        let inner = if link_ranges.is_empty() {
            styled.into_any_element()
        } else {
            ctx.counter += 1;
            let id = ElementId::Name(format!("{}-{}", ctx.id_base, ctx.counter).into());
            let on_wiki = ctx.on_wiki_link.clone();
            let hover_layout = styled.layout().clone();
            let hover_ranges = link_ranges.clone();
            let hover_targets = targets.clone();
            let on_hover = ctx.on_link_hover.clone();
            InteractiveText::new(id, styled)
                .on_click(link_ranges.clone(), move |ix, window, cx| {
                    // The click was on a link range; consume it so it doesn't also reach
                    // a surrounding host handler (e.g. the click-to-caret below).
                    cx.stop_propagation();
                    follow_link(targets.get(ix), &on_wiki, window, cx);
                })
                .when_some(on_hover, |el, on_hover| {
                    el.on_hover(move |ix, _ev, window, cx| {
                        // The hovered char → its link range → that range's box on the
                        // row it starts on (a wrapped link reports its first row).
                        let hit = ix
                            .and_then(|ix| hover_ranges.iter().position(|r| r.contains(&ix)))
                            .and_then(|k| {
                                let r = &hover_ranges[k];
                                let start = hover_layout.position_for_index(r.start)?;
                                let end = hover_layout.position_for_index(r.end);
                                let right = match end {
                                    Some(e) if e.y == start.y => e.x,
                                    _ => hover_layout.bounds().right(),
                                };
                                let b = Bounds::new(
                                    start,
                                    size((right - start.x).max(px(0.)), hover_layout.line_height()),
                                );
                                Some((link_hit(hover_targets.get(k)?), b))
                            });
                        on_hover(hit, window, cx);
                    })
                })
                .into_any_element()
        };
        (inner, layout)
    };
    let layout = Rc::new(layout);

    // Click-to-caret: outside a link (link clicks `stop_propagation` above), map the
    // click to a source offset and report it so the host can place its editor caret
    // there. No handler (e.g. the journal feed) → just the inner element.
    // Window-space bounds of each painted inline image + its src, filled by the
    // canvas below and read by the click handler so a click on an image opens a
    // preview instead of entering edit mode.
    let image_hits: ImageHits = Rc::default();
    let el = match ctx.on_click_source.clone() {
        None => inner,
        Some(on_click_source) => {
            let source_map = inl.source_map;
            let click_layout = layout.clone();
            let hits = image_hits.clone();
            let preview = ctx.on_image_preview.clone();
            div()
                .child(inner)
                .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
                    // An inline image → open its preview (before caret / edit).
                    if let Some(p) = &preview
                        && let Some((_, src)) =
                            hits.borrow().iter().find(|(b, _)| b.contains(&ev.position))
                    {
                        cx.stop_propagation();
                        p(src.clone(), window, cx);
                        return;
                    }
                    let rendered = click_layout
                        .index_for_position(ev.position)
                        .unwrap_or_else(|e| e);
                    // A click on a link belongs to the link (its on_click fires on
                    // mouse-up) — don't hijack it for the caret.
                    if link_ranges.iter().any(|r| r.contains(&rendered)) {
                        return;
                    }
                    if let Some(src) = map_to_source(&source_map, rendered) {
                        // Consume so the host's surrounding click-to-edit doesn't also fire;
                        // pass the click's y so the host can keep the caret under the cursor.
                        cx.stop_propagation();
                        on_click_source(src, ev.position.y, window, cx);
                    }
                })
                .into_any_element()
        }
    };
    if math.is_empty() {
        return align(el);
    }
    // A paragraph with inline formulas: paint each raster over its spacer via a canvas painted
    // AFTER the text (so the text layout is populated + gives the spacer's window position), and
    // grow the line height so a tall formula (a fraction) doesn't overlap the neighbouring line.
    let tallest = math.iter().fold(0f32, |a, &(_, _, _, _, h, _)| a.max(h));
    let line_h = px((f32::from(ctx.style.text_size) * 1.4).max(tallest + 6.0));
    let with_math = div()
        .relative()
        .line_height(line_h)
        .child(el)
        .child(
            canvas(
                |_, _, _| {},
                move |_bounds, _: (), window, _cx| {
                    let row_h = layout.line_height();
                    let mut hits = image_hits.borrow_mut();
                    hits.clear();
                    for (off, len, img, w, h, src) in &math {
                        if let Some(p) = layout.raster_origin(*off..*off + *len) {
                            let y = p.y + (row_h - px(*h)) / 2.0;
                            let b = Bounds::new(point(p.x, y), size(px(*w), px(*h)));
                            let _ =
                                window.paint_image(b, b, Corners::default(), img.clone(), 0, false);
                            if let Some(src) = src {
                                hits.push((b, src.clone()));
                            }
                        }
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element();
    align(with_math)
}

fn build_inline(
    nodes: &[mdast::Node],
    cur: HighlightStyle,
    style: &MarkdownStyle,
    defs: &HashMap<String, String>,
    im: Option<&InlineMathRenderer>,
    ii: Option<&InlineImageRenderer>,
    out: &mut Inline,
) {
    // Mutable so `<mark>` / `</mark>` — flat sibling HTML tags, not a wrapping node —
    // can toggle the highlight on the runs between them.
    let mut cur = cur;
    for node in nodes {
        match node {
            mdast::Node::Text(t) => push_text(&t.value, node_src(node), cur, style, out),
            mdast::Node::Strong(s) => {
                let mut c = cur;
                c.font_weight = Some(FontWeight::BOLD);
                build_inline(&s.children, c, style, defs, im, ii, out);
            }
            mdast::Node::Emphasis(e) => {
                let mut c = cur;
                c.font_style = Some(FontStyle::Italic);
                build_inline(&e.children, c, style, defs, im, ii, out);
            }
            mdast::Node::InlineCode(ic) => {
                let mut c = cur;
                c.color = Some(style.code_color);
                // A subtle chip background sets inline code apart from prose. (A
                // monospace font can't be applied per text-run — `HighlightStyle` has no
                // font field — so inline code keeps the body font but gets the tint.)
                c.background_color = Some(style.code_bg);
                out.map(node_src(node) + 1); // +1 past the opening backtick
                push_run(&ic.value, c, out);
            }
            mdast::Node::InlineMath(m) => {
                // A ready formula reserves a non-breaking spacer (≈ its width) that a canvas
                // paints the raster over; until it's ready (or with no renderer) the raw
                // `$latex$` shows so the source is never lost.
                out.map(node_src(node));
                match im.and_then(|f| f(m.value.clone().into())) {
                    Some((img, w, h)) => {
                        let space_w = (f32::from(style.text_size) * 0.26).max(1.0);
                        let n = ((w / space_w).ceil() as usize).max(1);
                        out.math
                            .push((out.text.len(), n * '\u{00A0}'.len_utf8(), img, w, h, None));
                        out.text.extend(std::iter::repeat_n('\u{00A0}', n));
                    }
                    None => push_run(&format!("${}$", m.value), cur, out),
                }
            }
            mdast::Node::Link(l) => {
                let mut c = cur;
                c.color = Some(style.link_color);
                let start = out.text.len();
                build_inline(&l.children, c, style, defs, im, ii, out);
                let end = out.text.len();
                if start < end {
                    out.links
                        .push((start..end, LinkTarget::Url(l.url.clone().into())));
                }
            }
            mdast::Node::Break(_) => push_run("\n", cur, out),
            mdast::Node::Delete(d) => {
                let mut c = cur;
                c.strikethrough = Some(StrikethroughStyle {
                    thickness: px(1.0),
                    color: None,
                });
                build_inline(&d.children, c, style, defs, im, ii, out);
            }
            mdast::Node::Image(img) => {
                out.map(node_src(node));
                // A ready raster reserves a non-breaking spacer (≈ its width)
                // that the paragraph's canvas paints the image over — the same
                // mechanism as inline math. Until it's ready (or with no
                // renderer) a clickable label shows so nothing is lost.
                match ii.and_then(|f| f(img.url.clone().into())) {
                    Some((raster, w, h)) => {
                        let space_w = (f32::from(style.text_size) * 0.26).max(1.0);
                        let n = ((w / space_w).ceil() as usize).max(1);
                        out.math.push((
                            out.text.len(),
                            n * '\u{00A0}'.len_utf8(),
                            raster,
                            w,
                            h,
                            Some(img.url.clone().into()),
                        ));
                        out.text.extend(std::iter::repeat_n('\u{00A0}', n));
                    }
                    None => {
                        let label = if img.alt.is_empty() {
                            "🖼 image".to_string()
                        } else {
                            format!("🖼 {}", img.alt)
                        };
                        let mut c = cur;
                        c.color = Some(style.link_color);
                        let start = out.text.len();
                        push_run(&label, c, out);
                        let end = out.text.len();
                        out.links
                            .push((start..end, LinkTarget::Url(img.url.clone().into())));
                    }
                }
            }
            mdast::Node::FootnoteReference(f) => {
                // Render `[label]` as a marker (not clickable — jumping would
                // need anchors this text renderer doesn't have).
                let label = f.label.clone().unwrap_or_else(|| f.identifier.clone());
                let mut c = cur;
                c.color = Some(style.link_color);
                push_run(&format!("[{label}]"), c, out);
            }
            mdast::Node::LinkReference(l) => {
                // `[text][id]` resolved against the collected definitions; if
                // unresolved, the text still renders (just not linked).
                if let Some(url) = defs.get(&l.identifier).cloned() {
                    let mut c = cur;
                    c.color = Some(style.link_color);
                    let start = out.text.len();
                    build_inline(&l.children, c, style, defs, im, ii, out);
                    let end = out.text.len();
                    if start < end {
                        out.links.push((start..end, LinkTarget::Url(url.into())));
                    }
                } else {
                    build_inline(&l.children, cur, style, defs, im, ii, out);
                }
            }
            mdast::Node::ImageReference(img) => {
                let label = if img.alt.is_empty() {
                    "🖼 image".to_string()
                } else {
                    format!("🖼 {}", img.alt)
                };
                let mut c = cur;
                c.color = Some(style.link_color);
                let start = out.text.len();
                push_run(&label, c, out);
                let end = out.text.len();
                if let Some(url) = defs.get(&img.identifier) {
                    out.links
                        .push((start..end, LinkTarget::Url(url.clone().into())));
                }
            }
            // Inline raw HTML stays literal (never executed) — except the
            // styling tags both views honor (`syntax::styled_tag`): `<mark>`
            // (a highlight, the theme's tint or a chosen `background`),
            // `<span style="color:…">`, and `<u>` (underline — markdown has
            // none). Each toggles its style on the runs it wraps.
            mdast::Node::Html(h) => {
                use crate::syntax::StyledKind;
                if let Some(t) = crate::syntax::styled_tag(&h.value) {
                    match t.kind {
                        StyledKind::Mark => {
                            cur.background_color =
                                Some(t.background.map_or(style.mark_bg, hsla_of));
                        }
                        StyledKind::Span => {
                            if let Some(c) = t.color {
                                cur.color = Some(hsla_of(c));
                            }
                            if let Some(bg) = t.background {
                                cur.background_color = Some(hsla_of(bg));
                            }
                        }
                        StyledKind::Underline => {
                            cur.underline = Some(gpui::UnderlineStyle {
                                thickness: px(1.0),
                                color: None,
                                wavy: false,
                            });
                        }
                    }
                } else if let Some(kind) = crate::syntax::styled_close(&h.value) {
                    match kind {
                        StyledKind::Mark => cur.background_color = None,
                        StyledKind::Span => {
                            cur.color = None;
                            cur.background_color = None;
                        }
                        StyledKind::Underline => cur.underline = None,
                    }
                } else {
                    push_run(&h.value, cur, out);
                }
            }
            // Recurse into any other container node; ignore leaves we
            // don't special-case.
            other => {
                if let Some(children) = node_children(other) {
                    build_inline(children, cur, style, defs, im, ii, out);
                }
            }
        }
    }
}

/// Push plain text, splitting out `[[wiki-links]]` and `#tags` into
/// clickable runs. Both navigate to a page; a tag keeps its `#` in the
/// display text but targets the bare name.
fn push_text(
    value: &str,
    src_base: usize,
    cur: HighlightStyle,
    style: &MarkdownStyle,
    out: &mut Inline,
) {
    let bytes = value.as_bytes();
    let mut plain_start = 0;
    let mut i = 0;
    while i < value.len() {
        // `==` highlight marker (validated per paragraph by `highlight_marks`):
        // hidden, toggling the highlight tint for the text that follows.
        if bytes[i] == b'=' && out.marks.contains(&(src_base + i)) {
            out.map(src_base + plain_start);
            push_run(&value[plain_start..i], marked(cur, out), out);
            out.mark_on = !out.mark_on;
            i += 2;
            plain_start = i;
            continue;
        }
        // [[wiki-link]]
        if value[i..].starts_with("[[") {
            if let Some(close) = value[i + 2..].find("]]") {
                let inner = &value[i + 2..i + 2 + close];
                // `[[target|label]]` shows `label` but links to `target`; `[[name]]`
                // uses the name for both. An empty label falls back to the target.
                let (target, display) = crate::syntax::wiki_target_display(inner);
                if !target.is_empty() {
                    out.map(src_base + plain_start);
                    push_run(&value[plain_start..i], marked(cur, out), out);
                    out.map(src_base + i + 2); // the display text sits just past `[[`
                    // An unaliased anchor link (`[[Note#^id]]` / `[[Note#Heading]]`)
                    // reads as `Note → anchor` — the editor renders the same, and
                    // an alias still overrides the display entirely.
                    let anchored = (display == target).then(|| {
                        let (page, block) = crate::syntax::split_block_anchor(display);
                        if let Some(id) = block {
                            // A resolvable block link shows the target
                            // block's text (matching WYSIWYG).
                            if let Some(label) =
                                style.block_label.as_ref().and_then(|f| f(page, id))
                            {
                                return Some(label);
                            }
                            return Some(format!("{page} → {id}"));
                        }
                        let (page, heading) = crate::syntax::split_heading_anchor(display);
                        heading.map(|h| format!("{page} → {}", h.trim()))
                    });
                    if let Some(Some(shown)) = anchored {
                        push_link(&shown, target, style.link_color, marked(cur, out), out);
                    } else {
                        push_link(display, target, style.link_color, marked(cur, out), out);
                    }
                    i += 2 + close + 2;
                    plain_start = i;
                    continue;
                }
            }
            i += 1; // not a valid link; the '[' stays plain
            continue;
        }
        // A `((id))` frontend block ref (only a construct when the resolver
        // knows the id — prose parens stay prose): the target block's text,
        // linked through the anchor-only `#^id` target the host resolves.
        if value[i..].starts_with("((")
            && let Some(close) = value[i + 2..].find("))")
        {
            let id = &value[i + 2..i + 2 + close];
            if !id.is_empty()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                && let Some(label) = style.block_label.as_ref().and_then(|f| f("", id))
            {
                out.map(src_base + plain_start);
                push_run(&value[plain_start..i], marked(cur, out), out);
                out.map(src_base + i);
                push_link(
                    &label,
                    &format!("#^{id}"),
                    style.link_color,
                    marked(cur, out),
                    out,
                );
                i += 2 + close + 2;
                plain_start = i;
                continue;
            }
        }
        // An Obsidian block-id anchor (` ^some-id` at a line's end) is an
        // addressing artifact, not content — hide it (like Obsidian's preview).
        // A referenced block instead shows a small count badge (Logseq-style),
        // linked through `refs:^id` for the host to list the referencers.
        // Text nodes carry soft breaks, so a "line end" is a `\n` or the end of
        // the value.
        if bytes[i] == b' ' && value[i + 1..].starts_with('^') {
            let end = value[i..].find('\n').map_or(value.len(), |p| i + p);
            if let Some((at, id)) = crate::syntax::block_id(&value[..end])
                && at == i
            {
                out.map(src_base + plain_start);
                push_run(&value[plain_start..i], marked(cur, out), out);
                let refs = style.block_ref_count.as_ref().map_or(0, |f| f(id));
                if refs > 0 {
                    out.map(src_base + i);
                    push_link(
                        &format!(" {}", crate::syntax::superscript(refs)),
                        &format!("refs:^{id}"),
                        style.tag_color,
                        cur,
                        out,
                    );
                }
                i = end;
                plain_start = i;
                continue;
            }
        }
        // #tag — at a word boundary, followed by tag characters (the shared
        // grammar: namespaced `#a/b` included, boundary = any non-word char).
        if bytes[i] == b'#' && (i == 0 || !crate::syntax::is_word_char(bytes[i - 1])) {
            let mut j = i + 1;
            while j < value.len() && crate::syntax::is_tag_char(bytes[j]) {
                j += 1;
            }
            if j > i + 1 {
                let name = &value[i + 1..j];
                out.map(src_base + plain_start);
                push_run(&value[plain_start..i], marked(cur, out), out);
                out.map(src_base + i); // the tag (with its `#`) is verbatim in the source
                push_link(&value[i..j], name, style.tag_color, marked(cur, out), out);
                i = j;
                plain_start = i;
                continue;
            }
        }
        i += value[i..].chars().next().map_or(1, |c| c.len_utf8());
    }
    out.map(src_base + plain_start);
    push_run(&value[plain_start..], marked(cur, out), out);
}

/// `cur` with the paragraph's `==` highlight tint applied while one is open
/// (an explicit background — a `<mark style>` — wins).
fn marked(cur: HighlightStyle, out: &Inline) -> HighlightStyle {
    let mut c = cur;
    if out.mark_on && c.background_color.is_none() {
        c.background_color = Some(out.mark_bg);
    }
    c
}

/// Source offsets of the valid `==` markers across a paragraph's inline nodes,
/// scanned on the SOURCE slice they span — so a highlight may wrap nested
/// constructs (`==**bold**==`) the parser split into sibling nodes, and
/// `push_text` only has to test membership.
fn highlight_marks(nodes: &[mdast::Node], source: &str) -> Vec<usize> {
    let (Some(first), Some(last)) = (
        nodes.first().and_then(|n| n.position()),
        nodes.last().and_then(|n| n.position()),
    ) else {
        return Vec::new();
    };
    let start = first.start.offset;
    match source.get(start..last.end.offset) {
        Some(s) => crate::syntax::highlight_markers(s)
            .into_iter()
            .map(|m| m + start)
            .collect(),
        None => Vec::new(),
    }
}

/// `0xRRGGBBAA` (from `syntax::css_color`) as a gpui color.
fn hsla_of(rgba: u32) -> Hsla {
    gpui::rgba(rgba).into()
}

/// Source byte offset where `node` begins (0 if the parser recorded none).
fn node_src(node: &mdast::Node) -> usize {
    node.position().map_or(0, |p| p.start.offset)
}

/// Toggle the GFM task checkbox on the source line containing byte `offset` (a task
/// item's offset, as reported by [`MarkdownView::on_task_toggle`]). Returns the full
/// `content` with that one checkbox flipped (`[ ]`↔`[x]`), or `None` if there's no
/// task checkbox on that line. Length is unchanged (one ASCII byte swapped).
pub fn toggle_task_at(content: &str, offset: usize) -> Option<String> {
    if offset > content.len() {
        return None;
    }
    let line_start = content[..offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = content[offset..]
        .find('\n')
        .map_or(content.len(), |p| offset + p);
    let line = &content.as_bytes()[line_start..line_end];
    // The checkbox is the first `[ ]`/`[x]` on the line (it precedes any body text).
    let lb = line.iter().position(|&b| b == b'[')?;
    if lb + 2 < line.len() && line[lb + 2] == b']' && matches!(line[lb + 1], b' ' | b'x' | b'X') {
        let box_byte = line_start + lb + 1; // the status char, in `content`
        let flipped = if matches!(line[lb + 1], b'x' | b'X') {
            " "
        } else {
            "x"
        };
        let mut out = content.to_string();
        out.replace_range(box_byte..box_byte + 1, flipped);
        return Some(out);
    }
    None
}

/// Map a rendered byte index to a source byte offset via the checkpoints recorded
/// while building the inline text. `None` when there's no checkpoint at/before the
/// index (the host then falls back to plain enter-edit).
fn map_to_source(map: &[(usize, usize)], rendered: usize) -> Option<usize> {
    map.iter()
        .rev()
        .find(|(r, _)| *r <= rendered)
        .map(|(r, s)| s + (rendered - r))
}

/// Push `display` as a clickable run that navigates to page `target`.
fn push_link(display: &str, target: &str, color: Hsla, cur: HighlightStyle, out: &mut Inline) {
    let mut c = cur;
    c.color = Some(color);
    let start = out.text.len();
    push_run(display, c, out);
    let end = out.text.len();
    out.links
        .push((start..end, LinkTarget::Wiki(target.into())));
}

fn push_run(s: &str, style: HighlightStyle, out: &mut Inline) {
    if s.is_empty() {
        return;
    }
    let start = out.text.len();
    out.text.push_str(s);
    out.highlights.push((start..out.text.len(), style));
}

/// Children of a container mdast node we don't explicitly handle, so we
/// can still surface their inline text.
fn node_children(node: &mdast::Node) -> Option<&[mdast::Node]> {
    match node {
        mdast::Node::Paragraph(n) => Some(&n.children),
        _ => None,
    }
}

/// Walk the whole tree collecting reference definitions (`[id]: url`) so that
/// `[text][id]` / `![alt][id]` resolve no matter where the definition appears.
fn collect_definitions(node: &mdast::Node, out: &mut HashMap<String, String>) {
    if let mdast::Node::Definition(d) = node {
        out.entry(d.identifier.clone())
            .or_insert_with(|| d.url.clone());
    }
    if let Some(children) = node.children() {
        for child in children {
            collect_definitions(child, out);
        }
    }
}

/// Render a GFM table as a bordered grid; the first row is the header.
/// Per-table visual style, from a `<!-- table:STYLE -->` marker comment on the
/// line above the table (mirrors the editor; the style names are the shared
/// contract). `Grid` is the default (no marker).
/// The embed target when paragraph `p` is exactly a standalone `![[target]]`
/// line (a single text child holding just the embed).
/// A heading's fold key — its trimmed first source line (`## Goals`), the
/// same key the WYSIWYG editor uses, so folds read the same across views.
fn heading_fold_key(h: &mdast::Heading, source: &str) -> Option<String> {
    let pos = h.position.as_ref()?;
    let s = source.get(pos.start.offset..pos.end.offset)?;
    Some(s.lines().next().unwrap_or(s).trim().to_string())
}

fn embed_target(p: &mdast::Paragraph) -> Option<String> {
    if p.children.len() != 1 {
        return None;
    }
    let mdast::Node::Text(t) = &p.children[0] else {
        return None;
    };
    crate::syntax::embed_line(&t.value).map(str::to_string)
}

/// Render a transclusion: a bordered box with a small clickable source label
/// (opens/jumps to the target) above the target content, rendered like any
/// note. Handlers that write back by source offset (click-to-caret, task and
/// callout toggles) and in-page search are suppressed inside — those offsets
/// belong to the embedding page, not the embedded one.
fn render_embed(
    target: &str,
    label: SharedString,
    content: SharedString,
    ctx: &mut Ctx,
    window: &mut Window,
) -> AnyElement {
    let saved = (
        ctx.on_click_source.take(),
        ctx.on_task_toggle.take(),
        ctx.on_alert_toggle.take(),
        ctx.query.take(),
        ctx.on_image.take(),
        // Heading folds are the embedding page's state — its keys would
        // misfire on the embedded page's same-named headings.
        ctx.on_heading_toggle.take(),
    );
    // Images render through the read-only variant inside embeds (no resize
    // grip): their source ranges belong to the embedded page, and a resize
    // written through the embedding page's handler would corrupt it.
    ctx.on_image = ctx.on_embed_image.clone();
    ctx.embed_depth += 1;
    let mut body = div().flex().flex_col().gap(px(8.0));
    // `parse_for_render`, not `parse_cached`: an embed of an expensive-shaped
    // note would otherwise freeze the *embedding* page's render (issue #60).
    // The host warms embedded bodies in `upsert_embed` alongside their images
    // and math, so the fallback below is at most one frame.
    let parsed = parse_for_render(&content);
    if let Some(parsed) = &parsed
        && let mdast::Node::Root(root) = parsed.as_ref()
    {
        for node in &root.children {
            collect_definitions(node, &mut ctx.definitions);
        }
        let mut pending_style = None;
        for node in &root.children {
            if let mdast::Node::Html(h) = node
                && let Some(style) = table_style_marker(&h.value)
            {
                pending_style = Some((style, table_col_widths(&h.value)));
                continue;
            }
            if let mdast::Node::Table(t) = node {
                let (style, widths) = pending_style.take().unwrap_or_default();
                body = body.child(render_table(t, ctx, style, widths, window));
                continue;
            }
            pending_style = None;
            if let Some(el) = render_block(node, ctx, window) {
                body = body.child(el);
            }
        }
    } else {
        // Same fallback the top-level renderer uses: the text, unstyled, until
        // the warm lands — never an empty box.
        body = body.child(StyledText::new(content.clone()));
    }
    ctx.embed_depth -= 1;
    (
        ctx.on_click_source,
        ctx.on_task_toggle,
        ctx.on_alert_toggle,
        ctx.query,
        ctx.on_image,
        ctx.on_heading_toggle,
    ) = saved;

    // Source label: muted, link-colored on the name, click opens/jumps.
    let mut header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .text_size(px(f32::from(ctx.style.text_size) * 0.8))
        .text_color(ctx.style.link_color)
        .child(label);
    if let Some(on_wiki) = ctx.on_wiki_link.clone() {
        let t: SharedString = target.to_string().into();
        header = header.cursor_pointer().on_mouse_down(
            MouseButton::Left,
            move |_: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                on_wiki(t.clone(), window, cx);
            },
        );
    }
    div()
        .border_l_2()
        .border_color(ctx.style.muted_color)
        .rounded(px(4.0))
        .pl(px(12.0))
        .py(px(4.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(header)
        .child(body)
        .into_any_element()
}

/// If every line of paragraph `p` is a `key:: value` property, return the rows
/// as `(key, value, value_offset)` — `value_offset` is the source byte offset
/// where the value text begins, so a click on it maps back precisely. `None`
/// (mixed or ordinary prose) falls through to normal paragraph rendering.
fn property_rows(p: &mdast::Paragraph, source: &str) -> Option<Vec<(String, String, usize)>> {
    let pos = p.position.as_ref()?;
    let raw = source.get(pos.start.offset..pos.end.offset)?;
    let mut rows = Vec::new();
    let mut line_start = pos.start.offset;
    for line in raw.split_inclusive('\n') {
        let (key, value) = crate::syntax::property(line)?;
        // Offset of the (trimmed) value within the source, past `key::` + any
        // leading whitespace, so click-to-edit lands on the value.
        let idx = line.find("::").unwrap_or(0) + 2;
        let lead = line[idx..].len() - line[idx..].trim_start().len();
        rows.push((key.to_string(), value.to_string(), line_start + idx + lead));
        line_start += line.len();
    }
    (!rows.is_empty()).then_some(rows)
}

/// One piece of a mixed prose/property paragraph: clipped prose children, or
/// a property run's rows (in [`property_rows`]' `(key, value, v_off)` form).
enum MixedSeg {
    Prose(Vec<mdast::Node>),
    Props(Vec<(String, String, usize)>),
}

/// A paragraph mixing prose lines and `key:: value` lines (the Logseq shape:
/// properties right under a block's first content line) renders as a column —
/// prose rows with a property panel per property run. `None` when the
/// paragraph isn't mixed (all-or-nothing paragraphs keep their fast paths) or
/// a segment boundary can't be clipped cleanly (falls back to plain prose).
fn render_mixed_properties(
    p: &mdast::Paragraph,
    ctx: &mut Ctx,
    window: &mut Window,
) -> Option<AnyElement> {
    let source = ctx.source.clone();
    let pos = p.position.as_ref()?;
    let raw = source.get(pos.start.offset..pos.end.offset)?;
    // The paragraph's lines with their SOURCE offsets (line-start byte).
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut off = pos.start.offset;
    for line in raw.split('\n') {
        lines.push((off, line));
        off += line.len() + 1;
    }
    let is_prop: Vec<bool> = lines
        .iter()
        .map(|(_, l)| crate::syntax::property(l).is_some())
        .collect();
    if is_prop.iter().all(|&b| b) || !is_prop.iter().any(|&b| b) {
        return None; // pure paragraphs keep their existing paths
    }
    // Contiguous runs of same-kind lines → segments, clipped up front so any
    // failure falls back to plain prose rendering before anything is built.
    let mut segs: Vec<MixedSeg> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let prop = is_prop[i];
        let start = i;
        while i < lines.len() && is_prop[i] == prop {
            i += 1;
        }
        if prop {
            let rows = lines[start..i]
                .iter()
                .map(|(off, line)| {
                    let (key, value) = crate::syntax::property(line).expect("classified above");
                    // Value offset: past `key::` + leading whitespace, so
                    // click-to-edit lands on the value (same as property_rows).
                    let idx = line.find("::").unwrap_or(0) + 2;
                    let lead = line[idx..].len() - line[idx..].trim_start().len();
                    (key.to_string(), value.to_string(), off + idx + lead)
                })
                .collect();
            segs.push(MixedSeg::Props(rows));
        } else {
            let seg_start = lines[start].0;
            let (last_off, last_line) = lines[i - 1];
            let children =
                clip_children(&p.children, seg_start..last_off + last_line.len(), &source)?;
            if !children.is_empty() {
                segs.push(MixedSeg::Prose(children));
            }
        }
    }
    let mut column = div().flex().flex_col().gap(px(6.0));
    for seg in segs {
        match seg {
            MixedSeg::Prose(children) => column = column.child(inline_element(&children, ctx)),
            MixedSeg::Props(rows) => {
                ctx.counter += 1;
                column = column.child(render_property_table(rows, ctx, ctx.counter, window));
            }
        }
    }
    Some(column.into_any_element())
}

/// The paragraph children falling inside the source range `seg` (a whole
/// number of lines): wholly-contained children clone as-is; a text node
/// straddling a boundary is split at its line boundaries (soft breaks live in
/// text nodes). `None` when a non-text child straddles — the caller falls
/// back rather than guessing.
fn clip_children(
    children: &[mdast::Node],
    seg: Range<usize>,
    source: &str,
) -> Option<Vec<mdast::Node>> {
    let mut out = Vec::new();
    for ch in children {
        let cpos = ch.position()?;
        let (cs, ce) = (cpos.start.offset, cpos.end.offset);
        if ce <= seg.start || cs >= seg.end {
            continue;
        }
        if cs >= seg.start && ce <= seg.end {
            out.push(ch.clone());
            continue;
        }
        // A hard break (two trailing spaces) straddling a segment boundary is
        // meaningless once the lines split — drop it.
        if matches!(ch, mdast::Node::Break(_)) {
            continue;
        }
        let mdast::Node::Text(t) = ch else {
            return None;
        };
        out.push(clip_text(t, &seg, source)?);
    }
    Some(out)
}

/// Slice a soft-wrapped text node to the lines intersecting `seg`. Value
/// lines map 1:1 to the node's source lines, with continuation indent the
/// parser stripped re-derived per line (`source_line.ends_with(value_line)`),
/// so the clipped node's position offsets stay exact in source coordinates.
fn clip_text(t: &mdast::Text, seg: &Range<usize>, source: &str) -> Option<mdast::Node> {
    let pos = t.position.as_ref()?;
    let (ns, ne) = (pos.start.offset, pos.end.offset);
    let src_lines: Vec<&str> = source.get(ns..ne)?.split('\n').collect();
    let val_lines: Vec<&str> = t.value.split('\n').collect();
    if src_lines.len() != val_lines.len() {
        return None;
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut span: Option<(usize, usize)> = None;
    let mut soff = ns;
    for (sl, vl) in src_lines.iter().zip(&val_lines) {
        // The parser strips trailing whitespace (soft breaks, `\r` of CRLF
        // endings) and leading continuation indent from value lines — compare
        // against the trimmed source line and re-derive the indent.
        let st = sl.trim_end();
        if !st.ends_with(vl) {
            return None;
        }
        let vstart = soff + (st.len() - vl.len());
        let vend = vstart + vl.len();
        if vstart < seg.end && vend > seg.start {
            // Kept lines are consecutive by construction: `seg` is a
            // contiguous line range, and node lines are walked in order.
            kept.push(vl);
            span = Some((span.map_or(vstart, |(s, _)| s), vend));
        }
        soff += sl.len() + 1;
    }
    let (start, end) = span?;
    let mut position = pos.clone();
    position.start.offset = start;
    position.end.offset = end;
    Some(mdast::Node::Text(mdast::Text {
        value: kept.join("\n"),
        position: Some(position),
    }))
}

/// Renders a run of `key:: value` properties as a two-column panel: a muted key
/// column and the value rendered inline (wiki-links/tags stay live), each row
/// highlighting on hover. The key column is measured to the widest key (plus
/// icon room), matching WYSIWYG's panel instead of a fixed width.
fn render_property_table(
    rows: Vec<(String, String, usize)>,
    ctx: &mut Ctx,
    id: usize,
    window: &mut Window,
) -> AnyElement {
    let mut key_font = window.text_style().font();
    key_font.weight = FontWeight::MEDIUM;
    let key_w = rows.iter().fold(px(0.0), |acc, (k, ..)| {
        let run = TextRun {
            len: k.len(),
            font: key_font.clone(),
            color: ctx.style.text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        acc.max(
            window
                .text_system()
                .shape_line(
                    SharedString::from(k.clone()),
                    ctx.style.text_size,
                    &[run],
                    None,
                )
                .width(),
        )
    });
    // Icon room (icon + its gap) when the host resolves icons, then the same
    // 20px of breathing room WYSIWYG's panel gives past the widest key.
    let icon_indent = if ctx.style.property_icon.is_some() {
        f32::from(ctx.style.text_size) + 6.0
    } else {
        0.0
    };
    let key_cell_w = key_w + px(icon_indent + 20.0);
    // The panel follows its VALUES, not its keys: a Persian note's property
    // keys are usually Latin (`tags`, `key`), so keying off the line would leave
    // an RTL note's properties reading the wrong way — and deciding per ROW
    // would stagger the fixed-width key column.
    let rtl = rows
        .iter()
        .any(|(_, v, _)| crate::syntax::content_direction(v).is_rtl());
    let key_col = ctx.style.muted_color;
    let tag_c = ctx.style.tag_color;
    let link_c = ctx.style.link_color;
    let hover_border = ctx.style.muted_color;
    let mut panel = div()
        .id(SharedString::from(format!("{}-props-{id}", ctx.id_base)))
        .flex()
        .flex_col();
    // Hover needs a stateful element: one id per pill within this panel.
    let mut pill_n = 0usize;
    for (key, value, _v_off) in rows.into_iter() {
        // Value: plain runs, plus tags/wiki-links as clickable pills.
        let mut val = div()
            .flex()
            .flex_wrap()
            // Segments read right-to-left. The cell does NOT grow in that case:
            // the reversed row puts the key at the right edge and the value
            // immediately beside it, whereas a growing cell would stretch to the
            // far margin and strand the value there — which is not a `justify`
            // that can be relied on to mean the same thing under row-reverse.
            .when(rtl, |d| d.flex_row_reverse())
            .when(!rtl, |d| d.flex_1())
            .items_center()
            .gap(px(5.0))
            .px(px(8.0))
            .py(px(3.0));
        for seg in crate::syntax::property_value_segments(&value) {
            match seg {
                crate::syntax::PropSeg::Text(t) => {
                    if !t.trim().is_empty() {
                        val = val.child(SharedString::from(t.trim().to_string()));
                    }
                }
                crate::syntax::PropSeg::Pill {
                    label,
                    target,
                    is_tag,
                } => {
                    let color = if is_tag { tag_c } else { link_c };
                    let mut bg = color;
                    bg.a = 0.16;
                    let on_wiki = ctx.on_wiki_link.clone();
                    // The pill's painted box, for the hover preview's anchor:
                    // a div doesn't learn its bounds, so a zero-cost canvas
                    // laid over it records them each paint.
                    let pill_bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>> = Rc::default();
                    let probe_bounds = pill_bounds.clone();
                    let hover_target = target.clone();
                    let on_hover = ctx.on_link_hover.clone();
                    pill_n += 1;
                    val = val.child(
                        div()
                            .id(SharedString::from(format!(
                                "{}-props-{id}-pill-{pill_n}",
                                ctx.id_base
                            )))
                            .relative()
                            .when_some(on_hover, |d, on_hover| {
                                d.on_hover(move |hovered: &bool, window, cx| {
                                    let hit = hovered
                                        .then(|| pill_bounds.get())
                                        .flatten()
                                        .map(|b| (hover_target.clone(), b));
                                    on_hover(hit, window, cx);
                                })
                            })
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _: (), _window, _cx| {
                                        probe_bounds.set(Some(bounds));
                                    },
                                )
                                .absolute()
                                .inset_0(),
                            )
                            .child(
                                div()
                                    .px(px(7.0))
                                    .py(px(1.0))
                                    .rounded(px(6.0))
                                    .bg(bg)
                                    .text_color(color)
                                    .cursor_pointer()
                                    .child(SharedString::from(label))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        move |_: &MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            match &target {
                                                crate::syntax::LinkHit::Page(t) => {
                                                    if let Some(h) = &on_wiki {
                                                        h(t.clone().into(), window, cx);
                                                    }
                                                }
                                                crate::syntax::LinkHit::BlockRef(id) => {
                                                    if let Some(h) = &on_wiki {
                                                        h(format!("#^{id}").into(), window, cx);
                                                    }
                                                }
                                                // Same allowlist as the inline-link
                                                // click above: property values are
                                                // untrusted markdown too.
                                                crate::syntax::LinkHit::Url(u) => {
                                                    if crate::syntax::is_safe_external_url(u) {
                                                        cx.open_url(u);
                                                    }
                                                }
                                            }
                                        },
                                    ),
                            ),
                    );
                }
            }
        }
        // Key cell: an optional host-resolved icon, then the muted key name.
        let icon_path = ctx.style.property_icon.as_ref().and_then(|f| f(&key));
        let mut key_cell = div()
            .w(key_cell_w)
            .flex_shrink_0()
            .flex()
            // The icon leads the key name, so it swaps sides too.
            .when(rtl, |d| d.flex_row_reverse())
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(3.0))
            .text_color(key_col);
        if let Some(path) = icon_path {
            let sz = px(f32::from(ctx.style.text_size));
            key_cell = key_cell.child(
                svg()
                    .path(path)
                    .text_color(key_col)
                    .w(sz)
                    .h(sz)
                    .flex_shrink_0(),
            );
        }
        key_cell = key_cell.child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(key)),
        );
        // A clean row (no grid lines); the whole row shows a rounded border on
        // hover (Obsidian-style). A reserved transparent border keeps the layout
        // from shifting when it appears.
        panel = panel.child(
            div()
                .flex()
                // Key on the right, value to its left.
                .when(rtl, |d| d.flex_row_reverse())
                .items_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(gpui::transparent_black())
                .hover(|s| s.border_color(hover_border))
                .child(key_cell)
                .child(val),
        );
    }
    panel.into_any_element()
}

fn render_table(
    table: &mdast::Table,
    ctx: &mut Ctx,
    style: TableStyle,
    explicit_widths: Option<Vec<f32>>,
    window: &mut Window,
) -> AnyElement {
    let border = ctx.style.muted_color;
    let shade = ctx.style.code_bg;
    // Content-measured column widths, like WYSIWYG's `table_column_widths`
    // (the old equal-width `flex_1` columns stretched every table to the full
    // content width). Cells measure at their rendered size — bold header —
    // with the same 10px cell pad and 48px floor; an over-wide table scrolls
    // horizontally in its own row (the wide-image pattern) instead of
    // squeezing.
    let cell_pad = px(10.0);
    let ncols = table
        .children
        .iter()
        .filter_map(|r| match r {
            mdast::Node::TableRow(r) => Some(r.children.len()),
            _ => None,
        })
        .max()
        .unwrap_or(1)
        .max(1);
    // An RTL table reads right-to-left: the first column sits on the RIGHT
    // (#66). Direction from the table's source — pipes, dashes and digits are
    // neutral under UAX #9, so the first strong character in a cell decides.
    let rtl = table
        .position
        .as_ref()
        .and_then(|p| ctx.source.get(p.start.offset..p.end.offset))
        .is_some_and(|s| crate::syntax::content_direction(s).is_rtl());
    let base_font = window.text_style().font();
    let mut widths = vec![px(0.0); ncols];
    for (ri, row) in table.children.iter().enumerate() {
        let mdast::Node::TableRow(r) = row else {
            continue;
        };
        for (ci, cell) in r.children.iter().enumerate().take(ncols) {
            let mdast::Node::TableCell(c) = cell else {
                continue;
            };
            let text = inline_text(&c.children, &ctx.style, &ctx.definitions);
            if text.is_empty() {
                continue;
            }
            let mut font = base_font.clone();
            if ri == 0 {
                font.weight = FontWeight::BOLD;
            }
            let run = TextRun {
                len: text.len(),
                font,
                color: ctx.style.text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let w = window
                .text_system()
                .shape_line(SharedString::from(text), ctx.style.text_size, &[run], None)
                .width();
            widths[ci] = widths[ci].max(w + cell_pad * 2.0);
        }
    }
    for w in &mut widths {
        *w = (*w).max(px(48.0));
    }
    // The marker's `cols=` widths (the editor's drag-to-resize) override the
    // measurement, floored so a column can't vanish.
    if let Some(explicit) = &explicit_widths {
        for (ci, w) in explicit.iter().enumerate().take(ncols) {
            widths[ci] = px(w.max(24.0));
        }
    }
    // The grid must be sized explicitly: gpui's default stretch alignment
    // would otherwise fill the parent's full width, leaving a void after the
    // last column (borders: one vline per cell + the outer box).
    let total: Pixels = widths.iter().copied().sum::<Pixels>() + px((ncols + 2) as f32);
    let boxed = matches!(style, TableStyle::Grid);
    let vlines = matches!(style, TableStyle::Grid);
    let row_lines = matches!(style, TableStyle::Grid);
    // A single rule under the header instead of a divider between every row.
    let header_rule = matches!(style, TableStyle::Striped | TableStyle::Minimal);

    let mut grid = div().flex().flex_col().w(total).flex_shrink_0();
    if boxed {
        grid = grid
            .border_1()
            .border_color(border)
            .rounded(px(6.0))
            .overflow_hidden();
    }

    for (ri, row) in table.children.iter().enumerate() {
        let mdast::Node::TableRow(r) = row else {
            continue;
        };
        let is_header = ri == 0;
        // The mdast table has no separator child: row 0 is the header, row 1 the
        // first body row (body_index 0).
        let body_index = ri.checked_sub(1);
        // Reversing the row lays column 0 at the right without touching the
        // width/alignment bookkeeping, which stays keyed by logical index.
        let mut row_el = div()
            .flex()
            .when(rtl, |d| d.flex_row_reverse())
            .when(!rtl, |d| d.flex_row());
        // Top divider: under every row (Grid), just under the header
        // (Striped/Minimal → the first body row's top), or never (Header).
        let top_divider = if row_lines {
            !is_header
        } else {
            header_rule && body_index == Some(0)
        };
        if top_divider {
            row_el = row_el.border_t_1().border_color(border);
        }
        // Row shading: the header (Header style) or alternate body rows (Striped).
        let shaded = match style {
            TableStyle::Header => is_header,
            TableStyle::Striped => body_index.is_some_and(|b| b % 2 == 1),
            _ => false,
        };
        if shaded {
            row_el = row_el.bg(shade);
        }
        for (ci, cell) in r.children.iter().enumerate() {
            let mdast::Node::TableCell(c) = cell else {
                continue;
            };
            let mut cell_el = div()
                .w(widths.get(ci).copied().unwrap_or(px(48.0)))
                .flex_shrink_0()
                .px(px(10.0))
                // WYSIWYG's row height (text x 1.45 + 12); wrapped cells grow.
                .min_h(px(f32::from(ctx.style.text_size) * 1.45 + 12.0))
                .flex()
                .items_center();
            // Column separators sit between adjacent cells. Reversed, cell `i`
            // is drawn LEFT of cell `i-1`, so the divider belongs on every
            // cell except index 0 (the rightmost) — mirroring the LTR rule of
            // "every cell except the last".
            let needs_divider = if rtl {
                ci > 0
            } else {
                ci + 1 < r.children.len()
            };
            if vlines && needs_divider {
                cell_el = cell_el.border_r_1().border_color(border);
            }
            // Honor the column's GFM alignment (`:---:` / `---:`).
            match table.align.get(ci) {
                Some(mdast::AlignKind::Center) => cell_el = cell_el.text_center(),
                Some(mdast::AlignKind::Right) => cell_el = cell_el.text_right(),
                _ => {}
            }
            if is_header {
                cell_el = cell_el.font_weight(FontWeight::BOLD);
            }
            row_el = row_el.child(cell_el.child(inline_element(&c.children, ctx)));
        }
        grid = grid.child(row_el);
    }
    // Content-sized: the grid hugs its columns; a table wider than the note
    // column scrolls horizontally in its own row (like oversized images)
    // while the text around it keeps wrapping normally.
    ctx.counter += 1;
    div()
        .id(("md-table", ctx.counter))
        // WYSIWYG indents tables into a 22px gutter (its row handles live
        // there); mirror it so the grid sits at the same x in both views.
        // An RTL table takes that gutter on its other side and hugs the right
        // edge, where the rest of an RTL block starts.
        .when(rtl, |d| d.mr(px(22.0)).ml_auto())
        .when(!rtl, |d| d.ml(px(22.0)))
        .max_w_full()
        .overflow_x_scroll()
        .child(grid)
        .into_any_element()
}

// --- In-page search ---
//
// Pure, host-agnostic search over the *rendered* (visible) text — markup already
// stripped, the same text the reader sees. `match_count` sizes a host find bar's
// "n of m"; `MarkdownView::search` paints the matches. Both walk the inline blocks
// in the same document order, so the running match index lines up. No I/O, no
// storage — only the source string, so any app can reuse it (db or not).

/// Case-insensitive (ASCII-folded), non-overlapping byte ranges of `query` in
/// `text`. Ranges land on char boundaries, so they're safe for `StyledText`.
/// Non-ASCII is matched exactly (no Unicode case-folding — a rare nicety).
fn scan_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let (t, q) = (text.as_bytes(), query.as_bytes());
    let mut out = Vec::new();
    if q.is_empty() || q.len() > t.len() {
        return out;
    }
    let mut i = 0;
    while i + q.len() <= t.len() {
        if t[i..i + q.len()].eq_ignore_ascii_case(q)
            && text.is_char_boundary(i)
            && text.is_char_boundary(i + q.len())
        {
            out.push(i..i + q.len());
            i += q.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Merge `search` backgrounds into a block's existing (sorted, non-overlapping)
/// `formatting` runs, producing a sorted, non-overlapping set — splitting at every
/// boundary and OR-ing the search background onto any overlapping formatting run.
/// `with_highlights` / `compute_runs` require that ordering, so a match landing on
/// a bold/italic/link span can't just be appended.
fn overlay_search(
    formatting: Vec<(Range<usize>, HighlightStyle)>,
    search: &[(Range<usize>, Hsla)],
) -> Vec<(Range<usize>, HighlightStyle)> {
    if search.is_empty() {
        return formatting;
    }
    let mut points: Vec<usize> = Vec::with_capacity((formatting.len() + search.len()) * 2);
    for (r, _) in &formatting {
        points.push(r.start);
        points.push(r.end);
    }
    for (r, _) in search {
        points.push(r.start);
        points.push(r.end);
    }
    points.sort_unstable();
    points.dedup();

    let mut out = Vec::new();
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        // Boundaries split at every range edge, so each segment is fully inside or
        // fully outside every range — containment is a simple test.
        let fmt = formatting
            .iter()
            .find(|(r, _)| r.start <= a && b <= r.end)
            .map(|(_, s)| *s);
        let bg = search
            .iter()
            .find(|(r, _)| r.start <= a && b <= r.end)
            .map(|(_, c)| *c);
        match (fmt, bg) {
            (None, None) => {} // plain run — compute_runs fills the gap with the default
            (Some(s), None) => out.push((a..b, s)),
            (None, Some(c)) => out.push((
                a..b,
                HighlightStyle {
                    background_color: Some(c),
                    ..Default::default()
                },
            )),
            (Some(mut s), Some(c)) => {
                s.background_color = Some(c);
                out.push((a..b, s));
            }
        }
    }
    out
}

/// The visible text of an inline run (markup stripped), exactly as `inline_element`
/// renders it. Style/definitions don't affect the *text*, so defaults are fine.
fn inline_text(
    nodes: &[mdast::Node],
    style: &MarkdownStyle,
    defs: &HashMap<String, String>,
) -> String {
    let mut inl = Inline::default();
    build_inline(
        nodes,
        HighlightStyle::default(),
        style,
        defs,
        None,
        None,
        &mut inl,
    );
    inl.text
}

/// Visit each block's visible inline text in document order, mirroring exactly the
/// nodes `render_block` sends through `inline_element` — so search counts and
/// indices match what's painted. Code blocks / raw HTML render text directly and
/// aren't searched.
fn for_each_inline_text(
    nodes: &[mdast::Node],
    style: &MarkdownStyle,
    defs: &HashMap<String, String>,
    f: &mut impl FnMut(&str),
) {
    for node in nodes {
        match node {
            mdast::Node::Paragraph(p) => f(&inline_text(&p.children, style, defs)),
            mdast::Node::Heading(h) => f(&inline_text(&h.children, style, defs)),
            mdast::Node::TableCell(c) => f(&inline_text(&c.children, style, defs)),
            mdast::Node::List(l) => for_each_inline_text(&l.children, style, defs, f),
            mdast::Node::ListItem(li) => for_each_inline_text(&li.children, style, defs, f),
            mdast::Node::Blockquote(b) => {
                // Mirror the alert-marker strip in `render_block`, so search
                // match indices line up with what's painted.
                if let Some((_, children)) = alert_children(b) {
                    for_each_inline_text(&children, style, defs, f);
                } else {
                    for_each_inline_text(&b.children, style, defs, f);
                }
            }
            mdast::Node::Table(t) => for_each_inline_text(&t.children, style, defs, f),
            mdast::Node::TableRow(r) => for_each_inline_text(&r.children, style, defs, f),
            mdast::Node::FootnoteDefinition(fd) => {
                for_each_inline_text(&fd.children, style, defs, f)
            }
            _ => {}
        }
    }
}

/// Count case-insensitive matches of `query` in the rendered (visible) text of
/// `source` — the same matches [`MarkdownView::search`] highlights, in the same
/// order. Pure: parses the markdown, no I/O or storage. Empty query → 0. Use it to
/// size a host find bar's "n of m" and to bound the active-match index.
pub fn match_count(source: &str, query: &str) -> usize {
    find_matches(source, query).len()
}

/// The **block index** (top-level column-child index, as rendered) of each match of
/// `query` in `source`, in document order. Pair with [`MarkdownView::track_blocks`]:
/// the host reads `bounds_for_item(find_matches(..)[current])` to scroll the active
/// match's block into view. No I/O; shares the render path's memoized parse,
/// so calling it per keystroke costs one parse, not one per character.
pub fn find_matches(source: &str, query: &str) -> Vec<usize> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    let style = MarkdownStyle::default();
    let defs = HashMap::new();
    // The same cached tree `render` walks — keyed and normalized identically.
    // Sharing it is what keeps a find-bar keystroke from re-parsing the page
    // (issue #60: seconds, per character, on the shapes that go superlinear),
    // and it makes the block indices below line up with the blocks actually
    // rendered instead of a separately-parsed approximation.
    if let Some(node) = parse_cached(&crate::syntax::normalize_math_fences(source))
        && let mdast::Node::Root(root) = node.as_ref()
    {
        // Walk top-level blocks in render order, assigning each a column-child index
        // (only blocks that render to a child get one — same as `render`), and push
        // that index once per match found inside it (recursing through inline text).
        let mut block_ix = 0usize;
        for node in &root.children {
            if !renders_to_block(node) {
                continue;
            }
            let mut n = 0;
            for_each_inline_text(std::slice::from_ref(node), &style, &defs, &mut |t| {
                n += scan_matches(t, query).len();
            });
            out.extend(std::iter::repeat_n(block_ix, n));
            block_ix += 1;
        }
    }
    out
}

/// Whether a top-level node renders to a column child (mirrors `render_block`
/// returning `Some`). Kept in sync with `render_block` so `find_matches`' block
/// indices line up with the `track_blocks` handle's `bounds_for_item`.
fn renders_to_block(node: &mdast::Node) -> bool {
    match node {
        // A `<!-- table:STYLE -->` marker is hidden (folded into the next table),
        // so it isn't a column child; other HTML renders as a muted block.
        mdast::Node::Html(h) => table_style_marker(&h.value).is_none(),
        _ => matches!(
            node,
            mdast::Node::Paragraph(_)
                | mdast::Node::Heading(_)
                | mdast::Node::List(_)
                | mdast::Node::Code(_)
                | mdast::Node::Blockquote(_)
                | mdast::Node::ThematicBreak(_)
                | mdast::Node::Table(_)
                | mdast::Node::FootnoteDefinition(_)
                | mdast::Node::Text(_)
        ),
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn math_align_marker_adjacency() {
        let src = "<!-- math:left -->\n$$x$$\n";
        assert!(matches!(
            preceding_math_align(src, Some(19)),
            Some(MathMarkerAlign::Left)
        ));
        // A blank line between breaks adjacency; center (None) wins.
        let gap = "<!-- math:right -->\n\n$$x$$\n";
        assert!(preceding_math_align(gap, Some(21)).is_none());
        // Not a marker line at all.
        assert!(preceding_math_align("text\n$$x$$", Some(5)).is_none());
        // Block at the top of the document.
        assert!(preceding_math_align("$$x$$", Some(0)).is_none());
    }

    #[test]
    fn scan_is_ascii_case_insensitive_and_nonoverlapping() {
        assert_eq!(
            scan_matches("Hello hello HELLO", "hello"),
            vec![0..5, 6..11, 12..17]
        );
        assert_eq!(scan_matches("aaaa", "aa"), vec![0..2, 2..4]); // non-overlapping
        assert!(scan_matches("abc", "").is_empty());
        assert!(scan_matches("abc", "xyz").is_empty());
    }

    #[test]
    fn table_style_marker_parses() {
        assert_eq!(
            table_style_marker("<!-- table:striped -->"),
            Some(TableStyle::Striped)
        );
        assert_eq!(
            table_style_marker("<!--table:header-->"),
            Some(TableStyle::Header)
        );
        assert_eq!(table_style_marker("<!-- table:nope -->"), None);
        assert_eq!(table_style_marker("<!-- ordinary -->"), None);
        // Width attributes ride along without breaking the style parse.
        assert_eq!(
            table_style_marker("<!-- table:striped cols=120,80 -->"),
            Some(TableStyle::Striped)
        );
        assert_eq!(
            table_col_widths("<!-- table:grid cols=120,80.5 -->"),
            Some(vec![120.0, 80.5])
        );
        assert_eq!(table_col_widths("<!-- table:grid -->"), None);
        assert_eq!(table_col_widths("<!-- table:grid cols=x,1 -->"), None);
        assert_eq!(table_col_widths("<!-- table:grid cols=0,1 -->"), None);
        // The writer round-trips: Grid + no widths needs no marker at all.
        assert_eq!(
            crate::syntax::table_marker_text(TableStyle::Grid, None),
            None
        );
        assert_eq!(
            crate::syntax::table_marker_text(TableStyle::Grid, Some(&[100.4, 50.0])),
            Some("<!-- table:grid cols=100,50 -->".to_string())
        );
        assert_eq!(
            crate::syntax::table_marker_text(TableStyle::Minimal, None),
            Some("<!-- table:minimal -->".to_string())
        );
    }

    #[test]
    fn match_count_searches_visible_text_not_markup() {
        // Markup is stripped: the word is found, the syntax isn't.
        assert_eq!(match_count("**bold** and _italic_", "bold"), 1);
        assert_eq!(match_count("**bold**", "*"), 0);
        // Across blocks, case-insensitive (heading + paragraph).
        assert_eq!(match_count("# Title\n\nthe title here", "title"), 2);
        // List items + table cells are searched; nested too.
        assert_eq!(match_count("- one\n- two one", "one"), 2);
        assert_eq!(match_count("| a | one |\n|---|---|\n| one | b |", "one"), 2);
        assert_eq!(match_count("", "x"), 0);
        // A fenced code block is NOT searched (renders text directly).
        assert_eq!(match_count("```\nsecret\n```", "secret"), 0);
    }

    #[test]
    fn find_matches_reports_block_indices() {
        // Heading is block 0 (1 match), paragraph is block 1 (2 matches).
        assert_eq!(
            find_matches("# title here\n\nthe title and a title", "title"),
            vec![0, 1, 1]
        );
        // A code block holds block index 0 (no inline text), shifting the paragraph
        // with the matches to block 1 — so the host scrolls to the right child.
        assert_eq!(
            find_matches("```\ncode\n```\n\nfind me, find", "find"),
            vec![1, 1]
        );
        assert!(find_matches("x", "").is_empty());
    }

    fn first_paragraph_inline(source: &str) -> Inline {
        let style = MarkdownStyle::default();
        let defs = HashMap::new();
        let mdast::Node::Root(root) =
            markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap()
        else {
            panic!("not a root")
        };
        let mdast::Node::Paragraph(p) = &root.children[0] else {
            panic!("not a paragraph")
        };
        let mut inl = Inline {
            mark_bg: style.mark_bg,
            marks: highlight_marks(&p.children, source),
            ..Default::default()
        };
        build_inline(
            &p.children,
            HighlightStyle::default(),
            &style,
            &defs,
            None,
            None,
            &mut inl,
        );
        inl
    }

    #[test]
    fn double_equals_highlights_and_hides_its_markers() {
        let style = MarkdownStyle::default();
        let inl = first_paragraph_inline("a ==b== c");
        assert_eq!(inl.text, "a b c");
        assert!(inl.highlights.iter().any(|(r, s)| {
            &inl.text[r.clone()] == "b" && s.background_color == Some(style.mark_bg)
        }));
        // Across nested constructs, and only the highlighted run is tinted.
        let inl = first_paragraph_inline("==**bold** tail== plain");
        assert_eq!(inl.text, "bold tail plain");
        assert!(inl.highlights.iter().all(|(r, s)| {
            (s.background_color == Some(style.mark_bg)) == !inl.text[r.clone()].contains("plain")
        }));
        // Emphasis-style pairing: spaced `==` stays literal.
        assert_eq!(first_paragraph_inline("a == b == c").text, "a == b == c");
    }

    #[test]
    fn colored_mark_and_span_tags_style_their_body() {
        let inl = first_paragraph_inline(
            r#"x <mark style="background:#ff0000">hi</mark> <span style="color:#00ff00">go</span>"#,
        );
        assert_eq!(inl.text, "x hi go");
        let run = |needle: &str| {
            inl.highlights
                .iter()
                .find(|(r, _)| &inl.text[r.clone()] == needle)
                .map(|(_, s)| *s)
                .unwrap()
        };
        assert_eq!(run("hi").background_color, Some(hsla_of(0xff0000ff)));
        assert_eq!(run("go").color, Some(hsla_of(0x00ff00ff)));
        assert_eq!(run("go").background_color, None);
    }

    #[test]
    fn source_map_maps_rendered_clicks_back_to_source() {
        // "See [[Foo]] now" renders "See Foo now"; clicking past the (stripped) link
        // must land on the matching source byte.
        let src = "See [[Foo]] now";
        let inl = first_paragraph_inline(src);
        assert_eq!(inl.text, "See Foo now");
        let rendered_now = inl.text.find("now").unwrap(); // 8
        let s = map_to_source(&inl.source_map, rendered_now).unwrap();
        assert_eq!(&src[s..s + 3], "now");
        // Plain head maps 1:1.
        assert_eq!(map_to_source(&inl.source_map, 1), Some(1));
    }

    #[test]
    fn map_to_source_interpolates_and_handles_empty() {
        let map = vec![(0, 0), (4, 6), (7, 11)];
        assert_eq!(map_to_source(&map, 2), Some(2)); // 0 + 2
        assert_eq!(map_to_source(&map, 5), Some(7)); // 6 + (5-4)
        assert_eq!(map_to_source(&map, 9), Some(13)); // 11 + (9-7)
        assert!(map_to_source(&[], 3).is_none());
    }

    #[test]
    fn overlay_splits_and_merges_overlap() {
        // A match (2..6) overlapping a bold run (0..4) → three sorted, non-overlapping
        // segments: bold-only, bold+bg, bg-only.
        let fmt = vec![(
            0..4,
            HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            },
        )];
        let search = vec![(2..6, rgba(0xFF0000FF).into())];
        let out = overlay_search(fmt, &search);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].0, 0..2);
        assert!(out[0].1.background_color.is_none() && out[0].1.font_weight.is_some());
        assert_eq!(out[1].0, 2..4);
        assert!(out[1].1.background_color.is_some() && out[1].1.font_weight.is_some());
        assert_eq!(out[2].0, 4..6);
        assert!(out[2].1.background_color.is_some() && out[2].1.font_weight.is_none());
    }
}

// --- Editor helpers: markdown list / quote continuation + indent ---
//
// Pure, host-agnostic transforms over `(text, caret)` — no gpui or editor
// dependency. A markdown editor built on this crate wires Enter to
// `list_continuation` and Tab / Shift+Tab to `indent_list_line` / `outdent_line`,
// then applies the returned edit to its own input.

/// What pressing Enter should do on a markdown list / blockquote line.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ListEdit {
    /// Insert this text at the caret — a newline plus the continued marker
    /// (e.g. `"\n- "`, `"\n2. "`, `"\n> "`, `"\n- [ ] "`), indent preserved.
    Continue(String),
    /// The current item is empty (just a marker); remove it. Delete the byte
    /// range `start..end` and leave the caret at `start` (an empty line).
    Exit { start: usize, end: usize },
}

/// Decide how Enter continues a markdown list/quote at `cursor` in `value`.
/// Recognizes `-`/`*`/`+` bullets, `N.`/`N)` ordered items, `- [ ]` task items,
/// and `>` blockquotes (leading indent preserved). A non-empty item continues
/// with the next marker; an empty item exits the list. `None` when the current
/// line isn't a list/quote item.
pub fn list_continuation(value: &str, cursor: usize) -> Option<ListEdit> {
    let cursor = cursor.min(value.len());
    let line_start = value[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let line_end = value[cursor..]
        .find('\n')
        .map_or(value.len(), |i| cursor + i);
    let line = &value[line_start..line_end];
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    let (indent, rest) = line.split_at(indent_len);
    let (marker, content) = parse_list_marker(rest)?;
    if content.trim().is_empty() {
        Some(ListEdit::Exit {
            start: line_start,
            end: line_end,
        })
    } else {
        Some(ListEdit::Continue(format!("\n{indent}{marker}")))
    }
}

/// Parse a list/quote marker at the start of `rest` (after indent). Returns the
/// marker to begin the *next* line with, plus the content after this line's
/// marker. Task items are checked before plain bullets.
fn parse_list_marker(rest: &str) -> Option<(String, &str)> {
    let bullet = rest.chars().next().filter(|c| matches!(c, '-' | '*' | '+'));
    if let Some(b) = bullet
        && let Some(after) = rest[1..].strip_prefix(' ')
    {
        // Task item: `<bullet> [ ] content` (the box char is ignored — new items
        // start unchecked).
        if after.len() >= 3
            && after.starts_with('[')
            && after.as_bytes()[2] == b']'
            && let Some(content) = after[3..].strip_prefix(' ')
        {
            return Some((format!("{b} [ ] "), content));
        }
        // Plain bullet: `<bullet> content`.
        return Some((format!("{b} "), after));
    }
    // Ordered: `N. content` or `N) content` — continue with the next number.
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0
        && let Ok(n) = rest[..digits].parse::<u64>()
    {
        let after_num = &rest[digits..];
        for sep in ['.', ')'] {
            if let Some(after_sep) = after_num.strip_prefix(sep)
                && let Some(content) = after_sep.strip_prefix(' ')
            {
                return Some((format!("{}{sep} ", n + 1), content));
            }
        }
    }
    // Blockquote: `> content` (or `>content`).
    if let Some(after) = rest.strip_prefix('>') {
        let content = after.strip_prefix(' ').unwrap_or(after);
        return Some(("> ".to_string(), content));
    }
    None
}

/// The default indent level (two spaces) for Tab / Shift+Tab on list items. The
/// host passes its configured indent to [`indent_list_line`] / [`outdent_line`];
/// this is just the fallback / a convenience for callers without a setting.
pub const INDENT: &str = "  ";

/// If the caret's line is a list/quote item, indent it one level (insert `indent`
/// at the line start), returning the new text and shifted caret. `None` when the
/// line isn't a list item, so the caller can insert a literal tab instead.
pub fn indent_list_line(value: &str, cursor: usize, indent: &str) -> Option<(String, usize)> {
    let cursor = cursor.min(value.len());
    let line_start = value[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let line_end = value[cursor..]
        .find('\n')
        .map_or(value.len(), |i| cursor + i);
    let line = &value[line_start..line_end];
    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
    parse_list_marker(&line[indent_len..])?; // only list / quote lines
    let new = format!("{}{indent}{}", &value[..line_start], &value[line_start..]);
    Some((new, cursor + indent.len()))
}

/// Re-indent every space-indented list / quote item in `content` from `old`-space
/// nesting units to `new`-space units (e.g. when the list-indent setting changes),
/// so existing nesting matches the new width. Each item's level is its leading
/// spaces ÷ `old`. Non-list lines, top-level items, and tab-indented lines are
/// left untouched. `None` when nothing changes.
pub fn reindent(content: &str, old: usize, new: usize) -> Option<String> {
    if old == 0 || old == new {
        return None;
    }
    let mut changed = false;
    let out = content
        .split('\n')
        .map(|line| {
            let ws = line.len() - line.trim_start_matches(' ').len();
            if ws > 0 && parse_list_marker(&line[ws..]).is_some() {
                let new_ws = (ws / old) * new;
                if new_ws != ws {
                    changed = true;
                }
                format!("{}{}", " ".repeat(new_ws), &line[ws..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    changed.then_some(out)
}

/// Outdent the caret's line one level: remove up to `indent`'s width of leading
/// spaces (or one leading tab). Returns the new text and caret, or `None` if the
/// line has no leading indent to remove.
pub fn outdent_line(value: &str, cursor: usize, indent: &str) -> Option<(String, usize)> {
    let cursor = cursor.min(value.len());
    let line_start = value[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let line = &value[line_start..];
    let removed = if line.starts_with('\t') {
        1
    } else {
        line.bytes()
            .take(indent.len().max(1))
            .take_while(|b| *b == b' ')
            .count()
    };
    if removed == 0 {
        return None;
    }
    let new = format!("{}{}", &value[..line_start], &value[line_start + removed..]);
    let caret = cursor.saturating_sub(removed).max(line_start);
    Some((new, caret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_block_detected_with_value_offsets() {
        let src = "attendees:: Bob\ntime:: 3pm";
        let mdast::Node::Root(r) = parse_cached(src).unwrap().as_ref().clone() else {
            panic!("root");
        };
        let mdast::Node::Paragraph(p) = &r.children[0] else {
            panic!("paragraph");
        };
        let rows = property_rows(p, src).expect("all lines are properties");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "attendees");
        assert_eq!(rows[0].1, "Bob");
        // Value offset points at the value text in the source, not the key.
        assert_eq!(&src[rows[0].2..rows[0].2 + 3], "Bob");
        assert_eq!(&src[rows[1].2..rows[1].2 + 3], "3pm");

        // A paragraph with a non-property line is not a property block.
        let mdast::Node::Root(r2) = parse_cached("k:: v\njust prose").unwrap().as_ref().clone()
        else {
            panic!("root");
        };
        let mdast::Node::Paragraph(p2) = &r2.children[0] else {
            panic!("paragraph");
        };
        assert!(property_rows(p2, "k:: v\njust prose").is_none());
    }

    #[test]
    fn mixed_property_clipping_survives_real_world_line_endings() {
        // The Logseq shape: a prose first line, then property lines — with
        // CRLF endings, a trailing space, and a two-space hard break (a
        // `Break` child), all of which real vaults carry.
        let src =
            "#meeting with [[IPC]] ^abc  \r\ntime:: 14:01 \r\nsubject:: EMS comms\r\nattendees::";
        let mdast::Node::Root(r) = parse_cached(src).unwrap().as_ref().clone() else {
            panic!("root");
        };
        let mdast::Node::Paragraph(p) = &r.children[0] else {
            panic!("paragraph");
        };
        // Prose segment = line 1 (source byte range).
        let line1_end = src.find('\n').unwrap(); // includes the \r — a whole line
        let kids = clip_children(&p.children, 0..line1_end, src)
            .expect("clip must survive CRLF + trailing spaces + hard breaks");
        assert_eq!(kids.len(), 1);
        let mdast::Node::Text(t) = &kids[0] else {
            panic!("text");
        };
        assert_eq!(t.value, "#meeting with [[IPC]] ^abc");
        let pos = t.position.as_ref().unwrap();
        assert_eq!(
            &src[pos.start.offset..pos.end.offset],
            "#meeting with [[IPC]] ^abc"
        );
        // And the property lines still classify (\r-tolerant).
        assert!(crate::syntax::property("time:: 14:01 \r").is_some());
    }

    /// The parse cache is process-global now, and `cargo test` runs these on
    /// parallel threads — the tests that assert on cache identity or eviction
    /// would otherwise evict each other's entries. Only they need the lock;
    /// every other test just parses and uses the result.
    static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_cache_hits_and_evicts() {
        let _serial = CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Same source → the same cached tree (pointer-equal Arc).
        let a1 = parse_cached("# cache me\n\n- item").unwrap();
        let a2 = parse_cached("# cache me\n\n- item").unwrap();
        assert!(Arc::ptr_eq(&a1, &a2));
        // Changed content is a different key — never a stale tree.
        let b = parse_cached("# cache me\n\n- item edited").unwrap();
        assert!(!Arc::ptr_eq(&a1, &b));
        // Filling past the cap evicts the least-recently used, not the
        // just-touched entry.
        let keep = parse_cached("keep me").unwrap();
        for i in 0..PARSE_CACHE_CAP {
            let _ = parse_cached(&format!("filler {i}"));
            let again = parse_cached("keep me").unwrap(); // stays hot
            assert!(Arc::ptr_eq(&keep, &again));
        }
        // And math stays enabled through the cached options.
        let math = parse_cached("$$\nx^2\n$$").unwrap();
        let mdast::Node::Root(root) = &*math else {
            panic!("root")
        };
        assert!(matches!(root.children.first(), Some(mdast::Node::Math(_))));
    }

    #[test]
    fn cheap_parse_scan_classifies_shapes() {
        // A big note of ordinary prose still parses inline — it's linear, and
        // no first frame of raw text for a note anyone actually wrote.
        let prose = "Some prose with a [link](x) and *emphasis*.\n\n".repeat(1_000);
        assert!(prose.len() > 40_000);
        assert!(is_cheap_to_parse(&prose));
        // A pasted CSV-sized table does not.
        let table = format!(
            "| a | b |\n|---|---|\n{}",
            "| one | two |\n".repeat(MAX_TABLE_ROWS + 1)
        );
        assert!(!is_cheap_to_parse(&table));
        // Deep blockquote nesting does not (6 s at 48 KB).
        assert!(!is_cheap_to_parse(&format!("{}deep\n", "> ".repeat(5_000))));
        // Nor a document of `[[[[[…` (unmatched brackets, however spread).
        assert!(!is_cheap_to_parse(&format!(
            "{}a\n",
            "[".repeat(MAX_BRACKETS + 1)
        )));
        assert!(!is_cheap_to_parse(&"[[a\n".repeat(MAX_BRACKETS)));
        // No false positive on ordinary quoting: an alert, a nested reply
        // chain, a short table, and the wiki/embed/footnote bracket forms.
        let normal = "> [!NOTE]\n> heads up\n\n> quoted\n> > nested\n> > > deeper\n\n\
             See [[Page]], ![[Page#^ab12]], [^1] and [text](url).\n\n\
             | a | b |\n|---|---|\n| 1 | 2 |\n";
        assert!(is_cheap_to_parse(normal));
        // Many small tables are cheap even though their rows outnumber one
        // pasted CSV: the superlinear cost lives inside a single table.
        let many = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n\nbetween\n\n".repeat(200);
        assert!(is_cheap_to_parse(&many));
        // Emphasis that never resolves is the shape with no other tell: no
        // brackets, quotes or tables, and under the size cap, yet 2.6 s.
        let unresolved = format!("{}a{}", "*_".repeat(15_000), "_*".repeat(15_000));
        assert!(unresolved.len() < INLINE_PARSE_MAX);
        assert!(!is_cheap_to_parse(&unresolved));
        // But ordinary formatting stays inline — it pairs up, so it's linear.
        assert!(is_cheap_to_parse(
            &"Some *bold* and _italic_ text.\n\n".repeat(500)
        ));
    }

    #[test]
    fn warm_parse_crosses_threads() {
        let _serial = CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Expensive-shaped, so the render path refuses to parse it inline.
        let source = format!(
            "| a | b |\n|---|---|\n{}",
            "| one | two |\n".repeat(MAX_TABLE_ROWS + 1)
        );
        assert!(needs_warm(&source));
        assert!(parse_for_render(&source).is_none());
        // Warming on another thread fills the shared cache…
        let warmed = source.clone();
        std::thread::spawn(move || warm_parse(&warmed))
            .join()
            .unwrap();
        // …and this thread now renders the real tree.
        assert!(!needs_warm(&source));
        assert!(parse_for_render(&source).is_some());
    }

    #[test]
    fn alert_marker_and_strip() {
        assert!(matches!(
            alert_marker("[!NOTE]\nbody"),
            Some((AlertKind::Note, 8, None))
        ));
        assert!(matches!(
            alert_marker("[!WARNING]"),
            Some((AlertKind::Warning, 10, None))
        ));
        // Lenient form: body text on the marker line (strip includes the space).
        assert!(matches!(
            alert_marker("[!NOTE] trailing"),
            Some((AlertKind::Note, 8, None))
        ));
        // Wrong case / glued text → plain blockquote.
        assert!(alert_marker("[!note]\nbody").is_none());
        assert!(alert_marker("[!NOTEXT]").is_none());

        // End-to-end through mdast: the marker strips, its text's source
        // offset advances, and the body survives.
        let ast =
            markdown::to_mdast("> [!TIP]\n> body here", &markdown::ParseOptions::gfm()).unwrap();
        let mdast::Node::Root(root) = &ast else {
            panic!("no root")
        };
        let mdast::Node::Blockquote(b) = &root.children[0] else {
            panic!("no blockquote")
        };
        let (kind, children) = alert_children(b).expect("alert detected");
        assert_eq!(kind.label(), "Tip");
        let mdast::Node::Paragraph(p) = &children[0] else {
            panic!("no paragraph")
        };
        let mdast::Node::Text(t) = &p.children[0] else {
            panic!("no text")
        };
        assert_eq!(t.value, "body here");
        // Source offset advanced past "[!TIP]\n" (starts at "> " = 2, +7).
        assert_eq!(t.position.as_ref().unwrap().start.offset, 9);
    }

    fn cont(s: &str) -> Option<ListEdit> {
        list_continuation(s, s.len())
    }

    #[test]
    fn reindent_converts_list_widths() {
        let content = "- a\n    - b\n        - c\nplain\n    not a list";
        // 4 → 2 spaces/level: nested list items shrink; non-list + top-level stay.
        assert_eq!(
            reindent(content, 4, 2).unwrap(),
            "- a\n  - b\n    - c\nplain\n    not a list"
        );
        // 4 → 8: nested items grow.
        assert_eq!(reindent("    - b", 4, 8).unwrap(), "        - b");
        // No-op when unchanged or nothing nested.
        assert!(reindent(content, 4, 4).is_none());
        assert!(reindent("- a\n- b", 4, 8).is_none());
    }

    #[test]
    fn list_continues_bullets() {
        assert_eq!(cont("- a"), Some(ListEdit::Continue("\n- ".into())));
        assert_eq!(cont("* a"), Some(ListEdit::Continue("\n* ".into())));
        assert_eq!(cont("+ a"), Some(ListEdit::Continue("\n+ ".into())));
    }

    #[test]
    fn list_continues_ordered_incrementing() {
        assert_eq!(cont("1. a"), Some(ListEdit::Continue("\n2. ".into())));
        assert_eq!(cont("9. x"), Some(ListEdit::Continue("\n10. ".into())));
        assert_eq!(cont("3) y"), Some(ListEdit::Continue("\n4) ".into())));
    }

    #[test]
    fn list_continues_task_unchecked() {
        assert_eq!(
            cont("- [x] done"),
            Some(ListEdit::Continue("\n- [ ] ".into()))
        );
        assert_eq!(
            cont("- [ ] todo"),
            Some(ListEdit::Continue("\n- [ ] ".into()))
        );
    }

    #[test]
    fn list_continues_blockquote_and_preserves_indent() {
        assert_eq!(cont("> hi"), Some(ListEdit::Continue("\n> ".into())));
        assert_eq!(cont("  - a"), Some(ListEdit::Continue("\n  - ".into())));
    }

    #[test]
    fn list_empty_item_exits() {
        assert_eq!(cont("- "), Some(ListEdit::Exit { start: 0, end: 2 }));
        assert_eq!(cont("1. "), Some(ListEdit::Exit { start: 0, end: 3 }));
        assert_eq!(cont("> "), Some(ListEdit::Exit { start: 0, end: 2 }));
        assert_eq!(cont("- [ ] "), Some(ListEdit::Exit { start: 0, end: 6 }));
    }

    #[test]
    fn list_continuation_ignores_non_lists() {
        assert_eq!(cont("hello"), None);
        assert_eq!(cont(""), None);
        assert_eq!(cont("-nospace"), None);
    }

    #[test]
    fn list_continuation_uses_the_caret_line() {
        // Cursor at the end of the second (list) line continues that line.
        let v = "intro\n- one";
        assert_eq!(
            list_continuation(v, v.len()),
            Some(ListEdit::Continue("\n- ".into()))
        );
    }

    #[test]
    fn tab_indents_list_lines() {
        assert_eq!(indent_list_line("- a", 3, "  "), Some(("  - a".into(), 5)));
        assert_eq!(indent_list_line("* x", 1, "  "), Some(("  * x".into(), 3)));
        assert_eq!(
            indent_list_line("1. y", 4, "  "),
            Some(("  1. y".into(), 6))
        );
        // The indent unit is the caller's: a 4-space setting inserts four.
        assert_eq!(
            indent_list_line("- a", 3, "    "),
            Some(("    - a".into(), 7))
        );
        // Only the caret's line is indented.
        assert_eq!(
            indent_list_line("- a\n- b", 7, "  "),
            Some(("- a\n  - b".into(), 9))
        );
    }

    #[test]
    fn tab_ignores_non_list_lines() {
        assert_eq!(indent_list_line("hello", 5, "  "), None);
    }

    #[test]
    fn shift_tab_outdents() {
        assert_eq!(outdent_line("  - a", 5, "  "), Some(("- a".into(), 3)));
        assert_eq!(outdent_line("    x", 5, "  "), Some(("  x".into(), 3)));
        assert_eq!(outdent_line("- a", 3, "  "), None);
        // A 4-space unit removes up to four leading spaces in one outdent.
        assert_eq!(outdent_line("    - a", 7, "    "), Some(("- a".into(), 3)));
    }

    fn inline_of(text: &str) -> Inline {
        let mut inl = Inline::default();
        let nodes = vec![mdast::Node::Text(mdast::Text {
            value: text.into(),
            position: None,
        })];
        build_inline(
            &nodes,
            HighlightStyle::default(),
            &MarkdownStyle::default(),
            &HashMap::new(),
            None,
            None,
            &mut inl,
        );
        inl
    }

    #[test]
    fn wikilinks_become_clickable_runs_without_brackets() {
        let inl = inline_of("see [[Foo]] and [[Bar]] ok");
        assert_eq!(inl.text, "see Foo and Bar ok");
        assert_eq!(inl.links.len(), 2);
        let titles: Vec<&str> = inl
            .links
            .iter()
            .map(|(r, t)| match t {
                LinkTarget::Wiki(_) => &inl.text[r.clone()],
                _ => "",
            })
            .collect();
        assert_eq!(titles, vec!["Foo", "Bar"]);
    }

    #[test]
    fn aliased_wikilink_shows_label_links_target() {
        let inl = inline_of("jump [[file.pdf#p3|\u{2197}]] here");
        assert_eq!(inl.text, "jump \u{2197} here");
        assert_eq!(inl.links.len(), 1);
        let (range, target) = &inl.links[0];
        assert_eq!(&inl.text[range.clone()], "\u{2197}"); // displayed label
        match target {
            LinkTarget::Wiki(t) => assert_eq!(t.as_ref(), "file.pdf#p3"), // link target
            _ => panic!("expected wiki link"),
        }
    }

    #[test]
    fn hashtags_become_clickable_links_targeting_bare_name() {
        let inl = inline_of("a #foo and #bar-baz end");
        assert_eq!(inl.text, "a #foo and #bar-baz end"); // display keeps the '#'
        assert_eq!(inl.links.len(), 2);
        assert_eq!(&inl.text[inl.links[0].0.clone()], "#foo");
        match (&inl.links[0].1, &inl.links[1].1) {
            (LinkTarget::Wiki(a), LinkTarget::Wiki(b)) => {
                assert_eq!(a.as_ref(), "foo");
                assert_eq!(b.as_ref(), "bar-baz");
            }
            _ => panic!("expected wiki targets"),
        }
    }

    #[test]
    fn mark_tag_highlights_text_other_html_stays_literal() {
        let html = |v: &str| {
            mdast::Node::Html(mdast::Html {
                value: v.into(),
                position: None,
            })
        };
        let text = |v: &str| {
            mdast::Node::Text(mdast::Text {
                value: v.into(),
                position: None,
            })
        };
        let style = MarkdownStyle::default();

        // `<mark>hi</mark>`: the tags are consumed and `hi` carries the mark background.
        let mut inl = Inline::default();
        build_inline(
            &[html("<mark>"), text("hi"), html("</mark>")],
            HighlightStyle::default(),
            &style,
            &HashMap::new(),
            None,
            None,
            &mut inl,
        );
        assert_eq!(inl.text, "hi");
        assert!(
            inl.highlights
                .iter()
                .any(|(r, s)| &inl.text[r.clone()] == "hi"
                    && s.background_color == Some(style.mark_bg)),
            "wrapped text should carry the mark background"
        );

        // Any other inline HTML is still shown literally.
        let mut inl2 = Inline::default();
        build_inline(
            &[html("<br>")],
            HighlightStyle::default(),
            &style,
            &HashMap::new(),
            None,
            None,
            &mut inl2,
        );
        assert_eq!(inl2.text, "<br>");
    }

    #[test]
    fn heading_hash_with_space_is_not_a_tag() {
        let inl = inline_of("# not a tag");
        assert!(inl.links.is_empty());
    }

    #[test]
    fn plain_text_has_no_links() {
        let inl = inline_of("just text");
        assert_eq!(inl.text, "just text");
        assert!(inl.links.is_empty());
    }

    #[test]
    fn empty_brackets_are_literal() {
        let inl = inline_of("a [[]] b");
        assert_eq!(inl.text, "a [[]] b");
        assert!(inl.links.is_empty());
    }

    #[test]
    fn parse_leading_width_variants() {
        assert_eq!(parse_leading_width("{width=320}"), Some((320.0, "")));
        assert_eq!(
            parse_leading_width("{width=200px}\nHere"),
            Some((200.0, "\nHere"))
        );
        assert_eq!(parse_leading_width("{width=0}"), None);
        assert_eq!(parse_leading_width("just text"), None);
    }

    #[test]
    fn leading_image_detection() {
        fn para_children(md: &str) -> Vec<mdast::Node> {
            let tree = markdown::to_mdast(md, &markdown::ParseOptions::gfm()).unwrap();
            if let mdast::Node::Root(root) = tree {
                for n in root.children {
                    if let mdast::Node::Paragraph(p) = n {
                        return p.children;
                    }
                }
            }
            vec![]
        }
        let (info, rest) = leading_image(&para_children("![alt](/x.png)")).unwrap();
        assert_eq!(info.src.as_ref(), "/x.png");
        assert_eq!(info.alt.as_ref(), "alt");
        assert_eq!(info.width, None);
        assert!(rest.is_empty());

        let (sized, rest) = leading_image(&para_children("![](/x.png){width=200}")).unwrap();
        assert_eq!(sized.width, Some(200.0));
        // The attribute span is non-empty so a resize replaces it in place.
        assert!(sized.attr_target.start < sized.attr_target.end);
        assert!(rest.is_empty());

        // A caption typed on the next line: the image still renders, and the
        // text is returned as `rest` to show below it (the reported bug).
        let (capt, rest) = leading_image(&para_children("![](/x.png){width=200}\nHere")).unwrap();
        assert_eq!(capt.width, Some(200.0));
        assert!(!rest.is_empty());

        // Text before the image isn't a leading-image block.
        assert!(leading_image(&para_children("see ![](/x.png) here")).is_none());
    }

    #[test]
    fn images_enumerates_list_items() {
        // The real journal/page format: images live in bullet list items.
        let src = "- ![](a.jpg){width=2000}\n- ![](b.jpg)\n- text only\n- ![](c.jpg)";
        let imgs = images(src);
        assert_eq!(imgs.len(), 3); // the text-only item is skipped
        // The explicit width is parsed; its attr_target spans the {width=N}.
        assert_eq!(imgs[0].width, Some(2000.0));
        assert!(imgs[0].attr_target.start < imgs[0].attr_target.end);
        // A width-less image reports None and an empty (insertion-point) range
        // that sits right after its `)` — so inserting `{width=N}` there is valid.
        assert_eq!(imgs[1].width, None);
        assert_eq!(imgs[1].attr_target.start, imgs[1].attr_target.end);
        assert_eq!(
            &src[imgs[1].attr_target.start..imgs[1].attr_target.start + 1],
            "\n"
        );
    }

    fn first_para(md: &str) -> Vec<mdast::Node> {
        let tree = markdown::to_mdast(md, &markdown::ParseOptions::gfm()).unwrap();
        if let mdast::Node::Root(root) = tree {
            for n in root.children {
                if let mdast::Node::Paragraph(p) = n {
                    return p.children;
                }
            }
        }
        vec![]
    }

    #[test]
    fn reference_link_resolves_against_definition() {
        let md = "[the docs][d]\n\n[d]: https://example.com";
        let tree = markdown::to_mdast(md, &markdown::ParseOptions::gfm()).unwrap();
        let mut defs = HashMap::new();
        if let mdast::Node::Root(root) = &tree {
            for n in &root.children {
                collect_definitions(n, &mut defs);
            }
        }
        assert_eq!(
            defs.get("d").map(String::as_str),
            Some("https://example.com")
        );

        let mut inl = Inline::default();
        build_inline(
            &first_para(md),
            HighlightStyle::default(),
            &MarkdownStyle::default(),
            &defs,
            None,
            None,
            &mut inl,
        );
        assert_eq!(inl.text, "the docs");
        assert_eq!(inl.links.len(), 1);
        assert!(
            matches!(&inl.links[0].1, LinkTarget::Url(u) if u.as_ref() == "https://example.com")
        );
    }

    #[test]
    fn footnote_reference_renders_marker() {
        let mut inl = Inline::default();
        build_inline(
            &first_para("text[^1]\n\n[^1]: the note"),
            HighlightStyle::default(),
            &MarkdownStyle::default(),
            &HashMap::new(),
            None,
            None,
            &mut inl,
        );
        assert_eq!(inl.text, "text[1]");
    }

    #[test]
    fn document_extracts_inline_text_from_blocks() {
        // Walk representative markdown the way `render_block` does: pull
        // inline text out of heading/paragraph blocks.
        let md = "# Title\n\nSome **bold** and *italic* and `code` with [[Link]].\n\n- a\n- b\n";
        let tree = markdown::to_mdast(md, &markdown::ParseOptions::default()).unwrap();
        let mut text = String::new();
        if let mdast::Node::Root(root) = tree {
            for n in &root.children {
                let children = match n {
                    mdast::Node::Heading(h) => &h.children,
                    mdast::Node::Paragraph(p) => &p.children,
                    _ => continue,
                };
                let mut inl = Inline::default();
                build_inline(
                    children,
                    HighlightStyle::default(),
                    &MarkdownStyle::default(),
                    &HashMap::new(),
                    None,
                    None,
                    &mut inl,
                );
                text.push_str(&inl.text);
                text.push('\n');
            }
        }
        assert!(text.contains("Title"), "got: {text:?}");
        assert!(text.contains("bold"));
        assert!(text.contains("Link")); // [[Link]] rendered as "Link"
    }
}
