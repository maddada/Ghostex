//! Shared markdown-construct **recognition** — the definitions both of
//! Zorite's engines consume so they can never drift apart (links navigated in
//! the reader for months while WYSIWYG ignored clicks; alerts were once
//! recognized in three separate places). The reader (this crate's view),
//! the WYSIWYG editor (`zorite-editor`), and any other consumer (PDF export)
//! share *what counts as a construct and what's its payload*; each keeps its
//! own rendering. Everything here is engine-neutral and gpui-free.

/// The five GitHub alert kinds (`> [!NOTE]` …).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// `(kind, marker text)` for each alert, in matching order.
pub const ALERT_MARKERS: [(AlertKind, &str); 5] = [
    (AlertKind::Note, "[!NOTE]"),
    (AlertKind::Tip, "[!TIP]"),
    (AlertKind::Important, "[!IMPORTANT]"),
    (AlertKind::Warning, "[!WARNING]"),
    (AlertKind::Caution, "[!CAUTION]"),
];

impl AlertKind {
    /// The title rendered in place of the marker ("Note", "Tip", …).
    pub fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
}

/// Match an alert marker at the start of a blockquote's text content: the
/// marker must be uppercase and either alone on its first line (GitHub's
/// form) or followed by a space and the body (`[!NOTE] like so` — the way
/// people naturally type it). An Obsidian-style fold char directly after the
/// `]` makes the callout foldable: `[!NOTE]-` = folded by default, `[!NOTE]+`
/// = open (`None` = not foldable). Returns the kind, how many bytes to strip
/// (marker, fold char, and the newline/space separator), and the fold state.
pub fn alert_marker(value: &str) -> Option<(AlertKind, usize, Option<bool>)> {
    for (kind, m) in ALERT_MARKERS {
        if let Some(rest) = value.strip_prefix(m) {
            let (fold, flen) = match rest.as_bytes().first() {
                Some(b'-') => (Some(true), 1),
                Some(b'+') => (Some(false), 1),
                _ => (None, 0),
            };
            let rest = &rest[flen..];
            if rest.is_empty() {
                return Some((kind, m.len() + flen, fold));
            }
            if rest.starts_with('\n') || rest.starts_with(' ') {
                return Some((kind, m.len() + flen + 1, fold));
            }
        }
    }
    None
}

/// [`alert_marker`] for a single line's body (text after a blockquote's `>`
/// prefix): tolerates leading spaces and returns the kind, the byte length
/// consumed within `body` (spaces, marker, fold char, one separator space) —
/// what a line-oriented editor hides before painting the label — and the fold
/// state (`Some(true)` = folded).
pub fn alert_prefix(body: &str) -> Option<(AlertKind, usize, Option<bool>)> {
    let trimmed = body.trim_start();
    let ws = body.len() - trimmed.len();
    for (kind, m) in ALERT_MARKERS {
        if let Some(rest) = trimmed.strip_prefix(m) {
            let (fold, flen) = match rest.as_bytes().first() {
                Some(b'-') => (Some(true), 1),
                Some(b'+') => (Some(false), 1),
                _ => (None, 0),
            };
            let rest = &rest[flen..];
            if rest.is_empty() {
                return Some((kind, ws + m.len() + flen, fold));
            }
            if rest.starts_with(' ') {
                return Some((kind, ws + m.len() + flen + 1, fold));
            }
        }
    }
    None
}

/// The fold char of the alert marker on `line` (a full source line, `>` prefix
/// included): its byte offset within the line and the current state
/// (`true` = `-`/folded). `None` when the line isn't a foldable alert marker.
pub fn alert_fold_char(line: &str) -> Option<(usize, bool)> {
    let b = line.as_bytes();
    let mut p = 0;
    while p < b.len() && (b[p] == b'>' || b[p] == b' ') {
        p += 1;
    }
    let (_, _, fold) = alert_prefix(&line[p..])?;
    let folded = fold?;
    // The fold char sits right after the marker's closing `]`.
    let close = line[p..].find(']')? + p;
    Some((close + 1, folded))
}

/// Flip the fold state (`-` ↔ `+`) of the foldable alert marker on the line
/// containing byte `offset`, returning the new content — what a click on a
/// callout's chevron persists (the checkbox-toggle pattern).
pub fn toggle_alert_fold_at(content: &str, offset: usize) -> Option<String> {
    if offset > content.len() {
        return None;
    }
    let line_start = content[..offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = content[offset..]
        .find('\n')
        .map_or(content.len(), |p| offset + p);
    let (at, folded) = alert_fold_char(&content[line_start..line_end])?;
    let mut out = content.to_string();
    out.replace_range(
        line_start + at..line_start + at + 1,
        if folded { "+" } else { "-" },
    );
    Some(out)
}

/// Visual style of a GFM table, chosen per-table via a `<!-- table:STYLE -->`
/// marker comment on the line directly above it. The renderers honor it;
/// standard Markdown viewers ignore the comment and show a plain table.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TableStyle {
    /// Full outer box + all row/column gridlines.
    #[default]
    Grid,
    /// Alternate body rows shaded; no gridlines; a rule under the header.
    Striped,
    /// Only the header row shaded; no gridlines.
    Header,
    /// No box or gridlines — just a rule under the header.
    Minimal,
}

impl TableStyle {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "grid" => Some(Self::Grid),
            "striped" => Some(Self::Striped),
            "header" => Some(Self::Header),
            "minimal" => Some(Self::Minimal),
            _ => None,
        }
    }
}

/// Parse a `<!-- table:STYLE -->` marker (a whole line or an HTML comment's
/// value) into its [`TableStyle`]. `None` for anything unrecognized, so an
/// unknown marker stays a plain HTML comment.
pub fn table_style_marker(text: &str) -> Option<TableStyle> {
    let body = table_marker_body(text)?;
    // The style name is the first token; later tokens are attributes
    // (`cols=…` column widths).
    TableStyle::from_name(body.split_whitespace().next().unwrap_or(""))
}

/// The inner body of a `<!-- table:… -->` marker (style name + attributes),
/// or `None` for any other text.
fn table_marker_body(text: &str) -> Option<&str> {
    let inner = text
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    Some(inner.strip_prefix("table:")?.trim())
}

/// Explicit column widths (logical px) from a table marker's `cols=` attribute
/// — `<!-- table:grid cols=120,80,200 -->` — written by the editor's
/// drag-to-resize. `None` when absent or malformed (the table stays
/// content-measured).
pub fn table_col_widths(text: &str) -> Option<Vec<f32>> {
    let body = table_marker_body(text)?;
    let attr = body
        .split_whitespace()
        .find_map(|tok| tok.strip_prefix("cols="))?;
    let widths: Vec<f32> = attr
        .split(',')
        .map(|w| w.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    (!widths.is_empty() && widths.iter().all(|w| w.is_finite() && *w > 0.)).then_some(widths)
}

/// A table marker line for `style` (+ optional explicit column widths) — the
/// inverse of the parsers above. `None` when the marker would say nothing
/// (Grid, no widths): the default needs no marker.
pub fn table_marker_text(style: TableStyle, widths: Option<&[f32]>) -> Option<String> {
    let name = match style {
        TableStyle::Grid => "grid",
        TableStyle::Striped => "striped",
        TableStyle::Header => "header",
        TableStyle::Minimal => "minimal",
    };
    match widths {
        Some(w) if !w.is_empty() => {
            let list = w
                .iter()
                .map(|w| (w.round() as i64).to_string())
                .collect::<Vec<_>>()
                .join(",");
            Some(format!("<!-- table:{name} cols={list} -->"))
        }
        _ if style != TableStyle::Grid => Some(format!("<!-- table:{name} -->")),
        _ => None,
    }
}

/// Font-size multiplier for a heading of the given depth (h1 largest, h6 =
/// body) — one scale for reading, editing, and export.
pub fn heading_scale(depth: u8) -> f32 {
    match depth {
        // Local change for the Ghostex Docs prototype: the Docs editor's heading scale.
        1 => 1.85,
        2 => 1.45,
        3 => 1.18,
        4 => 1.08,
        5 => 1.0,
        _ => 0.94,
    }
}

/// The marker for ordered item `n` (1-based) at nesting `depth`, Word-style:
/// `1.` -> `a.` -> `i.`, cycling for deeper levels. Both views paint ordered
/// lists with this scheme (a deliberate divergence from CommonMark's
/// digits-everywhere), so nesting is readable at a glance.
pub fn ordered_marker(depth: usize, n: u32) -> String {
    match depth % 3 {
        0 => format!("{n}."),
        1 => format!("{}.", letters(n)),
        _ => format!("{}.", roman(n)),
    }
}

/// 1 → `a`, 26 → `z`, 27 → `aa` (bijective base 26).
fn letters(mut n: u32) -> String {
    let mut s = String::new();
    while n > 0 {
        n -= 1;
        s.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    s
}

/// Lowercase roman numerals (`0` has none; empty string).
fn roman(mut n: u32) -> String {
    let mut s = String::new();
    for (v, r) in [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ] {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

// --- Linkables ---

/// What a click on a link-like construct targets. `Page` opens a page by
/// title (a `[[wiki-link]]` or a `#tag` — Logseq semantics); `Url` is an
/// inline or bare URL (hosts open http(s) externally, resolve files
/// themselves).
#[derive(Debug, PartialEq, Clone)]
pub enum LinkHit {
    Page(String),
    Url(String),
    /// A `((id))` block reference (Logseq-style frontend form) — the host
    /// resolves the id to its page + anchor.
    BlockRef(String),
}

/// Whether `url` may be handed to the OS URL opener (`cx.open_url` —
/// `NSWorkspace openURL:` on macOS, `ShellExecute` on Windows).
///
/// Link targets are *attacker-authorable*: a synced note, an imported vault, a
/// PDF's `/URI` annotation. The OS opener runs whichever handler owns the
/// scheme, so `smb://` leaks NTLM hashes on Windows, `file://` launches local
/// content, and app-registered schemes (`ms-msdt:` …) are reachable. Hence an
/// allowlist, never a denylist: only `http://`, `https://`, and `mailto:`
/// pass. Schemes are case-insensitive per RFC 3986, so `HTTP://` passes too.
///
/// `mailto:` is on the list because `[write us](mailto:x@y.com)` is ordinary
/// markdown and dropping it would break real notes. It opens a compose window
/// rather than running anything, and the mail client — not us — owns parsing
/// its query (a percent-encoded `%0D%0A` can't be neutralized here, and
/// clients have long restricted which headers a `mailto:` may set).
///
/// Whitespace and control characters anywhere are a rejection rather than
/// something to trim — openers strip them, so ` javascript:…` and
/// `java\tscript:…` would otherwise walk past a prefix check. Everything else
/// is rejected: `javascript:`, `data:`, `file:`, `smb:`, UNC
/// `\\server\share`, scheme-relative `//host/path`, and bare relative paths.
/// A host that wants to resolve local files does so itself, before the opener.
pub fn is_safe_external_url(url: &str) -> bool {
    !url.chars().any(|c| c.is_whitespace() || c.is_control())
        && ["http://", "https://", "mailto:"].iter().any(|scheme| {
            url.as_bytes()
                .get(..scheme.len())
                .is_some_and(|got| got.eq_ignore_ascii_case(scheme.as_bytes()))
        })
}

/// A block's base writing direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn is_rtl(self) -> bool {
        self == Direction::Rtl
    }
}

/// The base direction of `text`, by the first *strong* directional character
/// (Unicode UAX #9 rules P2/P3 — the same rule Logseq and browsers' `dir=auto`
/// use). Neutral characters (digits, punctuation, whitespace, markdown
/// markers) are skipped, so `- سلام` and `## سلام` detect as RTL; text with no
/// strong character at all is LTR.
///
/// Ranges rather than a full Unicode table: the strong-RTL blocks are
/// contiguous and stable (Hebrew, Arabic and its supplements, Syriac, Thaana,
/// N'Ko, Samaritan, Mandaic, plus the Arabic presentation forms), and pulling
/// a bidi-class table in for one predicate isn't worth the dependency.
pub fn base_direction(text: &str) -> Direction {
    for c in text.chars() {
        if is_strong_rtl(c) {
            return Direction::Rtl;
        }
        if is_strong_ltr(c) {
            return Direction::Ltr;
        }
    }
    Direction::Ltr
}

/// The direction of a source line's CONTENT, ignoring its markdown markers.
///
/// [`base_direction`] takes the first strong character, and a marker can supply
/// one: the `x` in `- [x] یک کار` is strong left-to-right, so a COMPLETED task
/// read as LTR while the identical unchecked line read as RTL, and the two sat
/// on opposite sides of the note. Blockquote arrows, list bullets, task boxes
/// and heading hashes are syntax, not prose, so they are skipped first.
pub fn content_direction(line: &str) -> Direction {
    content_direction_opt(line).unwrap_or(Direction::Ltr)
}

/// [`content_direction`], but `None` when the line has no strong character at
/// all — it is blank, or nothing but markers.
///
/// `> [!NOTE]` is the case that matters: strip the quote arrow and the alert
/// marker and nothing is left, so the line has no direction of its own and must
/// take the surrounding text's. Answering `Ltr` there put a Persian callout's
/// title on one side and its body on the other.
pub fn content_direction_opt(line: &str) -> Option<Direction> {
    let mut rest = line.trim_start();
    loop {
        let before = rest;
        // Blockquote markers, however deep.
        rest = rest.strip_prefix('>').unwrap_or(rest).trim_start();
        // A bullet, or an ordered marker like `12.` / `3)`.
        if let Some(r) = rest
            .strip_prefix("- ")
            .or_else(|| rest.strip_prefix("* "))
            .or_else(|| rest.strip_prefix("+ "))
        {
            rest = r.trim_start();
        } else {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            if digits > 0
                && let Some(r) = rest[digits..]
                    .strip_prefix(". ")
                    .or_else(|| rest[digits..].strip_prefix(") "))
            {
                rest = r.trim_start();
            }
        }
        // A GitHub alert marker (`[!NOTE]`, `[!TIP]` …), plus the Obsidian
        // fold char after it. Its LABEL is Latin, so it decided the direction
        // of every Persian callout — the same trap the task box set.
        if let Some(r) = rest.strip_prefix("[!")
            && let Some(close) = r.find(']')
        {
            let after = &r[close + 1..];
            rest = after
                .strip_prefix('-')
                .or_else(|| after.strip_prefix('+'))
                .unwrap_or(after)
                .trim_start();
        }
        // A task box — the one that started this.
        if let Some(r) = rest
            .strip_prefix("[ ] ")
            .or_else(|| rest.strip_prefix("[x] "))
            .or_else(|| rest.strip_prefix("[X] "))
        {
            rest = r.trim_start();
        }
        // Heading hashes.
        if rest.starts_with('#') {
            let hashes = rest.chars().take_while(|c| *c == '#').count();
            if let Some(r) = rest[hashes..].strip_prefix(' ') {
                rest = r.trim_start();
            }
        }
        if rest == before {
            break;
        }
    }
    rest.chars().find_map(|c| {
        if is_strong_rtl(c) {
            Some(Direction::Rtl)
        } else if is_strong_ltr(c) {
            Some(Direction::Ltr)
        } else {
            None
        }
    })
}

/// Does `text` contain ANY right-to-left character?
///
/// Distinct from [`base_direction`], which answers which side a line starts on.
/// A line can read left-to-right and still hold a Persian name in the middle,
/// and that run needs the same logical↔visual mapping an RTL line does — the
/// caret misplaces inside it otherwise.
pub fn contains_rtl(text: &str) -> bool {
    text.chars().any(is_strong_rtl)
}

/// Strong right-to-left: Hebrew through Arabic-script languages.
fn is_strong_rtl(c: char) -> bool {
    matches!(c,
        '\u{0590}'..='\u{05FF}'   // Hebrew
        | '\u{0600}'..='\u{06FF}' // Arabic (incl. Persian, Urdu)
        | '\u{0700}'..='\u{074F}' // Syriac
        | '\u{0750}'..='\u{077F}' // Arabic Supplement
        | '\u{0780}'..='\u{07BF}' // Thaana
        | '\u{07C0}'..='\u{07FF}' // N'Ko
        | '\u{0800}'..='\u{083F}' // Samaritan
        | '\u{0840}'..='\u{085F}' // Mandaic
        | '\u{08A0}'..='\u{08FF}' // Arabic Extended-A
        | '\u{FB1D}'..='\u{FDFF}' // Hebrew/Arabic presentation forms
        | '\u{FE70}'..='\u{FEFF}' // Arabic presentation forms-B
    )
}

/// Strong left-to-right — Latin/Greek/Cyrillic and the CJK/Indic scripts.
/// Deliberately approximate in the same direction as [`is_strong_rtl`]: what
/// matters is that a strong LTR character stops the scan, and every alphabetic
/// character that isn't strong-RTL does.
fn is_strong_ltr(c: char) -> bool {
    c.is_alphabetic() && !is_strong_rtl(c)
}

/// Split a wiki-link's inner text into `(target, display)`:
/// `target|label` shows `label` (falling back to the target when the label is
/// empty); `name` shows itself. Both sides trimmed.
pub fn wiki_target_display(inner: &str) -> (&str, &str) {
    match inner.split_once('|') {
        Some((t, l)) if !l.trim().is_empty() => (t.trim(), l.trim()),
        Some((t, _)) => (t.trim(), t.trim()),
        None => (inner.trim(), inner.trim()),
    }
}

/// Whether `c` can appear inside a `#tag` name (after the `#`). `/` is
/// included — Logseq-style namespaced tags (`#area/sub`) are one tag.
pub fn is_tag_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'/')
}

/// A word character for boundary checks (a `#` glued to a word isn't a tag;
/// a URL glued to a word isn't a link).
pub fn is_word_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Where a bare URL starting at `start` ends: consumes to whitespace or a
/// wrapping delimiter, then backs off trailing punctuation (GFM-ish).
pub fn url_end(line: &str, start: usize) -> usize {
    let b = line.as_bytes();
    let mut j = start;
    while j < line.len()
        && !b[j].is_ascii_whitespace()
        && !matches!(b[j], b'<' | b'>' | b'"' | b'`')
    {
        j += 1;
    }
    while j > start
        && matches!(
            b[j - 1],
            b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']'
        )
    {
        j -= 1;
    }
    j
}

/// Every clickable link in `line`, as `(source byte range, target)`.
/// Wiki-links (anywhere on one opens its target; the alias is display-only),
/// inline `[text](url)` links, `#tags`, and bare `http(s)://` URLs. Images
/// (`![](src)`), footnote refs, and anything inside inline code are opaque —
/// not links. One grammar for every renderer's click hit-tests, hover
/// cursors, and styling.
pub fn links(line: &str) -> Vec<(std::ops::Range<usize>, LinkHit)> {
    let b = line.as_bytes();
    let end = line.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < end {
        let c = b[i];
        // Inline code: the span is opaque (a URL inside backticks is verbatim).
        if c == b'`'
            && let Some(close) = find1(b, i + 1, end, b'`')
        {
            i = close + 1;
            continue;
        }
        // Wiki-link: [[target]] / [[target|alias]].
        if c == b'['
            && i + 1 < end
            && b[i + 1] == b'['
            && let Some(close) = find2(b, i + 2, end, b']', b']')
        {
            let (target, _) = wiki_target_display(&line[i + 2..close]);
            if !target.is_empty() {
                out.push((i..close + 2, LinkHit::Page(target.to_string())));
            }
            i = close + 2;
            continue;
        }
        // Block ref: `((id))` — an anchor-shaped id (word chars / `-`).
        if c == b'('
            && i + 1 < end
            && b[i + 1] == b'('
            && let Some(close) = find2(b, i + 2, end, b')', b')')
        {
            let id = &line[i + 2..close];
            if !id.is_empty()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                out.push((i..close + 2, LinkHit::BlockRef(id.to_string())));
                i = close + 2;
                continue;
            }
        }
        // Footnote reference [^label]: styled like a link but not one.
        if c == b'['
            && i + 1 < end
            && b[i + 1] == b'^'
            && let Some(rb) = find1(b, i + 2, end, b']')
            && rb > i + 2
        {
            i = rb + 1;
            continue;
        }
        // Inline link [text](url) — or an image ![alt](src), which is NOT a
        // link click (images render as widgets / have their own machinery).
        if c == b'['
            && let Some(rb) = find1(b, i + 1, end, b']')
            && rb + 1 < end
            && b[rb + 1] == b'('
            && let Some(rp) = find1(b, rb + 2, end, b')')
        {
            let is_image = i > 0 && b[i - 1] == b'!';
            let url = line[rb + 2..rp].trim();
            if !is_image && !url.is_empty() {
                out.push((i..rp + 1, LinkHit::Url(url.to_string())));
            }
            i = rp + 1;
            continue;
        }
        // Tag: #tag → the page of that name (Logseq semantics).
        if c == b'#' && (i == 0 || !is_word_char(b[i - 1])) {
            let mut j = i + 1;
            while j < end && is_tag_char(b[j]) {
                j += 1;
            }
            if j > i + 1 {
                out.push((i..j, LinkHit::Page(line[i + 1..j].to_string())));
                i = j;
                continue;
            }
        }
        // Bare URL: http(s)://… at a word boundary (GFM autolink literal).
        // Compare BYTES: `i` walks bytes, so a str slice here would panic
        // mid-char on any non-ASCII text.
        if (b[i..].starts_with(b"http://") || b[i..].starts_with(b"https://"))
            && (i == 0 || !is_word_char(b[i - 1]))
        {
            let j = url_end(line, i);
            if j > i + 8 {
                out.push((i..j, LinkHit::Url(line[i..j].to_string())));
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The link under byte `col` of `line`, if any (see [`links`]).
pub fn link_at(line: &str, col: usize) -> Option<LinkHit> {
    links(line)
        .into_iter()
        .find(|(r, _)| r.contains(&col))
        .map(|(_, hit)| hit)
}

/// The Obsidian block-id anchor at the end of `line` (` ^some-id`): the byte
/// where its leading space starts (so renderers can hide the whole tail) and
/// the id itself. The id must be non-empty, made of word chars / `-`, and sit
/// at the line's end (trailing whitespace tolerated).
pub fn block_id(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_end();
    let (before, id) = trimmed.rsplit_once(" ^")?;
    if id.is_empty() || !id.bytes().all(|b| is_word_char(b) || b == b'-') {
        return None;
    }
    Some((before.len(), id))
}

/// Split a wiki-link target into `(page, block id)`: `Note#^id` links to the
/// block carrying `^id` on the page `Note`; anything else is a plain page
/// target. Only the `#^` form is an anchor — a bare `#` stays part of the
/// title (page names may contain it, and `file.pdf#p3` has its own meaning).
/// Superscript digits (`¹²…`) for the block reference-count badge — reads
/// small at any text size, so the badge doesn't shout on heading lines.
pub fn superscript(n: usize) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    n.to_string()
        .bytes()
        .map(|b| DIGITS[(b - b'0') as usize])
        .collect()
}

pub fn split_block_anchor(target: &str) -> (&str, Option<&str>) {
    match target.split_once("#^") {
        Some((page, id)) if !page.is_empty() && !id.is_empty() => (page, Some(id)),
        _ => (target, None),
    }
}

/// Split a wiki-link target into `(page, heading)`: `Note#My Heading` links to
/// the heading on the page `Note`. Splits at the first `#` when both sides are
/// non-empty and the page part isn't a PDF (`file.pdf#p3` keeps its page-jump
/// meaning). Block anchors (`#^`) are the caller's first check —
/// [`split_block_anchor`] — and a Zorite page title may itself contain `#`, so
/// navigation should prefer an existing literal-titled page before splitting.
pub fn split_heading_anchor(target: &str) -> (&str, Option<&str>) {
    match target.split_once('#') {
        Some((page, heading))
            if !page.is_empty()
                && !heading.trim().is_empty()
                && !heading.starts_with('^')
                && !page.to_ascii_lowercase().ends_with(".pdf") =>
        {
            (page, Some(heading))
        }
        _ => (target, None),
    }
}

/// The byte offset of the start of the line carrying the ATX heading whose
/// text matches `heading` (case-insensitive, trimmed; fenced code skipped),
/// searching top to bottom. Drives navigation for `[[Note#Heading]]` links.
pub fn find_heading_line(content: &str, heading: &str) -> Option<usize> {
    let want = heading.trim().to_lowercase();
    let mut start = 0;
    let mut in_fence = false;
    for line in content.split('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            let t = line.trim_start();
            let level = t.bytes().take_while(|&b| b == b'#').count();
            if (1..=6).contains(&level)
                && let Some(text) = t[level..].strip_prefix(' ')
                && text.trim().to_lowercase() == want
            {
                return Some(start);
            }
        }
        start += line.len() + 1;
    }
    None
}

/// The byte offset of the start of the line carrying the block anchor `^id`,
/// searching top to bottom. Drives navigation for `[[Note#^id]]` links.
pub fn find_block_line(content: &str, id: &str) -> Option<usize> {
    let mut start = 0;
    for line in content.split('\n') {
        if block_id(line).is_some_and(|(_, i)| i == id) {
            return Some(start);
        }
        start += line.len() + 1;
    }
    None
}

/// The embed target when `line` is a standalone transclusion — exactly
/// `![[target]]` (Obsidian's embed syntax) and nothing else on the line.
/// Mid-text embeds don't count; they render as plain links.
pub fn embed_line(line: &str) -> Option<&str> {
    let t = line.trim();
    let inner = t.strip_prefix("![[")?.strip_suffix("]]")?;
    (!inner.trim().is_empty() && !inner.contains("]]")).then(|| inner.trim())
}

/// Every standalone embed target in `content`, in order — what a host
/// pre-resolves before rendering (recursing into resolved content itself for
/// nested embeds).
pub fn embed_targets(content: &str) -> Vec<String> {
    content
        .split('\n')
        .filter_map(embed_line)
        .map(str::to_string)
        .collect()
}

/// The source range of the block carrying the anchor `^id` — its whole line —
/// for embedding (`![[Note#^id]]`).
pub fn extract_block(content: &str, id: &str) -> Option<std::ops::Range<usize>> {
    let start = find_block_line(content, id)?;
    let end = content[start..]
        .find('\n')
        .map_or(content.len(), |p| start + p);
    Some(start..end)
}

/// The source range of the section under `heading` — the heading line through
/// the line before the next heading of the same or higher level (fenced code
/// skipped) — for embedding (`![[Note#Heading]]`).
pub fn extract_section(content: &str, heading: &str) -> Option<std::ops::Range<usize>> {
    let start = find_heading_line(content, heading)?;
    let level = content[start..]
        .trim_start()
        .bytes()
        .take_while(|&b| b == b'#')
        .count();
    let mut pos = content[start..]
        .find('\n')
        .map_or(content.len(), |p| start + p + 1);
    let mut in_fence = false;
    while pos < content.len() {
        let line_end = content[pos..].find('\n').map_or(content.len(), |p| pos + p);
        let line = &content[pos..line_end];
        let t = line.trim_start();
        if t.starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            let l = t.bytes().take_while(|&b| b == b'#').count();
            if (1..=level).contains(&l) && t[l..].starts_with(' ') {
                return Some(start..pos.saturating_sub(1).max(start));
            }
        }
        pos = line_end + 1;
    }
    Some(start..content.len())
}

/// Split a `key:: value` property line into `(key, value)`. The key must look
/// like an identifier (starts with a letter; letters/digits/`-_.` after) so
/// prose containing `::` — Zorite `[[wiki]]` links, `C++::method` — isn't
/// mistaken for a property. Leading indentation is ignored; the value is
/// trimmed. One grammar for the reader, the editor, and the importers.
pub fn property(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_start();
    let idx = rest.find("::")?;
    let key = &rest[..idx];
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || !key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    Some((key, rest[idx + 2..].trim()))
}

/// [`property`], tolerating a leading list marker — the Logseq shape for a
/// props-only block (`- key:: value`, also `* ` / `+ ` / `1. ` / `1) `).
/// Returns `(prefix, key, value)`; `prefix` is everything before the key
/// (indent + marker), so editors can write the line back unchanged.
pub fn prefixed_property(line: &str) -> Option<(&str, &str, &str)> {
    let ws = line.len() - line.trim_start().len();
    if let Some((k, v)) = property(line) {
        return Some((&line[..ws], k, v));
    }
    let rest = &line[ws..];
    let body = if let Some(r) = ["- ", "* ", "+ "].iter().find_map(|m| rest.strip_prefix(m)) {
        r
    } else {
        let d = rest.bytes().take_while(u8::is_ascii_digit).count();
        let b = rest.as_bytes();
        if d > 0 && rest.len() > d + 1 && matches!(b[d], b'.' | b')') && b[d + 1] == b' ' {
            &rest[d + 2..]
        } else {
            return None;
        }
    };
    let (k, v) = property(body)?;
    Some((&line[..line.len() - body.len()], k, v))
}

/// A rendered piece of a property value: literal text, or a link "pill" (a
/// wiki-link, `#tag`, or URL shown as a rounded chip). Both panels render values
/// through this so they pill-ify identically.
pub enum PropSeg {
    Text(String),
    Pill {
        /// The chip's display text: a wiki-link's label, a tag without its `#`,
        /// or a link's text.
        label: String,
        target: LinkHit,
        /// A `#tag` (vs a wiki-link / URL) — panels tint tags differently.
        is_tag: bool,
    },
}

/// Split a property value into display segments — plain runs and link pills
/// (wiki-links show their label, tags drop the `#`, `[text](url)` shows its
/// text, bare URLs show themselves). Built on [`links`], so the pill spans match
/// the reader's and editor's click hit-tests.
pub fn property_value_segments(value: &str) -> Vec<PropSeg> {
    let mut out = Vec::new();
    let mut pos = 0;
    for (range, hit) in links(value) {
        if range.start > pos {
            out.push(PropSeg::Text(value[pos..range.start].to_string()));
        }
        let raw = &value[range.clone()];
        let (label, is_tag) =
            if let Some(inner) = raw.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
                (wiki_target_display(inner).1.to_string(), false)
            } else if let Some(tag) = raw.strip_prefix('#') {
                (tag.to_string(), true)
            } else if let Some(rest) = raw.strip_prefix('[') {
                // `[text](url)` — show the text.
                (
                    rest.split_once(']').map_or(raw, |(t, _)| t).to_string(),
                    false,
                )
            } else {
                (raw.to_string(), false) // bare URL
            };
        out.push(PropSeg::Pill {
            label,
            target: hit,
            is_tag,
        });
        pos = range.end;
    }
    if pos < value.len() {
        out.push(PropSeg::Text(value[pos..].to_string()));
    }
    out
}

fn find1(b: &[u8], from: usize, end: usize, c: u8) -> Option<usize> {
    (from..end).find(|&i| b[i] == c)
}

fn find2(b: &[u8], from: usize, end: usize, c1: u8, c2: u8) -> Option<usize> {
    (from..end.saturating_sub(1)).find(|&i| b[i] == c1 && b[i + 1] == c2)
}

// --- Forgiving `$$` fences (words attached) ----------------------------------

/// Classify a words-attached `$$` fence line: `Some(true)` for an opener
/// (`words $$` — the `$$` trails), `Some(false)` for a closer (`$$ words` —
/// the `$$` leads). A bare `$$` (the strict form) and lines whose `$$` pairs
/// up on the same line classify as neither.
pub fn math_fence_words(line: &str) -> Option<bool> {
    let t = line.trim();
    if t == "$$" || t.len() <= 2 {
        return None;
    }
    if t.ends_with("$$") && !t[..t.len() - 2].contains("$$") {
        return Some(true);
    }
    if t.starts_with("$$") && !t[2..].contains("$$") {
        return Some(false);
    }
    None
}

/// A single complete `$$…$$` pair on a line that ALSO carries other text —
/// strict inner rules (non-empty, no interior `$$`, no delimiter-adjacent
/// whitespace), so prose about prices (`$$5 and $$10`) never matches. Table
/// rows keep their cells.
pub fn embedded_math(line: &str) -> Option<(usize, usize)> {
    let t = line.trim_start();
    if t.starts_with('|') {
        return None;
    }
    let s = line.find("$$")?;
    let e = line.rfind("$$")?;
    if e < s + 3 {
        return None;
    }
    let inner = &line[s + 2..e];
    (!inner.is_empty()
        && !inner.contains("$$")
        && !inner.starts_with(char::is_whitespace)
        && !inner.ends_with(char::is_whitespace))
    .then_some((s, e + 2))
}

/// Split words-attached `$$` fences onto their own lines when they pair up
/// (an opener needs a closer before the next blank line) — `wer $$` → `wer` +
/// `$$`, `$$ wer` → `$$` + `wer` — and split a words-mixed complete pair onto
/// its own line (`text $$x$$ text` → three lines), so a math parser sees
/// well-formed display blocks. Code fences are left alone; unpaired `$$`s
/// (prose, prices) pass through untouched. Borrowed when nothing changes.
pub fn normalize_math_fences(source: &str) -> std::borrow::Cow<'_, str> {
    if !source.contains("$$") {
        return std::borrow::Cow::Borrowed(source);
    }
    let lines: Vec<&str> = source.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut changed = false;
    let mut in_code = false;
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with("```") {
            in_code = !in_code;
            out.push(lines[i].to_string());
            i += 1;
            continue;
        }
        if !in_code && let Some((s, e)) = embedded_math(lines[i]) {
            let (before, after) = (lines[i][..s].trim_end(), lines[i][e..].trim_start());
            if !before.is_empty() {
                out.push(before.to_string());
            }
            out.push(lines[i][s..e].to_string());
            if !after.is_empty() {
                out.push(after.to_string());
            }
            changed = changed || !before.is_empty() || !after.is_empty();
            i += 1;
            continue;
        }
        if !in_code && math_fence_words(lines[i]) == Some(true) {
            // A closer before the next blank line pairs the fences; a blank
            // first means this was prose, not math.
            let closer = (i + 1..lines.len())
                .take_while(|&j| !lines[j].trim().is_empty())
                .find(|&j| lines[j].trim() == "$$" || math_fence_words(lines[j]) == Some(false));
            if let Some(j) = closer {
                let open = lines[i].trim_end();
                out.push(open[..open.len() - 2].trim_end().to_string());
                out.push("$$".to_string());
                for inner in &lines[i + 1..j] {
                    out.push(inner.to_string());
                }
                out.push("$$".to_string());
                let close = lines[j].trim_start();
                let rest = close[2..].trim_start();
                if !rest.is_empty() {
                    out.push(rest.to_string());
                }
                changed = true;
                i = j + 1;
                continue;
            }
        }
        out.push(lines[i].to_string());
        i += 1;
    }
    if changed {
        std::borrow::Cow::Owned(out.join("\n"))
    } else {
        std::borrow::Cow::Borrowed(source)
    }
}

// --- Highlights and colors ------------------------------------------------
//
// `==text==` is the highlight most markdown apps agree on (Obsidian, Typora,
// Bear); `<mark>` is its inline-HTML twin. A *chosen* color has no markdown
// spelling at all, so it rides the HTML Obsidian (and its Highlightr plugin)
// writes: `<mark style="background:#ffd54f">` and `<span style="color:#e11">`.
// Both views recognize all of these through the functions below.

/// The closing `==` for the highlight opener at byte `open` (`line[open..]`
/// starts with `==`), under the emphasis-like rules markdown-it-mark uses so
/// `a == b == c` and `====` stay literal: the opener is followed by a
/// non-space that isn't `=`, the closer is preceded by a non-space and is
/// exactly two `=`, and the body is non-empty.
pub fn highlight_close(line: &str, open: usize) -> Option<usize> {
    let b = line.as_bytes();
    let body = open + 2;
    match b.get(body) {
        Some(c) if !c.is_ascii_whitespace() && *c != b'=' => {}
        _ => return None,
    }
    let mut from = body;
    while let Some(rel) = line[from..].find("==") {
        let close = from + rel;
        let run = b[close..].iter().take_while(|c| **c == b'=').count();
        if run == 2 && close > body && !b[close - 1].is_ascii_whitespace() && b[close - 1] != b'\\'
        {
            return Some(close);
        }
        from = close + run;
    }
    None
}

/// Byte offsets of every `==` highlight marker in `line` — openers and closers,
/// ascending — skipping backtick code spans and backslash escapes. What both
/// views hide; the text between an opener and its closer is highlighted.
pub fn highlight_markers(line: &str) -> Vec<usize> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'`' => match line[i + 1..].find('`') {
                Some(rel) => i += rel + 2,
                None => break,
            },
            b'\\' => i += 1 + line[i + 1..].chars().next().map_or(0, char::len_utf8),
            b'=' if b.get(i + 1) == Some(&b'=') => match highlight_close(line, i) {
                Some(close) => {
                    out.push(i);
                    out.push(close);
                    i = close + 2;
                }
                None => i += b[i..].iter().take_while(|c| **c == b'=').count(),
            },
            _ => i += 1,
        }
    }
    out
}

/// Which inline-HTML tag a [`StyledTag`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyledKind {
    /// `<mark>` — a highlight, the theme's tint unless `background` says otherwise.
    Mark,
    /// `<span style="color:…">` — colored text (and/or a colored background).
    Span,
    /// `<u>` — underline (markdown has none).
    Underline,
}

impl StyledKind {
    /// The closing tag, as it appears in the source.
    pub fn close(self) -> &'static str {
        match self {
            StyledKind::Mark => "</mark>",
            StyledKind::Span => "</span>",
            StyledKind::Underline => "</u>",
        }
    }
}

/// An inline-HTML opening tag both views style rather than print.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyledTag {
    pub kind: StyledKind,
    /// `color:` from the `style` attribute, as `0xRRGGBBAA`.
    pub color: Option<u32>,
    /// `background:` / `background-color:` from the `style` attribute, as
    /// `0xRRGGBBAA`. A bare `<mark>` has none: the theme's highlight.
    pub background: Option<u32>,
}

/// Parse an opening tag (`<mark>`, `<mark style="background:#ffd54f">`,
/// `<span style="color:#e11">`, `<u>`), the tag text including its angle
/// brackets. A `<span>` that sets neither color is not styled — `None`, and
/// it prints literally like any other HTML.
pub fn styled_tag(tag: &str) -> Option<StyledTag> {
    let inner = tag.trim().strip_prefix('<')?.strip_suffix('>')?;
    let (name, attrs) = match inner.find(|c: char| c.is_ascii_whitespace()) {
        Some(i) => (&inner[..i], inner[i..].trim()),
        None => (inner, ""),
    };
    let kind = match name.to_ascii_lowercase().as_str() {
        "mark" => StyledKind::Mark,
        "span" => StyledKind::Span,
        "u" => StyledKind::Underline,
        _ => return None,
    };
    let (mut color, mut background) = (None, None);
    if let Some(style) = attr_value(attrs, "style") {
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                match k.trim().to_ascii_lowercase().as_str() {
                    "color" => color = css_color(v),
                    "background" | "background-color" => background = css_color(v),
                    _ => {}
                }
            }
        }
    }
    if kind == StyledKind::Span && color.is_none() && background.is_none() {
        return None;
    }
    Some(StyledTag {
        kind,
        color,
        background,
    })
}

/// The kind a closing tag (`</mark>`, `</span>`, `</u>`) closes.
pub fn styled_close(tag: &str) -> Option<StyledKind> {
    match tag.trim().to_ascii_lowercase().as_str() {
        "</mark>" => Some(StyledKind::Mark),
        "</span>" => Some(StyledKind::Span),
        "</u>" => Some(StyledKind::Underline),
        _ => None,
    }
}

/// `name="value"` / `name='value'` / `name=value` out of an attribute list.
fn attr_value<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let at = attrs.to_ascii_lowercase().find(name)?;
    let rest = attrs[at + name.len()..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    match rest.chars().next()? {
        q @ ('"' | '\'') => {
            let end = rest[1..].find(q)?;
            Some(&rest[1..1 + end])
        }
        _ => rest.split_whitespace().next(),
    }
}

/// A CSS color as `0xRRGGBBAA`: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`,
/// `rgb(r, g, b)` / `rgba(r, g, b, a)`, and the basic named colors.
pub fn css_color(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let d: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
        let (r, g, b, a) = match d.as_slice() {
            [r, g, b] => (r * 17, g * 17, b * 17, 255),
            [r, g, b, a] => (r * 17, g * 17, b * 17, a * 17),
            [r1, r2, g1, g2, b1, b2] => (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, 255),
            [r1, r2, g1, g2, b1, b2, a1, a2] => {
                (r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, a1 * 16 + a2)
            }
            _ => return None,
        };
        return Some(r << 24 | g << 16 | b << 8 | a);
    }
    let lower = s.to_ascii_lowercase();
    if let Some(inner) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|r| r.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() < 3 {
            return None;
        }
        let channel = |p: &str| p.parse::<f32>().ok().map(|v| v.clamp(0.0, 255.0) as u32);
        let (r, g, b) = (channel(parts[0])?, channel(parts[1])?, channel(parts[2])?);
        let a = match parts.get(3) {
            Some(p) => (p.parse::<f32>().ok()?.clamp(0.0, 1.0) * 255.0).round() as u32,
            None => 255,
        };
        return Some(r << 24 | g << 16 | b << 8 | a);
    }
    let rgb = match lower.as_str() {
        "black" => 0x000000,
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "orange" => 0xffa500,
        "purple" => 0x800080,
        "pink" => 0xffc0cb,
        "gray" | "grey" => 0x808080,
        "brown" => 0xa52a2a,
        "cyan" => 0x00ffff,
        "magenta" => 0xff00ff,
        "lime" => 0x00ff00,
        "navy" => 0x000080,
        "teal" => 0x008080,
        _ => return None,
    };
    Some(rgb << 8 | 0xff)
}

/// `0xRRGGBBAA` as CSS hex — `#rrggbb`, or `#rrggbbaa` when not opaque. How
/// the editor writes a chosen color into a `style` attribute.
pub fn hex_color(rgba: u32) -> String {
    if rgba & 0xff == 0xff {
        format!("#{:06x}", rgba >> 8)
    } else {
        format!("#{rgba:08x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_markers_follow_the_pairing_rules() {
        assert_eq!(highlight_markers("a ==b== c"), vec![2, 5]);
        // Spaces inside, `===` runs, a lone opener, and code spans stay literal.
        assert!(highlight_markers("a == b == c").is_empty());
        assert!(highlight_markers("x ==== y").is_empty());
        assert!(highlight_markers("==open only").is_empty());
        assert!(highlight_markers("`a ==b== c`").is_empty());
        assert_eq!(highlight_markers("==one== and ==two=="), vec![0, 5, 12, 17]);
        assert_eq!(highlight_close("a ==b== c", 2), Some(5));
        assert_eq!(highlight_close("a ==b=== c", 2), None);
    }

    #[test]
    fn styled_tags_and_css_colors_parse() {
        let t = styled_tag(r#"<mark style="background:#ff0000">"#).unwrap();
        assert_eq!((t.kind, t.background), (StyledKind::Mark, Some(0xff0000ff)));
        let t = styled_tag("<mark>").unwrap();
        assert_eq!(
            (t.kind, t.background, t.color),
            (StyledKind::Mark, None, None)
        );
        let t = styled_tag("<span style='color: rgb(0, 255, 0); font-weight: bold'>").unwrap();
        assert_eq!((t.kind, t.color), (StyledKind::Span, Some(0x00ff00ff)));
        assert_eq!(styled_tag("<span class=\"x\">"), None);
        assert_eq!(
            styled_tag("<u>").map(|t| t.kind),
            Some(StyledKind::Underline)
        );
        assert_eq!(styled_tag("<b>"), None);
        assert_eq!(styled_close("</MARK>"), Some(StyledKind::Mark));
        assert_eq!(css_color("#abc"), Some(0xaabbccff));
        assert_eq!(css_color("#11223344"), Some(0x11223344));
        assert_eq!(css_color("rgba(255, 0, 0, 0.5)"), Some(0xff000080));
        assert_eq!(css_color("Orange"), Some(0xffa500ff));
        assert_eq!(css_color("#12"), None);
        assert_eq!(hex_color(0xffd54fff), "#ffd54f");
        assert_eq!(hex_color(0xff000080), "#ff000080");
    }

    #[test]
    fn math_fence_normalization() {
        // Paired words-fences split onto their own lines.
        assert_eq!(
            normalize_math_fences("wer $$\nx+y\n$$ wer").as_ref(),
            "wer\n$$\nx+y\n$$\nwer"
        );
        // An opener with no closer before a blank line is prose — untouched
        // (and Borrowed).
        let prose = "cost $$\n\nlater";
        assert!(matches!(
            normalize_math_fences(prose),
            std::borrow::Cow::Borrowed(_)
        ));
        // Code fences are left alone.
        let code = "```sh\necho $$\n$$\n```";
        assert!(matches!(
            normalize_math_fences(code),
            std::borrow::Cow::Borrowed(_)
        ));
        // A words-mixed complete pair splits onto its own line.
        assert_eq!(
            normalize_math_fences("What if words $$E=mc^2$$ more").as_ref(),
            "What if words\n$$E=mc^2$$\nmore"
        );
        // Prices (delimiter-adjacent whitespace) stay prose.
        assert!(matches!(
            normalize_math_fences("fees are $$5 and $$10 total"),
            std::borrow::Cow::Borrowed(_)
        ));
        // A pair ALONE on its line is already well-formed.
        assert!(matches!(
            normalize_math_fences("$$y$$"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn markers_do_not_decide_a_line_s_direction() {
        // The bug: `x` is strong left-to-right, so a COMPLETED task read LTR
        // while the same line unchecked read RTL — the two sat on opposite
        // sides of the note.
        assert!(content_direction("- [x] یک کار انجام‌شده").is_rtl());
        assert!(content_direction("- [ ] یک کار انجام‌نشده").is_rtl());
        assert!(content_direction("- مورد فهرست").is_rtl());
        assert!(content_direction("1. مورد شماره‌دار").is_rtl());
        assert!(content_direction("## سلام دنیا").is_rtl());
        assert!(content_direction("> یک نقل‌قول").is_rtl());
        // An alert's label is Latin whatever the prose is.
        assert!(content_direction("> [!NOTE]\n> یک هشدار فارسی").is_rtl());
        assert!(content_direction("> [!WARNING]- یک هشدار").is_rtl());
        assert!(!content_direction("> [!NOTE]\n> an english callout").is_rtl());
        // A marker-only line has NO direction of its own — the caller decides
        // whether that means the line above or the content below.
        assert_eq!(content_direction_opt("> [!NOTE]"), None);
        assert_eq!(content_direction_opt("- "), None);
        assert_eq!(content_direction_opt(""), None);
        assert!(!content_direction("> > [!NOTE]\n").is_rtl(), "no content");
        // Latin content still reads left-to-right, markers or not.
        assert!(!content_direction("- [x] a done task").is_rtl());
        assert!(!content_direction("## English heading").is_rtl());
        // A line that is only markers has no direction of its own.
        assert!(!content_direction("- [ ] ").is_rtl());
    }

    #[test]
    fn base_direction_follows_the_first_strong_character() {
        use Direction::*;
        // Plain cases.
        assert_eq!(base_direction("hello"), Ltr);
        assert_eq!(base_direction("سلام دنیا"), Rtl); // Persian
        assert_eq!(base_direction("שלום עולם"), Rtl); // Hebrew
        // Neutrals lead — markdown markers, digits, punctuation, whitespace
        // are all skipped, which is what makes list items and headings work.
        assert_eq!(base_direction("- سلام"), Rtl);
        assert_eq!(base_direction("## سلام"), Rtl);
        assert_eq!(base_direction("> «سلام»"), Rtl);
        assert_eq!(base_direction("  \t123 — سلام"), Rtl);
        assert_eq!(base_direction("- hello"), Ltr);
        // The FIRST strong character decides, not the majority: a line opening
        // in English stays LTR however much Persian follows (and vice versa).
        assert_eq!(base_direction("Rust سلام دنیا و بیشتر"), Ltr);
        assert_eq!(base_direction("سلام Rust and more English"), Rtl);
        // No strong character anywhere → LTR.
        assert_eq!(base_direction(""), Ltr);
        assert_eq!(base_direction("123 — !?"), Ltr);
        // Other scripts are LTR.
        assert_eq!(base_direction("日本語"), Ltr);
        assert_eq!(base_direction("Привет"), Ltr);
        assert!(Direction::Rtl.is_rtl() && !Direction::Ltr.is_rtl());
    }

    #[test]
    fn only_http_reaches_the_os_opener() {
        for ok in [
            "http://example.com",
            "https://example.com/a?b=c#d",
            "HTTPS://EXAMPLE.COM",
            "HtTp://example.com",
            // Ordinary markdown — an email link opens a compose window.
            "mailto:a@b.c",
            "MAILTO:a@b.c",
        ] {
            assert!(is_safe_external_url(ok), "should allow {ok:?}");
        }
        for bad in [
            "javascript:alert(1)",
            "data:text/html,<script>x</script>",
            "file:///etc/passwd",
            "smb://evil/share",
            "vbscript:msgbox",
            "ms-msdt:/id",
            "//evil.com/path",
            r"\\evil\share",
            "/local/path",
            "example.com",
            " javascript:alert(1)",
            "\tjavascript:alert(1)",
            "java\tscript:alert(1)",
            "http\n://example.com",
            "https://exa\u{0}mple.com",
            "https://example.com\r\nHost: evil",
            "",
        ] {
            assert!(!is_safe_external_url(bad), "should reject {bad:?}");
        }
    }

    #[test]
    fn alert_recognition_both_forms() {
        assert!(matches!(
            alert_marker("[!NOTE]\nbody"),
            Some((AlertKind::Note, 8, None))
        ));
        assert!(matches!(
            alert_marker("[!NOTE] inline"),
            Some((AlertKind::Note, 8, None))
        ));
        assert!(alert_marker("[!note] no").is_none());
        assert!(alert_marker("[!NOTEXT]").is_none());

        assert!(matches!(
            alert_prefix("  [!TIP] x"),
            Some((AlertKind::Tip, 9, None))
        ));
        assert_eq!(AlertKind::Caution.label(), "Caution");
    }

    #[test]
    fn alert_fold_markers_and_toggle() {
        // `-` = folded, `+` = open; the strip consumes the fold char.
        assert!(matches!(
            alert_marker("[!NOTE]-\nbody"),
            Some((AlertKind::Note, 9, Some(true)))
        ));
        assert!(matches!(
            alert_marker("[!NOTE]+ inline"),
            Some((AlertKind::Note, 9, Some(false)))
        ));
        assert!(matches!(
            alert_prefix(" [!TIP]- x"),
            Some((AlertKind::Tip, 9, Some(true)))
        ));
        // A `-` not directly after `]` is body text, not a fold marker.
        assert!(matches!(
            alert_marker("[!NOTE] - item"),
            Some((AlertKind::Note, 8, None))
        ));

        // The fold char locates + flips within a full source line.
        assert_eq!(alert_fold_char("> [!NOTE]- body"), Some((9, true)));
        assert_eq!(alert_fold_char("> [!NOTE] body"), None);
        let src = "before\n> [!TIP]- hidden\n> more\nafter";
        let toggled = toggle_alert_fold_at(src, 10).unwrap();
        assert_eq!(toggled, "before\n> [!TIP]+ hidden\n> more\nafter");
        let back = toggle_alert_fold_at(&toggled, 10).unwrap();
        assert_eq!(back, src);
        assert!(toggle_alert_fold_at("plain text", 2).is_none());
    }

    #[test]
    fn block_ids_and_anchor_links() {
        assert_eq!(
            block_id("Decision made. ^decision1"),
            Some((14, "decision1"))
        );
        assert_eq!(block_id("trailing space ^id  "), Some((14, "id")));
        assert_eq!(block_id("no anchor"), None);
        assert_eq!(block_id("mid ^id not at end"), None);
        assert_eq!(block_id("bad chars ^a b"), None);

        assert_eq!(split_block_anchor("Note#^id"), ("Note", Some("id")));
        assert_eq!(split_block_anchor("Note"), ("Note", None));
        // A bare `#` is part of the title, not an anchor.
        assert_eq!(split_block_anchor("C# Notes"), ("C# Notes", None));
        assert_eq!(split_block_anchor("file.pdf#p3"), ("file.pdf#p3", None));

        let src = "intro\nthe fact ^fact-1\nmore";
        assert_eq!(find_block_line(src, "fact-1"), Some(6));
        assert_eq!(find_block_line(src, "nope"), None);
    }

    #[test]
    fn embeds_and_extraction() {
        assert_eq!(embed_line("![[Note]]"), Some("Note"));
        assert_eq!(embed_line("  ![[Note#^id]]  "), Some("Note#^id"));
        assert_eq!(embed_line("text ![[Note]]"), None); // not standalone
        assert_eq!(embed_line("![[]]"), None);
        assert_eq!(embed_line("[[Note]]"), None);

        let src = "pre\nthe block ^b1\n## Sec\nbody\nmore\n### Sub\ndeep\n## Next\nafter";
        assert_eq!(&src[extract_block(src, "b1").unwrap()], "the block ^b1");
        // A section runs through its subsections, stopping at the next
        // same-or-higher heading.
        assert_eq!(
            &src[extract_section(src, "Sec").unwrap()],
            "## Sec\nbody\nmore\n### Sub\ndeep"
        );
        assert_eq!(
            &src[extract_section(src, "Next").unwrap()],
            "## Next\nafter"
        );
        assert!(extract_section(src, "missing").is_none());
    }

    #[test]
    fn heading_anchors() {
        assert_eq!(
            split_heading_anchor("Note#My Heading"),
            ("Note", Some("My Heading"))
        );
        assert_eq!(split_heading_anchor("Note"), ("Note", None));
        // Block anchors, PDFs, and empty sides don't split as headings.
        assert_eq!(split_heading_anchor("Note#^id"), ("Note#^id", None));
        assert_eq!(split_heading_anchor("file.pdf#p3"), ("file.pdf#p3", None));
        assert_eq!(split_heading_anchor("#Heading"), ("#Heading", None));
        assert_eq!(split_heading_anchor("Note#"), ("Note#", None));

        let src = "intro\n## My Heading\nbody\n```\n# not a heading\n```\n### Deep One";
        // Case-insensitive, trimmed; fences skipped.
        assert_eq!(find_heading_line(src, "my heading"), Some(6));
        assert_eq!(find_heading_line(src, " Deep One "), Some(49));
        assert_eq!(find_heading_line(src, "not a heading"), None);
        assert_eq!(find_heading_line(src, "missing"), None);
    }

    #[test]
    fn property_recognition() {
        assert_eq!(
            property("attendees:: Bob, Sue"),
            Some(("attendees", "Bob, Sue"))
        );
        assert_eq!(property("  time::3:00pm"), Some(("time", "3:00pm")));
        assert_eq!(property("owner:: [[Sue]]"), Some(("owner", "[[Sue]]")));
        // Not properties: prose with `::`, wiki links, empty/bad keys.
        assert_eq!(property("See [[Page::sub]] here"), None);
        assert_eq!(property("just prose"), None);
        assert_eq!(property(":: value"), None);
        assert_eq!(property("1key:: v"), None);
    }

    #[test]
    fn prefixed_property_tolerates_list_markers() {
        // Plain / indented lines: prefix is the indent.
        assert_eq!(prefixed_property("k:: v"), Some(("", "k", "v")));
        assert_eq!(prefixed_property("  k:: v"), Some(("  ", "k", "v")));
        // List markers (Logseq props-only block), bullets and numbers.
        assert_eq!(prefixed_property("- k:: v"), Some(("- ", "k", "v")));
        assert_eq!(prefixed_property("  * k:: v"), Some(("  * ", "k", "v")));
        assert_eq!(prefixed_property("2. k:: v"), Some(("2. ", "k", "v")));
        // Not properties: a plain bullet, a task, a numberless dot.
        assert_eq!(prefixed_property("- plain bullet"), None);
        assert_eq!(prefixed_property("- [ ] k:: v"), None);
        assert_eq!(prefixed_property(". k:: v"), None);
    }

    #[test]
    fn property_value_segments_pill_and_plain() {
        let segs = property_value_segments("[[Bob]], [[Sue|Susan]] and #work done");
        // Bob pill, ", " text, Susan pill, " and " text, work tag, " done" text.
        assert!(matches!(&segs[0], PropSeg::Pill { label, is_tag: false, .. } if label == "Bob"));
        assert!(matches!(&segs[1], PropSeg::Text(t) if t == ", "));
        assert!(matches!(&segs[2], PropSeg::Pill { label, .. } if label == "Susan"));
        assert!(matches!(&segs[4], PropSeg::Pill { label, is_tag: true, .. } if label == "work"));
        // A plain value is a single text segment.
        assert!(
            matches!(property_value_segments("active").as_slice(), [PropSeg::Text(t)] if t == "active")
        );
    }

    #[test]
    fn links_cover_every_kind() {
        let hits = links("see [[Page|alias]] and [x](https://a.io) #tag/sub https://b.io/p, done");
        assert_eq!(
            hits.iter().map(|(_, h)| h).collect::<Vec<_>>(),
            vec![
                &LinkHit::Page("Page".into()),
                &LinkHit::Url("https://a.io".into()),
                &LinkHit::Page("tag/sub".into()),
                &LinkHit::Url("https://b.io/p".into()), // trailing comma trimmed
            ]
        );
        // Opaque: code spans, images, footnotes, glued #.
        assert!(links("`https://x.io` ![a](i.png) [^1] word#no").is_empty());
        // Regression: multi-byte text before a URL must not panic the
        // byte-wise walk (it once str-sliced at a continuation byte).
        let hits = links("shrug ¯\\_(ツ)_/¯ then https://a.io done");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn ordered_markers_cycle_word_style() {
        assert_eq!(ordered_marker(0, 2), "2.");
        assert_eq!(ordered_marker(1, 1), "a.");
        assert_eq!(ordered_marker(1, 27), "aa.");
        assert_eq!(ordered_marker(2, 4), "iv.");
        assert_eq!(ordered_marker(2, 9), "ix.");
        assert_eq!(ordered_marker(3, 2), "2."); // cycle restarts
    }

    #[test]
    fn table_style_markers_parse() {
        assert_eq!(
            table_style_marker("<!-- table:striped -->"),
            Some(TableStyle::Striped)
        );
        assert_eq!(table_style_marker("<!-- math:left -->"), None);
        assert_eq!(table_style_marker("plain text"), None);
    }
}
