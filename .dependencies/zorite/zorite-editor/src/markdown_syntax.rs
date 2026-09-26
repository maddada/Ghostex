//! Inline-markdown styling for live-preview ("WYSIWYG") editing — W1.
//!
//! A fast, line-scoped scanner that turns markdown source into styled text runs
//! **without changing the byte buffer**: syntax markers (`**`, `` ` ``, `[` …)
//! are kept in the text and merely dimmed, so the displayed characters still
//! match the buffer one-to-one and the caret/offset model is untouched.
//!
//! Scope (W1): inline constructs at the editor's normal text size — bold,
//! italic, strikethrough, inline code, links, wiki-links, and tags. Heading
//! sizes need per-line font sizes (variable line heights) and are deferred to
//! W2; block widgets (images, fenced code, tables, …) to W4.

use std::ops::Range;

use gpui::{
    Font, FontStyle, FontWeight, Hsla, SharedString, StrikethroughStyle, TextRun, UnderlineStyle,
    hsla, px,
};

use crate::Diagnostic;

/// Colors + monospace font for inline markdown styling, supplied by the host so
/// the editor stays theme-agnostic. Install via
/// [`crate::EditorState::set_markdown_style`]; absent it, the editor renders
/// plain text (only spell-check underlines).
#[derive(Clone)]
pub struct SyntaxStyle {
    /// Dimmed color for the syntax markers themselves (`**`, `*`, `~~`, `[`,
    /// `]`, `[[`, `]]`, `]( … )`).
    pub marker: Hsla,
    /// Inline `code` text color.
    pub code: Hsla,
    /// Inline `code` background.
    pub code_bg: Hsla,
    /// `[text](url)` and `[[wiki-links]]`.
    pub link: Hsla,
    /// `#tags`.
    pub tag: Hsla,
    /// Blockquote text + left-border color (a muted tone).
    pub quote: Hsla,
    /// GitHub-style alert (`> [!NOTE]` …) border + marker colors, per kind.
    pub alert_note: Hsla,
    pub alert_tip: Hsla,
    pub alert_important: Hsla,
    pub alert_warning: Hsla,
    pub alert_caution: Hsla,
    /// SVG asset paths for the alert title icons, resolved through the host's
    /// `AssetSource`. `None` (the default host choice) paints the bold label
    /// alone, keeping the crate asset-free.
    pub alert_icons: Option<AlertIcons>,
    /// Thematic break (`---`) divider color.
    pub rule: Hsla,
    /// `<mark>` highlight background.
    pub mark_bg: Hsla,
    /// Resolves a block link's display text: `(page, id)` → the target
    /// block's text, host-supplied. `None` (or a `None` return) keeps the
    /// `Page → id` form.
    #[allow(clippy::type_complexity)]
    pub block_label: Option<std::rc::Rc<dyn Fn(&str, &str) -> Option<String>>>,
    /// Bumped by the host when resolved labels change — part of the line-run
    /// cache epoch, so cached lines re-key when a target block edits.
    pub block_label_gen: u64,
    /// How many pages reference a block id — a non-zero count paints a small
    /// badge in place of the hidden ` ^id` anchor (Logseq-style; clicking it
    /// emits `OpenWikiLink("refs:^id")` for the host to list the referencers).
    /// Count changes ride `block_label_gen` for cache re-keying.
    pub block_ref_count: Option<BlockRefCountFn>,
    /// Popover/menu surface background (e.g. the right-click table menu).
    pub popover_bg: Hsla,
    /// Popover/menu border.
    pub popover_border: Hsla,
    /// Popover/menu foreground text.
    pub popover_fg: Hsla,
    /// Popover/menu hovered-row background (a soft accent tint).
    pub popover_hover: Hsla,
    /// Popover/menu group divider.
    pub popover_divider: Hsla,
    /// Popover/menu destructive rows ("Delete …" — label + glyph in red).
    pub popover_danger: Hsla,
    /// Monospace font for inline code.
    pub mono: Font,
    /// Resolves a property key (`tags`, `status`, …) to an icon shown before it
    /// in the property panel. Host-provided (asset path through the host's
    /// `AssetSource`) so the crate stays asset-agnostic; `None` = no icons.
    pub property_icon: Option<PropertyIconFn>,
    /// Heading text colour (`None` = the body colour). Local addition for the Docs prototype.
    pub heading: Option<Hsla>,
}

impl Style {
    /// `self` layered over `base` (the enclosing construct's style): the
    /// booleans OR, the options prefer `self`'s — so `code` inside bold keeps
    /// its color/tint while inheriting the bold weight.
    fn over(mut self, base: &Style) -> Style {
        self.bold |= base.bold;
        self.italic |= base.italic;
        self.strike |= base.strike;
        self.underline |= base.underline;
        self.mono |= base.mono;
        if self.color.is_none() {
            self.color = base.color;
        }
        if self.bg.is_none() {
            self.bg = base.bg;
        }
        self
    }
}

/// Maps a property key to an icon asset path the host serves, or `None` for no
/// icon. Host-provided so the crate makes no assumption about which assets exist.
pub type PropertyIconFn = std::rc::Rc<dyn Fn(&str) -> Option<gpui::SharedString>>;

/// Maps a block id to how many pages reference it (0 = no badge).
pub type BlockRefCountFn = std::rc::Rc<dyn Fn(&str) -> usize>;

/// Styling a scanned span adds on top of the editor's base run.
#[derive(Clone, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    mono: bool,
    color: Option<Hsla>,
    bg: Option<Hsla>,
    /// A syntax marker (`**`, `#`, `[`, …) — dimmed when shown, and removed
    /// entirely when the line's markers are hidden (W6, reveal-on-caret).
    hide: bool,
    /// A formatting marker (`**`, `*`, `~~`, `` ` ``, `<mark>`, `<u>`) stays
    /// hidden even with the caret inside its construct — Cditor-style: the
    /// styling IS the feedback, and the toggles (⌘B/I/U/…, the selection
    /// format bar) edit it. Structural markers (links, headings, math) keep
    /// the reveal, since their raw text is the only way to edit them.
    always_hide: bool,
    /// Non-zero on a formatting marker: the opener and closer of one construct
    /// share an id, so `fmt_marker_pairs` can pair them even with NESTED
    /// constructs between (adjacency no longer identifies the pair).
    pair_id: u16,
    /// What a hidden marker paints in its place (e.g. a block link's `#^`
    /// shows ` → `): every replacement byte maps back to the span's start, so
    /// the display↔source maps stay consistent. `None` = plain removal.
    replace: Option<SharedString>,
}

struct Span {
    range: Range<usize>,
    style: Style,
}

/// Build the editor's text runs: the base style, plus inline-markdown styling
/// when `md` is `Some`, plus a red wavy underline on each diagnostic span.
///
/// With `md = None` and no diagnostics this is a single plain run, so it
/// subsumes the former diagnostics-only builder.
pub(crate) fn styled_runs(
    text: &str,
    base_font: &Font,
    base_color: Hsla,
    diagnostics: &[Diagnostic],
    md: Option<&SyntaxStyle>,
) -> Vec<TextRun> {
    let spans = md.map(|s| scan(text, s)).unwrap_or_default();
    let squiggle = UnderlineStyle {
        color: Some(hsla(0., 0.8, 0.55, 1.)),
        thickness: px(1.5),
        wavy: true,
    };

    // Every point where styling can change: span + diagnostic edges (clamped).
    let mut bounds: Vec<usize> = vec![0, text.len()];
    for s in &spans {
        bounds.push(s.range.start);
        bounds.push(s.range.end);
    }
    for d in diagnostics {
        if d.range.start < d.range.end && d.range.end <= text.len() {
            bounds.push(d.range.start);
            bounds.push(d.range.end);
        }
    }
    bounds.retain(|&b| b <= text.len());
    bounds.sort_unstable();
    bounds.dedup();

    let mut runs = Vec::new();
    for win in bounds.windows(2) {
        let (a, b) = (win[0], win[1]);
        if a >= b {
            continue;
        }
        // Spans don't overlap, so the first covering one is THE one.
        let style = spans
            .iter()
            .find(|s| s.range.start <= a && a < s.range.end)
            .map(|s| &s.style);
        let underline = diagnostics
            .iter()
            .any(|d| d.range.start <= a && a < d.range.end && d.range.end <= text.len());

        let mut font = match style {
            Some(s) if s.mono => md.map_or_else(|| base_font.clone(), |m| m.mono.clone()),
            _ => base_font.clone(),
        };
        if let Some(s) = style {
            if s.bold {
                font.weight = FontWeight::SEMIBOLD;
            }
            if s.italic {
                font.style = FontStyle::Italic;
            }
        }
        runs.push(TextRun {
            len: b - a,
            font,
            color: style.and_then(|s| s.color).unwrap_or(base_color),
            background_color: style.and_then(|s| s.bg),
            underline: if underline {
                Some(squiggle)
            } else {
                style.filter(|s| s.underline).map(|_| UnderlineStyle {
                    thickness: px(1.0),
                    color: None,
                    wavy: false,
                })
            },
            strikethrough: style.filter(|s| s.strike).map(|_| StrikethroughStyle {
                thickness: px(1.5),
                color: None,
            }),
        });
    }
    runs
}

/// Build a `TextRun` of byte length `len` from a scanned span's style (or the
/// base style when `None`). Shared by the hidden-line renderer.
fn run_for(
    len: usize,
    style: Option<&Style>,
    base_font: &Font,
    base_color: Hsla,
    md: Option<&SyntaxStyle>,
    underline: Option<UnderlineStyle>,
) -> TextRun {
    let mut font = match style {
        Some(s) if s.mono => md.map_or_else(|| base_font.clone(), |m| m.mono.clone()),
        _ => base_font.clone(),
    };
    if let Some(s) = style {
        if s.bold {
            font.weight = FontWeight::SEMIBOLD;
        }
        if s.italic {
            font.style = FontStyle::Italic;
        }
    }
    TextRun {
        len,
        font,
        color: style.and_then(|s| s.color).unwrap_or(base_color),
        background_color: style.and_then(|s| s.bg),
        underline: underline.or_else(|| {
            style.filter(|s| s.underline).map(|_| UnderlineStyle {
                thickness: px(1.0),
                color: None,
                wavy: false,
            })
        }),
        strikethrough: style.filter(|s| s.strike).map(|_| StrikethroughStyle {
            thickness: px(1.5),
            color: None,
        }),
    }
}

/// The maximal run of adjacent spans (a markdown construct: its markers + body)
/// containing source byte `c`, or an empty range when `c` is in plain text.
/// `spans` must be sorted by `range.start` and non-overlapping. Used to reveal
/// only the construct the caret sits in (#5).
fn construct_at(spans: &[Span], c: usize) -> Range<usize> {
    let mut i = 0;
    while i < spans.len() {
        let start = spans[i].range.start;
        let mut end = spans[i].range.end;
        let mut j = i + 1;
        while j < spans.len() && spans[j].range.start == end {
            end = spans[j].range.end;
            j += 1;
        }
        if start <= c && c <= end {
            return start..end;
        }
        i = j;
    }
    0..0
}

/// Render `line` with its syntax markers HIDDEN (W6): returns the display string
/// (source minus the marker chars), the styled runs over it, and a per-display-
/// byte map back to the source byte offset (length `display.len() + 1`, so the
/// end position maps too). Used for every styled line that isn't fully revealed;
/// the construct under `caret_col` (if any) keeps its markers (#5). The caller
/// maps caret/selection columns through the returned map (see `display_col`).
/// Spans don't overlap and cover the line in order, so each non-marker segment
/// contributes one run of its byte length.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hidden_runs(
    line: &str,
    base_font: &Font,
    base_color: Hsla,
    diagnostics: &[Diagnostic],
    caret_col: Option<usize>,
    reveal_prefix: usize,
    // Bytes whose marker spans stay hidden even inside the caret's construct
    // — the line-level prefix while its gutter mark (bullet/number/box) is
    // painted, else the glyph and the raw marker would BOTH show.
    hide_prefix: usize,
    // Show every inline marker (a selection touches this line, so the
    // highlighted glyphs must be the copied bytes) while the prefix stays
    // hidden behind its painted mark.
    reveal_inline: bool,
    md: &SyntaxStyle,
) -> (String, Vec<TextRun>, Vec<usize>) {
    let mut spans = scan(line, md);
    spans.sort_by_key(|s| s.range.start);

    // The construct (a maximal run of adjacent spans: its markers + body) the
    // caret sits in keeps its markers visible — everything else hides them (#5,
    // per-construct reveal). Empty range when the caret is in plain text / absent.
    let reveal = caret_col.map_or(0..0, |c| construct_at(&spans, c));

    // The visible segments (markers dropped), each a source byte range + style
    // (+ a replacement string when a hidden marker paints something in its
    // place). A visible segment is copied verbatim, so source↔display is 1:1
    // within it — which lets a diagnostic (source coords) map straight onto
    // the display.
    let mut segs: Vec<(Range<usize>, Option<&Style>, Option<SharedString>)> = Vec::new();
    let mut pos = 0;
    for span in &spans {
        if span.range.start > pos {
            segs.push((pos..span.range.start, None, None));
        }
        // A marker is shown when it's inside the caret's construct OR within the
        // revealed line-level prefix (e.g. a blockquote's `>` while the caret is
        // anywhere on the line).
        let in_construct = reveal.start <= span.range.start && span.range.end <= reveal.end;
        let in_prefix = span.range.end <= reveal_prefix;
        let force = span.range.end <= hide_prefix;
        let hidden = span.style.hide
            && (span.style.always_hide || force || (!reveal_inline && !in_construct && !in_prefix));
        if !hidden {
            segs.push((span.range.clone(), Some(&span.style), None));
        } else if let Some(rep) = span.style.replace.clone() {
            segs.push((span.range.clone(), Some(&span.style), Some(rep)));
        }
        pos = span.range.end;
    }
    if pos < line.len() {
        segs.push((pos..line.len(), None, None));
    }

    let squiggle = UnderlineStyle {
        color: Some(hsla(0., 0.8, 0.55, 1.)),
        thickness: px(1.5),
        wavy: true,
    };
    let mut display = String::with_capacity(line.len());
    let mut runs: Vec<TextRun> = Vec::new();
    let mut map: Vec<usize> = Vec::with_capacity(line.len() + 1);
    for (src, style, replace) in &segs {
        // A replacement paints in the hidden marker's place: its bytes all map
        // back to the span's start (so caret/click math lands on the marker),
        // and it takes one run in the marker's own style.
        if let Some(rep) = replace {
            display.push_str(rep.as_ref());
            map.extend(std::iter::repeat_n(src.start, rep.len()));
            runs.push(run_for(
                rep.len(),
                *style,
                base_font,
                base_color,
                Some(md),
                None,
            ));
            continue;
        }
        display.push_str(&line[src.clone()]);
        map.extend(src.clone());
        // Split the segment at any diagnostic edges falling inside it, so the
        // covered pieces get a spell-check squiggle (W6 lines kept their markers
        // hidden but were dropping these underlines).
        let mut edges = vec![src.start, src.end];
        for d in diagnostics.iter().filter(|d| d.range.start < d.range.end) {
            for e in [d.range.start, d.range.end] {
                if e > src.start && e < src.end {
                    edges.push(e);
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        for w in edges.windows(2) {
            let (a, b) = (w[0], w[1]);
            let under = diagnostics
                .iter()
                .any(|d| d.range.start <= a && a < d.range.end);
            runs.push(run_for(
                b - a,
                *style,
                base_font,
                base_color,
                Some(md),
                under.then_some(squiggle),
            ));
        }
    }
    map.push(line.len());
    (display, runs, map)
}

/// Scan the whole document line by line for inline markdown constructs.
fn scan(text: &str, st: &SyntaxStyle) -> Vec<Span> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut line_start = 0;
    for i in 0..=bytes.len() {
        if i == bytes.len() || bytes[i] == b'\n' {
            scan_line(text, line_start, i, st, &mut out);
            line_start = i + 1;
        }
    }
    out
}

/// The always-hidden formatting-marker PAIRS on a line, as
/// `(opening range, closing range)` per construct — for the editor's
/// Cditor-style delete (skip the invisible bytes; collapse an emptied pair).
pub(crate) fn fmt_marker_pairs(line: &str, st: &SyntaxStyle) -> Vec<(Range<usize>, Range<usize>)> {
    let mut spans = scan(line, st);
    spans.sort_by_key(|s| s.range.start);
    // The scanner stamps each construct's opener + closer with a shared
    // `pair_id`, so pairing survives NESTED constructs between them (the old
    // adjacency walk assumed at most one body span).
    let mut open: Vec<(u16, Range<usize>)> = Vec::new();
    let mut pairs = Vec::new();
    for sp in spans.into_iter().filter(|s| s.style.always_hide) {
        let id = sp.style.pair_id;
        if id == 0 {
            continue;
        }
        match open.iter().position(|(oid, _)| *oid == id) {
            Some(k) => {
                let (_, o) = open.remove(k);
                pairs.push((o, sp.range));
            }
            None => open.push((id, sp.range)),
        }
    }
    pairs.sort_by_key(|(o, _)| o.start);
    pairs
}

fn marker(out: &mut Vec<Span>, range: Range<usize>, color: Hsla) {
    out.push(Span {
        range,
        style: Style {
            color: Some(color),
            hide: true,
            ..Default::default()
        },
    });
}

/// A formatting marker that NEVER reveals (not even with the caret inside its
/// construct) — see [`Style::always_hide`]. The construct's opener and closer
/// share `pair_id` (see [`fmt_marker_pairs`]).
/// `0xRRGGBBAA` (from `syntax::css_color`) as a gpui color.
fn hsla_of(rgba: u32) -> Hsla {
    gpui::rgba(rgba).into()
}

fn fmt_marker(out: &mut Vec<Span>, range: Range<usize>, color: Hsla, pair_id: u16) {
    out.push(Span {
        range,
        style: Style {
            color: Some(color),
            hide: true,
            always_hide: true,
            pair_id,
            ..Default::default()
        },
    });
}

/// Recursively scan a formatting construct's body: inner constructs nest
/// (bold inside italic, code inside bold, …) with `style` layered under them,
/// and the gaps — the construct's own plain text — carry `style` itself.
/// Depth-capped; at the cap the body is one flat styled span.
#[allow(clippy::too_many_arguments)]
fn scan_styled_body(
    text: &str,
    s: usize,
    e: usize,
    st: &SyntaxStyle,
    out: &mut Vec<Span>,
    style: Style,
    depth: u8,
    next_id: &mut u16,
) {
    if s >= e {
        return;
    }
    if depth >= 3 {
        push(out, s..e, style);
        return;
    }
    let before = out.len();
    scan_inline(text, s, e, st, out, &style, depth + 1, next_id);
    // Fill the gaps between the inner constructs with the body style.
    let mut covered: Vec<Range<usize>> = out[before..].iter().map(|sp| sp.range.clone()).collect();
    covered.sort_by_key(|r| r.start);
    let mut pos = s;
    let mut fills: Vec<Range<usize>> = Vec::new();
    for r in covered {
        if r.start > pos {
            fills.push(pos..r.start);
        }
        pos = pos.max(r.end);
    }
    if pos < e {
        fills.push(pos..e);
    }
    for f in fills {
        push(out, f, style.clone());
    }
}

/// Heading styling (`#`..`######` + a space) for `text[from..end]`: dim the
/// marker, bold the rest. The larger heading SIZE is applied per line at
/// layout time (variable line heights), not here — so the rest of the line
/// isn't scanned for inline constructs in W2. (W2) Returns whether it matched,
/// so callers can `return` and skip inline scanning the same as a bare
/// heading line. Shared by the line-start check and the post-list-marker
/// check (`- ### Heading` nests like the reading view's AST does).
fn apply_heading(
    text: &str,
    from: usize,
    end: usize,
    st: &SyntaxStyle,
    out: &mut Vec<Span>,
) -> bool {
    let Some(level) = heading_level(&text[from..end]) else {
        return false;
    };
    let b = text.as_bytes();
    let mut marker_end = from + level as usize;
    if marker_end < end && b[marker_end] == b' ' {
        marker_end += 1;
    }
    marker(out, from..marker_end, st.marker);
    if marker_end < end {
        push(
            out,
            marker_end..end,
            Style {
                bold: true,
                color: st.heading,
                ..Default::default()
            },
        );
    }
    true
}

/// Scan one line `text[start..end]`. Markers are ASCII, so byte scanning is
/// UTF-8-safe (an ASCII byte never appears inside a multi-byte char).
fn scan_line(text: &str, start: usize, end: usize, st: &SyntaxStyle, out: &mut Vec<Span>) {
    let b = text.as_bytes();
    // An Obsidian block-id anchor at the line's end (` ^some-id`) is addressing,
    // not content: a marker span dims it and W6 hides it (reveal-on-caret).
    // A referenced block instead paints a small count badge in the anchor's
    // place (Logseq-style; the anchor hitbox/click emit `refs:^id`).
    // Hidden BEFORE the heading fast-path — heading lines carry anchors too
    // (`### Notes ^id` used to show the raw tail).
    let end = match zorite_markdown::syntax::block_id(&text[start..end]) {
        Some((at, id)) => {
            let refs = st.block_ref_count.as_ref().map_or(0, |f| f(id));
            if refs > 0 {
                let badge = format!(" {}", zorite_markdown::syntax::superscript(refs));
                out.push(Span {
                    range: start + at..end,
                    style: Style {
                        color: Some(st.tag),
                        hide: true,
                        replace: Some(SharedString::from(badge)),
                        ..Default::default()
                    },
                });
            } else {
                marker(out, start + at..end, st.marker);
            }
            start + at
        }
        None => end,
    };
    if apply_heading(text, start, end, st, out) {
        return;
    }
    // Blockquote: leading `>` (GFM nesting) + optional spaces. Hide the markers;
    // the body keeps inline styling over a muted base color (set by the caller).
    let mut i = start;
    if b.get(start) == Some(&b'>') {
        let mut p = start;
        while p < end && b[p] == b'>' {
            p += 1;
            if p < end && b[p] == b' ' {
                p += 1;
            }
        }
        // A GitHub alert's `[!NOTE]`-style marker hides with the quote prefix —
        // the paint draws a bold colored label in its place (LineMark::Alert).
        if let Some((_, mlen, _)) = alert_prefix(&text[p..end]) {
            p += mlen;
        }
        marker(out, start..p, st.marker);
        i = p;
    } else if let Some(prefix_len) = task_prefix(&text[start..end])
        .map(|(p, ..)| p)
        .or_else(|| list_prefix(&text[start..end]).map(|(p, ..)| p))
    {
        // List / task item: hide the leading whitespace + marker (+ checkbox); a
        // bullet/number/box is painted in its place, body keeps inline styling.
        marker(out, start..start + prefix_len, st.marker);
        i = start + prefix_len;
        // A heading right after the marker (`- ### Notes`) — same treatment as
        // a top-level heading, skipping inline scanning past it.
        if apply_heading(text, i, end, st, out) {
            return;
        }
    }
    let mut next_id = 1u16;
    scan_inline(text, i, end, st, out, &Style::default(), 0, &mut next_id);
}

/// The inline scanning loop, recursive so formatting constructs NEST: a
/// construct's body re-scans with the construct's style as `base` (layered by
/// [`Style::over`]), matching the reader's CommonMark nesting — `**bold
/// *italic* bold**` styles the inner run bold+italic. `start` is the segment
/// start (word-boundary and image-bang checks are segment-local).
#[allow(clippy::too_many_arguments)]
fn scan_inline(
    text: &str,
    start: usize,
    end: usize,
    st: &SyntaxStyle,
    out: &mut Vec<Span>,
    base: &Style,
    depth: u8,
    next_id: &mut u16,
) {
    let b = text.as_bytes();
    let mut i = start;
    while i < end {
        let c = b[i];
        // A backslash escape (`\*`, `\|`, `\\`, … — any ASCII punctuation):
        // the backslash is a hideable marker (the reader consumes it), the
        // escaped character is literal content — skip it so no construct
        // opens on it. Reveal-on-caret shows the backslash for editing.
        if c == b'\\'
            && i + 1 < end
            && b[i + 1].is_ascii_punctuation()
            && !is_backslash_escaped(b, i)
        {
            marker(out, i..i + 1, st.marker);
            i += 2;
            continue;
        }
        // Inline code: `code` — backticks are hideable markers, the body a
        // highlight (code color on a tint, body font) matching the reading
        // view. CommonMark run matching: an opening run of N backticks closes
        // at the next run of EXACTLY N, so ``a`b`` keeps its inner backtick
        // (backslashes are literal INSIDE a span; only the opener can be
        // escaped).
        if c == b'`' && !is_backslash_escaped(b, i) {
            let mut n = 1;
            while i + n < end && b[i + n] == b'`' {
                n += 1;
            }
            let mut j = i + n;
            let mut close = None;
            while j < end {
                if b[j] == b'`' {
                    let mut m = 1;
                    while j + m < end && b[j + m] == b'`' {
                        m += 1;
                    }
                    if m == n {
                        close = Some(j);
                        break;
                    }
                    j += m;
                } else {
                    j += 1;
                }
            }
            if let Some(close) = close {
                let id = *next_id;
                *next_id += 1;
                fmt_marker(out, i..i + n, st.marker, id);
                push(
                    out,
                    i + n..close,
                    Style {
                        color: Some(st.code),
                        bg: Some(st.code_bg),
                        ..Default::default()
                    }
                    .over(base),
                );
                fmt_marker(out, close..close + n, st.marker, id);
                i = close + n;
                continue;
            }
            // No matching run: the whole backtick STRING is one token
            // (CommonMark) — skip past it so a shorter run inside it can't
            // start a span the reader wouldn't.
            i += n;
            continue;
        }
        // Bold: **text** (check before single-* italic). A `\*` (escaped)
        // opener or closer is literal text, matching the reader's CommonMark.
        if c == b'*'
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'*'
            && let Some(close) = find2_unescaped(b, i + 2, end, b'*', b'*')
        {
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..i + 2, st.marker, id);
            scan_styled_body(
                text,
                i + 2,
                close,
                st,
                out,
                Style {
                    bold: true,
                    ..Default::default()
                }
                .over(base),
                depth,
                next_id,
            );
            fmt_marker(out, close..close + 2, st.marker, id);
            i = close + 2;
            continue;
        }
        // Italic: *text* — escaped `\*` stays literal.
        if c == b'*'
            && !is_backslash_escaped(b, i)
            && let Some(close) = find1_unescaped(b, i + 1, end, b'*')
            && close > i + 1
        {
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..i + 1, st.marker, id);
            scan_styled_body(
                text,
                i + 1,
                close,
                st,
                out,
                Style {
                    italic: true,
                    ..Default::default()
                }
                .over(base),
                depth,
                next_id,
            );
            fmt_marker(out, close..close + 1, st.marker, id);
            i = close + 1;
            continue;
        }
        // Underscore emphasis (__bold__ / _italic_), CommonMark's flanking
        // rules so snake_case identifiers never light up: `_` only opens
        // after a non-word character (or the segment start) and only closes
        // before one, and a backslash-escaped `\_` (the shruggie's arms)
        // stays literal. The reader's CommonMark parser applies the same
        // word-boundary rule, so the views agree.
        if c == b'_' && (i == start || (!is_word_byte(b[i - 1]) && b[i - 1] != b'\\')) {
            // __bold__ (check before single-_ italic).
            if i + 1 < end
                && b[i + 1] == b'_'
                && i + 2 < end
                && b[i + 2] != b' '
                && let Some(close) = find_underscore_close(b, i + 2, end, true)
            {
                let id = *next_id;
                *next_id += 1;
                fmt_marker(out, i..i + 2, st.marker, id);
                scan_styled_body(
                    text,
                    i + 2,
                    close,
                    st,
                    out,
                    Style {
                        bold: true,
                        ..Default::default()
                    }
                    .over(base),
                    depth,
                    next_id,
                );
                fmt_marker(out, close..close + 2, st.marker, id);
                i = close + 2;
                continue;
            }
            if i + 1 < end
                && b[i + 1] != b'_'
                && b[i + 1] != b' '
                && let Some(close) = find_underscore_close(b, i + 1, end, false)
            {
                let id = *next_id;
                *next_id += 1;
                fmt_marker(out, i..i + 1, st.marker, id);
                scan_styled_body(
                    text,
                    i + 1,
                    close,
                    st,
                    out,
                    Style {
                        italic: true,
                        ..Default::default()
                    }
                    .over(base),
                    depth,
                    next_id,
                );
                fmt_marker(out, close..close + 1, st.marker, id);
                i = close + 1;
                continue;
            }
        }
        // Highlight: ==text== — the pairing rules are the reading view's
        // (`syntax::highlight_close`), so `a == b == c` stays literal in both.
        if c == b'='
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'='
            && let Some(close) = zorite_markdown::syntax::highlight_close(&text[..end], i)
        {
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..i + 2, st.marker, id);
            scan_styled_body(
                text,
                i + 2,
                close,
                st,
                out,
                Style {
                    bg: Some(st.mark_bg),
                    ..Default::default()
                }
                .over(base),
                depth,
                next_id,
            );
            fmt_marker(out, close..close + 2, st.marker, id);
            i = close + 2;
            continue;
        }
        // Strikethrough: ~~text~~
        if c == b'~'
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'~'
            && let Some(close) = find2_unescaped(b, i + 2, end, b'~', b'~')
        {
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..i + 2, st.marker, id);
            scan_styled_body(
                text,
                i + 2,
                close,
                st,
                out,
                Style {
                    strike: true,
                    ..Default::default()
                }
                .over(base),
                depth,
                next_id,
            );
            fmt_marker(out, close..close + 2, st.marker, id);
            i = close + 2;
            continue;
        }
        // Footnote reference: [^label] — rendered `[label]` in link color (the
        // `^` hidden), matching the reading view's resolved marker.
        if c == b'['
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'^'
            && let Some(rb) = find1(b, i + 2, end, b']')
            && rb > i + 2
        {
            push(
                out,
                i..i + 1,
                Style {
                    color: Some(st.link),
                    ..Default::default()
                }
                .over(base),
            );
            marker(out, i + 1..i + 2, st.marker); // hide the `^`
            push(
                out,
                i + 2..rb + 1,
                Style {
                    color: Some(st.link),
                    ..Default::default()
                }
                .over(base),
            );
            i = rb + 1;
            continue;
        }
        // Block ref, frontend form: `((id))` — a construct ONLY when the
        // host's resolver knows the id (so prose parens stay prose). Renders
        // as the target block's text, link-colored; raw on caret.
        if c == b'('
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'('
            && let Some(close) = find2(b, i + 2, end, b')', b')')
        {
            let id = &text[i + 2..close];
            let label = (!id.is_empty()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'))
            .then(|| st.block_label.as_ref().and_then(|f| f("", id)))
            .flatten();
            if let Some(label) = label {
                out.push(Span {
                    range: i..close + 2,
                    style: Style {
                        color: Some(st.link),
                        hide: true,
                        replace: Some(SharedString::from(label)),
                        ..Default::default()
                    },
                });
                i = close + 2;
                continue;
            }
        }
        // Wiki-link: [[Page]] (check before single-[ link).
        if c == b'['
            && !is_backslash_escaped(b, i)
            && i + 1 < end
            && b[i + 1] == b'['
            && let Some(close) = find2(b, i + 2, end, b']', b']')
        {
            marker(out, i..i + 2, st.marker);
            let link = Style {
                color: Some(st.link),
                ..Default::default()
            }
            .over(base);
            // An anchor in the target — a block ref (`[[Note#^id]]`) or a
            // heading (`[[Note#My Heading]]`) — renders its `#^`/`#` as ` → `
            // (the reader does the same), keeping the anchor text readable:
            // `Note → id`. Raw form comes back on caret like any marker; a PDF
            // page jump (`file.pdf#p3`) keeps its literal `#`. Any `|alias`
            // after it keeps the link color.
            let inner = &text[i + 2..close];
            let target_end = inner.find('|').unwrap_or(inner.len());
            let anchor = match inner[..target_end].find('#') {
                Some(a) if !inner[..a].to_ascii_lowercase().ends_with(".pdf") => {
                    let alen = if inner[a + 1..target_end].starts_with('^') {
                        2
                    } else {
                        1
                    };
                    Some((a, alen))
                }
                _ => None,
            };
            // (See also the `((id))` frontend form handled below.)
            // A resolvable `[[Page#^id]]` (no alias) displays the TARGET
            // BLOCK'S TEXT instead of `Page → id` — the whole inner span
            // swaps for the resolved label (reveal-on-caret shows the raw
            // form for editing, like any hidden construct).
            let resolved = match anchor {
                Some((a, 2)) if a > 0 && a + 2 < target_end && target_end == inner.len() => st
                    .block_label
                    .as_ref()
                    .and_then(|f| f(&inner[..a], &inner[a + 2..target_end])),
                _ => None,
            };
            match (resolved, anchor) {
                (Some(label), _) => {
                    out.push(Span {
                        range: i + 2..close,
                        style: Style {
                            color: Some(st.link),
                            hide: true,
                            replace: Some(SharedString::from(label)),
                            ..Default::default()
                        },
                    });
                }
                (None, Some((a, alen))) if a > 0 && a + alen < target_end => {
                    push(out, i + 2..i + 2 + a, link.clone());
                    out.push(Span {
                        range: i + 2 + a..i + 2 + a + alen,
                        style: Style {
                            color: Some(st.marker),
                            hide: true,
                            replace: Some(" → ".into()),
                            ..Default::default()
                        },
                    });
                    push(out, i + 2 + a + alen..close, link);
                }
                _ => push(out, i + 2..close, link),
            }
            marker(out, close..close + 2, st.marker);
            i = close + 2;
            continue;
        }
        // Link: [text](url) — or an image `![alt](src)` (the caret's own line
        // falls through to this generic scan, since the block-image widget is
        // suppressed there — W4's image_row() only handles other lines). Treated
        // the same: `text`/`alt` colored, brackets dimmed — plus the `!` for an
        // image, so an empty-alt `![](src)` hides cleanly instead of leaving a
        // bare unhidden `!`.
        if c == b'['
            && !is_backslash_escaped(b, i)
            && let Some(rb) = find1(b, i + 1, end, b']')
            && rb + 1 < end
            && b[rb + 1] == b'('
            && let Some(rp) = find1(b, rb + 2, end, b')')
        {
            let marker_start = if i > start && b[i - 1] == b'!' {
                i - 1
            } else {
                i
            };
            marker(out, marker_start..i + 1, st.marker);
            push(
                out,
                i + 1..rb,
                Style {
                    color: Some(st.link),
                    ..Default::default()
                }
                .over(base),
            );
            marker(out, rb..rp + 1, st.marker);
            i = rp + 1;
            continue;
        }
        // Reference link: [text][id] — `text` colored, brackets + `[id]` dimmed.
        // (Inline `[text](url)` is handled just above; this is the `][` form.)
        if c == b'['
            && !is_backslash_escaped(b, i)
            && let Some(rb) = find1(b, i + 1, end, b']')
            && rb + 1 < end
            && b[rb + 1] == b'['
            && let Some(rb2) = find1(b, rb + 2, end, b']')
        {
            marker(out, i..i + 1, st.marker);
            push(
                out,
                i + 1..rb,
                Style {
                    color: Some(st.link),
                    ..Default::default()
                }
                .over(base),
            );
            marker(out, rb..rb2 + 1, st.marker);
            i = rb2 + 1;
            continue;
        }
        // Styled inline HTML — `<mark>` / `<mark style="background:…">`,
        // `<span style="color:…">`, `<u>` — the tags hidden, the body styled.
        // Recognition is the reading view's (`syntax::styled_tag`), so the two
        // views can't disagree about which tags count.
        if c == b'<'
            && let Some(gt) = text[i..end].find('>')
            && let Some(tag) = zorite_markdown::syntax::styled_tag(&text[i..i + gt + 1])
            && let Some(rel) = text[i + gt + 1..end].find(tag.kind.close())
        {
            use zorite_markdown::syntax::StyledKind;
            let body = i + gt + 1;
            let close = body + rel;
            let close_len = tag.kind.close().len();
            let id = *next_id;
            *next_id += 1;
            fmt_marker(out, i..body, st.marker, id);
            let style = match tag.kind {
                StyledKind::Mark => Style {
                    bg: Some(tag.background.map_or(st.mark_bg, hsla_of)),
                    ..Default::default()
                },
                StyledKind::Span => Style {
                    color: tag.color.map(hsla_of),
                    bg: tag.background.map(hsla_of),
                    ..Default::default()
                },
                StyledKind::Underline => Style {
                    underline: true,
                    ..Default::default()
                },
            };
            scan_styled_body(text, body, close, st, out, style.over(base), depth, next_id);
            fmt_marker(out, close..close + close_len, st.marker, id);
            i = close + close_len;
            continue;
        }
        // Bare URL: colored like a link (it clicks like one — see the shared
        // `links()` grammar; `url_end` keeps styling and hit-tests identical).
        // Compare BYTES: `i` walks bytes, so a str slice here would panic
        // mid-char on any non-ASCII text (`¯\_(ツ)_/¯` did, v0.5.0 dev).
        if (b[i..end].starts_with(b"http://") || b[i..end].starts_with(b"https://"))
            && (i == start || !is_word(b[i - 1]))
        {
            let j = i + zorite_markdown::syntax::url_end(&text[i..end], 0);
            if j > i + 8 {
                push(
                    out,
                    i..j,
                    Style {
                        color: Some(st.link),
                        ..Default::default()
                    }
                    .over(base),
                );
                i = j;
                continue;
            }
        }
        // Tag: #tag (at a non-word boundary; needs at least one tag char).
        if c == b'#' && (i == start || !is_word(b[i - 1])) {
            let mut j = i + 1;
            while j < end && is_tag(b[j]) {
                j += 1;
            }
            if j > i + 1 {
                push(
                    out,
                    i..j,
                    Style {
                        color: Some(st.tag),
                        ..Default::default()
                    }
                    .over(base),
                );
                i = j;
                continue;
            }
        }
        i += 1;
    }
}

fn push(out: &mut Vec<Span>, range: Range<usize>, style: Style) {
    out.push(Span { range, style });
}

/// First index of byte `c` in `b[from..end]`.
fn find1(b: &[u8], from: usize, end: usize, c: u8) -> Option<usize> {
    (from..end).find(|&k| b[k] == c)
}

/// First UNESCAPED index of byte `c` — a `\`-escaped occurrence (odd
/// preceding backslash run) is literal text, not a marker (CommonMark).
fn find1_unescaped(b: &[u8], from: usize, end: usize, c: u8) -> Option<usize> {
    (from..end).find(|&k| b[k] == c && !is_backslash_escaped(b, k))
}

/// First index of the pair `c1 c2` in `b[from..end]`.
fn find2(b: &[u8], from: usize, end: usize, c1: u8, c2: u8) -> Option<usize> {
    (from..end.saturating_sub(1)).find(|&k| b[k] == c1 && b[k + 1] == c2)
}

/// First UNESCAPED index of the pair — see [`find1_unescaped`].
fn find2_unescaped(b: &[u8], from: usize, end: usize, c1: u8, c2: u8) -> Option<usize> {
    (from..end.saturating_sub(1))
        .find(|&k| b[k] == c1 && b[k + 1] == c2 && !is_backslash_escaped(b, k))
}

/// A "word" byte for `_`-emphasis flanking: ASCII alphanumerics, plus any
/// non-ASCII byte — accented/CJK letters count as word characters, so
/// intraword underscores stay literal (CommonMark's rule for `_`).
fn is_word_byte(x: u8) -> bool {
    x.is_ascii_alphanumeric() || x >= 0x80
}

/// The closing `_` (or `__`) for an underscore-emphasis span whose content
/// starts at `from`: the first candidate with non-empty content that doesn't
/// end in a space, that isn't backslash-escaped, and whose following
/// character is not a word character (CommonMark right-flank). `None` if the
/// span never closes.
fn find_underscore_close(b: &[u8], from: usize, end: usize, double: bool) -> Option<usize> {
    let width = if double { 2 } else { 1 };
    let mut j = from + 1; // content is at least one byte
    while j + width <= end {
        if b[j] == b'_'
            && (!double || b[j + 1] == b'_')
            && b[j - 1] != b' '
            && b[j - 1] != b'\\'
            && (j + width >= end || !is_word_byte(b[j + width]))
        {
            return Some(j);
        }
        j += 1;
    }
    None
}

// Linkables (wiki/tag/url/bare-url) are shared with the reader
// (`zorite_markdown::syntax`) — one grammar for clicks, hover cursors, and
// styling in every renderer.
pub(crate) use zorite_markdown::syntax::{LinkHit, link_at, links};

/// ATX heading depth (1–6) if `line` is a heading: 1–6 leading `#` followed by
/// a space or end-of-line. `None` otherwise.
pub(crate) fn heading_level(line: &str) -> Option<u8> {
    let b = line.as_bytes();
    let mut n = 0;
    while n < b.len() && b[n] == b'#' {
        n += 1;
    }
    ((1..=6).contains(&n) && (n == b.len() || b[n] == b' ')).then_some(n as u8)
}

/// [`heading_level`], also matching a heading behind a list marker
/// (`- ### Notes`, the outliner shape). Returns the depth plus the line's
/// indent when it's a LIST heading (`None` = a top-level heading) — list
/// headings fold by indent, not by heading boundaries.
pub(crate) fn line_heading_level(line: &str) -> Option<(u8, Option<usize>)> {
    if let Some(l) = heading_level(line) {
        return Some((l, None));
    }
    let p = list_prefix(line).map(|(p, ..)| p)?;
    let l = heading_level(&line[p..])?;
    Some((l, Some(line.len() - line.trim_start().len())))
}

/// Line ranges (heading line through section end) of collapsed heading
/// sections: `folded` holds the trimmed source lines of folded headings
/// (`## Goals`). A top-level section runs from its heading to the next
/// heading of the same or a higher level, fence-aware — mirrors
/// [`zorite_markdown::syntax::extract_section`]. A LIST heading's section
/// (`- ### Notes`) is its indented children: it runs while lines sit deeper
/// than the heading's own indent (blank lines included). Every line matching
/// a folded key folds (duplicate headings fold together; the key is the line
/// text).
pub(crate) fn heading_fold_regions(
    content: &str,
    folded: &std::collections::HashSet<String>,
) -> Vec<Range<usize>> {
    if folded.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = content.split('\n').collect();
    let indent = |l: &str| l.len() - l.trim_start().len();
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("```") {
            in_fence = !in_fence;
            i += 1;
            continue;
        }
        let level = if in_fence {
            None
        } else {
            line_heading_level(lines[i])
        };
        match level {
            Some((l, list_indent)) if folded.contains(lines[i].trim()) => {
                let start = i;
                i += 1;
                while i < lines.len() {
                    if lines[i].trim_start().starts_with("```") {
                        in_fence = !in_fence;
                    } else if !in_fence {
                        let stop = match list_indent {
                            None => heading_level(lines[i]).is_some_and(|n| n <= l),
                            Some(ind) => !lines[i].trim().is_empty() && indent(lines[i]) <= ind,
                        };
                        if stop {
                            break;
                        }
                    }
                    i += 1;
                }
                out.push(start..i);
            }
            _ => i += 1,
        }
    }
    out
}

/// True if `line` is a thematic break (horizontal rule): three or more of the
/// same `-`, `*`, or `_`, separated only by optional spaces — e.g. `---`, `***`,
/// `- - -`. The editor paints a divider in its place (reveal-on-caret). Matches
/// CommonMark, ignoring the rare setext-underline ambiguity this app doesn't use.
pub(crate) fn thematic_break(line: &str) -> bool {
    let mut chars = line.bytes().filter(|b| !b.is_ascii_whitespace());
    let Some(first) = chars.next() else {
        return false;
    };
    if !matches!(first, b'-' | b'*' | b'_') {
        return false;
    }
    let mut count = 1;
    for b in chars {
        if b != first {
            return false;
        }
        count += 1;
    }
    count >= 3
}

/// Byte length of the `[^label]:` prefix if `line` is a footnote definition, so
/// the editor can render the whole line muted (the reading view does the same).
/// `None` otherwise.
pub(crate) fn footnote_def(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("[^")?;
    let close = rest.find(']')?;
    // Label must be non-empty and the `]` immediately followed by `:`.
    (close > 0 && rest[close + 1..].starts_with(':')).then_some(2 + close + 2)
}

/// True if `line` looks like a block-level raw HTML line — `<tag …>`, `</tag>`,
/// or `<!-- … -->` at the start (after optional indentation). The editor renders
/// it muted, matching the reading view (raw HTML is shown literally, never run).
pub(crate) fn html_block(line: &str) -> bool {
    let t = line.trim_start().as_bytes();
    matches!(t, [b'<', rest, ..] if rest.is_ascii_alphabetic() || *rest == b'/' || *rest == b'!')
}

/// Byte length of a blockquote's leading marker — one or more `>` (GFM nesting),
/// each with an optional trailing space — if `line` is a blockquote. `None`
/// otherwise. The editor hides this marker (reveal-on-caret) and renders the line
/// with a muted color + a left border.
// Alert recognition is shared with the reader (`zorite_markdown::syntax`) —
// what a marker IS lives in one place; this crate only decides how to paint
// it (hide the prefix, label + colored bar, reveal on caret).
pub(crate) use zorite_markdown::syntax::{AlertKind, alert_prefix};

/// Per-kind SVG asset paths for the alert title icons.
#[derive(Clone)]
pub struct AlertIcons {
    pub note: SharedString,
    pub tip: SharedString,
    pub important: SharedString,
    pub warning: SharedString,
    pub caution: SharedString,
}

impl AlertIcons {
    /// The icon path for one alert kind.
    pub(crate) fn get(&self, kind: AlertKind) -> SharedString {
        match kind {
            AlertKind::Note => self.note.clone(),
            AlertKind::Tip => self.tip.clone(),
            AlertKind::Important => self.important.clone(),
            AlertKind::Warning => self.warning.clone(),
            AlertKind::Caution => self.caution.clone(),
        }
    }
}

impl SyntaxStyle {
    /// The themed color for one alert kind.
    pub(crate) fn alert_color(&self, kind: AlertKind) -> Hsla {
        match kind {
            AlertKind::Note => self.alert_note,
            AlertKind::Tip => self.alert_tip,
            AlertKind::Important => self.alert_important,
            AlertKind::Warning => self.alert_warning,
            AlertKind::Caution => self.alert_caution,
        }
    }
}

/// The alert kind if `body` — a blockquote line's text after its `>` prefix —
/// starts with an alert marker (see [`zorite_markdown::syntax::alert_prefix`]).
pub(crate) fn alert_kind(body: &str) -> Option<AlertKind> {
    alert_prefix(body).map(|(kind, ..)| kind)
}

/// Foldable-callout regions: for each alert whose marker carries a fold char
/// (`> [!NOTE]-` / `+`), the line range it spans (marker + its `>` continuation
/// lines, ending at a blank/non-quote line or the next alert marker) and
/// whether it's folded (`-`). Body lines of a folded region collapse in the
/// WYSIWYG view unless the caret is inside (reveal-on-caret).
pub(crate) fn alert_fold_regions(content: &str) -> Vec<(Range<usize>, bool)> {
    // `Some(fold)` when the line is an alert marker; fold = its fold char.
    fn marker_fold(line: &str) -> Option<Option<bool>> {
        let p = blockquote_prefix(line)?;
        alert_prefix(&line[p..]).map(|(_, _, fold)| fold)
    }
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(Some(folded)) = marker_fold(lines[i]) {
            let start = i;
            i += 1;
            while i < lines.len()
                && blockquote_prefix(lines[i]).is_some()
                && marker_fold(lines[i]).is_none()
            {
                i += 1;
            }
            out.push((start..i, folded));
        } else {
            i += 1;
        }
    }
    out
}

pub(crate) fn blockquote_prefix(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    if b.first() != Some(&b'>') {
        return None;
    }
    let mut p = 0;
    while p < b.len() && b[p] == b'>' {
        p += 1;
        if p < b.len() && b[p] == b' ' {
            p += 1;
        }
    }
    Some(p)
}

/// If `line` is a list item, return `(prefix_len, indent, ordered, number)`.
/// `prefix_len` is the byte length of the leading whitespace plus the marker
/// (a bullet `-`/`*`/`+`, or digits then `.`/`)`) plus one space; `indent` is the
/// leading-whitespace length (nesting depth); `ordered`/`number` describe an
/// ordered item. The editor hides this prefix and paints a bullet/number,
/// revealing the raw prefix on caret.
pub(crate) fn list_prefix(line: &str) -> Option<(usize, usize, bool, u32)> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    let indent = i;
    // Unordered: `-`/`*`/`+` then a space.
    if i < b.len() && matches!(b[i], b'-' | b'*' | b'+') && b.get(i + 1) == Some(&b' ') {
        return Some((i + 2, indent, false, 0));
    }
    // Ordered: one or more digits, then `.` or `)`, then a space.
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i > ds && matches!(b.get(i), Some(b'.') | Some(b')')) && b.get(i + 1) == Some(&b' ') {
        let num = line[ds..i].parse::<u32>().unwrap_or(1);
        return Some((i + 2, indent, true, num));
    }
    None
}

/// `(position, nesting level)` of every ordered list item, both 0-based-
/// level / 1-based-position. Word-style, NOT CommonMark: every list counts
/// from 1 (source digits are display-irrelevant), a nested list is its own
/// list, and any break — a blank line or prose — ends the open lists, so the
/// next list starts over. The level is STRUCTURAL (how many lists are open
/// above), not an indent-width guess, so it survives any spaces-per-level
/// setting. Non-items are (0, 0).
pub(crate) fn ordered_numbers(lines: &[&str]) -> Vec<(u32, usize)> {
    let mut out = vec![(0u32, 0usize); lines.len()];
    // One entry per open list level: (indent, is_ordered, next number).
    let mut stack: Vec<(usize, bool, u32)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((_, indent, ordered, _)) = list_prefix(line) else {
            // Indented text hangs under the innermost item; anything else —
            // including a blank line — ends the open lists.
            let ws = line
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            let hanging =
                !line.trim().is_empty() && stack.last().is_some_and(|&(ind, ..)| ws > ind);
            if !hanging {
                stack.clear();
            }
            continue;
        };
        while stack.last().is_some_and(|&(ind, ..)| ind > indent) {
            stack.pop();
        }
        let depth = stack.len();
        match stack.last_mut() {
            Some((ind, ord, next)) if *ind == indent => {
                if *ord != ordered {
                    // A different marker type at the same indent starts a
                    // new list.
                    *ord = ordered;
                    *next = 1;
                }
                if ordered {
                    out[i] = (*next, depth - 1);
                    *next += 1;
                }
            }
            _ => {
                stack.push((indent, ordered, 2));
                out[i] = (1, depth);
            }
        }
    }
    out
}

/// A copied slice with its ordered markers rewritten to their DISPLAYED
/// positions — still digit markdown (letters/romans are display-only), so a
/// nested block pastes starting at 1 and a continuation pastes as `3.`,
/// exactly the counting the screen showed, in any markdown app. Lines whose
/// marker sits outside the slice are copied verbatim. Positions come from
/// the WHOLE document, so a mid-list copy keeps its true numbering.
pub(crate) fn renumber_copy(content: &str, range: std::ops::Range<usize>) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let nums = ordered_numbers(&lines);
    let mut out = String::with_capacity(range.len());
    let mut line_start = 0;
    for (i, line) in lines.iter().enumerate() {
        let line_end = line_start + line.len();
        // The slice of this line (excluding its newline) inside the range.
        let seg_start = range.start.max(line_start);
        let seg_end = range.end.min(line_end);
        if seg_start <= seg_end && range.start <= line_end && range.end >= line_start {
            let rel = (seg_start - line_start)..(seg_end - line_start);
            let marker = list_prefix(line).filter(|&(_, _, ordered, _)| ordered);
            match marker {
                // Rewrite only when the whole marker is inside the slice.
                Some((plen, indent, _, _))
                    if rel.start == 0 && rel.end >= plen && nums[i].0 > 0 =>
                {
                    out.push_str(&line[..indent]);
                    out.push_str(&nums[i].0.to_string());
                    // Keep the source's `.`/`)` punctuation and the body.
                    let digits_end = indent
                        + line.as_bytes()[indent..]
                            .iter()
                            .take_while(|b| b.is_ascii_digit())
                            .count();
                    out.push_str(&line[digits_end..rel.end]);
                }
                _ => out.push_str(&line[rel]),
            }
        }
        // The newline between this line and the next, when it's in range.
        if range.start <= line_end && line_end < range.end && i + 1 < lines.len() {
            out.push('\n');
        }
        line_start = line_end + 1;
    }
    out
}

/// If `line` is a GFM task item — a list item whose body starts with `[ ]`,
/// `[x]`, or `[X]` then a space — return `(prefix_len, indent, checked)`, where
/// `prefix_len` covers the list marker plus the checkbox. The editor hides this
/// prefix and paints a ☐/☑ box (parity with the reading view).
pub(crate) fn task_prefix(line: &str) -> Option<(usize, usize, bool)> {
    let (list_len, indent, ..) = list_prefix(line)?;
    let rest = &line.as_bytes()[list_len..];
    if rest.len() >= 4
        && rest[0] == b'['
        && rest[2] == b']'
        && rest[3] == b' '
        && matches!(rest[1], b' ' | b'x' | b'X')
    {
        return Some((list_len + 4, indent, matches!(rest[1], b'x' | b'X')));
    }
    None
}

/// Toggle a GFM task item's checkbox in `line` — flip the char between the
/// brackets (`[ ]`↔`[x]`). Returns the rewritten line, or `None` if `line` isn't
/// a task item. The length is unchanged (one ASCII byte swapped).
pub(crate) fn toggle_task_checkbox(line: &str) -> Option<String> {
    let (prefix_len, _indent, checked) = task_prefix(line)?;
    let box_byte = prefix_len - 3; // the char between `[` and `]`
    let mut out = line.to_string();
    out.replace_range(box_byte..box_byte + 1, if checked { " " } else { "x" });
    Some(out)
}

/// If `line` is a standalone image — `![alt](src)`, optionally followed by a
/// `{width=N}` / `{width=Npx}` attribute, with only whitespace around it —
/// return `(src, explicit_width)`. The editor renders such a line as the image
/// (W4) when the caret is elsewhere; a line with any other trailing text stays
/// plain source.
pub(crate) fn image_line(line: &str) -> Option<(&str, Option<f32>)> {
    let rest = line.trim().strip_prefix("![")?;
    let close_alt = rest.find("](")?;
    let after_alt = &rest[close_alt + 2..];
    let close_src = after_alt.find(')')?;
    let src = after_alt[..close_src].trim();
    let tail = after_alt[close_src + 1..].trim();
    let width = if tail.is_empty() {
        None
    } else {
        let w = tail.strip_prefix("{width=")?.strip_suffix('}')?;
        Some(
            w.strip_suffix("px")
                .unwrap_or(w)
                .trim()
                .parse::<f32>()
                .ok()?,
        )
    };
    (!src.is_empty()).then_some((src, width))
}

/// Like [`image_line`], but also matches an image that is the sole body of a
/// list item — `- ![](src)` / `1. ![](src){width=N}`. Returns `(src, width,
/// marker_len)`, where `marker_len` is the byte length of the leading list
/// marker (0 for a plain standalone image). The editor renders the image inset
/// past the marker so a bulleted image keeps its bullet, instead of the row
/// collapsing to the image or falling back to raw source.
pub(crate) fn image_row(line: &str) -> Option<(&str, Option<f32>, usize)> {
    if let Some((src, width)) = image_line(line) {
        return Some((src, width, 0));
    }
    let (plen, ..) = list_prefix(line)?;
    let (src, width) = image_line(&line[plen..])?;
    Some((src, width, plen))
}

/// Font-size multiplier for a line — larger for headings (matching the reading
/// view's scale), 1.0 for body text. Drives the editor's variable line heights.
/// A heading right after a list/task marker (`- ### Notes`) counts too, same
/// as [`apply_heading`].
pub(crate) fn line_scale(line: &str) -> f32 {
    let body = list_prefix(line).map_or(line, |(p, ..)| &line[p..]);
    match heading_level(body) {
        Some(1) => 1.8,
        Some(2) => 1.5,
        Some(3) => 1.3,
        Some(4) => 1.15,
        Some(5) => 1.05,
        _ => 1.0,
    }
}

/// Fenced ` ```mermaid ` blocks: each entry is `(line_range, source)` — the line
/// range covering both fences (so it can collapse), and the diagram source (the
/// lines between the fences, joined). Used to render the block as a diagram.
pub(crate) fn mermaid_blocks(content: &str) -> Vec<(Range<usize>, String)> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if t.starts_with("```") && t[3..].trim() == "mermaid" {
            let start = i;
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                j += 1;
            }
            let source = lines[start + 1..j].join("\n");
            let end = (j + 1).min(lines.len()); // include the closing fence
            out.push((start..end, source));
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// `$$…$$` math blocks: each entry is `(line_range, source)` — the range covering both
/// `$$` fence lines (so it collapses) and the LaTeX between them. The fences are bare
/// `$$` lines (markdown's `math_flow` form, no info word).
pub(crate) fn math_blocks(content: &str) -> Vec<(Range<usize>, String)> {
    math_regions(content)
        .into_iter()
        .map(|r| (r.range, r.source))
        .collect()
}

/// Horizontal alignment of a display `$$…$$` block, chosen per-block via a
/// `<!-- math:left -->` / `<!-- math:right -->` marker comment on the line directly above it.
/// `Center` is the default (no marker), matching LaTeX display math; standard Markdown viewers
/// ignore the comment.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MathAlign {
    Left,
    #[default]
    Center,
    Right,
}

impl MathAlign {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    /// The marker line for this alignment, or `None` for the default (`Center`) — which is
    /// stored as no marker, keeping centered math (the common case) clean.
    pub(crate) fn marker(self) -> Option<&'static str> {
        match self {
            Self::Center => None,
            Self::Left => Some("<!-- math:left -->"),
            Self::Right => Some("<!-- math:right -->"),
        }
    }
}

/// Parse a `<!-- math:ALIGN -->` marker line. `None` if it isn't one.
pub(crate) fn math_align_marker(line: &str) -> Option<MathAlign> {
    let inner = line
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    MathAlign::from_name(inner.strip_prefix("math:")?.trim())
}

/// A detected `$$…$$` block: its line range (both fences), the LaTeX between them, its
/// alignment, and the optional `<!-- math:ALIGN -->` marker line directly above it.
#[derive(Clone)]
pub(crate) struct MathRegion {
    pub range: Range<usize>,
    pub source: String,
    pub align: MathAlign,
    pub marker_line: Option<usize>,
}

pub(crate) fn math_regions(content: &str) -> Vec<MathRegion> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut in_code = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("```") {
            // `$$` inside a fenced code block (shell PIDs!) is never math.
            in_code = !in_code;
            i += 1;
            continue;
        }
        if in_code {
            i += 1;
            continue;
        }
        if lines[i].trim() == "$$" {
            let start = i;
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim() != "$$" {
                j += 1;
            }
            let source = lines[start + 1..j].join("\n");
            let end = (j + 1).min(lines.len()); // include the closing fence
            // An alignment marker on the line directly above the opening fence.
            let (align, marker_line) = match start
                .checked_sub(1)
                .map(|m| (m, math_align_marker(lines[m])))
            {
                Some((m, Some(a))) => (a, Some(m)),
                _ => (MathAlign::default(), None),
            };
            out.push(MathRegion {
                range: start..end,
                source,
                align,
                marker_line,
            });
            i = end;
        } else if let Some(inner) = one_line_math(lines[i]) {
            // `$$x^2$$` ALONE on a line is display math (issue #54): block
            // treatment (size, centering, alignment marker). Lines mixing
            // words with a pair are normalized apart at edit time (see
            // `apply_auto_replace`) — never hidden.
            let (align, marker_line) =
                match i.checked_sub(1).map(|m| (m, math_align_marker(lines[m]))) {
                    Some((m, Some(a))) => (a, Some(m)),
                    _ => (MathAlign::default(), None),
                };
            out.push(MathRegion {
                range: i..i + 1,
                source: inner.to_string(),
                align,
                marker_line,
            });
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The inner LaTeX when the whole (trimmed) line is one `$$…$$` pair with
/// non-empty content — the one-line display-math form.
fn one_line_math(line: &str) -> Option<&str> {
    let t = line.trim();
    let inner = t.strip_prefix("$$")?.strip_suffix("$$")?;
    let inner = inner.trim();
    // `$$$$`, `$$ $$`, and interior `$$`s (two pairs jammed together) are
    // not the one-line form.
    (!inner.is_empty() && !inner.contains("$$")).then_some(inner)
}

/// Inline `$…$` math spans within a single text line (NOT block `$$` fences) — byte ranges
/// covering the whole span, both `$` delimiters included. Follows the common (pandoc) rule so
/// prose like "it cost $5 and $10" isn't mistaken for math: the opening `$` is followed by a
/// non-space, the closing `$` is preceded by a non-space and not followed by a digit, the
/// content is non-empty, `$$` is skipped (block-fence-ish / empty), and `\$` is a literal.
///
/// `$` and `\` are ASCII bytes that can't occur inside a multi-byte UTF-8 sequence, so the
/// byte scan is char-safe and the returned ranges fall on char boundaries.
/// Every inline `![alt](src)` image on `line`, as `(full span, src range)`.
/// A whole-line image is handled as a block widget (`image_row`) before a line
/// reaches inline shaping, so these are the mixed-line (text + image) ones.
/// Images inside inline code aren't matched.
pub(crate) fn inline_image_spans(line: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'!'
            && b[i + 1] == b'['
            && let Some(rb) = line[i + 2..].find(']')
            && line[i + 2 + rb + 1..].starts_with('(')
            && let Some(rp) = line[i + 2 + rb + 2..].find(')')
        {
            let src = (i + 2 + rb + 2)..(i + 2 + rb + 2 + rp);
            out.push((i..(src.end + 1), src.clone()));
            i = src.end + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The LaTeX inside an inline-math `span` of `line` — strips one `$` per
/// side, or two for a same-line `$$…$$` display pair.
pub(crate) fn inline_math_latex<'a>(line: &'a str, span: &Range<usize>) -> &'a str {
    let d = if line[span.clone()].starts_with("$$") {
        2
    } else {
        1
    };
    &line[span.start + d..span.end - d]
}

pub(crate) fn inline_math_spans(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' || is_backslash_escaped(bytes, i) {
            i += 1;
            continue;
        }
        // `$$…$$` on ONE line is display math (issue #54; block `$$` fences
        // live on their own lines and never form a same-line pair). Unlike
        // `$…$` there are no space rules — pandoc display math allows them —
        // but the pair must close on this line around non-space content.
        if bytes.get(i + 1) == Some(&b'$') {
            let mut j = i + 2;
            let mut close = None;
            while j + 1 < bytes.len() {
                if bytes[j] == b'$' && bytes[j + 1] == b'$' && !is_backslash_escaped(bytes, j) {
                    close = Some(j);
                    break;
                }
                j += 1;
            }
            match close {
                Some(c) if line[i + 2..c].trim().is_empty() => {}
                Some(c) => {
                    out.push(i..c + 2);
                    i = c + 2;
                    continue;
                }
                None => {}
            }
            i += 2;
            continue;
        }
        // Opening `$`: reject a space right after.
        match bytes.get(i + 1) {
            None => {
                i += 1;
                continue;
            }
            Some(c) if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            _ => {}
        }
        // Scan for a valid closing `$`: unescaped, non-space before, not a digit after. A `$`
        // that can't close (space before, e.g. the second `$` in "$5 and $10") is skipped and
        // the scan continues for a later valid one, as pandoc does.
        let mut j = i + 1;
        let mut close = None;
        while j < bytes.len() {
            if bytes[j] == b'$' && !is_backslash_escaped(bytes, j) {
                let space_before = bytes[j - 1].is_ascii_whitespace();
                let digit_after = bytes.get(j + 1).is_some_and(|c| c.is_ascii_digit());
                if !space_before && !digit_after {
                    close = Some(j);
                    break;
                }
            }
            j += 1;
        }
        match close {
            Some(c) => {
                out.push(i..c + 1);
                i = c + 1;
            }
            None => i += 1,
        }
    }
    out
}

/// Where one inline formula sits in a shaped line: the byte offset of its spacer in the DISPLAY
/// string (to position the painted image) and the SOURCE byte range of the `$…$` span (to
/// hit-test a click back to the formula and to target its edit/commit).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineMathPlace {
    pub display_off: usize,
    /// Byte length of the spacer. Needed to place the raster on an RTL row,
    /// where `display_off` is the spacer's RIGHT edge, not its left.
    pub len: usize,
    pub source: Range<usize>,
}

/// The sub-slice of `runs` covering display byte range `[lo, hi)`, clipping the runs that
/// straddle an edge. `runs` must tile the display in order (as `hidden_runs` emits them).
fn clip_runs(runs: &[TextRun], lo: usize, hi: usize) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut off = 0;
    for r in runs {
        let (s, e) = (off, off + r.len);
        off = e;
        let (cs, ce) = (s.max(lo), e.min(hi));
        if cs < ce {
            out.push(TextRun {
                len: ce - cs,
                ..r.clone()
            });
        }
    }
    out
}

/// Replace each inline formula's `$…$` glyphs with an invisible spacer the caller paints the
/// typeset image over. Takes `hidden_runs` output (`display`, `runs`, `map` — the display→source
/// byte map of length `display.len() + 1`) and the line's `formulas` as `(source range, spacer
/// width in spaces)`, sorted by source start and non-overlapping. Returns the rewritten
/// `display` / `runs` / `map` plus each formula's placement. The spacer borrows `gap`'s font/
/// color (its glyphs are spaces, so only the advance matters) and maps back to the span start,
/// so a click on it lands the caret at the formula (which then reveals raw source / opens the
/// editor).
pub(crate) fn splice_inline_math(
    display: &str,
    runs: &[TextRun],
    map: &[usize],
    formulas: &[(Range<usize>, usize)],
    gap: &TextRun,
) -> (String, Vec<TextRun>, Vec<usize>, Vec<InlineMathPlace>) {
    let mut nd = String::with_capacity(display.len());
    let mut nm: Vec<usize> = Vec::with_capacity(map.len());
    let mut nr: Vec<TextRun> = Vec::new();
    let mut places = Vec::new();
    let mut cursor = 0; // display byte offset consumed so far
    for (src, n) in formulas {
        // The display range covering this span: first display offset reaching the span's
        // start, up to the first reaching its end (`map` is monotonic non-decreasing).
        let d0 = map
            .iter()
            .position(|&s| s >= src.start)
            .unwrap_or(display.len());
        let d1 = map
            .iter()
            .position(|&s| s >= src.end)
            .unwrap_or(display.len());
        if d0 < cursor {
            continue; // overlapping / out-of-order formula — skip defensively
        }
        // Verbatim text before the formula.
        nd.push_str(&display[cursor..d0]);
        nm.extend_from_slice(&map[cursor..d0]);
        nr.extend(clip_runs(runs, cursor, d0));
        // The spacer, mapped back to the span start.
        places.push(InlineMathPlace {
            display_off: nd.len(),
            len: *n,
            source: src.clone(),
        });
        let src_at = map.get(d0).copied().unwrap_or(src.start);
        for _ in 0..*n {
            nd.push(' ');
        }
        nm.extend(std::iter::repeat_n(src_at, *n));
        nr.push(TextRun {
            len: *n,
            ..gap.clone()
        });
        cursor = d1;
    }
    // Tail after the last formula.
    nd.push_str(&display[cursor..]);
    nm.extend_from_slice(&map[cursor..display.len()]);
    nr.extend(clip_runs(runs, cursor, display.len()));
    nm.push(*map.last().unwrap_or(&0)); // final source-end sentinel
    (nd, nr, nm, places)
}

/// Per-column text alignment of a GFM table.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

/// The shared table-style enum — recognition lives in `zorite_markdown::syntax`
/// (the sanctioned dependency), including the marker's `cols=` widths.
pub(crate) use zorite_markdown::syntax::TableStyle;

/// Parse a `<!-- table:STYLE -->` marker line into its [`TableStyle`]. `None` if
/// the line isn't a recognized table-style marker (so an unknown marker stays a
/// plain HTML comment).
pub(crate) fn table_style_marker(line: &str) -> Option<TableStyle> {
    zorite_markdown::syntax::table_style_marker(line)
}

/// A detected GFM table region: the half-open range of logical line indices it
/// spans (header, separator, then body rows) and its per-column alignment, plus
/// an optional `<!-- table:STYLE -->` marker line directly above it.
#[derive(Clone)]
pub(crate) struct TableRegion {
    pub lines: Range<usize>,
    pub aligns: Vec<Align>,
    pub style: TableStyle,
    /// Index of the style-marker comment line above the header, if present — the
    /// editor hides it (revealed only when the caret lands on it).
    pub marker_line: Option<usize>,
    /// Explicit column widths (logical px) from the marker's `cols=` attribute
    /// — written by drag-to-resize; `None` = content-measured.
    pub col_widths_attr: Option<Vec<f32>>,
    /// The table reads right-to-left (#66), so its grid hugs the note column's
    /// RIGHT edge with the 22px gutter on the other side — the reader's
    /// `.mr(22).ml_auto()`. Column ORDER is not mirrored; see [`EditorState::table_left`].
    pub rtl: bool,
}

/// Detect GFM table regions in `content` (W4c). A region is a row line (trimmed
/// text starts with `|`) immediately followed by a separator row
/// (`| --- | :--: |`), then any further row lines. Returns regions in order.
pub(crate) fn table_regions(content: &str) -> Vec<TableRegion> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if is_table_row(lines[i])
            && let Some(aligns) = separator_aligns(lines[i + 1])
        {
            let start = i;
            let mut end = i + 2; // header + separator
            // Stop before a new table jammed directly below: a row that is itself
            // immediately followed by a `|---|` separator is the next table's
            // header, not a body row of this one — otherwise adjacent tables (no
            // blank line between) merge into one grid and the second separator
            // shows up as `---` cells.
            while end < lines.len()
                && is_table_row(lines[end])
                && separator_aligns(lines.get(end + 1).copied().unwrap_or("")).is_none()
            {
                end += 1;
            }
            // A `<!-- table:STYLE -->` comment on the line directly above sets the
            // table's visual style (and is hidden by the editor).
            let (style, marker_line) = match start
                .checked_sub(1)
                .map(|m| (m, table_style_marker(lines[m])))
            {
                Some((m, Some(s))) => (s, Some(m)),
                _ => (TableStyle::Grid, None),
            };
            let col_widths_attr =
                marker_line.and_then(|m| zorite_markdown::syntax::table_col_widths(lines[m]));
            // Writing direction of the whole table (#66) — the reader runs
            // `base_direction` over the table's byte span, so join the region's
            // lines and ask the same question: the first strong character
            // anywhere in the table decides (pipes, dashes and digits are
            // neutral under UAX #9). Once per scan, not per frame.
            let rtl =
                zorite_markdown::syntax::base_direction(&lines[start..end].join("\n")).is_rtl();
            out.push(TableRegion {
                lines: start..end,
                aligns,
                style,
                marker_line,
                col_widths_attr,
                rtl,
            });
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// A table row is a line whose trimmed text starts with `|`.
pub(crate) fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// Contiguous runs of `key:: value` property lines (Obsidian/Logseq-style
/// metadata) — each renders as a two-column panel, the WYSIWYG twin of the
/// reader's `render_property_table`. A run is one or more adjacent property
/// lines; any non-property line ends it. A leading list marker is tolerated
/// (`- key:: value`, the Logseq props-only-block shape — the reader's parser
/// consumes the marker and panels it, so the editor matches). Fenced code is
/// skipped so a `Type::method()` code line isn't mistaken for a property.
/// Returns line-index ranges in order.
pub(crate) fn property_regions(content: &str) -> Vec<Range<usize>> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("```") {
            in_fence = !in_fence;
            i += 1;
            continue;
        }
        if !in_fence && zorite_markdown::syntax::prefixed_property(lines[i]).is_some() {
            let start = i;
            i += 1;
            while i < lines.len()
                && !lines[i].trim_start().starts_with("```")
                && zorite_markdown::syntax::prefixed_property(lines[i]).is_some()
            {
                i += 1;
            }
            out.push(start..i);
        } else {
            i += 1;
        }
    }
    out
}

/// Split a `| a | b |` row into trimmed cell strings (the bounding pipes drop the
/// empty leading/trailing cells they'd otherwise create).
pub(crate) fn table_cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    split_unescaped_pipes(t).map(str::trim).collect()
}

/// Split `s` at its UNESCAPED `|` bytes — a `\|` (odd run of preceding
/// backslashes) is literal cell content per GFM, not a cell separator. The
/// escape stays in the returned slices (cells are verbatim source), so cell
/// ranges and caret math keep their exact source mapping, and a rewrite that
/// re-joins the cells round-trips the escape.
fn split_unescaped_pipes(s: &str) -> impl Iterator<Item = &str> {
    let b = s.as_bytes();
    let mut cuts = vec![0usize];
    for (i, &c) in b.iter().enumerate() {
        if c == b'|' && !is_backslash_escaped(b, i) {
            cuts.push(i + 1);
        }
    }
    cuts.push(s.len() + 1);
    (0..cuts.len() - 1).map(move |k| &s[cuts[k]..cuts[k + 1] - 1])
}

/// Whether the byte at `i` is escaped: preceded by an ODD number of `\`s.
fn is_backslash_escaped(b: &[u8], i: usize) -> bool {
    let mut n = 0;
    while n < i && b[i - 1 - n] == b'\\' {
        n += 1;
    }
    n % 2 == 1
}

/// The byte range of each cell's *trimmed content* within `line` (line-local), in
/// the same order as [`table_cells`]. Lets the editor place the caret inside a
/// rendered cell and hit-test a click back to a source offset. An empty cell is a
/// zero-width range at its content position.
pub(crate) fn table_cell_ranges(line: &str) -> Vec<Range<usize>> {
    let (Some(first), Some(last)) = (line.find('|'), line.rfind('|')) else {
        return Vec::new();
    };
    if last <= first {
        return Vec::new();
    }
    let base = first + 1; // start of the inter-pipe region in `line`
    let inner = &line[base..last];
    let mut out = Vec::new();
    let mut seg = 0; // offset of the current cell within `inner`
    for piece in split_unescaped_pipes(inner) {
        let lead = piece.len() - piece.trim_start().len();
        let trail = piece.len() - piece.trim_end().len();
        let start = base + seg + lead;
        out.push(start..(base + seg + piece.len() - trail).max(start));
        seg += piece.len() + 1; // + the `|`
    }
    out
}

/// If `line` is a table separator row (every cell is dashes with optional
/// alignment colons), return its per-column alignment, else `None`.
fn separator_aligns(line: &str) -> Option<Vec<Align>> {
    if !is_table_row(line) {
        return None;
    }
    let cells = table_cells(line);
    if cells.is_empty() {
        return None;
    }
    cells
        .iter()
        .map(|c| {
            let left = c.starts_with(':');
            let right = c.ends_with(':');
            let dashes = c.trim_matches(':');
            (!dashes.is_empty() && dashes.bytes().all(|b| b == b'-')).then_some(
                match (left, right) {
                    (true, true) => Align::Center,
                    (false, true) => Align::Right,
                    _ => Align::Left,
                },
            )
        })
        .collect()
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

// One tag grammar with the reader (namespaced `#a/b` included).
use zorite_markdown::syntax::is_tag_char as is_tag;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_renumber_to_displayed_positions() {
        // Source digits are stale (7., 9.) and the nested list starts at 3 —
        // the copy carries what the SCREEN showed: 1,2 nested from 1.
        let doc = "1. a\n7. b\n  3. c\n  9. d\n5. e";
        assert_eq!(
            renumber_copy(doc, 0..doc.len()),
            "1. a\n2. b\n  1. c\n  2. d\n3. e"
        );
        // A mid-list copy keeps its true positions (paste renders 2,3).
        let from = doc.find("7.").unwrap();
        assert_eq!(
            renumber_copy(doc, from..doc.len()),
            "2. b\n  1. c\n  2. d\n3. e"
        );
        // A partial first line (marker outside the slice) is verbatim.
        let from = doc.find("b").unwrap();
        assert_eq!(
            renumber_copy(doc, from..doc.find('c').unwrap() + 1),
            "b\n  1. c"
        );
        // Non-list text passes through untouched.
        assert_eq!(renumber_copy("plain\ntext", 0..10), "plain\ntext");
    }

    #[test]
    fn selection_reveal_shows_inline_but_not_prefix() {
        let (font, c, st) = (Font::default(), Hsla::default(), test_style());
        // A selected list line: the `- ` prefix stays hidden behind its
        // painted bullet — and formatting markers stay hidden too now
        // (always_hide wins over the selection reveal; structural markers
        // like links still come back).
        let (disp, _, _) = hidden_runs("- a **b** c", &font, c, &[], None, 0, 2, true, &st);
        assert_eq!(disp, "a b c");
        let (disp, _, _) = hidden_runs("- [t](u) c", &font, c, &[], None, 0, 2, true, &st);
        assert_eq!(disp, "[t](u) c");
        // Same line unselected: everything hidden.
        let (disp, _, _) = hidden_runs("- a **b** c", &font, c, &[], None, 0, 2, false, &st);
        assert_eq!(disp, "a b c");
    }

    /// The styled substrings a line scans to, as `(text, bold, italic)` —
    /// markers excluded (they carry a color, not bold/italic).
    fn emphasis_spans(line: &str) -> Vec<(String, bool, bool)> {
        let st = test_style();
        let mut out = Vec::new();
        scan_line(line, 0, line.len(), &st, &mut out);
        out.iter()
            .filter(|sp| sp.style.bold || sp.style.italic)
            .map(|sp| {
                (
                    line[sp.range.clone()].to_string(),
                    sp.style.bold,
                    sp.style.italic,
                )
            })
            .collect()
    }

    #[test]
    fn underscore_emphasis_at_word_boundaries() {
        assert_eq!(
            emphasis_spans("_italic_ and __bold__"),
            vec![
                ("italic".to_string(), false, true),
                ("bold".to_string(), true, false)
            ]
        );
        assert_eq!(
            emphasis_spans("mid _span here_ end"),
            vec![("span here".to_string(), false, true)]
        );
    }

    #[test]
    fn underscores_inside_words_stay_plain() {
        // The reason `_` was originally skipped — identifiers must not light up.
        assert_eq!(emphasis_spans("snake_case_name"), vec![]);
        assert_eq!(emphasis_spans("a foo_bar_baz b"), vec![]);
        assert_eq!(emphasis_spans("CONST_MAX_VALUE = 3"), vec![]);
        // Closing candidate followed by a word character doesn't close.
        assert_eq!(emphasis_spans("_a_b"), vec![]);
        // Space-adjacent content never opens/closes.
        assert_eq!(emphasis_spans("_ nope_"), vec![]);
        assert_eq!(emphasis_spans("_nope _"), vec![]);
    }

    #[test]
    fn escaped_underscores_stay_literal() {
        // The shruggie's arms are backslash-escaped underscores.
        assert_eq!(emphasis_spans(r"¯\_(ツ)_/¯"), vec![]);
        assert_eq!(emphasis_spans(r"\_not italic\_"), vec![]);
    }

    #[test]
    fn double_equals_highlight_hides_markers_and_tints_the_body() {
        let text = "an ==important== word, a == b == c";
        let st = test_style();
        let mut out = Vec::new();
        scan_line(text, 0, text.len(), &st, &mut out);
        let open = 3;
        let body = open + 2;
        let close = text.find("== word").unwrap();
        assert!(out.iter().any(|s| s.range == (open..body) && s.style.hide));
        assert!(
            out.iter()
                .any(|s| s.range == (close..close + 2) && s.style.hide)
        );
        assert!(
            out.iter()
                .any(|s| s.range.start == body && s.style.bg == Some(st.mark_bg))
        );
        // The spaced `==`s after it are prose: nothing hidden past the closer.
        assert!(
            !out.iter()
                .any(|s| s.range.start > close + 2 && s.style.hide)
        );
    }

    #[test]
    fn colored_mark_and_span_tags_style_their_body() {
        let text =
            r#"a <mark style="background:#ff0000">hi</mark> <span style="color:#00ff00">go</span>"#;
        let st = test_style();
        let mut out = Vec::new();
        scan_line(text, 0, text.len(), &st, &mut out);
        let hi = text.find("hi<").unwrap();
        let go = text.find("go<").unwrap();
        assert!(
            out.iter()
                .any(|s| s.range.start == hi && s.style.bg == Some(hsla_of(0xff0000ff)))
        );
        assert!(
            out.iter()
                .any(|s| s.range.start == go && s.style.color == Some(hsla_of(0x00ff00ff)))
        );
        // Both tag pairs are hidden markers.
        assert!(
            out.iter()
                .any(|s| s.range.start == 2 && s.range.end == hi && s.style.hide)
        );
        assert!(
            out.iter()
                .any(|s| s.range == (hi + 2..hi + 9) && s.style.hide)
        );
    }

    #[test]
    fn underline_tags_hide_and_style_the_body() {
        let text = "an <u>underlined</u> word";
        let st = test_style();
        let mut out = Vec::new();
        scan_line(text, 0, text.len(), &st, &mut out);
        // Tags are hidden markers; the body span carries the underline.
        let open = text.find("<u>").unwrap();
        let body = open + 3;
        assert!(out.iter().any(|s| s.range == (open..body) && s.style.hide));
        assert!(
            out.iter()
                .any(|s| s.range.start == body && s.style.underline)
        );
    }

    #[test]
    fn scan_line_survives_multibyte_text() {
        // Regression: byte-wise scanning once str-sliced at continuation
        // bytes and panicked on this exact line.
        let text = "- I don't know what is wrong ¯\\_(ツ)_/¯ https://a.io";
        let st = test_style();
        let mut out = Vec::new();
        scan_line(text, 0, text.len(), &st, &mut out);
        // The trailing URL still gets recognized (spans cover it).
        assert!(
            out.iter()
                .any(|s| s.range.start == text.find("https").unwrap())
        );
    }

    /// Helper: the substrings the spans cover, for readable assertions.
    fn spans(line: &str) -> Vec<&str> {
        inline_math_spans(line)
            .into_iter()
            .map(|r| &line[r])
            .collect()
    }

    #[test]
    fn inline_math_basic() {
        assert_eq!(spans("the area $\\pi r^2$ of a circle"), vec!["$\\pi r^2$"]);
        assert_eq!(spans("$x$"), vec!["$x$"]);
        assert_eq!(spans("a $x$ and $y$ b"), vec!["$x$", "$y$"]);
        // Spaces are fine *inside* a span; only the immediate inner edges must be non-space.
        assert_eq!(spans("$x = 5$ done"), vec!["$x = 5$"]);
    }

    #[test]
    fn inline_math_rejects_money_and_empties() {
        assert!(spans("it cost $5 and $10 total").is_empty());
        assert!(spans("$ x $").is_empty()); // space right after opener / before closer
        assert!(spans("a $$ b").is_empty()); // `$$` is not an inline span
        assert!(spans("lone $ dollar").is_empty());
        // Closer immediately followed by a digit isn't a closer (pandoc rule).
        assert!(spans("$x$5").is_empty());
    }

    #[test]
    fn inline_math_escapes_and_later_close() {
        // `\$` is a literal dollar, not a delimiter.
        assert!(spans("price is 5\\$ even").is_empty());
        assert_eq!(spans("\\$ and $x$"), vec!["$x$"]);
        // A `$` that can't close is skipped; a later valid one closes the span.
        assert_eq!(spans("$a $5 b$ end"), vec!["$a $5 b$"]);
    }

    /// A plain text run of `len` bytes (style is irrelevant to the splice; only `len` matters).
    fn run(len: usize) -> TextRun {
        TextRun {
            len,
            font: gpui::font("Helvetica"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    /// Total bytes the runs cover — must always equal the display length.
    fn runs_len(runs: &[TextRun]) -> usize {
        runs.iter().map(|r| r.len).sum()
    }

    #[test]
    fn splice_one_formula() {
        // "a $x$ b" with the `$x$` span (bytes 2..5) → a 3-space spacer. display == source here,
        // so the map is the identity (length 8).
        let display = "a $x$ b";
        let map: Vec<usize> = (0..=display.len()).collect();
        let (nd, nr, nm, places) =
            splice_inline_math(display, &[run(display.len())], &map, &[(2..5, 3)], &run(0));
        assert_eq!(nd, "a     b"); // "a " + 3 spaces + " b" = a + 5 spaces + b
        assert_eq!(
            places,
            vec![InlineMathPlace {
                display_off: 2,
                len: 3,
                source: 2..5
            }]
        );
        assert_eq!(nm, vec![0, 1, 2, 2, 2, 5, 6, 7]); // spacer bytes map to the span start (2)
        assert_eq!(nm.len(), nd.len() + 1);
        assert_eq!(runs_len(&nr), nd.len()); // runs still tile the display
    }

    #[test]
    fn splice_two_formulas_and_leading() {
        // Two spans, the first at the very start: "$a$ and $b$".
        let display = "$a$ and $b$";
        let map: Vec<usize> = (0..=display.len()).collect();
        let (nd, nr, nm, places) = splice_inline_math(
            display,
            &[run(display.len())],
            &map,
            &[(0..3, 2), (8..11, 4)],
            &run(0),
        );
        assert_eq!(nd, "   and     "); // 2 spaces + " and " + 4 spaces
        assert_eq!(
            places,
            vec![
                InlineMathPlace {
                    display_off: 0,
                    len: 2,
                    source: 0..3
                },
                InlineMathPlace {
                    display_off: 7,
                    len: 4,
                    source: 8..11
                },
            ]
        );
        assert_eq!(nm.len(), nd.len() + 1);
        assert_eq!(runs_len(&nr), nd.len());
        assert_eq!(*nm.last().unwrap(), display.len()); // sentinel preserved
    }

    #[test]
    fn splice_no_formulas_is_identity() {
        let display = "plain text";
        let map: Vec<usize> = (0..=display.len()).collect();
        let (nd, nr, nm, places) =
            splice_inline_math(display, &[run(display.len())], &map, &[], &run(0));
        assert_eq!(nd, display);
        assert_eq!(nm, map);
        assert!(places.is_empty());
        assert_eq!(runs_len(&nr), nd.len());
    }

    #[test]
    fn inline_math_multibyte_safe() {
        // A multi-byte char (é, —) adjacent to a delimiter counts as non-space; ranges stay
        // on char boundaries.
        let line = "café $x$ — π";
        assert_eq!(spans(line), vec!["$x$"]);
        for r in inline_math_spans(line) {
            assert!(line.is_char_boundary(r.start) && line.is_char_boundary(r.end));
        }
    }

    #[test]
    fn one_line_math_regions() {
        // `$$x$$` alone on a line is a display-math REGION (block treatment);
        // so is a single complete pair with words around it (see
        // `forgiving_math_forms`).
        let r = math_regions("before\n$$x^2$$\nafter");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].range, 1..2);
        assert_eq!(r[0].source, "x^2");
        // The alignment marker above applies, as with fenced blocks.
        let r = math_regions("<!-- math:right -->\n$$x$$");
        assert_eq!(r[0].marker_line, Some(0));
        // Degenerate forms are not regions.
        assert!(math_regions("$$$$").is_empty());
        assert!(math_regions("$$ $$").is_empty());
    }

    #[test]
    fn forgiving_math_forms() {
        // Words mixed with math are NORMALIZED apart (edit-time), never
        // hidden: mixed lines are not regions themselves.
        assert!(math_regions("words $$E=mc^2$$ more").is_empty());
        assert!(math_regions("fees are $$5 and $$10 total").is_empty());
        assert!(math_regions("wer $$\nu(m)=v(n,x)\n$$ wer").is_empty());
        // `$$` inside a fenced code block is never math.
        assert!(math_regions("```sh\necho $$\n$$\n```").is_empty());
    }

    #[test]
    fn inline_display_math_double_dollar() {
        // Same-line `$$…$$` is display math (issue #54): one span covering
        // both delimiter pairs, spaces inside allowed.
        assert_eq!(spans("$$E=mc^2$$"), vec!["$$E=mc^2$$"]);
        assert_eq!(spans("a $$ x + y $$ b"), vec!["$$ x + y $$"]);
        assert_eq!(inline_math_latex("$$E=mc^2$$", &(0..10)), "E=mc^2");
        assert_eq!(inline_math_latex("$x$", &(0..3)), "x");
        // Unclosed / empty pairs match nothing; a lone `$$` fence line is
        // the BLOCK scanner's business.
        assert_eq!(spans("$$"), Vec::<&str>::new());
        assert_eq!(spans("$$$$"), Vec::<&str>::new());
        assert_eq!(spans("$$x"), Vec::<&str>::new());
        // A `$$` pair doesn't swallow a later single-dollar span.
        assert_eq!(spans("$$a$$ then $b$"), vec!["$$a$$", "$b$"]);
    }

    #[test]
    fn detects_a_gfm_table() {
        let md = "intro\n\n| A | B |\n| --- | :-: |\n| 1 | 2 |\n| 3 | 4 |\n\nafter";
        let regions = table_regions(md);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].lines, 2..6); // header, separator, 2 body rows
        assert_eq!(regions[0].aligns, vec![Align::Left, Align::Center]);
    }

    #[test]
    fn mermaid_block_extraction() {
        let md = "intro\n\n```mermaid\ngraph TD\nA --> B\n```\n\nafter";
        let blocks = mermaid_blocks(md);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, 2..6); // ```mermaid(2), graph(3), A-->B(4), ```(5)
        assert_eq!(blocks[0].1, "graph TD\nA --> B");
        // A plain ``` block is not mermaid.
        assert!(mermaid_blocks("```rust\nfn x() {}\n```").is_empty());
        // Trailing-space lang still matches.
        assert_eq!(mermaid_blocks("```mermaid \npie\n```").len(), 1);
    }

    #[test]
    fn cells_split_and_trim() {
        assert_eq!(table_cells("| a | b |"), vec!["a", "b"]);
        assert_eq!(table_cells("|  |  |"), vec!["", ""]);
    }

    #[test]
    fn escaped_pipes_are_cell_content() {
        // GFM: `\|` is a literal pipe in the cell, not a separator — the
        // reader (GFM parser) agrees; a blind split miscounted cells and
        // column rewrites then corrupted the row.
        assert_eq!(
            table_cells(r"| left \| right | ok |"),
            vec![r"left \| right", "ok"]
        );
        // A double backslash escapes the backslash — that pipe IS a separator.
        assert_eq!(table_cells(r"| a \\| b |"), vec![r"a \\", "b"]);
        // Ranges walk the same split: two ranges, first spans the escape.
        let line = r"| left \| right | ok |";
        let ranges = table_cell_ranges(line);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&line[ranges[0].clone()], r"left \| right");
        assert_eq!(&line[ranges[1].clone()], "ok");
    }

    #[test]
    fn separator_required_and_alignment() {
        // Pipes in prose without a separator row are not a table.
        assert!(table_regions("a | b\nc | d").is_empty());
        assert!(table_regions("| not a table\njust text").is_empty());
        // Right alignment from `---:`.
        assert_eq!(
            table_regions("| h |\n| ---: |\n| x |")[0].aligns,
            vec![Align::Right]
        );
    }

    #[test]
    fn table_style_marker_and_region() {
        assert_eq!(
            table_style_marker("<!-- table:striped -->"),
            Some(TableStyle::Striped)
        );
        assert_eq!(
            table_style_marker("<!--table:minimal-->"),
            Some(TableStyle::Minimal)
        );
        assert_eq!(table_style_marker("<!-- table:bogus -->"), None);
        assert_eq!(table_style_marker("<!-- a comment -->"), None);
        assert_eq!(table_style_marker("| a | b |"), None);

        // The marker above a table sets its style + is recorded (not in `lines`).
        let r = table_regions("<!-- table:header -->\n| h |\n| --- |\n| x |");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].style, TableStyle::Header);
        assert_eq!(r[0].marker_line, Some(0));
        assert_eq!(r[0].lines, 1..4);

        // No marker → default Grid.
        let g = table_regions("| h |\n| --- |\n| x |");
        assert_eq!(g[0].style, TableStyle::Grid);
        assert_eq!(g[0].marker_line, None);
    }

    #[test]
    fn adjacent_tables_split_into_two_regions() {
        // Two tables jammed together with no blank line between must not merge: the
        // second table's header + `|---|` start a new region (otherwise the second
        // separator rendered as `---` cells in one merged grid).
        let r = table_regions(
            "| a | b |\n| --- | --- |\n| 1 | 2 |\n| c | d |\n| --- | --- |\n| 3 | 4 |",
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].lines, 0..3);
        assert_eq!(r[1].lines, 3..6);
    }

    #[test]
    fn heading_and_image_line() {
        assert_eq!(heading_level("## Hi"), Some(2));
        assert_eq!(heading_level("#notaheading"), None);
        assert_eq!(image_line("![a](b.png)"), Some(("b.png", None)));
        assert_eq!(
            image_line("![a](b.png){width=320}"),
            Some(("b.png", Some(320.0)))
        );
        assert_eq!(image_line("text ![a](b.png)"), None);
        // A list item whose sole body is an image renders too (its bullet stays).
        assert_eq!(
            image_row("- ![a](b.png){width=452}"),
            Some(("b.png", Some(452.0), 2))
        );
        assert_eq!(image_row("1. ![a](b.png)"), Some(("b.png", None, 3)));
        assert_eq!(image_row("![a](b.png)"), Some(("b.png", None, 0)));
        assert_eq!(image_row("- text ![a](b.png)"), None);
    }

    #[test]
    fn link_at_hits_every_linkable_construct() {
        use LinkHit::*;
        let line = "see [[Ops Net|the net]] and #scada plus [docs](https://x.io/d) `#not` [[]]";
        // Anywhere on the wiki link (brackets included) → its target, not the alias.
        assert_eq!(link_at(line, 4), Some(Page("Ops Net".into())));
        assert_eq!(link_at(line, 15), Some(Page("Ops Net".into())));
        // The tag, including its `#`.
        assert_eq!(link_at(line, 28), Some(Page("scada".into())));
        assert_eq!(link_at(line, 33), Some(Page("scada".into())));
        // The inline link: label, brackets, or url all navigate.
        assert_eq!(link_at(line, 41), Some(Url("https://x.io/d".into())));
        assert_eq!(link_at(line, 55), Some(Url("https://x.io/d".into())));
        // Plain text, a tag inside code, and an empty wiki link are not links.
        assert_eq!(link_at(line, 0), None);
        assert_eq!(link_at(line, 66), None); // inside `#not`
        assert_eq!(link_at(line, 72), None); // [[]]
        // An image is a widget, not a link — even on its url.
        assert_eq!(link_at("![alt](images/a.png)", 10), None);
        // A footnote ref is styled like a link but isn't one.
        assert_eq!(link_at("x [^1] y", 3), None);
    }

    #[test]
    fn heading_nested_in_list_item() {
        // A heading right after a list marker scales like a top-level heading
        // (`- ### Hi` nests like the reading view's AST does), not like plain text.
        assert_eq!(line_scale("### Hi"), 1.3);
        assert_eq!(line_scale("- ### Hi"), 1.3);
        assert_eq!(line_scale("  - ## Hi"), 1.5);
        assert_eq!(line_scale("1. # Hi"), 1.8);
        // Plain list text (no heading) is unaffected.
        assert_eq!(line_scale("- not a heading"), 1.0);

        // Both the list marker and the heading marker are hidden, leaving just
        // the heading text — same as a bare "## Hi" line.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, ..) = hidden_runs("- ### Notes", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "Notes");
    }

    fn test_style() -> SyntaxStyle {
        let c = hsla(0., 0., 0.5, 1.);
        SyntaxStyle {
            block_label: None,
            block_label_gen: 0,
            block_ref_count: None,
            marker: c,
            code: c,
            code_bg: c,
            link: c,
            tag: c,
            quote: c,
            alert_note: c,
            alert_tip: c,
            alert_important: c,
            alert_warning: c,
            alert_caution: c,
            alert_icons: None,
            rule: c,
            mark_bg: c,
            popover_bg: c,
            popover_border: c,
            popover_fg: c,
            popover_hover: c,
            popover_divider: c,
            popover_danger: c,
            mono: gpui::font("monospace"),
            property_icon: None,
        }
    }

    #[test]
    fn thematic_break_detection() {
        for s in ["---", "***", "___", "- - -", "  ---  ", "----"] {
            assert!(thematic_break(s), "{s:?} should be a rule");
        }
        for s in ["--", "**", "", "text", "---x", "- -- text", "===", "> ---"] {
            assert!(!thematic_break(s), "{s:?} should not be a rule");
        }
    }

    #[test]
    fn footnote_def_and_html_block() {
        assert_eq!(footnote_def("[^1]: a note"), Some(5)); // `[^1]:`
        assert_eq!(footnote_def("[^note]:x"), Some(8)); // `[^note]:`
        assert_eq!(footnote_def("[^1] not a def"), None);
        assert_eq!(footnote_def("[^]: empty label"), None);
        assert_eq!(footnote_def("plain"), None);

        assert!(html_block("<div>"));
        assert!(html_block("</section>"));
        assert!(html_block("<!-- comment -->"));
        assert!(html_block("  <span>indented"));
        assert!(!html_block("< 5 items"));
        assert!(!html_block("text <b>inline</b>"));
    }

    #[test]
    fn hidden_runs_removes_markers_and_maps_back() {
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();

        // "**bold**" → "bold"; display 0 maps to source 2, end (4) to 8.
        let (disp, _, map) = hidden_runs("**bold**", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "bold");
        assert_eq!(map.len(), disp.len() + 1);
        assert_eq!(map[0], 2);
        assert_eq!(map[4], 8);

        // "## Hi" → "Hi"; the `## ` prefix is gone, display 0 maps to source 3.
        let (disp, _, map) = hidden_runs("## Hi", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "Hi");
        assert_eq!(map[0], 3);
        assert_eq!(map[2], 5);

        // No markers → unchanged, identity map.
        let (disp, _, map) = hidden_runs("plain text", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "plain text");
        assert_eq!(map, (0..=10).collect::<Vec<_>>());
    }

    #[test]
    fn hidden_runs_image_link_hides_the_bang_too() {
        // `![](src)` (empty alt — the shape a fresh drop/paste inserts) must hide
        // the leading `!` along with the rest of the markers, not leave it as a
        // bare visible character (the editor falls back to this generic inline
        // scan on the caret's own image line, where the block-image widget is
        // suppressed).
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, ..) = hidden_runs("![](images/a.png)", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "");

        // A named alt still shows it, same as a plain link.
        let (disp, ..) = hidden_runs(
            "![alt](images/a.png)",
            &font,
            c,
            &[],
            None,
            0,
            0,
            false,
            &st,
        );
        assert_eq!(disp, "alt");
    }

    #[test]
    fn hidden_runs_squiggle_diagnostics() {
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();

        // A diagnostic on the bold body ("bold" at source 2..6) underlines the
        // whole display string — squiggles survive marker hiding (W6).
        let (disp, runs, _) = hidden_runs(
            "**bold**",
            &font,
            c,
            &[Diagnostic { range: 2..6 }],
            None,
            0,
            0,
            false,
            &st,
        );
        assert_eq!(disp, "bold");
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), disp.len());
        assert!(runs.iter().all(|r| r.underline.is_some()));

        // A partial diagnostic splits the segment: only "text" (6..10) squiggles.
        let (disp, runs, _) = hidden_runs(
            "plain text",
            &font,
            c,
            &[Diagnostic { range: 6..10 }],
            None,
            0,
            0,
            false,
            &st,
        );
        assert_eq!(disp, "plain text");
        let underlined: usize = runs
            .iter()
            .filter(|r| r.underline.is_some())
            .map(|r| r.len)
            .sum();
        assert_eq!(underlined, 4); // "text"
    }

    #[test]
    fn hidden_runs_reveals_only_caret_construct() {
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();

        let line = "**bold** *it*";
        // Formatting markers never reveal — not even under the caret
        // (Cditor-style; see `formatting_markers_never_reveal`).
        let (disp, _, _) = hidden_runs(line, &font, c, &[], Some(4), 0, 0, false, &st);
        assert_eq!(disp, "bold it");
        // Caret in plain text reveals nothing either.
        let (disp, _, _) = hidden_runs("a **b**", &font, c, &[], Some(0), 0, 0, false, &st);
        assert_eq!(disp, "a b");
        // No caret on the line → fully hidden (W6).
        let (disp, _, _) = hidden_runs(line, &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "bold it");
        // STRUCTURAL constructs keep the per-construct caret reveal: the
        // caret inside a link shows its brackets/URL; outside, hidden.
        let link = "a [t](u) b";
        let (disp, _, _) = hidden_runs(link, &font, c, &[], Some(3), 0, 0, false, &st);
        assert_eq!(disp, "a [t](u) b");
        let (disp, _, _) = hidden_runs(link, &font, c, &[], Some(0), 0, 0, false, &st);
        assert_eq!(disp, "a t b");
    }

    #[test]
    fn alert_kinds_detected() {
        assert_eq!(alert_kind("[!NOTE]"), Some(AlertKind::Note));
        assert_eq!(alert_kind(" [!CAUTION] "), Some(AlertKind::Caution));
        // Lenient form: body text on the marker line.
        assert_eq!(alert_kind("[!NOTE] text after"), Some(AlertKind::Note));
        for s in ["[!note]", "[!NOTEXT]", "note", ""] {
            assert_eq!(alert_kind(s), None, "{s:?}");
        }
    }

    #[test]
    fn blockquote_prefix_and_hidden() {
        assert_eq!(blockquote_prefix("> quote"), Some(2));
        assert_eq!(blockquote_prefix(">no space"), Some(1));
        assert_eq!(blockquote_prefix(">> nested"), Some(3));
        assert_eq!(blockquote_prefix("not a quote"), None);

        // The `> ` marker hides; inline styling in the body still applies.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, map) = hidden_runs("> a **b**", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "a b"); // "> " + "**" hidden
        assert_eq!(map[0], 2); // display 0 ('a') ← source 2

        // With the prefix revealed (caret on the line) the `> ` shows even when
        // the caret isn't in it; inline markers still hide unless under the caret.
        let (disp, _, _) = hidden_runs("> a **b**", &font, c, &[], Some(3), 2, 0, false, &st);
        assert_eq!(disp, "> a b");
    }

    #[test]
    fn backtick_runs_match_commonmark() {
        let st = test_style();
        // ``a`b`` — a double-backtick span keeps its inner backtick as body.
        let line = "``a`b`` x";
        let mut out = Vec::new();
        scan_line(line, 0, line.len(), &st, &mut out);
        assert!(
            out.iter()
                .any(|s| s.range == (2..5) && s.style.bg.is_some()),
            "body a`b styled as code"
        );
        // An unclosed run stays plain (no half-matched span).
        let mut out = Vec::new();
        scan_line("``a` x", 0, 6, &st, &mut out);
        assert!(out.iter().all(|s| s.style.bg.is_none()));
    }

    #[test]
    fn backslash_escapes_stay_literal() {
        let st = test_style();
        // \*text\* must NOT style — the reader shows literal asterisks.
        for line in [
            r"\*text\*",
            r"a \`x\` b",
            r"\~~s\~~",
            r"\[[Page]]",
            r"\[t](u)",
        ] {
            let mut out = Vec::new();
            scan_line(line, 0, line.len(), &st, &mut out);
            // Escape backslashes become (hidden) marker spans; nothing may
            // carry actual STYLING.
            assert!(
                out.iter().all(|s| !s.style.bold
                    && !s.style.italic
                    && !s.style.strike
                    && s.style.bg.is_none()
                    && (s.style.hide || s.style.color.is_none())),
                "{line:?} styled: {:?}",
                out.iter().map(|s| s.range.clone()).collect::<Vec<_>>()
            );
        }
        // An escaped closer doesn't close: **a \** stays unstyled (no pair),
        // while the reader treats it the same.
        let mut out = Vec::new();
        let line = r"**a \** x";
        scan_line(line, 0, line.len(), &st, &mut out);
        assert!(out.iter().all(|s| !s.style.bold), "escaped closer ignored");
    }

    #[test]
    fn escape_backslash_hides_like_the_reader() {
        // The reader consumes the `\` of an escape — the editor hides it the
        // same way (a marker span), leaving the escaped char as content.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, _) = hidden_runs(r"\*text\*", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "*text*");
        // `\\` shows a single backslash.
        let (disp, _, _) = hidden_runs(r"a \\ b", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, r"a \ b");
        // CommonMark: \**bold** = literal *, italic "bold", literal *.
        let (disp, _, _) = hidden_runs(r"\**bold**", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "*bold*");
    }

    #[test]
    fn nested_inline_styles_layer() {
        let st = test_style();
        // **bold *italic* bold** — the inner run carries BOTH styles, the
        // gaps around it stay bold (the reader nests the same way).
        let line = "**bold *it* bold**";
        let mut out = Vec::new();
        scan_line(line, 0, line.len(), &st, &mut out);
        let styled = |s: usize, e: usize| out.iter().find(|sp| sp.range == (s..e)).unwrap();
        let inner = styled(8, 10); // "it"
        assert!(inner.style.bold && inner.style.italic);
        let gap = styled(2, 7); // "bold " before the italic
        assert!(gap.style.bold && !gap.style.italic);
        // Inline code inside bold keeps its tint and inherits the weight.
        let line = "**b `c` d**";
        let mut out = Vec::new();
        scan_line(line, 0, line.len(), &st, &mut out);
        let code = out
            .iter()
            .find(|sp| sp.range == (5..6))
            .expect("code body span");
        assert!(code.style.bold && code.style.bg.is_some());
        // Display hides every marker, nested included.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let (disp, _, _) = hidden_runs("**bold *it* bold**", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "bold it bold");
        // Pairing survives nesting: two pairs, outer first.
        let pairs = fmt_marker_pairs("**bold *it* bold**", &st);
        assert_eq!(pairs, vec![(0..2, 16..18), (7..8, 10..11)]);
    }

    #[test]
    fn fmt_marker_pairs_stay_per_construct() {
        let st = test_style();
        // Adjacent constructs pair up separately — a delete between them must
        // never fuse one's closer with the other's opener.
        assert_eq!(
            fmt_marker_pairs("**a**~~b~~", &st),
            vec![(0..2, 3..5), (5..7, 8..10)]
        );
        // An empty pair (post-delete state) is still a pair.
        assert_eq!(fmt_marker_pairs("****", &st), vec![(0..2, 2..4)]);
        // Structural markers (links) aren't formatting pairs.
        assert_eq!(fmt_marker_pairs("[t](u)", &st), vec![]);
        assert_eq!(fmt_marker_pairs("<u>x</u>", &st), vec![(0..3, 4..8)]);
    }

    #[test]
    fn formatting_markers_never_reveal() {
        // Cditor-style: even with the caret INSIDE the construct, formatting
        // markers stay hidden (the styling is the feedback; toggles edit it).
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        for (line, disp) in [
            ("**bold** x", "bold x"),
            ("*it* x", "it x"),
            ("~~s~~ x", "s x"),
            ("`c` x", "c x"),
            ("<u>u</u> x", "u x"),
            ("<mark>m</mark> x", "m x"),
        ] {
            let caret_inside = Some(3.min(line.len() - 1));
            let (d, _, _) = hidden_runs(line, &font, c, &[], caret_inside, 0, 0, false, &st);
            assert_eq!(d, disp, "{line:?}");
        }
        // Structural constructs keep the caret reveal: a wiki link's brackets
        // show while the caret is inside it.
        let (d, _, _) = hidden_runs("[[Page]] x", &font, c, &[], Some(4), 0, 0, false, &st);
        assert_eq!(d, "[[Page]] x");
    }

    #[test]
    fn ordered_numbers_word_style() {
        // A nested list counts on its own from 1 (whatever the digits say)
        // one level deeper, and the outer list resumes after it. The level
        // is structural — ANY deeper indent nests, whatever the tab setting.
        assert_eq!(
            ordered_numbers(&["1. a", "2. b", "  3. c", "  9. d", "6. e"]),
            vec![(1, 0), (2, 0), (1, 1), (2, 1), (3, 0)]
        );
        // Source digits are display-irrelevant; every list starts at 1.
        assert_eq!(ordered_numbers(&["5. a", "9. b"]), vec![(1, 0), (2, 0)]);
        // A bullet at the same indent ends the ordered run (bullets still
        // occupy a level for anything nested under them).
        assert_eq!(
            ordered_numbers(&["1. a", "- b", "7. c"]),
            vec![(1, 0), (0, 0), (1, 0)]
        );
        // Any break — blank line or prose — starts numbering over.
        assert_eq!(
            ordered_numbers(&["1. a", "", "2. b"]),
            vec![(1, 0), (0, 0), (1, 0)]
        );
        assert_eq!(
            ordered_numbers(&["1. a", "prose", "7. b"]),
            vec![(1, 0), (0, 0), (1, 0)]
        );
        // A hanging indent (wrapped text under an item) keeps the list open.
        assert_eq!(
            ordered_numbers(&["1. a", "   wrapped", "7. b"]),
            vec![(1, 0), (0, 0), (2, 0)]
        );
    }

    #[test]
    fn list_prefix_detection() {
        assert_eq!(list_prefix("- item"), Some((2, 0, false, 0)));
        assert_eq!(list_prefix("* item"), Some((2, 0, false, 0)));
        assert_eq!(list_prefix("+ item"), Some((2, 0, false, 0)));
        assert_eq!(list_prefix("  - nested"), Some((4, 2, false, 0)));
        assert_eq!(list_prefix("3. third"), Some((3, 0, true, 3)));
        assert_eq!(list_prefix("  10) tenth"), Some((6, 2, true, 10)));
        // Not lists: italic, a bare dash, indented prose.
        assert_eq!(list_prefix("*italic*"), None);
        assert_eq!(list_prefix("-no space"), None);
        assert_eq!(list_prefix("  just indented"), None);

        // The marker hides; a nested item maps the body back past the prefix.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, map) = hidden_runs("  - hi", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "hi");
        assert_eq!(map[0], 4); // display 0 ('h') ← source 4
    }

    #[test]
    fn task_prefix_detection() {
        assert_eq!(task_prefix("- [ ] todo"), Some((6, 0, false)));
        assert_eq!(task_prefix("- [x] done"), Some((6, 0, true)));
        assert_eq!(task_prefix("- [X] done"), Some((6, 0, true)));
        assert_eq!(task_prefix("  - [ ] nested"), Some((8, 2, false)));
        // A plain list item is not a task.
        assert_eq!(task_prefix("- item"), None);

        // The `- [ ] ` prefix hides entirely; body maps back past it.
        let font = gpui::font("Helvetica");
        let c = hsla(0., 0., 0., 1.);
        let st = test_style();
        let (disp, _, map) = hidden_runs("- [x] go", &font, c, &[], None, 0, 0, false, &st);
        assert_eq!(disp, "go");
        assert_eq!(map[0], 6); // display 0 ('g') ← source 6
    }

    #[test]
    fn toggle_task_checkbox_flips() {
        assert_eq!(
            toggle_task_checkbox("- [ ] todo").as_deref(),
            Some("- [x] todo")
        );
        assert_eq!(
            toggle_task_checkbox("- [x] done").as_deref(),
            Some("- [ ] done")
        );
        assert_eq!(
            toggle_task_checkbox("- [X] done").as_deref(),
            Some("- [ ] done")
        );
        assert_eq!(
            toggle_task_checkbox("  - [ ] nested").as_deref(),
            Some("  - [x] nested")
        );
        assert_eq!(toggle_task_checkbox("- plain"), None);
    }

    #[test]
    fn alert_fold_regions_span_the_quote_block() {
        let src = "> [!NOTE]- hidden\n> body\n> more\nprose\n> [!TIP]+ open\n> b\n> [!NOTE] plain";
        let r = alert_fold_regions(src);
        // Folded NOTE spans its quote lines; open TIP's region ends at the
        // plain (non-foldable) alert marker, which starts no region itself.
        assert_eq!(r, vec![(0..3, true), (4..6, false)]);
    }

    #[test]
    fn heading_fold_regions_span_their_section() {
        let src = "# Top\nbody\n## Sub\nsub body\n```\n# not a heading\n```\n## Next\nx";
        let folded: std::collections::HashSet<String> = ["## Sub".to_string()].into();
        // `## Sub` folds through the fence (the `#` inside is code) up to
        // `## Next`; nothing else folds.
        assert_eq!(heading_fold_regions(src, &folded), vec![2..7]);
        // Folding `# Top` swallows everything (no later h1).
        let folded: std::collections::HashSet<String> = ["# Top".to_string()].into();
        assert_eq!(heading_fold_regions(src, &folded), vec![0..9]);
        assert!(heading_fold_regions(src, &Default::default()).is_empty());
        // A LIST heading folds its indented children (blanks included) and
        // stops at the next sibling — not at unrelated later headings.
        let src = "* ### Notes: ^abc\n  - one\n\n  - two\n* sibling\n# After";
        let folded: std::collections::HashSet<String> = ["* ### Notes: ^abc".to_string()].into();
        assert_eq!(heading_fold_regions(src, &folded), vec![0..4]);
    }

    #[test]
    fn property_regions_group_and_skip_code() {
        // Two adjacent property lines form one region; prose breaks it.
        let r = property_regions("attendees:: Bob\ntime:: 3pm\n\nprose\nowner:: Sue");
        assert_eq!(r, vec![0..2, 4..5]);
        // A `Type::method()` line inside a code fence isn't a property.
        let r2 = property_regions("```rust\nFoo::bar()\n```\nkey:: v");
        assert_eq!(r2, vec![3..4]);
        // A list-marker property line (Logseq props-only block) counts too;
        // an ordinary bullet doesn't.
        let r3 = property_regions("- status:: open\n  time:: 3pm\n- plain bullet");
        assert_eq!(r3, vec![0..2]);
    }
}
