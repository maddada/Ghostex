//! The session note sheet and the composer chrome that reads it.
//!
//! Port of `packages/shared/session-chat-controller/note.ts` and `native-composer-chrome.ts`.

use crate::document::{ComposerChrome, Note};

/// What the note editor holds between frames.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteState {
    pub open: bool,
    pub value: String,
    /// The last body this client acknowledged as written.
    pub saved: String,
    pub edited: bool,
    pub loading: bool,
    /// The `readSessionAgentNote` in flight, so its answer can be routed back.
    ///
    /// `toggleNote` awaits the read in a `try`/`finally`: whatever it answers, the sheet stops
    /// loading. Without an id the answer reached nobody and the sheet span forever.
    pub read_request: Option<u64>,
}

impl NoteState {
    /// The document's own shape.
    pub fn document(&self) -> Note {
        Note {
            open: self.open,
            value: self.value.clone(),
            saved: self.saved.clone(),
            edited: self.edited,
            loading: self.loading,
        }
    }

    /// The body a flush would write, or `None` when it would write what is already saved.
    ///
    /// Blur, close, and view disposal can flush the same edit; acknowledge each body once.
    pub fn pending_flush(&self) -> Option<String> {
        let next = self.value.trim().to_string();
        (next != self.saved).then_some(next)
    }

    /// Takes the acknowledgement, the way `flushSessionNote` advances `state.saved` before the
    /// write so a second flush of the same body is a no-op.
    pub fn begin_flush(&mut self) -> Option<(String, String)> {
        let next = self.pending_flush()?;
        let previous = std::mem::replace(&mut self.saved, next.clone());
        Some((previous, next))
    }

    /// Puts the previous body back when the write failed and nothing newer has been saved since.
    pub fn fail_flush(&mut self, previous: String, attempted: &str) {
        if self.saved == attempted {
            self.saved = previous;
        }
    }
}

/// The stash and note reads that feed the composer's own buttons: a stash count badge, a
/// session-note presence dot, and pressed Summary and Note buttons.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerChromeState {
    agent_session_id: Option<String>,
    /// Bumped on every refresh, so an answer that lands after the conversation moved on is dropped.
    generation: u64,
    note_text: String,
    /// Once the note editor has opened, its own state is newer than the presence read.
    note_owned: bool,
    session_id: Option<String>,
    stashed_prompt_count: usize,
    /// The refresh in flight: its generation, the two request ids, and what has answered.
    ///
    /// `refresh()` is one `Promise.all` over two reads, each with its own `.catch(() => null)`,
    /// so a refused read leaves the other half's answer intact and raises nothing on the
    /// composer's error bar.
    pending: Option<ChromeRefresh>,
}

/// One `composerChrome.refresh()` waiting for its two reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ChromeRefresh {
    generation: u64,
    prompts_request: Option<u64>,
    note_request: Option<u64>,
    prompts: Option<Vec<StashedPromptRow>>,
    note: Option<String>,
}

/// One row of the stashed-prompt list, as far as the badge needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashedPromptRow {
    #[serde(default)]
    pub agent_session_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

impl ComposerChromeState {
    /// Starts a refresh and answers the generation its results must carry.
    pub fn begin_refresh(&mut self, session_id: Option<Option<String>>) -> u64 {
        if let Some(session_id) = session_id {
            self.session_id = session_id;
        }
        self.generation += 1;
        self.generation
    }

    /// Whether the note read is worth making at all.
    pub fn wants_note_read(&self) -> bool {
        !self.note_owned
    }

    /// Records the two reads this refresh is waiting for.
    pub fn await_refresh(&mut self, prompts_request: u64, note_request: Option<u64>) {
        self.pending = Some(ChromeRefresh {
            generation: self.generation,
            prompts_request: Some(prompts_request),
            note_request,
            prompts: None,
            note: None,
        });
    }

    /// Whether this request is one of the refresh's two reads.
    pub fn awaits(&self, request_id: u64) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            pending.prompts_request == Some(request_id) || pending.note_request == Some(request_id)
        })
    }

    /// One of the two reads answered. `None` for a refusal, which the read's own `catch` swallows.
    ///
    /// The refresh folds in only once both have answered, which is what `Promise.all` waits for.
    pub fn settle_refresh(
        &mut self,
        request_id: u64,
        prompts: Option<Vec<StashedPromptRow>>,
        note: Option<String>,
    ) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        if pending.prompts_request == Some(request_id) {
            pending.prompts_request = None;
            pending.prompts = prompts;
        } else if pending.note_request == Some(request_id) {
            pending.note_request = None;
            pending.note = note;
        } else {
            return;
        }
        if pending.prompts_request.is_some() || pending.note_request.is_some() {
            return;
        }
        let finished = self.pending.take().unwrap_or_default();
        self.finish_refresh(
            finished.generation,
            finished.prompts.as_deref(),
            finished.note.as_deref(),
        );
    }

    /// Folds a refresh's answers in, unless the conversation moved on.
    pub fn finish_refresh(
        &mut self,
        generation: u64,
        prompts: Option<&[StashedPromptRow]>,
        note: Option<&str>,
    ) -> bool {
        if self.generation != generation {
            return false;
        }
        if let Some(prompts) = prompts {
            self.stashed_prompt_count = prompts
                .iter()
                .filter(|prompt| {
                    (self.agent_session_id.is_some()
                        && prompt.agent_session_id == self.agent_session_id)
                        || (self.session_id.is_some() && prompt.session_id == self.session_id)
                })
                .count();
        }
        if let Some(note) = note {
            self.note_text = note.to_string();
        }
        true
    }

    /// The two latches `projection()` sets before it answers.
    ///
    /// CDXC:SessionChat 2026-09-22 WHY:
    /// They used to be set inside `projection`, which `document()` could only call on a CLONE
    /// because it holds `&ChatState`, so both were thrown away every frame. `agent_session_id`
    /// stayed `None` and the stash badge never counted a prompt stashed under the agent session,
    /// and `note_owned` stayed false so every refresh re-read a note the editor already owned and
    /// the presence dot kept showing a note the user had just cleared. The latches belong to the
    /// settle, which runs on the real state before the document is assembled.
    pub fn adopt(&mut self, note_open: bool, agent_session_id: Option<&str>) {
        let agent_session_id = agent_session_id.map(str::to_string);
        if self.agent_session_id != agent_session_id {
            self.agent_session_id = agent_session_id;
        }
        if note_open {
            self.note_owned = true;
        }
    }

    /// The composer's own buttons: the note dot, the stash badge, and the pressed states.
    pub fn projection(&self, note: &NoteState, summary_mode: bool) -> ComposerChrome {
        let text = if self.note_owned {
            if note.open || note.edited {
                note.value.as_str()
            } else {
                note.saved.as_str()
            }
        } else {
            self.note_text.as_str()
        };
        ComposerChrome {
            note_presence: !text.trim().is_empty(),
            note_pressed: note.open,
            stash_badge: (self.stashed_prompt_count > 0)
                .then(|| self.stashed_prompt_count.min(9).to_string()),
            summary_pressed: summary_mode,
        }
    }
}
