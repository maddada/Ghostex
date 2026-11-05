//! The clean-up after transcription: aliases replace whole words in any case, fillers go from
//! English text only, and a polish answer is used only when it looks like the dictation.

use super::cleanup::{
    apply_aliases, clean, polish_prompt, polished_text, remove_fillers, skip_polish,
};
use super::settings::{DictateSettings, DictionaryWord};

fn dictionary() -> Vec<DictionaryWord> {
    vec![
        DictionaryWord {
            word: "gxserver".to_string(),
            aliases: vec!["GX server".to_string(), "g x server".to_string()],
        },
        DictionaryWord {
            word: "SpaceO".to_string(),
            aliases: vec!["space oh".to_string()],
        },
    ]
}

#[test]
fn an_alias_becomes_its_word_whatever_its_case() {
    assert_eq!(
        apply_aliases("Restart the gx Server, then open Space Oh.", &dictionary()),
        "Restart the gxserver, then open SpaceO."
    );
}

#[test]
fn the_longest_alias_wins_and_only_whole_words_match() {
    assert_eq!(
        apply_aliases("the g x server log", &dictionary()),
        "the gxserver log"
    );
    assert_eq!(
        apply_aliases("a GX servers list and aspace oh", &dictionary()),
        "a GX servers list and aspace oh",
        "no match inside a longer word"
    );
}

#[test]
fn text_without_aliases_is_unchanged() {
    assert_eq!(
        apply_aliases("Nothing to fix here.", &[]),
        "Nothing to fix here."
    );
}

#[test]
fn fillers_go_from_english_speech() {
    let cleaned = remove_fillers("So um I think, uh, we should ship it.", Some("en"));
    assert!(!cleaned.to_lowercase().contains(" um"), "{cleaned}");
    assert!(!cleaned.to_lowercase().contains("uh"), "{cleaned}");
    assert!(cleaned.contains("we should ship it."), "{cleaned}");
    let auto = remove_fillers("Um, open the file.", None);
    assert!(
        !auto.to_lowercase().starts_with("um"),
        "auto-detect reads as English: {auto}"
    );
}

#[test]
fn other_languages_keep_their_words() {
    let german = "Ähm, öffne die Datei.";
    assert_eq!(remove_fillers(german, Some("de")), german);
}

#[test]
fn clean_runs_aliases_then_fillers_as_the_settings_say() {
    let mut settings = DictateSettings::first_run(false, false);
    settings.dictionary = dictionary();
    let cleaned = clean("  Um, restart the GX server.  ", &settings);
    assert!(cleaned.contains("gxserver"), "{cleaned}");
    assert!(!cleaned.to_lowercase().starts_with("um"), "{cleaned}");
    settings.remove_fillers = false;
    let kept = clean("Um, restart the GX server.", &settings);
    assert!(kept.starts_with("Um,"), "{kept}");
}

#[test]
fn the_polish_prompt_carries_the_terms_and_marks_the_dictation() {
    let prompt = polish_prompt(
        "Fix it.",
        &["Ghostex".to_string(), "SpaceO".to_string()],
        "hello there",
    );
    assert!(prompt.starts_with("Fix it."));
    assert!(prompt.contains("Terms: Ghostex, SpaceO"));
    assert!(prompt.ends_with("<dictation>\nhello there\n</dictation>"));
    let no_terms = polish_prompt("Fix it.", &[], "hi");
    assert!(!no_terms.contains("Terms:"));
}

#[test]
fn a_polish_answer_is_used_only_when_it_looks_like_the_dictation() {
    let original = "so i think we should ship the fix today no i mean tomorrow";
    assert_eq!(
        polished_text("I think we should ship the fix tomorrow.\n", original).as_deref(),
        Some("I think we should ship the fix tomorrow.")
    );
    assert_eq!(
        polished_text("```text\nShip it tomorrow, I think.\n```", original).as_deref(),
        Some("Ship it tomorrow, I think."),
        "a code fence is unwrapped"
    );
    assert_eq!(polished_text("   ", original), None, "empty");
    let essay = "Sure! ".repeat(60);
    assert_eq!(
        polished_text(&essay, original),
        None,
        "far longer than the dictation"
    );
    assert_eq!(
        polished_text("Ok.", original),
        None,
        "far shorter than the dictation"
    );
}

#[test]
fn skip_reports_nothing_when_no_polish_runs() {
    assert!(!skip_polish("no-such-recording"));
}
