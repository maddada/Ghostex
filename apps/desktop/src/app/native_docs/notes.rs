//! Notes on documents (the Docs page's annotations): loading and saving the project's notes file,
//! adding, editing and removing notes, their highlights in the editor, and sending them to an
//! agent. The pure rules live in `annotations/`.

use std::time::{Duration, Instant};

use gpui::{
    AppContext as _, Bounds, ClipboardItem, Context, Entity, Hsla, Pixels, Subscription, Window,
    rgb,
};
use gpui_component::input::{InputEvent, InputState};
use serde_json::json;

use super::annotations::{
    DOCS_ANNOTATIONS_SAVE_DELAY_MS, DOCS_ANNOTATIONS_SIDECAR_PATH, DocsAnnotation,
    DocsAnnotationType, DocsFeedbackDocument, DocsFeedbackDocumentNames, DocsFeedbackScope,
    DocsNewAnnotation, DocsQuickLabelId, annotation_id, annotation_review_counts,
    capture_selection_quote, collect_annotation_ranges, format_annotation_feedback, now_ms,
    parse_docs_annotations_sidecar, random_hex_suffix, serialize_docs_annotations_sidecar_at,
};
use super::state::DocsFileKind;
use crate::GhostexGpuiApp;

/// How long Send's outcome stays on the button, and how long Clear stays armed.
pub(crate) const SEND_STATUS_DURATION: Duration = Duration::from_secs(4);
pub(crate) const CLEAR_ARM_DURATION: Duration = Duration::from_secs(3);

/// Where the last Send went, or why it did not.
#[derive(Clone, Debug)]
pub(crate) enum DocsSendStatus {
    Sending,
    Sent {
        delivery: &'static str,
        count: usize,
        files: usize,
    },
    Error(String),
    Notice(String),
}

/// The note composer: the text field and what the note will be attached to.
pub(crate) struct DocsComposer {
    pub(crate) input: Entity<InputState>,
    /// The selected source text; empty for a global comment.
    pub(crate) quote: String,
    /// The note being edited, if this is an edit rather than a new note.
    pub(crate) editing: Option<String>,
    /// Where the composer opens (the selection, or the global-comment button), in window
    /// coordinates.
    pub(crate) anchor: Bounds<Pixels>,
    pub(crate) _subscription: Subscription,
}

/// A `#rrggbb` colour at `alpha`.
pub(crate) fn hex(color: &str, alpha: f32) -> Hsla {
    let value = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0xe2b340);
    Hsla::from(rgb(value)).opacity(alpha)
}

impl GhostexGpuiApp {
    /// Reads the project's notes file. A missing or unreadable file is an empty set of notes.
    pub(crate) fn native_docs_load_notes(&mut self, cx: &mut Context<Self>) {
        let generation = self.native_docs.generation;
        let request =
            self.native_docs_request("read", json!({ "path": DOCS_ANNOTATIONS_SIDECAR_PATH }));
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            let content = response["file"]["content"].as_str().unwrap_or_default();
            let notes = parse_docs_annotations_sidecar(content, now_ms());
            this.native_docs.notes_saved_key = notes.stable_key();
            this.native_docs.notes = notes;
            this.native_docs.notes_loaded = true;
            this.native_docs.highlights_stale = true;
            this.native_docs_notify(cx);
        });
    }

    /// Records a change to the notes: highlights refresh now, the file is written a moment later.
    pub(crate) fn native_docs_notes_changed(&mut self, cx: &mut Context<Self>) {
        self.native_docs.highlights_stale = true;
        if !self.native_docs.notes_loaded {
            return;
        }
        let generation = self.native_docs.generation;
        self.native_docs.notes_save_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(DOCS_ANNOTATIONS_SAVE_DELAY_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.native_docs.generation != generation {
                    return;
                }
                let key = this.native_docs.notes.stable_key();
                if key == this.native_docs.notes_saved_key {
                    return;
                }
                // Review documents' notes live only in memory.
                let mut stored = this.native_docs.notes.clone();
                for path in stored.paths().map(str::to_string).collect::<Vec<_>>() {
                    if super::entry::is_review_path(&path) {
                        stored.remove(&path);
                    }
                }
                let content = serialize_docs_annotations_sidecar_at(&stored, now_ms());
                let request = this.native_docs_request(
                    "save",
                    json!({ "path": DOCS_ANNOTATIONS_SIDECAR_PATH, "content": content }),
                );
                this.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
                    if let Some(error) = response["error"].as_str() {
                        this.dispatch_gpui_workspace_action_toast(
                            "error",
                            "Couldn't save the notes",
                            error,
                            cx,
                        );
                    } else {
                        this.native_docs.notes_saved_key = key;
                    }
                });
            });
        }));
        self.native_docs_notify(cx);
    }

    /// The open document's notes.
    pub(crate) fn native_docs_active_notes(&self) -> &[DocsAnnotation] {
        self.native_docs
            .active
            .as_deref()
            .and_then(|path| self.native_docs.notes.get(path))
            .unwrap_or(&[])
    }

    /// The open Markdown document's editor text, which is what notes anchor into.
    fn native_docs_active_text(&self, cx: &gpui::App) -> Option<String> {
        let document = self.native_docs.active_document()?;
        (document.kind == DocsFileKind::Markdown).then_some(())?;
        super::live::document_text(document, cx)
    }

    /// Adds a note to the open document. `None` fields fall back to the selection.
    pub(crate) fn native_docs_add_note(
        &mut self,
        input: DocsNewAnnotation,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.native_docs.active.clone() else {
            return;
        };
        let now = now_ms();
        let Some(note) =
            DocsAnnotation::create(input, annotation_id(now, &random_hex_suffix()), now)
        else {
            return;
        };
        let mut notes = self
            .native_docs
            .notes
            .get(&path)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        notes.push(note);
        self.native_docs.notes.set(&path, notes);
        self.native_docs_notes_changed(cx);
    }

    /// The selected source text as a note quote, when the open Markdown editor has a selection,
    /// and where the selection is on screen.
    pub(crate) fn native_docs_selection_quote(
        &self,
        cx: &gpui::App,
    ) -> Option<(String, Bounds<Pixels>)> {
        let document = self.native_docs.active_document()?;
        if document.kind != DocsFileKind::Markdown {
            return None;
        }
        let editor = document.live.as_ref()?.read(cx);
        let range = editor.selected_range();
        if range.is_empty() {
            return None;
        }
        let quote = capture_selection_quote(editor.text(), range.start, range.end)?;
        let start = editor.bounds_for_offset(range.start)?;
        let end = editor.bounds_for_offset(range.end).unwrap_or(start);
        Some((quote, start.union(&end)))
    }

    /// A note from the selection toolbar (a redline or a quick label) needs no composer.
    pub(crate) fn native_docs_quick_note(
        &mut self,
        kind: DocsAnnotationType,
        label: Option<DocsQuickLabelId>,
        cx: &mut Context<Self>,
    ) {
        let Some((quote, _)) = self.native_docs_selection_quote(cx) else {
            return;
        };
        self.native_docs_add_note(
            DocsNewAnnotation {
                kind,
                quote,
                label_id: label,
                ..Default::default()
            },
            cx,
        );
    }

    /// Opens the composer for a new note on the selection (or a global one when `global`), or to
    /// edit an existing note. `initial` pre-fills the field (a key typed over the selection).
    pub(crate) fn native_docs_open_composer(
        &mut self,
        global_anchor: Option<Bounds<Pixels>>,
        editing: Option<String>,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (quote, anchor, text) = if let Some(id) = editing.as_deref() {
            let Some(note) = self
                .native_docs_active_notes()
                .iter()
                .find(|note| note.id == id)
            else {
                return;
            };
            let anchor = global_anchor.unwrap_or_default();
            (note.quote.clone(), anchor, note.note.clone())
        } else if let Some(anchor) = global_anchor {
            (String::new(), anchor, initial.to_string())
        } else {
            let Some((quote, anchor)) = self.native_docs_selection_quote(cx) else {
                return;
            };
            (quote, anchor, initial.to_string())
        };
        let placeholder = if quote.is_empty() {
            "Add a global comment"
        } else {
            "Add a comment"
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .placeholder(placeholder)
                .default_value(text)
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter {
                    secondary: true, ..
                } = event
                {
                    this.native_docs_commit_composer(window, cx);
                }
            },
        );
        let focus = input.clone();
        window.on_next_frame(move |window, cx| {
            focus.update(cx, |input, cx| input.focus(window, cx));
        });
        self.native_docs.composer = Some(DocsComposer {
            input,
            quote,
            editing,
            anchor,
            _subscription: subscription,
        });
        self.native_docs_notify(cx);
    }

    /// Add (or Save when editing): stores the note and closes the composer.
    ///
    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: the composer button says "Add" while annotating, not "Submit", because a note is added to the list of annotations and only the Send action submits them to the agent. The OS chord (Cmd+Enter, Ctrl+Enter on Windows and Linux) that adds the note is shown to the left of the button, outside it, and the button itself is a plain neutral button, not a colored one. The label stays "Save" when editing an existing note.
    pub(crate) fn native_docs_commit_composer(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(composer) = self.native_docs.composer.take() else {
            return;
        };
        let text = composer.input.read(cx).value().to_string();
        if let Some(id) = composer.editing {
            if let Some(path) = self.native_docs.active.clone()
                && let Some(notes) = self.native_docs.notes.get_mut(&path)
                && let Some(note) = notes.iter_mut().find(|note| note.id == id)
            {
                note.edit_note(&text, now_ms());
                self.native_docs_notes_changed(cx);
            }
            return;
        }
        self.native_docs_add_note(
            DocsNewAnnotation {
                kind: DocsAnnotationType::Comment,
                quote: composer.quote,
                note: text,
                ..Default::default()
            },
            cx,
        );
        self.native_docs_notify(cx);
    }

    pub(crate) fn native_docs_close_composer(&mut self, cx: &mut Context<Self>) {
        if self.native_docs.composer.take().is_some() {
            self.native_docs_notify(cx);
        }
    }

    pub(crate) fn native_docs_remove_note(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(path) = self.native_docs.active.clone() else {
            return;
        };
        let notes: Vec<DocsAnnotation> = self
            .native_docs_active_notes()
            .iter()
            .filter(|note| note.id != id)
            .cloned()
            .collect();
        self.native_docs.notes.set(&path, notes);
        self.native_docs_notes_changed(cx);
    }

    /// Clear: the first click arms it for a few seconds, the second clears every note on the file.
    pub(crate) fn native_docs_clear_notes(&mut self, cx: &mut Context<Self>) {
        let armed = self
            .native_docs
            .clear_armed_until
            .is_some_and(|until| until > Instant::now());
        if !armed {
            self.native_docs.clear_armed_until = Some(Instant::now() + CLEAR_ARM_DURATION);
            self.native_docs_notify_after(CLEAR_ARM_DURATION, cx);
            self.native_docs_notify(cx);
            return;
        }
        self.native_docs.clear_armed_until = None;
        if let Some(path) = self.native_docs.active.clone() {
            self.native_docs.notes.set(&path, Vec::new());
            self.native_docs_notes_changed(cx);
        }
    }

    /// Recomputes the open editor's note highlights when its text or the notes changed.
    pub(crate) fn native_docs_refresh_highlights(&mut self, cx: &mut Context<Self>) {
        if !self.native_docs.highlights_stale {
            return;
        }
        self.native_docs.highlights_stale = false;
        let Some(text) = self.native_docs_active_text(cx) else {
            return;
        };
        let notes = self.native_docs_active_notes().to_vec();
        let highlights: Vec<(std::ops::Range<usize>, Hsla, Option<Hsla>)> =
            collect_annotation_ranges(&text, &notes)
                .into_iter()
                .map(|found| {
                    let note = &notes[found.annotation_index];
                    let color = note.color();
                    let sent = !note.is_pending();
                    (
                        found.range,
                        hex(color, if sent { 0.14 } else { 0.28 }),
                        (note.kind == DocsAnnotationType::Redline).then(|| hex(color, 0.82)),
                    )
                })
                .collect();
        if let Some(editor) = self
            .native_docs
            .active_document()
            .and_then(|document| document.live.clone())
        {
            editor.update(cx, |editor, cx| editor.set_note_highlights(highlights, cx));
        }
    }

    /// Send: new notes while there are any, else all of them again, to the agent session last
    /// clicked in the sidebar (or the clipboard).
    ///
    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: the Send button can be pressed again and again. It sends the new notes while there are any, and once everything has been sent it sends all the notes again, instead of going dark with "Nothing new to send".
    pub(crate) fn native_docs_send_notes(&mut self, force_all: bool, cx: &mut Context<Self>) {
        let Some(path) = self.native_docs.active.clone() else {
            return;
        };
        let notes = self.native_docs_active_notes().to_vec();
        if notes.is_empty() {
            self.native_docs_set_send_status(
                DocsSendStatus::Notice("Nothing to send".to_string()),
                cx,
            );
            return;
        }
        let counts = annotation_review_counts(&notes);
        let scope = if !force_all && counts.pending > 0 {
            DocsFeedbackScope::Pending
        } else {
            DocsFeedbackScope::All
        };
        let content = self.native_docs_active_text(cx);
        let mut names = DocsFeedbackDocumentNames::new();
        let name = names.add(&path, &self.native_docs_display_path(&path));
        let feedback = format_annotation_feedback(
            &[DocsFeedbackDocument {
                name: &name,
                content: content.as_deref(),
                annotations: &notes,
            }],
            scope,
        );
        let ids_by_path = names.ids_by_path(&feedback);
        let origin = self
            .native_docs
            .active_document()
            .and_then(|document| document.origin_session);
        self.native_docs_deliver_feedback(
            feedback.text,
            feedback.count,
            feedback.document_count,
            ids_by_path,
            origin,
            cx,
        );
    }

    /// Send new across all files: every file's pending notes in one message, each file read fresh
    /// (the open one from its editor).
    pub(crate) fn native_docs_send_across_files(&mut self, cx: &mut Context<Self>) {
        let paths = self.native_docs.notes.paths_with_notes(true);
        let active = self.native_docs.active.clone();
        let active_text = self.native_docs_active_text(cx);
        self.native_docs_set_send_status(DocsSendStatus::Sending, cx);
        let mut contents: Vec<(String, Option<String>)> = Vec::new();
        self.native_docs_read_all(paths, active, active_text, &mut contents, cx);
    }

    fn native_docs_read_all(
        &mut self,
        mut remaining: Vec<String>,
        active: Option<String>,
        active_text: Option<String>,
        contents: &mut Vec<(String, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = (!remaining.is_empty()).then(|| remaining.remove(0)) else {
            let contents = std::mem::take(contents);
            self.native_docs_send_combined(contents, cx);
            return;
        };
        if active.as_deref() == Some(path.as_str()) {
            contents.push((path, active_text.clone()));
            return self.native_docs_read_all(remaining, active, active_text, contents, cx);
        }
        let mut collected = std::mem::take(contents);
        let request = self.native_docs_request("read", json!({ "path": path }));
        self.run_docs_files_request(request.to_string(), cx, move |this, response, cx| {
            collected.push((
                path,
                response["file"]["content"].as_str().map(str::to_string),
            ));
            this.native_docs_read_all(remaining, active, active_text, &mut collected, cx);
        });
    }

    fn native_docs_send_combined(
        &mut self,
        contents: Vec<(String, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        let mut names = DocsFeedbackDocumentNames::new();
        let named: Vec<(String, Option<String>, Vec<DocsAnnotation>)> = contents
            .into_iter()
            .map(|(path, content)| {
                let name = names.add(&path, &self.native_docs_display_path(&path));
                let notes = self
                    .native_docs
                    .notes
                    .get(&path)
                    .map(<[_]>::to_vec)
                    .unwrap_or_default();
                (name, content, notes)
            })
            .collect();
        let documents: Vec<DocsFeedbackDocument<'_>> = named
            .iter()
            .map(|(name, content, notes)| DocsFeedbackDocument {
                name,
                content: content.as_deref(),
                annotations: notes,
            })
            .collect();
        let feedback = format_annotation_feedback(&documents, DocsFeedbackScope::Pending);
        let ids_by_path = names.ids_by_path(&feedback);
        if feedback.count == 0 {
            self.native_docs_set_send_status(
                DocsSendStatus::Notice("Nothing new to send".to_string()),
                cx,
            );
            return;
        }
        // Notes from several files have no single origin: the sidebar decides.
        self.native_docs_deliver_feedback(
            feedback.text,
            feedback.count,
            feedback.document_count,
            ids_by_path,
            None,
            cx,
        );
    }

    fn native_docs_deliver_feedback(
        &mut self,
        text: String,
        count: usize,
        files: usize,
        ids_by_path: Vec<(String, Vec<String>)>,
        origin: Option<crate::TerminalSessionId>,
        cx: &mut Context<Self>,
    ) {
        self.native_docs_set_send_status(DocsSendStatus::Sending, cx);
        let generation = self.native_docs.generation;
        self.deliver_docs_annotation_feedback(text, origin, cx, move |this, delivery, cx| {
            if this.native_docs.generation != generation {
                return;
            }
            match delivery {
                Ok(delivery) => {
                    let sent_at = super::annotations::model::iso_timestamp_from_ms(now_ms());
                    if this.native_docs.notes.mark_sent(&ids_by_path, &sent_at) {
                        this.native_docs_notes_changed(cx);
                    }
                    this.native_docs_set_send_status(
                        DocsSendStatus::Sent {
                            delivery,
                            count,
                            files,
                        },
                        cx,
                    );
                }
                Err(error) => this.native_docs_set_send_status(DocsSendStatus::Error(error), cx),
            }
        });
    }

    fn native_docs_set_send_status(&mut self, status: DocsSendStatus, cx: &mut Context<Self>) {
        let transient = !matches!(status, DocsSendStatus::Sending);
        self.native_docs.send_status = Some((status, Instant::now()));
        if transient {
            self.native_docs_notify_after(SEND_STATUS_DURATION, cx);
        }
        self.native_docs_notify(cx);
    }

    /// Copy feedback: every note on the file to the clipboard, without marking any sent.
    pub(crate) fn native_docs_copy_feedback(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.native_docs.active.clone() else {
            return;
        };
        let notes = self.native_docs_active_notes().to_vec();
        let content = self.native_docs_active_text(cx);
        let mut names = DocsFeedbackDocumentNames::new();
        let name = names.add(&path, &self.native_docs_display_path(&path));
        let feedback = format_annotation_feedback(
            &[DocsFeedbackDocument {
                name: &name,
                content: content.as_deref(),
                annotations: &notes,
            }],
            DocsFeedbackScope::All,
        );
        cx.write_to_clipboard(ClipboardItem::new_string(feedback.text));
        crate::app::helpers::gpui_copy_feedback(cx);
    }
}
