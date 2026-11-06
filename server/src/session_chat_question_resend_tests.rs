use super::{claim, form_to_resend, resend_message};
use crate::session_chat::{SessionChatBlock, SessionChatMessage, SessionChatRole, SessionChatSource};

fn message(role: SessionChatRole, text: &str) -> SessionChatMessage {
    SessionChatMessage {
        id: format!("{role:?}-{}", text.len()),
        role,
        blocks: vec![SessionChatBlock::Text {
            text: text.to_string(),
        }],
        async_questions: None,
        timestamp: None,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        queued: false,
        api_error: None,
        custom_message: None,
    }
}

fn user(text: &str) -> SessionChatMessage {
    message(SessionChatRole::User, text)
}

fn reply(text: &str) -> SessionChatMessage {
    message(SessionChatRole::Assistant, text)
}

/// JSON no near-miss reading can save: a question with nothing to answer.
const BROKEN: &str = "Two things:\n\n```questions\n{\"questions\": [{\"tag\": \"Q1\", \"title\": \"Which layout?\"}]}\n```";
const READABLE: &str = "```questions\n{\"questions\": [{\"tag\": \"Q1\", \"title\": \"Which layout?\", \"choices\": [\"Tabs\", \"One page\"]}]}\n```";

#[test]
fn a_broken_block_in_the_finished_turn_is_asked_about_with_its_error_and_the_shape() {
    let form = form_to_resend(&[user("Plan it."), reply(BROKEN)]).expect("asks about it");
    let note = resend_message(&form);
    assert!(note.contains("Question 1 needs \"choices\""), "{note}");
    assert!(note.contains("{\"title\": \"Open questions\""), "{note}");
    assert!(note.contains("ghostex visual check"), "{note}");
}

#[test]
fn a_block_that_reads_or_reads_as_a_near_miss_is_never_asked_about() {
    let bare = "```questions\n[{\"id\":\"Q1\",\"question\":\"Ship it?\",\"options\":[{\"label\":\"Yes\",\"detail\":\"Now.\"}]}]\n```";
    assert!(form_to_resend(&[user("Plan it."), reply(READABLE)]).is_none());
    assert!(form_to_resend(&[user("Plan it."), reply(bare)]).is_none());
}

#[test]
fn a_thread_report_that_lands_before_the_check_does_not_hide_the_broken_block() {
    let messages = [
        user("Plan it."),
        reply(BROKEN),
        user("<agent-message>Thread report: done.</agent-message>"),
        reply("Noted."),
    ];
    assert!(form_to_resend(&messages).is_some());
}

#[test]
fn it_fires_once_per_broken_block() {
    let first = form_to_resend(&[user("Plan it."), reply(BROKEN)]).expect("asks about it");
    let asked = resend_message(&first);
    // The resend is in the transcript: the same block is not asked about again.
    assert!(form_to_resend(&[user("Plan it."), reply(BROKEN), user(&asked), reply(READABLE)]).is_none());
    assert!(form_to_resend(&[user("Plan it."), reply(BROKEN), user(&asked), reply("Busy.")]).is_none());
    // A block the agent has since re-asked in a readable form is left alone.
    assert!(form_to_resend(&[user("Plan it."), reply(BROKEN), user("Go on."), reply(READABLE)]).is_none());
    // Two turn ends seen for one block (or the next turn end before the resend reaches the
    // transcript) claim it once.
    assert!(claim(&first.id));
    assert!(!claim(&first.id));
}

#[test]
fn an_agent_that_keeps_failing_is_asked_at_most_twice_in_a_row() {
    let other = BROKEN.replace("Which layout?", "Which colour?");
    let first = form_to_resend(&[user("Plan it."), reply(BROKEN)]).unwrap();
    let second = form_to_resend(&[user(&resend_message(&first)), reply(&other)]).unwrap();
    let third = BROKEN.replace("Which layout?", "Which font?");
    assert!(form_to_resend(&[
        user("Plan it."),
        reply(BROKEN),
        user(&resend_message(&first)),
        reply(&other),
        user(&resend_message(&second)),
        reply(&third),
    ])
    .is_none());
}
