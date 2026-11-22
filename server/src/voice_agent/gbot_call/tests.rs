//! Calls with a GBot: which finished turn is the call's answer and what it is called (`reply.rs`),
//! and what a "Working:" update may say (`watch.rs`).

use serde_json::{json, Value};

use super::reply::{answers_call, finished_text};
use super::watch::{current_work, spoken_note};
use crate::gbots::CallBot;

fn bot() -> CallBot {
    CallBot {
        bot_id: "b_test".to_string(),
        name: "Researcher".to_string(),
        title: "Research helper".to_string(),
        description: String::new(),
        soul: String::new(),
        icon: "robot".to_string(),
        color: "periwinkle".to_string(),
        color_hex: "#9fb0ff".to_string(),
        voice: None,
    }
}

fn text(role: &str, text: &str) -> Value {
    json!({"role": role, "blocks": [{"type": "text", "text": text}]})
}

fn tool(name: &str) -> Value {
    json!({"role": "assistant", "blocks": [{"type": "tool-call", "name": name}]})
}

fn chat(messages: Vec<Value>) -> Value {
    json!({ "messages": messages })
}

const BRIEF: &str = "Read and adhere to this skill: $ghostex-gbots\n\nYou are Researcher, Research helper. Before your first reply, run `ghostex gbots brief`.";

#[test]
fn the_chat_s_startup_brief_is_not_the_call_s_answer() {
    let startup = chat(vec![
        text("user", BRIEF),
        tool("exec_command"),
        text("assistant", "Researcher is ready."),
    ]);
    assert!(!answers_call(&startup));
}

#[test]
fn a_turn_answering_the_call_s_message_is_its_answer() {
    let answered = chat(vec![
        text("user", BRIEF),
        text("assistant", "Researcher is ready."),
        text(
            "user",
            "[Voice call: the user is talking to you by voice.] What is two plus two?",
        ),
        text("assistant", "Two plus two is four."),
    ]);
    assert!(answers_call(&answered));
    let later = chat(vec![
        text("user", "[Voice call] And three plus three?"),
        text("assistant", "Six."),
    ]);
    assert!(answers_call(&later));
}

#[test]
fn a_turn_started_in_the_chat_itself_is_not_the_call_s_answer() {
    let typed = chat(vec![
        text("user", "[Voice call] What is two plus two?"),
        text("assistant", "Four."),
        text("user", "Now summarise the repo."),
        text("assistant", "Done."),
    ]);
    assert!(!answers_call(&typed));
}

#[test]
fn a_long_turn_with_no_user_message_in_view_still_counts() {
    let long = chat(vec![
        tool("exec_command"),
        text("assistant", "Here is the answer."),
    ]);
    assert!(answers_call(&long));
}

#[test]
fn a_finished_turn_names_the_gbot_not_the_chat() {
    assert_eq!(finished_text(&bot()), "Researcher finished its turn.");
}

#[test]
fn working_updates_never_carry_system_lines_or_commands() {
    let work = chat(vec![
        text("user", "[Voice call] What is two plus two?"),
        tool("exec_command"),
        text(
            "system",
            "<system-reminder>Command exited with code 1: rg --files /Users/me/.agents -g 'SKILL.md'</system-reminder>",
        ),
    ]);
    let said = current_work(&work);
    assert_eq!(said, "running a command");
    assert!(!said.contains("rg"));
    assert!(!said.contains('/'));
}

#[test]
fn working_updates_never_carry_tool_output() {
    let work = chat(vec![
        text("user", "[Voice call] Check the build."),
        json!({"role": "assistant", "blocks": [
            {"type": "tool-call", "name": "Bash", "input": {"command": "cargo build --release"}},
            {"type": "tool-result", "text": "error[E0432]: unresolved import `crate::foo`"}
        ]}),
        text("tool", "error: could not compile gxserver"),
    ]);
    assert_eq!(current_work(&work), "running a command");
}

#[test]
fn working_updates_use_the_agent_s_own_words_cleaned_up() {
    let work = chat(vec![
        text("user", "[Voice call] Look into it."),
        tool("Read"),
        text(
            "assistant",
            "**Reading the main guide**\n\nThen I'll check the tests.",
        ),
    ]);
    assert_eq!(
        current_work(&work),
        "reading files; its latest note: \"Reading the main guide\""
    );
}

#[test]
fn working_updates_say_still_working_when_nothing_is_safe_to_say() {
    let nothing = chat(vec![text("user", "[Voice call] Go.")]);
    assert_eq!(current_work(&nothing), "still working");
    let only_code = chat(vec![
        text("user", "[Voice call] Go."),
        text("assistant", "Running `cargo test` in server/src now."),
    ]);
    assert_eq!(current_work(&only_code), "still working");
    assert_eq!(current_work(&json!({})), "still working");
}

#[test]
fn a_note_with_a_path_file_name_link_or_command_is_not_said() {
    for unsafe_note in [
        "<system-reminder>anything</system-reminder>",
        "Looking in /Users/me/project for it.",
        "Opening ~/notes now.",
        "Editing main.rs first.",
        "Checking https://example.com for the docs.",
        "Running rg --files over the repo.",
        "Calling read_session_chat on it.",
        "Calling parse() on the input.",
        "Setting FOO=bar for the run.",
        "```\nls -la\n```",
        "Using `git status` to look.",
        "C:\\Users\\me is the folder.",
        "1234 5678",
    ] {
        assert_eq!(spoken_note(unsafe_note), None, "{unsafe_note}");
    }
}

#[test]
fn a_note_is_cut_to_its_first_sentence_and_kept_short() {
    assert_eq!(
        spoken_note("## Checking the sources. Then writing it up.").as_deref(),
        Some("Checking the sources")
    );
    let long = "Comparing what the three newest reviews say about battery life with what the maker claims on its own page and in the manual";
    let said = spoken_note(long).unwrap();
    assert!(said.chars().count() <= 101, "{said}");
    assert!(said.ends_with('…'));
}
