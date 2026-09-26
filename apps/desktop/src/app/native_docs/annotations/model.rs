//! The Docs annotation model: one note on a document (a comment or a "remove this" redline, on a
//! quoted passage or on the whole file), its quick labels and colours, the rules that decide
//! whether a note is valid and whether it still has to be sent, and the JavaScript-compatible text
//! helpers (whitespace class, trimming, UTF-16 lengths) the rest of the module needs to produce the
//! same values the React Docs page produces.
//!
//! CDXC:Docs 2026-09-24 SEE-ALSO: apps/desktop/views/manage/types.ts, annotation-store.tsx,
//! constants.ts and preview/manage-preview.tsx (`addAnnotation`). The field names, id shape,
//! quote normalization and limits must match the React page until it is deleted, because both
//! read and write the same sidecar.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Serialize, Serializer};

/// Longest stored quote, in UTF-16 code units (`MANAGE_SELECTION_MAX_LENGTH`).
pub(crate) const DOCS_ANNOTATION_QUOTE_MAX_LEN: usize = 700;
/// Longest stored note, in UTF-16 code units.
pub(crate) const DOCS_ANNOTATION_NOTE_MAX_LEN: usize = 4_000;
/// Largest image attachment, in bytes (`MANAGE_ANNOTATION_IMAGE_MAX_BYTES`).
pub(crate) const DOCS_ANNOTATION_IMAGE_MAX_BYTES: f64 = 512.0 * 1024.0;
/// Most image attachments on one note (`MANAGE_ANNOTATION_MAX_IMAGES`).
pub(crate) const DOCS_ANNOTATION_MAX_IMAGES: usize = 4;
/// Longest attachment file name, in UTF-16 code units.
pub(crate) const DOCS_ANNOTATION_ATTACHMENT_NAME_MAX_LEN: usize = 80;
/// Preview cards quote at most this many UTF-16 code units before cutting to `...`.
pub(crate) const DOCS_ANNOTATION_PREVIEW_MAX_LEN: usize = 150;

pub(crate) const DOCS_COMMENT_ANNOTATION_COLOR: &str = "#e2b340";
pub(crate) const DOCS_REDLINE_ANNOTATION_COLOR: &str = "#fda4af";
pub(crate) const DOCS_REMOVE_TOOLBAR_COLOR: &str = "#f87171";

pub(crate) const DOCS_ANNOTATION_ID_PREFIX: &str = "manage-annotation-";
pub(crate) const DOCS_ANNOTATION_IMAGE_ID_PREFIX: &str = "manage-annotation-image-";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DocsAnnotationType {
    #[default]
    Comment,
    Redline,
}

impl DocsAnnotationType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Redline => "redline",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "comment" => Some(Self::Comment),
            "redline" => Some(Self::Redline),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DocsAnnotationScope {
    Global,
    Selection,
}

impl DocsAnnotationScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Selection => "selection",
        }
    }

    /// A note with a quote is anchored to that text; one without is about the whole document.
    pub(crate) fn for_quote(quote: &str) -> Self {
        if quote.is_empty() {
            Self::Global
        } else {
            Self::Selection
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DocsQuickLabelId {
    Clarify,
    NeedsTests,
    LooksGood,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DocsQuickLabel {
    pub(crate) id: DocsQuickLabelId,
    pub(crate) text: &'static str,
    pub(crate) color: &'static str,
}

/// The selection toolbar's quick labels, in toolbar order (`MANAGE_QUICK_LABELS`).
pub(crate) const DOCS_QUICK_LABELS: [DocsQuickLabel; 3] = [
    DocsQuickLabel {
        id: DocsQuickLabelId::Clarify,
        text: "Clarify",
        color: "#a78bfa",
    },
    DocsQuickLabel {
        id: DocsQuickLabelId::NeedsTests,
        text: "Needs tests",
        color: "#f59e0b",
    },
    DocsQuickLabel {
        id: DocsQuickLabelId::LooksGood,
        text: "Looks good",
        color: "#86efac",
    },
];

impl DocsQuickLabelId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Clarify => "clarify",
            Self::NeedsTests => "needs-tests",
            Self::LooksGood => "looks-good",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "clarify" => Some(Self::Clarify),
            "needs-tests" => Some(Self::NeedsTests),
            "looks-good" => Some(Self::LooksGood),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static DocsQuickLabel {
        match self {
            Self::Clarify => &DOCS_QUICK_LABELS[0],
            Self::NeedsTests => &DOCS_QUICK_LABELS[1],
            Self::LooksGood => &DOCS_QUICK_LABELS[2],
        }
    }

    pub(crate) fn text(self) -> &'static str {
        self.label().text
    }

    pub(crate) fn color(self) -> &'static str {
        self.label().color
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DocsAnnotationImage {
    pub(crate) data_url: String,
    pub(crate) id: String,
    pub(crate) mime_type: String,
    pub(crate) name: String,
    /// A JavaScript number in the sidecar; written without a fraction when it is whole.
    #[serde(serialize_with = "serialize_js_number")]
    pub(crate) size: f64,
}

/// One note. Field order is the order the React page writes after loading the sidecar, so a
/// save from either side produces the same bytes.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DocsAnnotation {
    pub(crate) attachments: Vec<DocsAnnotationImage>,
    pub(crate) created_at: String,
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label_id: Option<DocsQuickLabelId>,
    pub(crate) note: String,
    pub(crate) quote: String,
    pub(crate) scope: DocsAnnotationScope,
    #[serde(rename = "type")]
    pub(crate) kind: DocsAnnotationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sent_at: Option<String>,
}

/// What the toolbar or composer hands over when the user adds a note.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocsNewAnnotation {
    pub(crate) kind: DocsAnnotationType,
    pub(crate) quote: String,
    pub(crate) note: String,
    pub(crate) label_id: Option<DocsQuickLabelId>,
    pub(crate) attachments: Vec<DocsAnnotationImage>,
}

impl DocsAnnotation {
    /// Builds a new note the way the page's `addAnnotation` does: the quote is normalized, the
    /// note trimmed (and capped at 4000 like every other write path), the scope derived from the
    /// quote. Returns `None` for a note the validation rules reject.
    pub(crate) fn create(input: DocsNewAnnotation, id: String, now_ms: i64) -> Option<Self> {
        let kind = input.kind;
        let quote = normalize_annotation_quote(&input.quote);
        let note = normalize_annotation_note(&input.note);
        if !is_valid_annotation(kind, &quote, &note, input.attachments.len()) {
            return None;
        }
        Some(Self {
            attachments: input.attachments,
            created_at: iso_timestamp_from_ms(now_ms),
            id,
            label_id: input.label_id,
            note,
            scope: DocsAnnotationScope::for_quote(&quote),
            quote,
            kind,
            updated_at: None,
            sent_at: None,
        })
    }

    /// Never sent, or edited after its last send (`isManageAnnotationPending`).
    pub(crate) fn is_pending(&self) -> bool {
        let Some(sent_at) = self.sent_at.as_deref() else {
            return true;
        };
        let Some(sent) = js_date_parse_ms(sent_at) else {
            return true;
        };
        let edited = self.updated_at.as_deref().unwrap_or(&self.created_at);
        js_date_parse_ms(edited).is_some_and(|edited| edited > sent)
    }

    /// Replaces the note (trimmed, capped) and stamps `updatedAt` strictly after the last send so
    /// the edit counts as pending (`editAnnotationNote`).
    pub(crate) fn edit_note(&mut self, note: &str, now_ms: i64) {
        self.note = normalize_annotation_note(note);
        self.updated_at = Some(timestamp_after(self.sent_at.as_deref(), now_ms));
    }

    /// The dropdown and card title: "Remove this", the quick label, "Comment" or "Global comment".
    pub(crate) fn type_label(&self) -> &'static str {
        if self.kind == DocsAnnotationType::Redline {
            return "Remove this";
        }
        if let Some(label_id) = self.label_id {
            return label_id.text();
        }
        match self.scope {
            DocsAnnotationScope::Global => "Global comment",
            DocsAnnotationScope::Selection => "Comment",
        }
    }

    /// The highlight colour: redline rose, quick-label colour, or the plain comment amber.
    pub(crate) fn color(&self) -> &'static str {
        annotation_color(self.kind, self.label_id)
    }

    /// The note as cards show it: trimmed, and empty when it only repeats the quick label.
    pub(crate) fn display_note(&self) -> &str {
        let note = js_trim(&self.note);
        match self.label_id {
            Some(label_id) if note == label_id.text() => "",
            _ => note,
        }
    }

    /// The one-line preview a card shows (`annotationPreviewText`).
    pub(crate) fn preview_text(&self) -> String {
        let note = self.display_note();
        if !note.is_empty() {
            return truncate_annotation_preview_text(note);
        }
        if let Some(label_id) = self.label_id {
            return label_id.text().to_string();
        }
        if self.kind == DocsAnnotationType::Redline {
            return "Marked for deletion".to_string();
        }
        truncate_annotation_preview_text(&self.quote)
    }
}

/// A redline needs a quote; a comment needs a quote, a note or an attachment.
pub(crate) fn is_valid_annotation(
    kind: DocsAnnotationType,
    quote: &str,
    note: &str,
    attachment_count: usize,
) -> bool {
    match kind {
        DocsAnnotationType::Redline => !quote.is_empty(),
        DocsAnnotationType::Comment => {
            !quote.is_empty() || !js_trim(note).is_empty() || attachment_count > 0
        }
    }
}

pub(crate) fn annotation_color(
    kind: DocsAnnotationType,
    label_id: Option<DocsQuickLabelId>,
) -> &'static str {
    match (kind, label_id) {
        (DocsAnnotationType::Redline, _) => DOCS_REDLINE_ANNOTATION_COLOR,
        (DocsAnnotationType::Comment, Some(label_id)) => label_id.color(),
        (DocsAnnotationType::Comment, None) => DOCS_COMMENT_ANNOTATION_COLOR,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocsAnnotationReviewCounts {
    pub(crate) pending: usize,
    pub(crate) sent: usize,
}

pub(crate) fn annotation_review_counts(
    annotations: &[DocsAnnotation],
) -> DocsAnnotationReviewCounts {
    let mut counts = DocsAnnotationReviewCounts::default();
    for annotation in annotations {
        if annotation.is_pending() {
            counts.pending += 1;
        } else {
            counts.sent += 1;
        }
    }
    counts
}

/// Stamps `sentAt` on exactly the listed notes (`markAnnotationsSent`). Returns whether any matched.
pub(crate) fn mark_annotations_sent(
    annotations: &mut [DocsAnnotation],
    ids: &[String],
    sent_at: &str,
) -> bool {
    let mut changed = false;
    for annotation in annotations.iter_mut() {
        if ids.contains(&annotation.id) {
            annotation.sent_at = Some(sent_at.to_string());
            changed = true;
        }
    }
    changed
}

/// `manage-annotation-<ms>-<hex>`, the page's id shape.
pub(crate) fn annotation_id(now_ms: i64, random_hex: &str) -> String {
    format!("{DOCS_ANNOTATION_ID_PREFIX}{now_ms}-{random_hex}")
}

/// `manage-annotation-image-<ms>-<hex>`, the page's attachment id shape.
pub(crate) fn annotation_image_id(now_ms: i64, random_hex: &str) -> String {
    format!("{DOCS_ANNOTATION_IMAGE_ID_PREFIX}{now_ms}-{random_hex}")
}

/// Thirteen random lowercase hex digits, the length `Math.random().toString(16).slice(2)` usually has.
pub(crate) fn random_hex_suffix() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..13].to_string()
}

/// Milliseconds since the Unix epoch, like `Date.now()`.
pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// `new Date(ms).toISOString()`: UTC with exactly three fraction digits and a `Z`.
pub(crate) fn iso_timestamp_from_ms(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// A timestamp strictly later than `after`, so an edit in the same millisecond as a send (or with
/// a clock that moved backwards) still counts as pending (`manageAnnotationTimestampAfter`).
pub(crate) fn timestamp_after(after: Option<&str>, now_ms: i64) -> String {
    let floor = after.and_then(js_date_parse_ms);
    let ms = match floor {
        Some(floor) if floor >= now_ms => floor + 1,
        _ => now_ms,
    };
    iso_timestamp_from_ms(ms)
}

/// `Date.parse` for the shapes Docs meets: full ISO/RFC 3339 timestamps (what `toISOString`
/// writes), date-only ISO (UTC), and ISO date-times without an offset (local time). Other legacy
/// formats V8 also accepts count as invalid.
pub(crate) fn js_date_parse_ms(value: &str) -> Option<i64> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.timestamp_millis());
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|midnight| midnight.and_utc().timestamp_millis());
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return Local
                .from_local_datetime(&naive)
                .earliest()
                .map(|local| local.timestamp_millis());
        }
    }
    None
}

/// Collapses whitespace runs to one space, trims, and caps at 700 UTF-16 units
/// (`normalizeAnnotationQuote`).
pub(crate) fn normalize_annotation_quote(text: &str) -> String {
    let collapsed = collapse_js_whitespace(text, " ");
    truncate_utf16(js_trim(&collapsed), DOCS_ANNOTATION_QUOTE_MAX_LEN).to_string()
}

/// A note as it is stored after add or edit: trimmed, at most 4000 UTF-16 units.
pub(crate) fn normalize_annotation_note(note: &str) -> String {
    truncate_utf16(js_trim(note), DOCS_ANNOTATION_NOTE_MAX_LEN).to_string()
}

/// Trims, turns whitespace runs into `-`, caps at 80 UTF-16 units, and falls back to `image`
/// (`normalizeAttachmentName`).
pub(crate) fn normalize_attachment_name(name: &str) -> String {
    let dashed = collapse_js_whitespace(js_trim(name), "-");
    if dashed.is_empty() {
        "image".to_string()
    } else {
        truncate_utf16(&dashed, DOCS_ANNOTATION_ATTACHMENT_NAME_MAX_LEN).to_string()
    }
}

/// `truncateManageAnnotationPreviewText`: normalized, then cut to 147 units plus `...` past 150.
pub(crate) fn truncate_annotation_preview_text(text: &str) -> String {
    let normalized = normalize_annotation_quote(text);
    if utf16_len(&normalized) > DOCS_ANNOTATION_PREVIEW_MAX_LEN {
        format!(
            "{}...",
            truncate_utf16(&normalized, DOCS_ANNOTATION_PREVIEW_MAX_LEN - 3)
        )
    } else {
        normalized
    }
}

/// The characters JavaScript's `\s` and `String.prototype.trim` treat as whitespace. Differs from
/// `char::is_whitespace` on U+0085 (not JS whitespace) and U+FEFF (JS whitespace).
pub(crate) fn is_js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{9}'..='\u{D}'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_whitespace)
}

pub(crate) fn js_trim_end(text: &str) -> &str {
    text.trim_end_matches(is_js_whitespace)
}

/// `text.replace(/\s+/g, replacement)`.
pub(crate) fn collapse_js_whitespace(text: &str, replacement: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut in_whitespace = false;
    for character in text.chars() {
        if is_js_whitespace(character) {
            if !in_whitespace {
                collapsed.push_str(replacement);
                in_whitespace = true;
            }
        } else {
            collapsed.push(character);
            in_whitespace = false;
        }
    }
    collapsed
}

/// A JavaScript string's `length`.
pub(crate) fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// `text.slice(0, max_units)` in UTF-16 units. Where JavaScript would keep the first half of a
/// surrogate pair, this stops before the character, since a Rust string cannot hold half of one.
pub(crate) fn truncate_utf16(text: &str, max_units: usize) -> &str {
    let mut units = 0;
    for (index, character) in text.char_indices() {
        units += character.len_utf16();
        if units > max_units {
            return &text[..index];
        }
    }
    text
}

/// Writes a number the way `JSON.stringify` does for the values Docs stores: whole numbers
/// without a fraction.
fn serialize_js_number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}
