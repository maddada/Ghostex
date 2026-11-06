//! The ```questions near-misses the chat reads as the form they mean (CDXC:SessionChat 2026-11-05
//! in packages/gx-visual/src/questions.rs), and the broken JSON it still refuses.

use ghostex_gx_chat_core::transcript::question_forms::{parse_form, QuestionForm};
use ghostex_gx_visual::questions::check_form;

fn shown(body: &str) -> QuestionForm {
    let form = parse_form(body);
    assert!(form.error.is_empty(), "{body}\n→ {}", form.error);
    form
}

fn tags(form: &QuestionForm) -> Vec<&str> {
    form.questions.iter().map(|q| q.tag.as_str()).collect()
}

/// The block from the 10.47.0 user's coordinator, as it wrote it: a bare list of questions.
const SCREENSHOT: &str = r#"[ {"id":"Q1","question":"Which plan should the new workspace use?","options":[{"label":"Team","detail":"Shared billing, up to 20 seats."},{"label":"Solo","detail":"One seat, cheapest."}]}, {"id":"Q2","question":"Who should get the invite first?","options":[{"label":"Just me","detail":"Try it alone before inviting anyone."},{"label":"The whole team","detail":"Everyone starts together."}]} ]"#;

#[test]
fn the_screenshots_bare_list_of_questions_shows_as_the_form() {
    let form = shown(SCREENSHOT);
    assert_eq!(form.title, "Ghostex form");
    assert_eq!(tags(&form), ["Q1", "Q2"]);
    assert_eq!(
        form.questions[0].title,
        "Which plan should the new workspace use?"
    );
    assert_eq!(form.questions[0].choices[0].label, "Team");
    assert_eq!(
        form.questions[0].choices[0].detail,
        "Shared billing, up to 20 seats."
    );
    assert_eq!(form.questions[1].choices.len(), 2);
    let (_, mended) = check_form(SCREENSHOT);
    assert_eq!(
        mended,
        ["Wrap the list of questions in one object: {\"questions\": [ … ]}."]
    );
}

#[test]
fn a_lone_question_object_is_a_form_of_one() {
    let form = shown(r#"{"tag": "Q7", "title": "Merge it?", "choices": ["Yes", "No"]}"#);
    assert_eq!(
        form.title, "Ghostex form",
        "the question's title is not the form's"
    );
    assert_eq!(tags(&form), ["Q7"]);
    assert_eq!(form.questions[0].title, "Merge it?");
    let text = shown(r#"{"tag": "Q8", "title": "What name?", "type": "text"}"#);
    assert!(text.questions[0].text);
}

#[test]
fn questions_holding_one_object_is_a_list_of_one() {
    let form = shown(
        r#"{"title": "Release", "questions": {"tag": "Q3", "title": "Ship today?", "choices": ["Yes", "No"]}}"#,
    );
    assert_eq!(form.title, "Release");
    assert_eq!(tags(&form), ["Q3"]);
}

#[test]
fn plain_string_options_are_choices() {
    let form =
        shown(r#"{"questions": [{"tag": "Q4", "question": "Which?", "options": ["A", "B"]}]}"#);
    assert_eq!(form.questions[0].choices[1].label, "B");
    assert!(check_form(
        r#"{"questions": [{"tag": "Q4", "question": "Which?", "options": ["A", "B"]}]}"#
    )
    .1
    .is_empty());
}

#[test]
fn text_or_question_stands_in_for_the_title() {
    let form = shown(
        r#"{"questions": [{"tag": "Q5", "text": "Which colour?", "choices": [{"text": "Blue"}, "Red"]}, {"tag": "Q6", "question": "Why?", "type": "text"}]}"#,
    );
    assert_eq!(form.questions[0].title, "Which colour?");
    assert_eq!(form.questions[0].choices[0].label, "Blue");
    assert_eq!(form.questions[1].title, "Why?");
}

#[test]
fn a_missing_tag_between_tagged_questions_is_numbered() {
    let form = shown(
        r#"{"questions": [{"tag": "Q40", "title": "A?", "choices": ["x"]}, {"title": "B?", "choices": ["x"]}, {"tag": "Q42", "title": "C?", "choices": ["x"]}, {"title": "D?", "choices": ["x"]}]}"#,
    );
    assert_eq!(tags(&form), ["Q40", "Q41", "Q42", "Q43"]);
    let leading = shown(
        r#"[{"title": "A?", "choices": ["x"]}, {"id": "Q12", "title": "B?", "choices": ["x"]}]"#,
    );
    assert_eq!(tags(&leading), ["Q11", "Q12"]);
}

#[test]
fn a_missing_tag_stays_unnumbered_when_numbering_it_is_not_safe() {
    // No tag in the block to count from: the conversation's next number is unknown here.
    assert_eq!(
        tags(&shown(
            r#"[{"title": "A?", "choices": ["x"]}, {"title": "B?", "choices": ["x"]}]"#
        )),
        ["", ""]
    );
    // No free number between Q1 and Q2.
    assert_eq!(
        tags(&shown(
            r#"[{"tag": "Q1", "title": "A?", "choices": ["x"]}, {"title": "B?", "choices": ["x"]}, {"tag": "Q2", "title": "C?", "choices": ["x"]}]"#
        )),
        ["Q1", "", "Q2"]
    );
    // Tags of two kinds.
    assert_eq!(
        tags(&shown(
            r#"[{"tag": "Q1", "title": "A?", "choices": ["x"]}, {"title": "B?", "choices": ["x"]}, {"tag": "R3", "title": "C?", "choices": ["x"]}]"#
        )),
        ["Q1", "", "R3"]
    );
}

#[test]
fn trailing_commas_and_smart_quotes_are_mended() {
    let form = shown(
        "{\u{201C}questions\u{201D}: [{\u{201C}tag\u{201D}: \u{201C}Q9\u{201D}, \u{201C}title\u{201D}: \u{201C}Say \"hi\"?\u{201D}, \u{201C}choices\u{201D}: [\u{201C}Yes\u{201D}, \u{201C}No\u{201D},],},],}",
    );
    assert_eq!(tags(&form), ["Q9"]);
    assert_eq!(form.questions[0].title, "Say \"hi\"?");
    assert_eq!(form.questions[0].choices.len(), 2);
    // A smart quote inside an ordinary string is text, and a comma inside a string stays.
    let text = shown(
        r#"{"questions": [{"tag": "Q10", "title": "Call it “Atlas”, or not?", "choices": ["Yes", "No",]}]}"#,
    );
    assert_eq!(text.questions[0].title, "Call it “Atlas”, or not?");
    assert_eq!(
        check_form(r#"{"questions": [{"tag": "Q10", "title": "A?", "choices": ["Yes",]}]}"#).1,
        ["Write plain JSON: straight double quotes (\") and no comma before a closing ] or }."]
    );
}

#[test]
fn genuinely_broken_json_keeps_its_strict_error() {
    let form = parse_form(r#"{"questions": [{"tag": "Q1", "title": "A?" "choices": ["x"]}]}"#);
    assert!(
        form.error
            .starts_with("The questions block is not valid JSON:"),
        "{}",
        form.error
    );
    let form = parse_form(r#""just text""#);
    assert_eq!(
        form.error,
        "A questions block is one JSON object with a \"questions\" list."
    );
    let form = parse_form(r#"{"title": "Nothing asked"}"#);
    assert_eq!(
        form.error,
        "A questions block needs a \"questions\" list with at least one question."
    );
    let form = parse_form(r#"[{"tag": "Q1", "title": "A?"}]"#);
    assert_eq!(
        form.error,
        "Question 1 needs \"choices\", or \"type\": \"text\" for a text answer."
    );
    assert!(check_form(r#"[{"tag": "Q1", "title": "A?"}]"#).1.is_empty());
}

#[test]
fn the_canonical_block_reads_with_nothing_to_mend() {
    let body = r#"{"title": "Open questions", "questions": [{"tag": "Q1", "title": "Which layout?", "choices": [{"label": "Tabs", "recommended": true}, "One long page"]}, {"tag": "Q2", "title": "Name?", "type": "text"}]}"#;
    let (form, mended) = check_form(body);
    assert!(form.error.is_empty());
    assert!(mended.is_empty(), "{mended:?}");
}
