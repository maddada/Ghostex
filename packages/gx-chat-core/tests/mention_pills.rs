//! An `@` mention in a user's message is a file or folder pill only when gxserver found it on disk
//! (`mention_paths`, the SessionChat 2026-10-28 DECISION in server/src/session_chat_mention_paths.rs).

use ghostex_gx_chat_core::transcript::markdown_links::markdown_references;
use ghostex_gx_chat_core::transcript::native_markdown::native_markdown_with_mentions;

/// The file and folder paths the message's pills open.
fn pills(text: &str, found: &[&str]) -> Vec<String> {
    let found: Vec<String> = found.iter().map(|path| path.to_string()).collect();
    let marked = native_markdown_with_mentions(text, true, &found);
    markdown_references(&marked)
        .into_iter()
        .filter(|reference| matches!(reference["kind"].as_str(), Some("file" | "folder")))
        .filter_map(|reference| reference["path"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn a_real_file_is_a_pill() {
    assert_eq!(
        pills("look at @src/main.rs", &["src/main.rs"]),
        vec!["src/main.rs"]
    );
}

#[test]
fn a_real_folder_is_a_pill() {
    assert_eq!(
        pills("everything in @src/app", &["src/app"]),
        vec!["src/app"]
    );
}

#[test]
fn a_missing_path_stays_text() {
    let text = "look at @src/missing.rs";
    assert!(pills(text, &[]).is_empty());
    assert_eq!(native_markdown_with_mentions(text, true, &[]), text);
}

#[test]
fn an_npm_scope_stays_text() {
    let text = "npm install -g @deepseek-ai/dsh <- command to install deepseek harness";
    assert!(pills(text, &[]).is_empty());
    assert_eq!(native_markdown_with_mentions(text, true, &[]), text);
}

#[test]
fn an_email_and_a_decorator_stay_text() {
    assert!(pills("write to someone@example.com about @Override", &[]).is_empty());
}

#[test]
fn a_mention_in_a_code_span_stays_text_even_when_it_exists() {
    let text = "run `cat @src/main.rs` now";
    assert!(pills(text, &["src/main.rs"]).is_empty());
}

#[test]
fn only_the_found_mentions_of_a_message_are_pills() {
    assert_eq!(
        pills("fix @src/main.rs, not @deepseek-ai/dsh", &["src/main.rs"]),
        vec!["src/main.rs"]
    );
}
