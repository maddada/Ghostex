//! A sent `[Image #N](path)` whose path holds spaces shows as its picture, as one in Ghostex's own
//! image folder does (CDXC:SessionChat 2026-11-12 in `transcript/native_markdown.rs`,
//! `bracket_spaced_image_references`). The fixture is the message the user sent on 2026-11-12:
//! three pictures pasted from the ClipBook clipboard app, whose folder is under
//! `Application Support`, which showed as raw text with each `#N` drawn as an issue link.

use ghostex_gx_chat_core::transcript::line_breaks::user_line_breaks;
use ghostex_gx_chat_core::transcript::native_markdown::{
    native_markdown_with_mentions, NATIVE_IMAGE_OPEN, NATIVE_MARK,
};
use serde_json::Value;

const CLIPBOOK: &str = "/Users/madda/Library/Application Support/ClipBook Rust/attachments";

/// The pictures the marked Markdown draws, as (label, path).
fn pictures(marked: &str) -> Vec<(String, String)> {
    marked
        .split(NATIVE_IMAGE_OPEN)
        .skip(1)
        .map(|rest| {
            let json = rest.split(NATIVE_MARK).next().unwrap_or_default();
            let source: Value = serde_json::from_str(json).expect("an image mark's source");
            (
                source["label"].as_str().unwrap_or_default().to_string(),
                source["path"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// A user's message, marked as the transcript marks it on the desktop and the phone.
fn sent(text: &str) -> String {
    native_markdown_with_mentions(&user_line_breaks(text), true, &[])
}

#[test]
fn a_clipboard_apps_pictures_with_spaces_in_their_path_show() {
    let first = format!("{CLIPBOOK}/15603337dab1384c/2026-11-12_Ghostex_10-34-56@2x.png");
    let second = format!("{CLIPBOOK}/fc4d3551f0651fc1/2026-11-12_Ghostex_10-35-47@2x.png");
    let third = format!("{CLIPBOOK}/73b073f2546515f8/2026-11-12_Ghostex_10-35-23@2x.png");
    let message = format!(
        "[Image #1]({first})\ngap is big here.. i want it always smaller between these cards, like here:\n[Image #2]({second})\n\nalso legacy model needs to be at bottom below the expand area\n[Image #3]({third})"
    );
    let marked = sent(&message);
    assert_eq!(
        pictures(&marked),
        vec![
            ("Image #1".to_string(), first),
            ("Image #2".to_string(), second),
            ("Image #3".to_string(), third),
        ]
    );
    assert!(!marked.contains("ghostex-issue:"), "{marked}");
    assert!(marked.contains("gap is big here"), "{marked}");
}

#[test]
fn an_agents_reply_shows_them_too() {
    let path = format!("{CLIPBOOK}/a/shot one.png");
    let marked = native_markdown_with_mentions(&format!("Here: [Image #4]({path})"), false, &[]);
    assert_eq!(pictures(&marked), vec![("Image #4".to_string(), path)]);
}

#[test]
fn a_path_with_brackets_or_backslashes_keeps_every_character() {
    let marked = sent(
        "[Image #1](/x y/a (1).png) and [Image #2](C:\\Users\\Me\\My Pics\\a.png) and [Image #3](/p q/a<b>.png)",
    );
    assert_eq!(
        pictures(&marked),
        vec![
            ("Image #1".to_string(), "/x y/a (1).png".to_string()),
            (
                "Image #2".to_string(),
                "C:\\Users\\Me\\My Pics\\a.png".to_string()
            ),
            ("Image #3".to_string(), "/p q/a<b>.png".to_string()),
        ]
    );
}

#[test]
fn a_reference_inside_code_stays_as_written() {
    let marked =
        sent("type `[Image #1](/a b/c.png)` to attach\n\n```\n[Image #2](/a b/d.png)\n```");
    assert!(pictures(&marked).is_empty(), "{marked}");
    assert!(marked.contains("`[Image #1](/a b/c.png)`"), "{marked}");
    assert!(marked.contains("[Image #2](/a b/d.png)"), "{marked}");
}

#[test]
fn ghostexs_own_folder_draws_as_before() {
    let path = "/Users/madda/.local/share/ghostex/i/1794465418909.png";
    let marked = sent(&format!("[Image #1]({path})\nlook"));
    assert_eq!(
        pictures(&marked),
        vec![("Image #1".to_string(), path.to_string())]
    );
}

#[test]
fn an_already_bracketed_path_is_left_alone() {
    let marked = sent("[Image #1](</a b/c.png>)");
    assert_eq!(
        pictures(&marked),
        vec![("Image #1".to_string(), "/a b/c.png".to_string())]
    );
}
