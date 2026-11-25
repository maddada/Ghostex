//! GitHub #768: Claude Code's slash command menu, open under the input box after Ghostex typed
//! `/compact`, rebuilt from the reporter's screen text (Claude Code 2.1.332). It is not a question,
//! so the Return that runs the command goes through; a real panel in the same slot is still one.

use crate::session_chat_claude_dialog::detect_claude_dialog;
use crate::session_chat_send::{guarded_write_refusal, InputGuard};

const RULE: &str = "────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────";

fn screen(slot: &str) -> String {
    format!("⏺ Done, the tests pass.\n\n{RULE}\n❯ /compact\n{RULE}\n{slot}")
}

const MENU: &str = "  ❯ /compact                                  Free up context by summarizing the conversation so far
    /autocompact                              Set how full the context gets before auto-summarizing
    /rea:compare_application_versions (MCP)   Compare two shipped application artifacts through complete inventory
                                              and optional static or runtime evidence without equating missing data
                                              with unchanged behavior. (arguments: left_target_path, right_target_path,
                                              left_manifest_id, right_manifest_id, focus_occurrence_id)
";

#[test]
fn the_command_menu_is_not_a_question_and_the_return_goes_through() {
    let screen = screen(MENU);
    assert!(detect_claude_dialog(&screen).is_none());
    assert_eq!(
        guarded_write_refusal(
            Some("claude"),
            &InputGuard::Holds("/compact".to_string()),
            Some(&screen)
        ),
        Ok(())
    );
}

#[test]
fn a_titled_panel_in_the_same_slot_is_still_a_question() {
    let panel = "  Run /compact now?\n\n  ❯ 1. Yes\n    2. No\n";
    let screen = screen(panel);
    let dialog = detect_claude_dialog(&screen).expect("dialog");
    assert_eq!(dialog.title, "Run /compact now?");
    assert!(guarded_write_refusal(
        Some("claude"),
        &InputGuard::Holds("/compact".to_string()),
        Some(&screen)
    )
    .is_err());
}
