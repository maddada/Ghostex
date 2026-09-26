//! The feedback text Send and Copy deliver to an agent: one `# Annotations on <name>` block per
//! document, then one numbered section per note in document order with its line span, a short
//! quote (the full quote in a fence for "remove this") and the note as a blockquote.
//!
//! CDXC:Docs 2026-09-24 SEE-ALSO: apps/desktop/views/manage/annotation-feedback.ts
//! (`formatManageAnnotationFeedback`, the 2026-09-15 format DECISION) and manage-app.tsx
//! (`sendAnnotationFeedback`, which names documents). The output must stay byte-identical to the
//! React page's until that page is deleted, and apps/desktop/src/app/docs_annotation_feedback.rs
//! delivers it.

use super::anchoring::first_annotation_match;
use super::model::{
    DocsAnnotation, DocsAnnotationScope, DocsAnnotationType, DocsQuickLabelId,
    collapse_js_whitespace, js_trim, js_trim_end, truncate_utf16, utf16_len,
};

/// How much of the annotated text a section quotes inline; the line span anchors the rest.
const FEEDBACK_QUOTE_MAX_LEN: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocsFeedbackScope {
    /// Every note (Resend all, Copy feedback).
    All,
    /// Only notes never sent or edited since their last send.
    Pending,
}

/// One document's notes going into the feedback.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DocsFeedbackDocument<'a> {
    /// The file as the Docs tree names it (already made unique), or a review document's title.
    pub(crate) name: &'a str,
    /// Source used to order the notes and number their lines: the draft for the open file, a
    /// fresh read for others. `None` or empty puts every note in insertion order without lines.
    pub(crate) content: Option<&'a str>,
    pub(crate) annotations: &'a [DocsAnnotation],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocsAnnotationFeedback {
    pub(crate) text: String,
    /// Notes the text covers.
    pub(crate) count: usize,
    /// Documents that contributed at least one note.
    pub(crate) document_count: usize,
    /// Ids of every note the text covers, by document name, in output order. Used to mark
    /// exactly those notes sent.
    pub(crate) annotation_ids_by_document: Vec<(String, Vec<String>)>,
}

/// Builds the feedback text (`formatManageAnnotationFeedback`). With nothing included the text
/// is `No annotations.\n`.
pub(crate) fn format_annotation_feedback(
    documents: &[DocsFeedbackDocument<'_>],
    scope: DocsFeedbackScope,
) -> DocsAnnotationFeedback {
    let mut sections: Vec<String> = Vec::new();
    let mut annotation_ids_by_document: Vec<(String, Vec<String>)> = Vec::new();
    let mut count = 0;
    for document in documents {
        let included: Vec<&DocsAnnotation> = document
            .annotations
            .iter()
            .filter(|annotation| scope == DocsFeedbackScope::All || annotation.is_pending())
            .collect();
        if included.is_empty() {
            continue;
        }
        let placed = place_annotations(&included, document.content.unwrap_or(""));
        let mut lines: Vec<String> =
            vec![format!("# Annotations on {}", document.name), String::new()];
        for (index, entry) in placed.iter().enumerate() {
            lines.extend(format_annotation_section(entry, index + 1));
            lines.push(String::new());
        }
        sections.push(js_trim_end(&lines.join("\n")).to_string());
        let ids: Vec<String> = placed
            .iter()
            .map(|entry| entry.annotation.id.clone())
            .collect();
        match annotation_ids_by_document
            .iter_mut()
            .find(|(name, _)| name == document.name)
        {
            Some((_, existing)) => *existing = ids,
            None => annotation_ids_by_document.push((document.name.to_string(), ids)),
        }
        count += placed.len();
    }
    let text = if sections.is_empty() {
        "No annotations.\n".to_string()
    } else {
        format!("{}\n", sections.join("\n\n"))
    };
    DocsAnnotationFeedback {
        document_count: sections.len(),
        text,
        count,
        annotation_ids_by_document,
    }
}

/// Gives each document of a send a unique name: a second document with a name already used is
/// called `name (path)`. Keeps the name-to-path map that turns the feedback's ids by document
/// back into ids by path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocsFeedbackDocumentNames {
    path_by_name: Vec<(String, String)>,
}

impl DocsFeedbackDocumentNames {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The name to put in the feedback for the document at `path` the tree calls `name`.
    pub(crate) fn add(&mut self, path: &str, name: &str) -> String {
        let unique = if self.path_for(name).is_some() {
            format!("{name} ({path})")
        } else {
            name.to_string()
        };
        match self
            .path_by_name
            .iter_mut()
            .find(|(candidate, _)| *candidate == unique)
        {
            Some((_, existing)) => *existing = path.to_string(),
            None => self.path_by_name.push((unique.clone(), path.to_string())),
        }
        unique
    }

    pub(crate) fn path_for(&self, name: &str) -> Option<&str> {
        self.path_by_name
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, path)| path.as_str())
    }

    /// The feedback's ids by document name as ids by path, for marking notes sent.
    pub(crate) fn ids_by_path(
        &self,
        feedback: &DocsAnnotationFeedback,
    ) -> Vec<(String, Vec<String>)> {
        let mut ids_by_path: Vec<(String, Vec<String>)> = Vec::new();
        for (name, ids) in &feedback.annotation_ids_by_document {
            let Some(path) = self.path_for(name) else {
                continue;
            };
            match ids_by_path
                .iter_mut()
                .find(|(candidate, _)| candidate == path)
            {
                Some((_, existing)) => *existing = ids.clone(),
                None => ids_by_path.push((path.to_string(), ids.clone())),
            }
        }
        ids_by_path
    }
}

struct PlacedAnnotation<'a> {
    annotation: &'a DocsAnnotation,
    /// 1-based first and last source line of the quote; absent for global notes and quotes no
    /// longer in the text.
    lines: Option<(usize, usize)>,
}

/// Document order by each quote's first match; global notes and lost quotes go last in
/// insertion order.
fn place_annotations<'a>(
    annotations: &[&'a DocsAnnotation],
    content: &str,
) -> Vec<PlacedAnnotation<'a>> {
    let mut placed: Vec<((bool, usize), PlacedAnnotation<'a>)> = annotations
        .iter()
        .enumerate()
        .map(|(index, annotation)| {
            let found = (annotation.scope == DocsAnnotationScope::Selection && !content.is_empty())
                .then(|| first_annotation_match(content, &annotation.quote))
                .flatten();
            match found {
                Some(range) => {
                    let first = line_number_at(content, range.start);
                    let last = line_number_at(content, range.start.max(range.end - 1));
                    (
                        (false, range.start),
                        PlacedAnnotation {
                            annotation,
                            lines: Some((first, last)),
                        },
                    )
                }
                None => (
                    (true, index),
                    PlacedAnnotation {
                        annotation,
                        lines: None,
                    },
                ),
            }
        })
        .collect();
    placed.sort_by_key(|(order, _)| *order);
    placed.into_iter().map(|(_, entry)| entry).collect()
}

fn line_number_at(content: &str, offset: usize) -> usize {
    let end = offset.min(content.len());
    1 + content.as_bytes()[..end]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

fn format_annotation_section(entry: &PlacedAnnotation<'_>, number: usize) -> Vec<String> {
    let annotation = entry.annotation;
    let line_label = match entry.lines {
        Some((first, last)) if first == last => format!(" (line {first})"),
        // The separator is an EN DASH (U+2013), part of the format the agent reads.
        Some((first, last)) => format!(" (lines {first}\u{2013}{last})"),
        None => String::new(),
    };
    let mut lines = vec![format!("## Annotation {number}{line_label}")];
    let note = js_trim(&annotation.note);
    if annotation.kind == DocsAnnotationType::Redline {
        lines.push("Remove this:".to_string());
        lines.extend(fenced(&annotation.quote));
        let body = if note.is_empty() {
            "I don't want this."
        } else {
            note
        };
        lines.push(format!("> {}", blockquote(body)));
    } else if annotation.scope == DocsAnnotationScope::Global {
        lines.push("General comment:".to_string());
        let body = if note.is_empty() {
            default_label_note(annotation)
        } else {
            note
        };
        lines.push(format!("> {}", blockquote(body)));
    } else {
        lines.push(format!(
            "{}: \"{}\"",
            label_heading(annotation),
            short_quote(&annotation.quote)
        ));
        let body = if !note.is_empty() {
            note
        } else if annotation
            .label_id
            .is_some_and(|label_id| label_id != DocsQuickLabelId::LooksGood)
        {
            default_label_note(annotation)
        } else {
            ""
        };
        if !body.is_empty() {
            lines.push(format!("> {}", blockquote(body)));
        }
    }
    for attachment in &annotation.attachments {
        lines.push(format!(
            "- Attachment {}: {}",
            attachment.name, attachment.data_url
        ));
    }
    lines
}

fn label_heading(annotation: &DocsAnnotation) -> &'static str {
    match annotation.label_id {
        Some(DocsQuickLabelId::LooksGood) => "Looks good",
        Some(DocsQuickLabelId::Clarify) => "Clarify",
        Some(DocsQuickLabelId::NeedsTests) => "Needs tests",
        None => "Comment on",
    }
}

fn default_label_note(annotation: &DocsAnnotation) -> &'static str {
    match annotation.label_id {
        Some(DocsQuickLabelId::Clarify) => "Please clarify this.",
        Some(DocsQuickLabelId::NeedsTests) => "This needs tests.",
        Some(DocsQuickLabelId::LooksGood) => DocsQuickLabelId::LooksGood.text(),
        None => "(see attachment)",
    }
}

/// The quote on one line, cut to 159 units plus an ellipsis past 160.
fn short_quote(quote: &str) -> String {
    let collapsed = collapse_js_whitespace(quote, " ");
    let single = js_trim(&collapsed);
    if utf16_len(single) > FEEDBACK_QUOTE_MAX_LEN {
        format!(
            "{}\u{2026}",
            js_trim_end(truncate_utf16(single, FEEDBACK_QUOTE_MAX_LEN - 1))
        )
    } else {
        single.to_string()
    }
}

/// Continues a multi-line note with `> ` on each line (`\r\n` and `\n` both break).
fn blockquote(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\n> ")
}

/// A fence longer than any backtick run inside the text (at least three), so quoted markdown
/// cannot escape.
fn fenced(text: &str) -> [String; 3] {
    let longest = text
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    [fence.clone(), text.to_string(), fence]
}
