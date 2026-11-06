//! The settled prefix: the turns no later row can change, projected once and reused by every
//! refresh after them.
//!
//! CDXC:SessionChat 2026-10-23 WHY:
//! `refresh` rebuilt the whole transcript on every event of a working chat (each streamed delta,
//! each tool row, each backfill batch): a dozen passes over every message, every row's model copied
//! into a new list, that list compared whole for the splice. On a 3,400-message chat that was 90 ms
//! and 300 MB of allocation per event. A turn that a later accepted prompt has closed can no longer
//! change through the rows after it, so its rows are built once, one chunk per turn; while a long
//! turn keeps working, its older rows (each drawn as its own row then) are built once too, in
//! active chunks that go as soon as the turn stops. Each event projects only the rows after those,
//! plus any chunk whose own inputs moved (a backfilled row, a Q-tag's state, its loaded work, a
//! name on an agent-message card, a thinking card). `build_scope` stays the reference: whenever a
//! check below fails, the list is settled afresh and built on that, and built whole by
//! `build_scope` only when it cannot be settled.
//!
//! What a later row could change in an earlier one, and the check that rules it out:
//!  * The sort: every later row sorts strictly after the prefix's newest `(rank, time)`, and no
//!    run of rows with one `(rank, time)` mixes rows with and without a byte offset (the order is
//!    not transitive there, so a row's place would depend on the sort's own path).
//!  * The folds: each turn starts at an accepted user prompt from the transcript, which ends the
//!    turn before it in every pass, and that no marker before it merges into; an active chunk ends
//!    before an assistant or reasoning row with prose, which no tool row folds past, and no prompt
//!    or wake after it restarts the response. No `/btw` question or redone form pairs across the
//!    seam, and no id is on both sides.
//!  * The rows: a settled turn is built again when a row of it drawn as a placeholder is projected
//!    now or falls in the eager tail, when its Q-tags read new states, when its loaded work
//!    changed, when a name or delivery its agent-message cards looked up reads differently, and
//!    (summary mode) when the turn whose reply opens as the newest moves into or out of it.
//!  * The scans that read the list in arrival order (forms, answers, askers) are kept per row and
//!    merged in that order.
//!  * The call an approval waits on is searched for in the rows after the prefix, then in the
//!    chunks' drawn rows; one found in a chunk has the list built again.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use ghostex_gx_protocol::{ChatMessage, ChatRole, ChatSource};

use crate::document::TranscriptItem;
use crate::extras::minimap::minimap_rows;
use crate::extras::minimap_rail::MinimapMarkerRow;
use crate::state::{ChatContext, ChatState, TranscriptViewState, EAGER_TAIL_ITEMS};
use crate::transcript::foreign::{is_working, order_cmp, order_key};
use crate::transcript::line_breaks::AgentLineBreaks;
use crate::transcript::open_questions::{
    asker_entries, linkable_tags, linked_words, mark_skipped, settle_askers, Asker, Found,
    QuestionTag,
};
use crate::transcript::page_answers::{page_answers_in, PageAnswers};
use crate::transcript::presentation::{
    build_items, drop_hidden_rows, item_index, line_breaks, Builder, Projection, ProjectionCache,
    ProjectionScope,
};
use crate::transcript::question_forms::{form_entries, mark_resent, redone_forms, QuestionForm};
use crate::transcript::sent_message::{
    learn_names, resolve_recipients_after, LearnedNames, NameCheck, RecipientLookups,
};
use crate::transcript::side_question::is_side_question_command;
use crate::transcript::thinking_card::RunningThinking;
use crate::transcript::transcript::{
    normalize_chat_transcript, project_chat_segment, project_chat_segment_with,
    TranscriptProjection,
};

/// What the prefix was projected under. Any change and it is settled again.
#[derive(Clone, Debug, PartialEq)]
struct SettledKey {
    summary: bool,
    line_breaks: AgentLineBreaks,
    agent_path: String,
    working_directory: Option<String>,
    session_directory_revision: u64,
    tool_cards: bool,
}

/// What one row tells the question scans (`scope`'s askers, `questionForms`, `pageAnswers`).
#[derive(Clone, Debug, PartialEq)]
struct ScanRow {
    /// The row's place in the sorted list, which finds where it sits in the list as it arrived.
    at: usize,
    answers: Option<PageAnswers>,
    found: Vec<Found>,
    forms: Vec<QuestionForm>,
    resent: Vec<String>,
}

/// The scans of the whole list, merged in arrival order.
#[derive(Clone, Debug, Default, PartialEq)]
struct Scans {
    answers: Vec<PageAnswers>,
    found: Vec<Found>,
    forms: Vec<QuestionForm>,
    resent: Vec<String>,
}

impl Scans {
    /// `question_forms` of the whole list.
    fn marked_forms(&self) -> Vec<QuestionForm> {
        let mut forms = self.forms.clone();
        mark_resent(&mut forms, &self.resent);
        forms
    }
}

/// A row whose model links Q-tags: it stands only while the tags it could link resolve the same
/// (`presentation::fresh`).
#[derive(Clone, Debug, PartialEq)]
struct TaggedRow {
    tag_words: Vec<String>,
    linked_tags: Vec<String>,
}

/// One settled turn (or run of turns no seam splits), or a stretch of the running turn, built.
#[derive(Clone, Debug, PartialEq)]
struct Chunk {
    /// Its rows: `SettledPrefix::messages[start..end]`.
    start: usize,
    end: usize,
    /// Rows of the turn still running, drawn one row each as the running turn draws them, which
    /// stand only while the turn keeps working ([`SettledPrefix::drop_active`]).
    active: bool,
    /// The ids it put into the prefix's set, which leave with it.
    added_ids: Vec<String>,
    /// Its reasoning rows, and the thinking card each was drawn with (`Some(live)`) or without.
    thinking: Vec<(String, Option<bool>)>,
    /// Its folded `/btw` rows, which the side-question card above the composer can hide.
    side_rows: Vec<ChatMessage>,
    /// Where its render items, summary turns and rows start in the whole list.
    first_render: usize,
    first_turn: usize,
    first_item: usize,
    render_count: usize,
    summary_turns: usize,
    /// Summary mode: its newest turn with a reply, and the turn it drew opening as the whole
    /// list's newest reply, when that turn is one of its own (both numbered in the whole list).
    last_reply: Option<usize>,
    opened_reply: Option<usize>,
    items: Arc<Vec<TranscriptItem>>,
    item_count: usize,
    used: HashSet<String>,
    /// Its rows drawn as placeholders, in order, and the newest render item that drew one.
    queued: Vec<ChatMessage>,
    last_queued: Option<usize>,
    /// It drew a row projected outside the cache, which every pass projects again.
    off_cache: bool,
    final_ids: Vec<String>,
    minimap: Vec<MinimapMarkerRow>,
    learned: LearnedNames,
    lookups: RecipientLookups,
    tagged: Vec<TaggedRow>,
    /// The loaded work each of its completed turns was built with.
    deferred: Vec<(String, Option<Vec<ChatMessage>>)>,
}

/// What a check of the chunks' names reads ([`SettledPrefix::checked_names`]).
type NamesRead = (
    u64,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
    LearnedNames,
);

/// A chunk built in this pass whose agent-message cards are not named yet: names come from the
/// whole list, so the chunks of a pass are named once every one of them is known.
struct Unresolved {
    index: usize,
    chunk: Chunk,
    items: Vec<TranscriptItem>,
}

/// The turns before the newest accepted prompt, and the older rows of a long running turn, built
/// once.
#[derive(Clone, Debug, PartialEq)]
pub struct SettledPrefix {
    key: SettledKey,
    /// The prefix's rows as they arrived, in the order the projection sorts them.
    messages: Vec<ChatMessage>,
    /// The composition the prefix was last checked against, the revision of that list's head
    /// (`TranscriptPart`), and that list's sorted order: an unchanged composition (a backfill
    /// batch, a tick) needs no second check, and one that kept its head needs only its tail checked.
    checked_generation: u64,
    checked_part: Option<u64>,
    order: Vec<usize>,
    /// The prefix's ids, with each row's time (`mark_inherited`).
    times: HashMap<String, Option<i64>>,
    /// Every id the prefix's rows carry: as they arrived, as the passes made them, and as its built
    /// rows drew them.
    ids: HashSet<String>,
    /// Every id its built rows drew, which the rows after it count as drawn already.
    used: HashSet<String>,
    side_questions: HashSet<String>,
    /// The newest `(rank, time)` among the prefix's rows, and among its settled turns' alone.
    order_max: (u8, Option<i64>),
    turn_order_max: (u8, Option<i64>),
    /// The forms the prefix redid (`hide_redone_forms`), which strip later replies too.
    redone: Vec<(String, String)>,
    /// The prefix's last turn opener (a prompt, or an OMP wake-up, `opens_omp_turn`) carries
    /// deferred work, which a following row without a byte offset would fold into
    /// (`completed_work_render_items`).
    last_opener_deferred: bool,
    scans: Vec<ScanRow>,
    prefix_scans: Scans,
    /// `prefix_scans.found` settled with nothing answered.
    prefix_askers: Vec<Asker>,
    /// The tags the chunks' tagged rows were last checked against.
    checked_tags: Vec<QuestionTag>,
    /// What every chunk's rows teach about names, one directory, first word winning, and how many
    /// times it changed.
    names: LearnedNames,
    names_revision: u64,
    /// What the last check of the chunks' names read, so a pass where none of it moved checks
    /// nothing: the prefix's names, the roster, the deliveries and what the stretch after the
    /// prefix taught.
    checked_names: Option<NamesRead>,
    chunks: Vec<Chunk>,
}

/// A running turn whose rows after the prefix reach this many settles all but its newest
/// [`ACTIVE_TAIL_ROWS`] into an active chunk.
const ACTIVE_STRETCH_ROWS: usize = 64;
const ACTIVE_TAIL_ROWS: usize = 24;

/// The settled chunks' rows, oldest first, as the list shares them.
pub type SharedChunks = Vec<Arc<Vec<TranscriptItem>>>;

/// A run of sorted rows with one order key: the key, how many rows, and whether some have a byte
/// offset and some none.
type OrderRun = ((u8, Option<i64>), usize, bool, bool);

/// The list as the projection sorts it, as positions in `messages`.
fn sorted_positions(messages: &[ChatMessage]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..messages.len()).collect();
    order.sort_by(|left, right| order_cmp(&messages[*left], &messages[*right]));
    order
}

/// Whether some run of sorted rows with one `(rank, time)` holds three or more rows, some with a
/// byte offset and some without: there the order compares offsets for some pairs and ids for
/// others, which is not transitive.
fn order_is_ambiguous<'a>(sorted: impl Iterator<Item = &'a ChatMessage>) -> bool {
    let mut run: Option<OrderRun> = None;
    for message in sorted {
        let key = order_key(message);
        let offset = message.byte_offset.is_some();
        run = Some(match run {
            Some((current, count, with, without)) if current == key => {
                let (count, with, without) = (count + 1, with || offset, without || !offset);
                if count >= 3 && with && without {
                    return true;
                }
                (current, count, with, without)
            }
            _ => (key, 1, offset, !offset),
        });
    }
    false
}

/// Whether a row can start a settled turn, or the stretch after the prefix: an accepted user
/// prompt from the transcript, which ends the turn before it in every pass, that no marker before
/// it merges into. In summary mode a message from another agent opens no turn there
/// (`turns::is_agent_conversation`), so it starts no settled turn either.
fn opens_turn(message: &ChatMessage, summary: bool) -> bool {
    use crate::transcript::noise::{is_hidden_message, suppressed_turn_label};
    !(summary && crate::transcript::turns::is_agent_conversation(message))
        && message.role == ChatRole::User
        && message.source == ChatSource::Transcript
        && order_key(message).0 == 0
        && !message.queued
        && suppressed_turn_label(message).is_none()
        && !is_hidden_message(message)
        && !crate::transcript::image_markers::image_prompt_marker_starts_message(message)
        && !crate::transcript::local_command::is_shell_command_output(
            &crate::transcript::prose::joined_text(&message.blocks),
        )
}

/// Whether a row of the running turn can start a stretch after an active chunk: an assistant or
/// reasoning row with prose, which no tool row after it folds past and which folds into nothing.
fn opens_active_stretch(message: &ChatMessage) -> bool {
    matches!(message.role, ChatRole::Assistant | ChatRole::Reasoning)
        && message.source == ChatSource::Transcript
        && order_key(message).0 == 0
        && !crate::transcript::tool_fold::is_tool_only_message(message)
}

/// Whether some row of `projection` after its first moves where the running response starts
/// (`active_response_start_index`): an accepted prompt, or a row that wakes the agent.
fn restarts_response(projection: &TranscriptProjection) -> bool {
    use crate::transcript::noise::suppressed_turn_label;
    let rendered = &projection.rendered;
    (1..rendered.len()).any(|at| {
        let row = &rendered[at];
        (row.role == ChatRole::User && !row.queued && suppressed_turn_label(row).is_none())
            || crate::transcript::turns::wakes_settled_agent(rendered, at)
    })
}

/// Whether some user row of `rows` asks the agent to send a ```questions block again, whose answer
/// a later row could bring.
fn asks_resend(rows: &[ChatMessage]) -> bool {
    rows.iter().any(|row| {
        row.role == ChatRole::User
            && crate::transcript::question_forms::resend_form_id(
                &crate::transcript::prose::prose_markdown(&row.blocks),
            )
            .is_some()
    })
}

/// Every row after the first of `rows` (sorted) that can open a turn and so end the one before:
/// it sorts strictly after the row before it (two prompts delivered in the same millisecond do
/// not), it has a byte offset (a row without one folds into a deferred turn before it), and it
/// splits no `/btw` question from its answer and no two rows sharing an id.
fn seams(rows: &[ChatMessage], summary: bool) -> Vec<usize> {
    cuts(rows, |row| {
        opens_turn(row, summary) && row.byte_offset.is_some()
    })
}

/// The rows of the running turn (sorted) an active chunk can end before, by the same rules
/// ([`opens_active_stretch`]).
fn active_seams(rows: &[ChatMessage]) -> Vec<usize> {
    cuts(rows, opens_active_stretch)
}

/// Every row after the first of `rows` that `opens`, sorts strictly after the row before it, and
/// splits no `/btw` question from its answer and no two rows sharing an id.
fn cuts(rows: &[ChatMessage], opens: impl Fn(&ChatMessage) -> bool) -> Vec<usize> {
    let mut seams: Vec<usize> = (1..rows.len())
        .filter(|at| {
            let row = &rows[*at];
            opens(row) && order_key(row) > order_key(&rows[*at - 1])
        })
        .collect();
    let mut first_at: HashMap<&str, usize> = HashMap::new();
    let mut joined: Vec<(usize, usize)> = Vec::new();
    for (at, row) in rows.iter().enumerate() {
        let id = row.id.as_str();
        if let Some(first) = first_at.get(id) {
            joined.push((*first, at));
        } else {
            first_at.insert(id, at);
        }
        if let Some(question) = id.strip_suffix(":output") {
            if let Some(asked) = first_at.get(question) {
                joined.push((*asked, at));
            }
        }
    }
    seams.retain(|seam| !joined.iter().any(|(from, to)| from < seam && seam <= to));
    seams
}

/// What a row tells the question scans, or `None` when it tells them nothing.
fn scan_row(message: &ChatMessage, at: usize) -> Option<ScanRow> {
    let one = std::slice::from_ref(message);
    let answers = page_answers_in(one).into_iter().next();
    let found = asker_entries(one, &[]);
    let (forms, resent) = form_entries(one, &[]);
    (answers.is_some() || !found.is_empty() || !forms.is_empty() || !resent.is_empty()).then_some(
        ScanRow {
            at,
            answers,
            found,
            forms,
            resent,
        },
    )
}

/// The scans of rows already in arrival order, each form, page and answer kept where it first
/// appears (an answer with the latest one sent for it), as `askers`, `question_forms` and
/// `page_answers_in` keep them.
fn merge_scans<'a>(rows: impl Iterator<Item = &'a ScanRow>) -> Scans {
    let mut merged = Scans::default();
    for row in rows {
        if let Some(answers) = &row.answers {
            // A later answer replaces the earlier one where it first arrived, as `page_answers_in`
            // keeps it (a one-click answer changed after the agent read it).
            match merged
                .answers
                .iter_mut()
                .find(|known| known.page_id == answers.page_id)
            {
                Some(known) => *known = answers.clone(),
                None => merged.answers.push(answers.clone()),
            }
        }
        for entry in &row.found {
            let seen = match entry {
                Found::Asker(asker) => merged
                    .found
                    .iter()
                    .any(|known| matches!(known, Found::Asker(known) if known.id == asker.id)),
                Found::Withdraw(_) | Found::Said(_) => false,
            };
            if !seen {
                merged.found.push(entry.clone());
            }
        }
        for form in &row.forms {
            if !merged.forms.iter().any(|known| known.id == form.id) {
                merged.forms.push(form.clone());
            }
        }
        merged.resent.extend(row.resent.iter().cloned());
    }
    merged
}

/// The ids a stretch's projection carries beyond the rows as they arrived.
fn projection_ids(projection: &TranscriptProjection) -> impl Iterator<Item = &str> {
    projection
        .normalized
        .iter()
        .chain(projection.rendered.iter())
        .map(|message| message.id.as_str())
}

/// The loaded sections a chunk's folds read, each once: every fold's own, and for a wake-up's fold
/// the prompt's before it, which several wake-ups share (`CompletedWorkTurn::section`).
fn deferred_keys(items: &[TranscriptItem]) -> Vec<&str> {
    let mut keys: Vec<&str> = Vec::new();
    for item in items {
        if let TranscriptItem::CompletedWork { id, deferred, .. } = item {
            let owner = deferred.as_ref().and_then(|work| work["startId"].as_str());
            for key in std::iter::once(id.as_str()).chain(owner) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
    }
    keys
}

/// The newest summary turn of `projection` with a reply, numbered from its first.
fn newest_reply_in(projection: &TranscriptProjection) -> Option<usize> {
    projection
        .summary_turns
        .iter()
        .rposition(|turn| turn.final_message.is_some())
}

/// The render items `projection` has in the current mode: summary turns or render items.
fn render_len(projection: &TranscriptProjection, summary: bool) -> usize {
    if summary {
        projection.summary_turns.len()
    } else {
        projection.items.len()
    }
}

impl SettledPrefix {
    fn key_of(state: &ChatState) -> SettledKey {
        let view = &state.transcript_view;
        SettledKey {
            summary: view.summary_mode,
            line_breaks: line_breaks(state),
            agent_path: view.agent_path.clone(),
            working_directory: view.working_directory.clone(),
            session_directory_revision: view.session_directory_revision,
            tool_cards: view.tool_cards,
        }
    }

    fn render_count(&self) -> usize {
        self.chunks
            .last()
            .map_or(0, |chunk| chunk.first_render + chunk.render_count)
    }

    fn summary_turns(&self) -> usize {
        self.chunks
            .last()
            .map_or(0, |chunk| chunk.first_turn + chunk.summary_turns)
    }

    /// Summary mode: the whole list's newest turn with a reply, given the newest in the rows after
    /// the prefix (`later`, numbered in the whole list), which wins when there is one.
    fn newest_reply(&self, later: Option<usize>) -> Option<usize> {
        later.or_else(|| self.chunks.iter().rev().find_map(|chunk| chunk.last_reply))
    }

    fn item_count(&self) -> usize {
        self.chunks
            .last()
            .map_or(0, |chunk| chunk.first_item + chunk.item_count)
    }

    /// Whether the prefix ends in rows of the running turn.
    fn has_active(&self) -> bool {
        self.chunks.last().is_some_and(|chunk| chunk.active)
    }

    /// Whether `rest` (sorted, the rows right after the prefix) can follow it: the sort, the seam,
    /// the `/btw` pairs and the ids. An empty prefix reaches into nothing.
    fn can_follow(&self, rest: &[ChatMessage]) -> bool {
        let Some(first) = rest.first() else {
            return false;
        };
        if order_is_ambiguous(rest.iter()) {
            return false;
        }
        if self.messages.is_empty() {
            return true;
        }
        let opens = if self.has_active() {
            opens_active_stretch(first)
        } else {
            opens_turn(first, self.key.summary)
                && !(self.last_opener_deferred && first.byte_offset.is_none())
        };
        if order_key(first) <= self.order_max || !opens {
            return false;
        }
        rest.iter().all(|message| {
            // A `/btw` answer whose question is settled, or a question whose answer is.
            let answers_settled = message
                .id
                .strip_suffix(":output")
                .is_some_and(|question| self.side_questions.contains(question));
            !self.ids.contains(&message.id)
                && !answers_settled
                && !self.ids.contains(&format!("{}:output", message.id))
        })
    }

    /// Builds one chunk from its rows' projection (its cards not named yet), placed after the
    /// chunks before `index`. `own` is the chunk being built again, whose drawn ids are its own.
    /// An `active` chunk is rows of the running turn, drawn with its thinking cards (`running`).
    #[allow(clippy::too_many_arguments)]
    fn build_chunk(
        &self,
        projection: &TranscriptProjection,
        rows: (usize, usize),
        index: usize,
        own: Option<&Chunk>,
        active: Option<&RunningThinking>,
        newest_reply: Option<usize>,
        eager_from: usize,
        scope: &ProjectionScope,
        context: &ChatContext,
        cache: &mut ProjectionCache,
    ) -> Unresolved {
        let settled_running = RunningThinking::new(&[], false);
        let running = active.unwrap_or(&settled_running);
        let before = &self.chunks[..index];
        let first_render = before
            .last()
            .map_or(0, |chunk| chunk.first_render + chunk.render_count);
        let first_turn = before
            .last()
            .map_or(0, |chunk| chunk.first_turn + chunk.summary_turns);
        let first_item = before
            .last()
            .map_or(0, |chunk| chunk.first_item + chunk.item_count);
        let mut builder = Builder::new(
            scope,
            context,
            cache,
            eager_from,
            Some(&self.used),
            own.map(|chunk| &chunk.used),
        );
        let mut items = build_items(
            &mut builder,
            projection,
            running,
            first_render,
            newest_reply,
        );
        let queued = std::mem::take(&mut builder.backfill);
        let used = std::mem::take(&mut builder.used);
        let (last_queued, off_cache) = (builder.last_queued, builder.off_cache);
        drop(builder);
        drop_hidden_rows(&mut items, &projection.hidden_rows);
        let turns: Vec<_> = projection
            .summary_turns
            .iter()
            .map(|turn| (&turn.user, turn.final_message.as_ref()))
            .collect();
        let chunk = Chunk {
            start: rows.0,
            end: rows.1,
            active: active.is_some(),
            added_ids: own.map(|chunk| chunk.added_ids.clone()).unwrap_or_default(),
            thinking: projection
                .rendered
                .iter()
                .filter(|message| message.role == ChatRole::Reasoning)
                .map(|message| (message.id.clone(), running.card(&message.id)))
                .collect(),
            side_rows: projection
                .normalized
                .iter()
                .filter(|message| {
                    crate::transcript::side_question::is_side_question_message(message)
                })
                .cloned()
                .collect(),
            first_render,
            first_turn,
            first_item,
            render_count: render_len(projection, self.key.summary),
            summary_turns: projection.summary_turns.len(),
            last_reply: projection
                .summary_turns
                .iter()
                .rposition(|turn| turn.final_message.is_some())
                .map(|at| first_turn + at),
            opened_reply: newest_reply.filter(|at| {
                (first_turn..first_turn + projection.summary_turns.len()).contains(at)
            }),
            items: Arc::new(Vec::new()),
            item_count: items.len(),
            queued,
            last_queued,
            off_cache,
            final_ids: projection.final_ids.clone(),
            minimap: minimap_rows(&turns, &item_index(&items, first_item)),
            learned: learn_names(&items),
            lookups: RecipientLookups::default(),
            tagged: used
                .iter()
                .filter_map(|id| cache.get(id))
                .filter(|entry| !entry.tag_words.is_empty())
                .map(|entry| TaggedRow {
                    tag_words: entry.tag_words.clone(),
                    linked_tags: entry.linked_tags.clone(),
                })
                .collect(),
            deferred: deferred_keys(&items)
                .into_iter()
                .map(|id| (id.to_string(), scope.deferred.get(id).cloned()))
                .collect(),
            used,
        };
        Unresolved {
            index,
            chunk,
            items,
        }
    }

    /// Takes the sorted rows of a new chunk, and the chunk itself, after the chunks it holds.
    /// `None` when they would share an id with it.
    fn absorb(
        &mut self,
        rows: &[ChatMessage],
        projection: &TranscriptProjection,
        built: &mut Unresolved,
        arrival_at: &[usize],
    ) -> Option<()> {
        let mut ids: HashSet<String> = rows.iter().map(|message| message.id.clone()).collect();
        ids.extend(projection_ids(projection).map(str::to_string));
        ids.extend(built.chunk.used.iter().cloned());
        if ids.iter().any(|id| self.ids.contains(id)) {
            return None;
        }
        built.chunk.added_ids = ids.iter().cloned().collect();
        let held = self.messages.len();
        for (offset, message) in rows.iter().enumerate() {
            self.times.insert(message.id.clone(), message.timestamp);
            if is_side_question_command(message) {
                self.side_questions.insert(message.id.clone());
            }
            if let Some(row) = scan_row(message, held + offset) {
                self.scans.push(row);
            }
            self.order_max = self.order_max.max(order_key(message));
        }
        self.scans.sort_by_key(|row| arrival_at[row.at]);
        self.ids.extend(ids);
        self.used.extend(built.chunk.used.iter().cloned());
        if !built.chunk.active {
            let rendered = &projection.rendered;
            if let Some(opener) = (0..rendered.len()).rev().find(|at| {
                rendered[*at].role == ChatRole::User
                    || crate::transcript::turns::opens_omp_turn(rendered, *at)
            }) {
                self.last_opener_deferred = rendered[opener].deferred_work.is_some();
            }
            self.turn_order_max = self.order_max;
        }
        self.messages.extend(rows.iter().cloned());
        self.names.extend_after(&built.chunk.learned);
        self.names_revision += 1;
        // Its rows go in place once its cards are named (`name_chunks`).
        self.chunks.push(built.chunk.clone());
        Some(())
    }

    /// Recomputes the merged scans after rows joined.
    fn settle_scans(&mut self) {
        self.prefix_scans = merge_scans(self.scans.iter());
        self.prefix_askers = settle_askers(&self.prefix_scans.found, &[]);
    }

    /// The question scans of the whole list when the rows after the prefix (sorted from its end
    /// on) say something to them: the prefix's merged with theirs, in the order the rows arrived
    /// (`arrival_at`, by sorted place). `None` when they say nothing, and the prefix's scans are
    /// the whole answer.
    fn later_scans(&self, arrival_at: &[usize], stretch: &[ChatMessage]) -> Option<Scans> {
        let held = self.messages.len();
        let later: Vec<ScanRow> = stretch
            .iter()
            .enumerate()
            .filter_map(|(offset, message)| scan_row(message, held + offset))
            .collect();
        if later.is_empty() {
            return None;
        }
        let mut rows: Vec<&ScanRow> = self.scans.iter().chain(later.iter()).collect();
        rows.sort_by_key(|row| arrival_at[row.at]);
        Some(merge_scans(rows.into_iter()))
    }

    /// The askers and linkable tags `scope` reads, from the merged scans.
    fn askers(
        &self,
        state: &ChatState,
        scans: &Scans,
        later_found: bool,
        stretch: &[ChatMessage],
    ) -> (Vec<Asker>, Vec<QuestionTag>) {
        let mut sent = scans.answers.clone();
        for pending in crate::questions::open_questions::answers_on_the_way(state) {
            if !sent.iter().any(|known| known.page_id == pending.page_id) {
                sent.push(pending);
            }
        }
        let answered: Vec<String> = sent.iter().map(|answers| answers.page_id.clone()).collect();
        let mut found = if later_found {
            settle_askers(&scans.found, &answered)
        } else {
            self.prefix_askers
                .iter()
                .cloned()
                .map(|mut asker| {
                    asker.answered = answered.contains(&asker.id) || asker.answered_in_words();
                    asker
                })
                .collect()
        };
        mark_skipped(&mut found, &sent);
        crate::questions::resource_index::settle_loaded(&mut found, &state.session.resource_index);
        // `mark_inherited`, with the prefix's times kept rather than searched for.
        if let Some(forked_at) = state.session.forked_at {
            for asker in found.iter_mut() {
                let time = match self.times.get(&asker.message_id) {
                    Some(time) => *time,
                    None => stretch
                        .iter()
                        .find(|message| message.id == asker.message_id)
                        .and_then(|message| message.timestamp),
                };
                asker.inherited = time.is_some_and(|at| at < forked_at);
            }
        }
        let tags = linkable_tags(&found);
        (found, tags)
    }

    /// Whether a chunk has to be built again for this pass: a placeholder of it is projected now
    /// or falls in the eager tail, it drew a row outside the cache, its Q-tags read new states, or
    /// its loaded work changed.
    #[allow(clippy::too_many_arguments)]
    fn chunk_moved(
        chunk: &Chunk,
        view: &TranscriptViewState,
        cache: &ProjectionCache,
        scope: &ProjectionScope,
        eager_from: usize,
        tags_moved: bool,
        running: &RunningThinking,
        newest_reply: Option<usize>,
    ) -> bool {
        let turns = chunk.first_turn..chunk.first_turn + chunk.summary_turns;
        chunk.off_cache
            || chunk.opened_reply != newest_reply.filter(|at| turns.contains(at))
            || chunk
                .thinking
                .iter()
                .any(|(id, card)| running.card(id) != *card)
            || chunk.last_queued.is_some_and(|at| at >= eager_from)
            || chunk.queued.iter().any(|message| {
                scope.backfilled.contains(&message.id)
                    || cache.get(&message.id).is_some_and(|entry| {
                        entry.source == *message
                            && entry.line_breaks == scope.line_breaks
                            && linked_words(&entry.tag_words, &scope.question_tags)
                                == entry.linked_tags
                    })
            })
            || (tags_moved
                && chunk.tagged.iter().any(|row| {
                    linked_words(&row.tag_words, &scope.question_tags) != row.linked_tags
                }))
            || chunk
                .deferred
                .iter()
                .any(|(id, built)| view.deferred.get(id) != built.as_ref())
    }

    /// Builds the chunk at `index` again from its rows. `None` when that would place its rows
    /// differently or have them draw ids another chunk or the stretch after the prefix holds.
    #[allow(clippy::too_many_arguments)]
    fn rebuild(
        &mut self,
        index: usize,
        eager_from: usize,
        running: &RunningThinking,
        newest_reply: Option<usize>,
        scope: &ProjectionScope,
        context: &ChatContext,
        cache: &mut ProjectionCache,
    ) -> Option<Unresolved> {
        let chunk = &self.chunks[index];
        let rows = &self.messages[chunk.start..chunk.end];
        let (projection, own_redone) = project_chat_segment(rows, chunk.active, &self.redone);
        if own_redone
            .iter()
            .any(|redone| !self.redone.contains(redone))
            || render_len(&projection, self.key.summary) != chunk.render_count
        {
            return None;
        }
        let mut built = self.build_chunk(
            &projection,
            (chunk.start, chunk.end),
            index,
            Some(chunk),
            chunk.active.then_some(running),
            newest_reply,
            eager_from,
            scope,
            context,
            cache,
        );
        if built.chunk.item_count != chunk.item_count {
            return None;
        }
        // Loaded work draws rows the turn did not draw before.
        if built.chunk.used != chunk.used {
            let added: Vec<&String> = built
                .chunk
                .used
                .iter()
                .filter(|id| !chunk.used.contains(*id))
                .collect();
            if added.iter().any(|id| self.ids.contains(*id)) {
                return None;
            }
            let added: Vec<String> = added.into_iter().cloned().collect();
            self.ids.extend(added.iter().cloned());
            built.chunk.added_ids.extend(added);
        }
        Some(built)
    }

    /// Takes the running turn's chunks back out, for a pass where they no longer stand: the turn
    /// stopped working, a prompt or a wake restarted the response, summary mode draws the turn as
    /// one row, or the rows after them cannot follow.
    fn drop_active(&mut self) {
        let Some(first) = self.chunks.iter().position(|chunk| chunk.active) else {
            return;
        };
        let start = self.chunks[first].start;
        for chunk in self.chunks.drain(first..) {
            for id in &chunk.added_ids {
                self.ids.remove(id);
            }
        }
        for message in &self.messages[start..] {
            self.times.remove(&message.id);
            self.side_questions.remove(&message.id);
        }
        self.messages.truncate(start);
        self.scans.retain(|row| row.at < start);
        self.settle_scans();
        self.used = self
            .chunks
            .iter()
            .flat_map(|chunk| chunk.used.iter().cloned())
            .collect();
        let mut names = LearnedNames::default();
        for chunk in &self.chunks {
            names.extend_after(&chunk.learned);
        }
        self.names = names;
        self.names_revision += 1;
        self.order_max = self.turn_order_max;
    }

    /// Names the cards of every chunk built in this pass against all the names the list teaches,
    /// builds again any settled chunk whose cards would now read another name or delivery, and puts
    /// the built chunks in place.
    #[allow(clippy::too_many_arguments)]
    fn name_chunks(
        &mut self,
        mut fresh: Vec<Unresolved>,
        tail_learned: &LearnedNames,
        eager_from: usize,
        running: &RunningThinking,
        newest_reply: Option<usize>,
        scope: &ProjectionScope,
        context: &ChatContext,
        cache: &mut ProjectionCache,
    ) -> Option<()> {
        // The prefix's names once this pass's chunks are in place: a chunk built again teaches what
        // it taught before unless a row of it was drawn as a placeholder.
        let rebuilt_names = fresh.iter().any(|built| {
            built.index < self.chunks.len()
                && built.chunk.learned != self.chunks[built.index].learned
        });
        if rebuilt_names {
            let mut names = LearnedNames::default();
            for (index, chunk) in self.chunks.iter().enumerate() {
                let learned = fresh
                    .iter()
                    .find(|built| built.index == index)
                    .map_or(&chunk.learned, |built| &built.chunk.learned);
                names.extend_after(learned);
            }
            self.names = names;
            self.names_revision += 1;
        }
        let read = (
            self.names_revision,
            scope.agent_fleet.cloned(),
            scope.agent_message_deliveries.cloned(),
            tail_learned.clone(),
        );
        if rebuilt_names || self.checked_names.as_ref() != Some(&read) {
            let layers = [&self.names, tail_learned];
            let check = NameCheck::new(scope.agent_fleet, &layers, scope.agent_message_deliveries);
            let stale: Vec<usize> = self
                .chunks
                .iter()
                .enumerate()
                .filter(|(index, chunk)| {
                    !fresh.iter().any(|built| built.index == *index) && !check.holds(&chunk.lookups)
                })
                .map(|(index, _)| index)
                .collect();
            for index in stale {
                let built = self.rebuild(
                    index,
                    eager_from,
                    running,
                    newest_reply,
                    scope,
                    context,
                    cache,
                )?;
                if built.chunk.learned != self.chunks[index].learned {
                    return None;
                }
                fresh.push(built);
            }
            self.checked_names = Some(read);
        }
        let layers = [&self.names, tail_learned];
        let used_moved = fresh
            .iter()
            .any(|built| built.chunk.used != self.chunks[built.index].used);
        for mut built in fresh {
            built.chunk.lookups = resolve_recipients_after(
                &mut built.items,
                scope.agent_fleet,
                scope.session_directory,
                scope.agent_message_deliveries,
                &layers,
                true,
            );
            built.chunk.items = Arc::new(built.items);
            self.chunks[built.index] = built.chunk;
        }
        if used_moved {
            self.used = self
                .chunks
                .iter()
                .flat_map(|chunk| chunk.used.iter().cloned())
                .collect();
        }
        Some(())
    }

    /// The prefix for the whole list after a whole build: everything before the newest accepted
    /// prompt that can start the stretch after it, one chunk per turn.
    fn settle(
        state: &ChatState,
        context: &ChatContext,
        cache: &mut ProjectionCache,
        scope: &ProjectionScope,
    ) -> Option<Self> {
        let summary = scope.summary;
        let composed = &state.messages.composed;
        let order = sorted_positions(composed);
        let sorted: Vec<ChatMessage> = order.iter().map(|at| composed[*at].clone()).collect();
        if order_is_ambiguous(sorted.iter()) {
            return None;
        }
        // The stretch after the prefix starts at the newest seam; with none (no turn closed yet) the
        // prefix starts empty, and the running turn's rows can still join it.
        let mut cuts = seams(&sorted, summary);
        let seam = cuts.pop().unwrap_or(0);
        let normalized = normalize_chat_transcript(&sorted[..seam]);
        let redone = redone_forms(&normalized);
        let question = scope.live_side_question.as_deref();
        let (rest, rest_redone, rest_hides) =
            project_chat_segment_with(&sorted[seam..], scope.working, &redone, question);
        // A stretch that redoes a form, or a side-question card whose row is in the prefix, makes
        // every pass on the prefix fall back (`build_after`), so it is not settled at all.
        let prefix_asks = question.is_some_and(|question| {
            normalized
                .iter()
                .any(|row| crate::transcript::side_question::asks_side_question(row, question))
        });
        if !rest_redone.is_empty() || (prefix_asks && !rest_hides) {
            return None;
        }
        drop(normalized);
        let mut settled = Self {
            key: Self::key_of(state),
            messages: Vec::new(),
            checked_generation: state.messages.compose_generation,
            checked_part: state
                .messages
                .transcript_part
                .as_ref()
                .map(|part| part.revision),
            order: Vec::new(),
            times: HashMap::new(),
            ids: HashSet::new(),
            used: HashSet::new(),
            side_questions: HashSet::new(),
            order_max: (0, None),
            turn_order_max: (0, None),
            redone,
            last_opener_deferred: false,
            scans: Vec::new(),
            prefix_scans: Scans::default(),
            prefix_askers: Vec::new(),
            checked_tags: scope.question_tags.clone(),
            names: LearnedNames::default(),
            names_revision: 0,
            checked_names: None,
            chunks: Vec::new(),
        };
        // Each turn's projection first: the eager tail and the newest reply are counted on the
        // whole list.
        let mut turns: Vec<((usize, usize), TranscriptProjection)> = Vec::new();
        let mut start = 0;
        for end in cuts.iter().copied().chain(std::iter::once(seam)) {
            if end == 0 {
                break;
            }
            let (projection, own_redone) =
                project_chat_segment(&sorted[start..end], false, &settled.redone);
            if own_redone
                .iter()
                .any(|redone| !settled.redone.contains(redone))
            {
                return None;
            }
            turns.push(((start, end), projection));
            start = end;
        }
        let total: usize = turns
            .iter()
            .map(|(_, projection)| render_len(projection, summary))
            .sum::<usize>()
            + render_len(&rest, summary);
        let eager_from = total.saturating_sub(EAGER_TAIL_ITEMS);
        let newest_reply = if summary {
            let mut first = 0;
            let mut newest = None;
            for (_, projection) in &turns {
                if let Some(at) = newest_reply_in(projection) {
                    newest = Some(first + at);
                }
                first += projection.summary_turns.len();
            }
            newest_reply_in(&rest).map(|at| first + at).or(newest)
        } else {
            None
        };
        let mut fresh = Vec::new();
        for ((start, end), projection) in turns {
            let rows = &sorted[start..end];
            if start > 0 && !settled.can_follow(rows) {
                return None;
            }
            let mut built = settled.build_chunk(
                &projection,
                (start, end),
                settled.chunks.len(),
                None,
                None,
                newest_reply,
                eager_from,
                scope,
                context,
                cache,
            );
            settled.absorb(rows, &projection, &mut built, &order)?;
            fresh.push(built);
        }
        settled.settle_scans();
        // Named against the prefix alone; the first pass on it checks the names against the
        // stretch after it as well.
        settled.name_chunks(
            fresh,
            &LearnedNames::default(),
            eager_from,
            &RunningThinking::new(&[], false),
            newest_reply,
            scope,
            context,
            cache,
        )?;
        settled.order = order;
        settled.can_follow(&sorted[seam..]).then_some(settled)
    }
}

#[path = "settled_build.rs"]
mod settled_build;

use settled_build::build_on;

/// The projection built on `settled`, or `None` when the prefix does not hold for this state and
/// the whole list has to be built ([`settle`] then settles it again). Hands back the rows the
/// prefix shares and the prefix itself, moved on to the newest accepted prompt it reached.
pub fn build(
    state: &ChatState,
    settled: SettledPrefix,
    context: &ChatContext,
    cache: &mut ProjectionCache,
) -> Option<(Projection, SharedChunks, SettledPrefix)> {
    let mut settled = settled;
    let (projection, chunks) = build_on(state, &mut settled, context, cache)?;
    Some((projection, chunks, settled))
}

/// Settles the list as it stands, after a whole build ([`SettledPrefix::settle`]).
pub fn settle(
    state: &ChatState,
    context: &ChatContext,
    cache: &mut ProjectionCache,
    scope: &ProjectionScope,
) -> Option<SettledPrefix> {
    SettledPrefix::settle(state, context, cache, scope)
}

/// The session's projection scope ([`crate::transcript::presentation::scope`]), with the question
/// scans read off the settled prefix when it was checked against this very composition (a backfill
/// batch between two events), rather than read again from every message.
pub fn scope<'a>(state: &'a ChatState, view: &'a TranscriptViewState) -> ProjectionScope<'a> {
    let composed = &state.messages.composed;
    let Some(settled) = view.settled.as_ref().filter(|settled| {
        settled.checked_generation == state.messages.compose_generation
            && settled.order.len() == composed.len()
    }) else {
        return crate::transcript::presentation::scope(state, view);
    };
    let stretch: Vec<ChatMessage> = settled.order[settled.messages.len()..]
        .iter()
        .map(|at| composed[*at].clone())
        .collect();
    let later = settled.later_scans(&settled.order, &stretch);
    let scans = later.as_ref().unwrap_or(&settled.prefix_scans);
    let (askers, question_tags) = settled.askers(state, scans, later.is_some(), &stretch);
    ProjectionScope {
        messages: composed,
        working: is_working(state),
        summary: view.summary_mode,
        deferred: &view.deferred,
        agent_path: &view.agent_path,
        working_directory: view.working_directory.as_deref(),
        line_breaks: line_breaks(state),
        live_side_question: crate::transcript::side_question::live_side_question(state),
        approval_wait: crate::transcript::approval_wait::approval_wait(state),
        agent_fleet: state.session.agent_fleet.as_ref(),
        session_directory: &view.session_directory,
        question_tags,
        askers,
        agent_message_deliveries: state.session.agent_message_deliveries.as_ref(),
        backfilled: &view.backfilled,
        question_forms: std::cell::OnceCell::from(scans.marked_forms()),
        tool_cards: view.tool_cards,
    }
}
