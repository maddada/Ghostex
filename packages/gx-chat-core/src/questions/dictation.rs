//! Dictate's words into a draft: the question form box the recording started in, or the chat box.
//!
//! The renderer knows which box had the keyboard when the recording started and where its caret
//! is; whether that box still takes text, and its text now, are the form's own state
//! (`questions/forms.rs`). So the renderer names the box and this writes the words, and keeps them
//! in the box's draft when a copy of the box that has not shown them yet writes its text.

use serde_json::{json, Value};

use crate::action::UserAction;
use crate::composer::text::{utf16_len, Utf16Text};
use crate::effect::Effect;
use crate::questions::forms::{self, FormBox};
use crate::state::{ChatContext, ChatState};

/// The host action that says a draft has Dictate's words in it.
pub const DICTATION_INSERTED: &str = "dictationInserted";

/// One dictation typed into a form box, as it was asked for: the words as heard, and Insert with
/// spacing (`QuestionsState::dictations`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Typed {
    /// The box's `rev` once these words were in.
    pub rev: u64,
    pub words: String,
    pub spacing: bool,
}

/// Names this core, with its first event, for the `rev`s it gives its form boxes
/// (`QuestionsState::core_id`).
pub fn name_core(state: &mut ChatState, context: &ChatContext) {
    if state.questions.core_id.is_empty() {
        state.questions.core_id = context.random_id(0);
    }
}

/// `insertDictation {text, spacing?, id?, draft, start, end, form?, slot?, shown?, shownStart?,
/// shownEnd?}`: types Dictate's words into a draft and says where they went.
///
/// `form` and `slot` (a question's index, or `"notes"`) name the form box the recording started
/// in; without them the chat box is the target. `shown`, `shownStart` and `shownEnd` are that box
/// as the renderer draws it now, text and selection, left out when it is not drawn. `draft`,
/// `start` and `end` are the chat box's, always sent: it takes the words when the form box no
/// longer does. Offsets are UTF-16. `spacing` is Insert with spacing (on when absent). `id` names
/// the recording for the renderer that keeps it until its words are in a draft; it comes back in
/// the answer and means nothing here.
///
/// Answered by the host action `dictationInserted {id, form, slot, core, rev, supersedes,
/// content, caret, text, spacing}`. `content` is the target's text now and `caret` the offset
/// after the words; every copy of the target that shows one of the texts in `supersedes` shows
/// `content` instead, without reporting it as an edit. For a form box (`form` and `slot` set) the
/// words are in its draft when the answer leaves, in the effects after the draft's save. For the
/// chat box (`form` null) the renderer holds the draft: a chat box that no longer shows a text in
/// `supersedes` asks again with its text as it is then (`text` and `spacing` are what it asked
/// with), and the words are in once it showed `content`.
///
/// A box's `rev` counts the dictations this core typed into it, and `core` names the core that
/// counted. A copy of the box sends both with its edits, as the answer or the form's document
/// gave them with the text it shows (`edited`). A core that starts again counts from nothing
/// under another name, so a `rev` the earlier one gave is never read as its own.
///
/// CDXC:Dictate 2026-11-10 WHY: The renderers first chose the box themselves and wrote its whole text back with `formText`. That write could reach the core after Send answers or Skip all, which refuses it without a word, so a dictation finished in that moment reached neither the answers nor the chat box; and it was built from one drawn copy of the box, so it undid what the reader had typed in the other (the transcript's box and the Resources copy share one draft). Here the check and the write are one step in the order the core receives them: a box of an open form the chat still has gets the words in its draft as it is now, anything else puts them in the chat box. The words then stay: a copy's whole-text write that was made before the copy showed them is given them again (`edited`), every one of them however far behind the copy is, so no later write and no Send takes them out, and a renderer never has to write them back itself. Whether a recording's words are already in a draft is the renderer's to know (it keeps the recording), not this module's: a list of recordings kept here was lost with the core and could not tell a dismissed recording that is transcribed again from one typed twice. A `rev` was the clock's time at first, so that a core that started again gave later numbers than the one before it. A clock goes back, and the new core has none of the dictations its restored drafts hold, so it could neither tell that a copy from before was behind nor give it the words. A write stamped by another core is refused instead and its copy shows the draft as this core has it: one rule for every writer that outlives its core, and nothing about dictations has to be stored.
/// CDXC:Dictate 2026-11-10 SEE-ALSO: `aim_dictation` and `dictation_inserted` in apps/desktop/src/app/native_chat/dictation.rs; apps/mobile/app/src/chat/native/composer/dictationTarget.ts, src/chat/rust/host.ts (`dictationInserted`) and src/voice/dictation.ts (the recording).
pub fn insert(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let words = string(action, "text").trim();
    if words.is_empty() {
        return Vec::new();
    }
    let spacing = action.param("spacing").and_then(Value::as_bool) != Some(false);
    let id = action.param("id").and_then(Value::as_str);
    let target = action
        .param("form")
        .and_then(Value::as_str)
        .zip(action.param("slot").and_then(FormBox::of));
    if let Some((form, slot)) = target {
        if let Some(before) = forms::box_text(state, form, slot).map(str::to_owned) {
            let shown = action.param("shown").and_then(Value::as_str);
            // The selection belongs to the text the renderer draws. A box whose draft is no longer
            // that text was edited in its other copy: the words go after what is there.
            let (start, end) = if shown == Some(before.as_str()) {
                (offset(action, "shownStart"), offset(action, "shownEnd"))
            } else {
                (usize::MAX, usize::MAX)
            };
            let placed = place(&before, words, start, end, spacing);
            let mut effects = write(state, form, slot, &placed.content);
            let rev = typed(state, form, slot, words, spacing);
            let mut supersedes = vec![before.as_str()];
            supersedes.extend(shown.filter(|shown| *shown != before));
            effects.push(answer(json!({
                "id": id,
                "form": form,
                "slot": slot.to_json(),
                "core": state.questions.core_id,
                "rev": rev,
                "supersedes": supersedes,
                "content": placed.content,
                "caret": placed.caret,
                "text": words,
                "spacing": spacing,
            })));
            return effects;
        }
    }
    let draft = string(action, "draft");
    let placed = place(
        draft,
        words,
        offset(action, "start"),
        offset(action, "end"),
        spacing,
    );
    vec![answer(json!({
        "id": id,
        "form": null,
        "slot": null,
        "supersedes": [draft],
        "content": placed.content,
        "caret": placed.caret,
        "text": words,
        "spacing": spacing,
    }))]
}

/// A reader's whole-text write to a form box: `formText {form, question, text, rev?, core?,
/// caret?}` or `formNotes {form, text, rev?, core?, caret?}`.
///
/// `rev` and `core` are the copy's, for the text it showed before the edit (see `insert`), and
/// `caret` is its caret after the edit. A copy behind the box's `rev` made its text without the
/// dictations since: their words go into the text at `caret` (the end without one), by the same
/// rule as when they were first typed, and the copies are told the box's text
/// (`dictationInserted`, with no `id`). A copy another core stamped shows a draft this core never
/// gave it: nothing of its text is written, and it is told the box's text as it is here. A write
/// without `rev` is from a renderer that types no dictation into a form (a released phone) and
/// goes in whole, as it always did.
pub fn edited(
    state: &mut ChatState,
    form: &str,
    slot: FormBox,
    action: &UserAction,
) -> Vec<Effect> {
    let text = string(action, "text");
    match reconciled(state, form, slot, text, action) {
        None => write(state, form, slot, text),
        Some((None, told)) => vec![told],
        Some((Some(content), told)) => {
            let mut effects = write(state, form, slot, &content);
            effects.push(told);
            effects
        }
    }
}

/// What a copy's write comes to when the copy does not show the box's draft as this core has it
/// (`edited`): the text to write in its place (none for another core's copy) and what the copies
/// are told. `None` for a write that goes in whole.
///
/// Every dictation of a box is kept while its form can still take text
/// (`QuestionsState::dictations`), so a copy is never too far behind to be given its words.
fn reconciled(
    state: &ChatState,
    form: &str,
    slot: FormBox,
    text: &str,
    action: &UserAction,
) -> Option<(Option<String>, Effect)> {
    let shown = action.param("rev").and_then(Value::as_u64)?;
    let was = forms::box_text(state, form, slot)?;
    let key = slot.key();
    let questions = &state.questions;
    let rev = questions
        .forms
        .get(form)
        .and_then(|entry| entry.dictated.get(&key))
        .copied()
        .unwrap_or(0);
    let told = |supersedes: &[&str], content: &str, caret: usize| {
        answer(json!({
            "id": null,
            "form": form,
            "slot": slot.to_json(),
            "core": questions.core_id,
            "rev": rev,
            "supersedes": supersedes,
            "content": content,
            "caret": caret,
        }))
    };
    if action.param("core").and_then(Value::as_str) != Some(questions.core_id.as_str()) {
        return Some((None, told(&[text], was, utf16_len(was))));
    }
    let missing = questions
        .dictations
        .get(form)?
        .get(&key)?
        .iter()
        .filter(|typed| typed.rev > shown);
    let mut content = text.to_string();
    let mut caret = offset(action, "caret");
    for typed in missing {
        let placed = place(&content, &typed.words, caret, caret, typed.spacing);
        content = placed.content;
        caret = placed.caret;
    }
    if content == text {
        return None;
    }
    let told = told(&[was, text], &content, caret);
    Some((Some(content), told))
}

fn write(state: &mut ChatState, form: &str, slot: FormBox, text: &str) -> Vec<Effect> {
    match slot {
        FormBox::Question(question) => forms::write(state, form, question, text),
        FormBox::Notes => forms::write_notes(state, form, text),
    }
}

/// Keeps a dictation typed into the box and answers the box's `rev` with it in.
fn typed(state: &mut ChatState, form: &str, slot: FormBox, words: &str, spacing: bool) -> u64 {
    let Some(entry) = state.questions.forms.get_mut(form) else {
        return 0;
    };
    let key = slot.key();
    let rev = entry.dictated.entry(key.clone()).or_default();
    *rev += 1;
    let rev = *rev;
    state
        .questions
        .dictations
        .entry(form.to_string())
        .or_default()
        .entry(key)
        .or_default()
        .push(Typed {
            rev,
            words: words.to_string(),
            spacing,
        });
    rev
}

fn string<'a>(action: &'a UserAction, name: &str) -> &'a str {
    action
        .param(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// A UTF-16 offset; a missing one is the end of the text.
fn offset(action: &UserAction, name: &str) -> usize {
    action
        .param(name)
        .and_then(Value::as_u64)
        .map_or(usize::MAX, |offset| offset as usize)
}

fn answer(params: Value) -> Effect {
    Effect::HostAction {
        action: DICTATION_INSERTED.to_string(),
        params: Box::new(params),
    }
}

/// A draft with the dictated words in it.
struct Placed {
    content: String,
    /// The UTF-16 offset after the words.
    caret: usize,
}

/// `words` over `[start, end)` of `before` (UTF-16 offsets, clamped to the text), spaced from the
/// characters on either side while `spacing` is on.
fn place(before: &str, words: &str, start: usize, end: usize, spacing: bool) -> Placed {
    let indexed = Utf16Text::new(before);
    let from = indexed.index_of_offset(start.min(indexed.utf16_len()));
    let to = indexed
        .index_of_offset(end.min(indexed.utf16_len()))
        .max(from);
    let inserted = if spacing {
        spaced_at_caret(
            from.checked_sub(1).and_then(|index| indexed.at(index)),
            indexed.at(to),
            words,
        )
    } else {
        words.to_string()
    };
    Placed {
        content: format!(
            "{}{inserted}{}",
            indexed.slice(0, from),
            indexed.slice(to, indexed.len())
        ),
        caret: indexed.offset(from) + utf16_len(&inserted),
    }
}

/// `text` with the spaces the caret's neighbours need: one before it after a word or a closing
/// mark (not at the start, after a space, or after an opening bracket or quote), and one after
/// it before a word (not before a space or a punctuation mark that hugs the word before it).
///
/// CDXC:VoiceMode 2026-11-05 DECISION: User (Q1264) picked "Spacing at the caret": the space
/// is decided by the characters next to the caret, not by the end of the draft, so text
/// dictated into the middle of a sentence gets a space on each side it needs one and none
/// before a comma or after an opening bracket. `dictateInsertSpacing` off types it as heard.
pub fn spaced_at_caret(before: Option<char>, after: Option<char>, text: &str) -> String {
    let first = text.chars().next();
    let last = text.chars().next_back();
    let hugs_left = |c: char| {
        matches!(
            c,
            ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '…' | '%'
        )
    };
    let opens = |c: char| {
        matches!(
            c,
            '(' | '[' | '{' | '"' | '\'' | '`' | '/' | '@' | '#' | '“' | '‘'
        )
    };
    let space_before = before.is_some_and(|c| !c.is_whitespace() && !opens(c))
        && first.is_some_and(|c| !c.is_whitespace() && !hugs_left(c));
    let space_after = after
        .is_some_and(|c| !c.is_whitespace() && !hugs_left(c) && c != '"' && c != '\'')
        && last.is_some_and(|c| !c.is_whitespace() && !opens(c));
    let mut spaced = String::with_capacity(text.len() + 2);
    if space_before {
        spaced.push(' ');
    }
    spaced.push_str(text);
    if space_after {
        spaced.push(' ');
    }
    spaced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_draft_or_a_caret_after_a_space_gets_no_leading_space() {
        assert_eq!(spaced_at_caret(None, None, "hello there"), "hello there");
        assert_eq!(spaced_at_caret(Some(' '), None, "hello"), "hello");
        assert_eq!(spaced_at_caret(Some('\n'), None, "hello"), "hello");
    }

    #[test]
    fn a_caret_right_after_a_word_gets_a_space_before() {
        assert_eq!(spaced_at_caret(Some('d'), None, "hello"), " hello");
        assert_eq!(spaced_at_caret(Some('.'), None, "Next one"), " Next one");
    }

    #[test]
    fn a_caret_in_the_middle_of_a_sentence_is_spaced_on_both_sides() {
        // "fix the|bug" with the caret between "the" and "bug".
        assert_eq!(spaced_at_caret(Some('e'), Some('b'), "login"), " login ");
        // "fix the |bug": the space is already before the caret.
        assert_eq!(spaced_at_caret(Some(' '), Some('b'), "login"), "login ");
        // "fix the| bug": the space is already after the caret.
        assert_eq!(spaced_at_caret(Some('e'), Some(' '), "login"), " login");
    }

    #[test]
    fn the_end_of_the_draft_is_not_what_decides() {
        // The draft ends in a word, but the caret sits at its start.
        assert_eq!(spaced_at_caret(None, Some('w'), "Hello"), "Hello ");
    }

    #[test]
    fn punctuation_and_brackets_hug_their_word() {
        assert_eq!(
            spaced_at_caret(Some('('), Some(')'), "see notes"),
            "see notes"
        );
        assert_eq!(spaced_at_caret(Some('d'), Some(','), "please"), " please");
        assert_eq!(spaced_at_caret(Some('d'), Some('.'), "now"), " now");
        assert_eq!(spaced_at_caret(Some('d'), None, ", then"), ", then");
        assert_eq!(spaced_at_caret(Some('"'), Some('"'), "quoted"), "quoted");
    }
}
