//! Docs annotations as pure logic: the note model, the sidecar file, finding quotes in a document,
//! and the feedback text sent to agents.

pub(crate) mod anchoring;
pub(crate) mod feedback;
pub(crate) mod model;
pub(crate) mod sidecar;

pub(crate) use anchoring::{
    DocsAnnotationRange, DocsNormalizedText, annotation_range_at, capture_selection_quote,
    collect_annotation_ranges, find_annotation_text_matches, first_annotation_match,
};
pub(crate) use feedback::{
    DocsAnnotationFeedback, DocsFeedbackDocument, DocsFeedbackDocumentNames, DocsFeedbackScope,
    format_annotation_feedback,
};
pub(crate) use model::{
    DOCS_QUICK_LABELS, DOCS_REMOVE_TOOLBAR_COLOR, DocsAnnotation, DocsAnnotationImage,
    DocsAnnotationReviewCounts, DocsAnnotationScope, DocsAnnotationType, DocsNewAnnotation,
    DocsQuickLabel, DocsQuickLabelId, annotation_id, annotation_review_counts,
    mark_annotations_sent, normalize_annotation_quote, now_ms, random_hex_suffix, timestamp_after,
};
pub(crate) use sidecar::{
    DOCS_ANNOTATIONS_SAVE_DELAY_MS, DOCS_ANNOTATIONS_SIDECAR_PATH, DocsAnnotationsByPath,
    parse_docs_annotations_sidecar, serialize_docs_annotations_sidecar,
    serialize_docs_annotations_sidecar_at,
};
