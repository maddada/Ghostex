//! Placing notes in a document. Notes store quote text, not offsets, so every render finds each
//! quote again: the document is whitespace-normalized with a map back to the source, the
//! normalized quote is searched for, and every occurrence is a highlighted range. All ranges are
//! byte ranges into the Rust string; they cover the same characters the page's UTF-16 ranges do.
//!
//! CDXC:Docs 2026-09-24 SEE-ALSO: apps/desktop/views/manage/annotation-store.tsx
//! (`findManageAnnotationTextMatches`, `buildManageNormalizedTextIndex`) and meo-toolbar.tsx
//! (`collectManageAnnotationRanges`, `findManageAnnotationRangeAtPosition`). A quote must resolve
//! to the same text on both sides while the React page still writes notes.

use std::ops::Range;

use super::model::{
    DocsAnnotation, DocsAnnotationScope, is_js_whitespace, normalize_annotation_quote, utf16_len,
};

/// The document with every whitespace run collapsed to one space and the ends trimmed, plus,
/// for every byte of that text, the source byte range of the character it came from. A collapsed
/// space maps to the first whitespace character of its run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocsNormalizedText {
    pub(crate) text: String,
    source_ranges: Vec<Range<usize>>,
}

impl DocsNormalizedText {
    pub(crate) fn new(source: &str) -> Self {
        let mut text = String::with_capacity(source.len());
        let mut source_ranges = Vec::with_capacity(source.len());
        let mut previous_was_whitespace = true;
        for (offset, character) in source.char_indices() {
            let source_range = offset..offset + character.len_utf8();
            if is_js_whitespace(character) {
                if !previous_was_whitespace {
                    text.push(' ');
                    source_ranges.push(source_range);
                    previous_was_whitespace = true;
                }
                continue;
            }
            text.push(character);
            source_ranges.extend(std::iter::repeat_n(source_range, character.len_utf8()));
            previous_was_whitespace = false;
        }
        if text.ends_with(' ') {
            text.pop();
            source_ranges.pop();
        }
        Self {
            text,
            source_ranges,
        }
    }

    /// Source byte ranges of every non-overlapping occurrence of `quote`, which is normalized
    /// first. An empty quote matches nothing.
    pub(crate) fn find_all(&self, quote: &str) -> Vec<Range<usize>> {
        let quote = normalize_annotation_quote(quote);
        if quote.is_empty() {
            return Vec::new();
        }
        let mut matches = Vec::new();
        let mut from = 0;
        while from < self.text.len() {
            let Some(found) = self.text[from..].find(&quote) else {
                break;
            };
            let start = from + found;
            let end = start + quote.len();
            matches.push(self.source_ranges[start].start..self.source_ranges[end - 1].end);
            from = end;
        }
        matches
    }
}

/// Every place `quote` occurs in `text`, as source byte ranges (`findManageAnnotationTextMatches`).
pub(crate) fn find_annotation_text_matches(text: &str, quote: &str) -> Vec<Range<usize>> {
    DocsNormalizedText::new(text).find_all(quote)
}

/// Source byte offset of the first occurrence, the feedback's document-order key.
pub(crate) fn first_annotation_match(text: &str, quote: &str) -> Option<Range<usize>> {
    find_annotation_text_matches(text, quote).into_iter().next()
}

/// One highlighted occurrence of a note's quote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocsAnnotationRange {
    /// Index into the annotations slice the range was resolved from.
    pub(crate) annotation_index: usize,
    /// Source byte range.
    pub(crate) range: Range<usize>,
}

/// Every occurrence of every selection note, in note order then document order
/// (`collectManageAnnotationRanges`). Global notes have no range.
pub(crate) fn collect_annotation_ranges(
    text: &str,
    annotations: &[DocsAnnotation],
) -> Vec<DocsAnnotationRange> {
    if !annotations
        .iter()
        .any(|annotation| annotation.scope == DocsAnnotationScope::Selection)
    {
        return Vec::new();
    }
    let normalized = DocsNormalizedText::new(text);
    let mut ranges = Vec::new();
    for (annotation_index, annotation) in annotations.iter().enumerate() {
        if annotation.scope != DocsAnnotationScope::Selection {
            continue;
        }
        for range in normalized.find_all(&annotation.quote) {
            ranges.push(DocsAnnotationRange {
                annotation_index,
                range,
            });
        }
    }
    ranges
}

/// The note under a caret at source byte `position` with nothing selected: the shortest range
/// containing it, the earlier one on a tie (`findManageAnnotationRangeAtPosition`). Length is
/// compared in UTF-16 units so the same note wins as on the page.
pub(crate) fn annotation_range_at(
    text: &str,
    annotations: &[DocsAnnotation],
    position: usize,
) -> Option<DocsAnnotationRange> {
    collect_annotation_ranges(text, annotations)
        .into_iter()
        .filter(|candidate| position >= candidate.range.start && position < candidate.range.end)
        .min_by_key(|candidate| {
            (
                utf16_len(&text[candidate.range.clone()]),
                candidate.range.start,
            )
        })
}

/// The quote a selection of source bytes `from..to` would store, or `None` when it is only
/// whitespace. Out-of-range or reversed bounds are clamped like the editor clamps them.
pub(crate) fn capture_selection_quote(text: &str, from: usize, to: usize) -> Option<String> {
    let start = floor_char_boundary(text, from.min(to).min(text.len()));
    let end = floor_char_boundary(text, from.max(to).min(text.len())).max(start);
    let quote = normalize_annotation_quote(&text[start..end]);
    (!quote.is_empty()).then_some(quote)
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
