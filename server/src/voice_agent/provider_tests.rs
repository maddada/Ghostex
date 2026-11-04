//! The limits a provider puts on the dynamic parts of `session.update`, and the log line a refused
//! session leaves behind.

use serde_json::{json, Value};

use super::provider::{
    openai_transcription_prompt, xai_keyterms, Provider, OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS,
    XAI_KEYTERMS_MAX_COUNT, XAI_KEYTERM_MAX_CHARS,
};

fn terms(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn a_short_list_reaches_openai_unchanged() {
    let prompt = openai_transcription_prompt(&terms(&["Ghostex", "Claude", "Alpha Beta"]));
    assert_eq!(prompt, "Names you may hear: Ghostex, Claude, Alpha Beta.");
}

#[test]
fn a_long_list_stays_under_openais_limit_and_keeps_whole_names() {
    // About 1,500 characters: the size that OpenAI refused with "got a string with length 1518".
    let names: Vec<String> = (0..80).map(|n| format!("Project Number {n:02}")).collect();
    let prompt = openai_transcription_prompt(&names);
    assert!(prompt.chars().count() <= OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS);
    assert!(
        prompt.chars().count() > OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS - 30,
        "as many names as fit are kept"
    );
    assert!(prompt.starts_with("Names you may hear: Project Number 00, Project Number 01"));
    assert!(prompt.ends_with('.'));
    let body = prompt
        .strip_prefix("Names you may hear: ")
        .and_then(|rest| rest.strip_suffix('.'))
        .expect("lead-in and full stop");
    let kept: Vec<&str> = body.split(", ").collect();
    for (index, name) in kept.iter().enumerate() {
        assert_eq!(*name, names[index], "name {index} is whole and in order");
    }
    assert!(
        kept.len() < names.len(),
        "the least useful names are dropped"
    );
}

#[test]
fn the_list_stops_at_the_first_name_that_does_not_fit() {
    let long = "x".repeat(OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS);
    let prompt = openai_transcription_prompt(&terms(&["Ghostex", &long, "Claude"]));
    assert_eq!(prompt, "Names you may hear: Ghostex.");
}

#[test]
fn multibyte_names_are_counted_in_characters() {
    let names: Vec<String> = (0..400).map(|_| "日本語の名前".to_string()).collect();
    let prompt = openai_transcription_prompt(&names);
    assert!(prompt.chars().count() <= OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS);
    assert!(prompt.ends_with("日本語の名前."));
}

#[test]
fn no_names_that_fit_leaves_the_prompt_out_of_the_session() {
    assert_eq!(openai_transcription_prompt(&[]), "");
    let update = Provider::OpenAi.session_update("marin", "hi", &json!([]), &[], true);
    let transcription = &update["session"]["audio"]["input"]["transcription"];
    assert_eq!(transcription["model"], "gpt-4o-transcribe");
    assert!(transcription.get("prompt").is_none());
}

#[test]
fn openai_session_update_carries_the_trimmed_prompt() {
    let names: Vec<String> = (0..80).map(|n| format!("Project Number {n:02}")).collect();
    let update = Provider::OpenAi.session_update("marin", "hi", &json!([]), &names, true);
    let prompt = update["session"]["audio"]["input"]["transcription"]["prompt"]
        .as_str()
        .expect("prompt");
    assert!(prompt.chars().count() <= OPENAI_TRANSCRIPTION_PROMPT_MAX_CHARS);
}

#[test]
fn xai_keeps_at_most_100_terms_of_50_characters() {
    let mut names: Vec<String> = (0..150).map(|n| format!("Project {n}")).collect();
    names.insert(3, "y".repeat(XAI_KEYTERM_MAX_CHARS + 1));
    let kept = xai_keyterms(&names);
    assert_eq!(kept.len(), XAI_KEYTERMS_MAX_COUNT);
    assert!(kept
        .iter()
        .all(|term| term.chars().count() <= XAI_KEYTERM_MAX_CHARS));
    assert_eq!(
        kept[3], "Project 3",
        "an over-long term is dropped, not cut"
    );
    let exact = "z".repeat(XAI_KEYTERM_MAX_CHARS);
    assert_eq!(xai_keyterms(&[exact.clone()]), vec![exact.as_str()]);
}

#[test]
fn xai_session_update_carries_the_capped_terms() {
    let names: Vec<String> = (0..150).map(|n| format!("Project {n}")).collect();
    let update = Provider::Xai.session_update("eve", "hi", &json!([]), &names, false);
    let sent = &update["session"]["audio"]["input"]["transcription"]["keyterms"];
    assert_eq!(sent.as_array().map(Vec::len), Some(XAI_KEYTERMS_MAX_COUNT));
}

#[test]
fn an_error_event_is_described_with_provider_type_code_and_message() {
    let event = json!({
        "type": "error",
        "event_id": "event_1",
        "error": {
            "type": "invalid_request_error",
            "code": "string_above_max_length",
            "message": "Invalid 'session.audio.input.transcription.prompt': string too long. Expected a string with maximum length 1024, but got a string with length 1518 instead.",
            "param": "session.audio.input.transcription.prompt",
        },
    });
    let report = Provider::OpenAi.describe_error(&event);
    assert!(report.message.contains("string too long"));
    assert_eq!(report.details["code"], "string_above_max_length");
    assert_eq!(report.details["provider"], "openai");
    assert_eq!(report.details["eventType"], "error");
    assert_eq!(
        report.details["param"],
        "session.audio.input.transcription.prompt"
    );
    assert_eq!(report.details["errorType"], "invalid_request_error");
}

#[test]
fn an_error_without_a_message_still_says_something() {
    let report = Provider::Xai.describe_error(&json!({"type": "error"}));
    assert_eq!(report.message, "The provider reported an error.");
    assert_eq!(report.details["provider"], "xai");
}

#[test]
fn a_refused_session_is_findable_in_the_log_with_the_providers_sentence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
    let logger = crate::logging::GxserverLogger::new(paths.clone());
    let event = json!({
        "type": "error",
        "error": {
            "code": "string_above_max_length",
            "message": "Invalid 'session.audio.input.transcription.prompt': string too long. Expected a string with maximum length 1024, but got a string with length 1518 instead.",
        },
    });
    let report = Provider::OpenAi.describe_error(&event);
    super::log_warn_error(
        &logger,
        "S1abc",
        "voiceAgent.sessionRefused",
        &report.message,
        report.details,
    );
    let text = std::fs::read_to_string(paths.log_file).expect("read log");
    let line: Value = serde_json::from_str(text.lines().last().expect("a log line")).expect("json");
    assert_eq!(line["event"], "voiceAgent.sessionRefused");
    assert_eq!(line["details"]["provider"], "openai");
    assert_eq!(line["details"]["eventType"], "error");
    assert!(line["error"]
        .as_str()
        .unwrap_or_default()
        .contains("string too long"));
    assert!(
        text.contains("voice"),
        "`ghostex logs --grep voice` matches"
    );
}
