//! The plan Claude's "Ready to code?" dialog asks to approve, read whole from Claude's plan file.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

use crate::session_chat_notice::SessionChatTerminalNotice;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

/// The title Claude gives its plan approval dialog.
const READY_TO_CODE: &str = "Ready to code?";
/// A plan larger than this is cut; nobody approves a plan this long from a card.
const PLAN_MAX_BYTES: u64 = 256 * 1024;
/// How far back from the transcript's end the plan file's path is looked for.
const TRANSCRIPT_SCAN_MAX_BYTES: u64 = 8 * 1024 * 1024;

/*
CDXC:SessionChat 2026-10-26 WHY:
"Ready to code?" draws the plan in a fixed-height pane that only Claude's own arrow keys scroll, so the chat card (and "Show terminal output") showed the first screenful and nothing else: the plan could not be read before approving it (GitHub #321). Claude writes the plan to its plan file before the dialog opens (ExitPlanMode carries no plan; the transcript's `plan_mode` attachment names the file as `planFilePath`), so the card shows that file whole, as Markdown, and keeps from the screen only the question under the pane.
SEE-ALSO: packages/gx-chat-core/src/questions/document.rs (`plan` presentation), apps/desktop/src/app/native_chat/notice.rs, apps/mobile/app/src/chat/native/cards/NoticeCard.tsx.
*/
pub(crate) fn enrich_claude_plan_notice(
    notice: &mut SessionChatTerminalNotice,
    screen: &str,
    transcript_path: Option<&str>,
) {
    let Some(dialog) = notice.dialog.as_mut() else {
        return;
    };
    // With "Tell Claude what to change" highlighted the dialog is a feedback field without rows;
    // the plan is shown there too, as the user writes about it.
    if dialog.title != READY_TO_CODE {
        return;
    }
    let Some(plan) = transcript_path
        .and_then(|path| latest_plan_file_path(Path::new(path.trim())))
        .and_then(|path| read_plan(Path::new(&path)))
    else {
        return;
    };
    let question = ready_to_code_question(screen).unwrap_or_default();
    dialog.body = question.clone();
    // The panel's blocks are the pane's screenful of the same plan (session_chat_claude_panel.rs).
    dialog.blocks = None;
    notice.detail = (!question.is_empty()).then_some(question);
    notice.plan = Some(plan);
}

fn read_plan(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(PLAN_MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let plan = String::from_utf8_lossy(&bytes).trim().to_string();
    (!plan.is_empty()).then_some(plan)
}

/// The newest `planFilePath` the transcript records, read backwards from its end in growing
/// windows so a long session costs only its tail.
fn latest_plan_file_path(transcript: &Path) -> Option<String> {
    let mut file = File::open(transcript).ok()?;
    let length = file.metadata().ok()?.len();
    let mut window = 256 * 1024_u64;
    loop {
        let start = length.saturating_sub(window);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(length - start)
            .read_to_end(&mut bytes)
            .ok()?;
        let text = String::from_utf8_lossy(&bytes);
        // A window that starts mid-file starts mid-line; that partial line is read next time.
        let lines = text.lines().skip(usize::from(start > 0));
        let found = lines
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .filter(|line| line.contains("\"planFilePath\""))
            .find_map(|line| {
                let record: Value = serde_json::from_str(line).ok()?;
                plan_file_path(&record).map(str::to_string)
            });
        if found.is_some() || start == 0 || window >= TRANSCRIPT_SCAN_MAX_BYTES {
            return found;
        }
        window *= 4;
    }
}

fn plan_file_path(record: &Value) -> Option<&str> {
    record
        .get("attachment")
        .and_then(|attachment| attachment.get("planFilePath"))
        .or_else(|| record.get("planFilePath"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
}

/// What "Ready to code?" asks under its plan pane ("Claude has written up a plan and is ready to
/// execute. Would you like to proceed?"): the lines between the pane's closing `─` rule and the
/// first option row.
pub(crate) fn ready_to_code_question(screen: &str) -> Option<String> {
    let lines: Vec<String> = screen
        .lines()
        .map(|line| normalize_spaces(&strip_ansi_sgr(line)).trim().to_string())
        .collect();
    let title = lines.iter().rposition(|line| line == READY_TO_CODE)?;
    let rule = title
        + lines[title..]
            .iter()
            .rposition(|line| line.chars().count() >= 20 && line.chars().all(|c| c == '─'))?;
    let question = lines[rule + 1..]
        .iter()
        .take_while(|line| {
            let row = line.trim_start_matches('❯').trim_start();
            !row.split_once(". ")
                .is_some_and(|(number, _)| number.parse::<u32>().is_ok())
        })
        .filter(|line| !line.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    (!question.is_empty()).then_some(question)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_path_is_the_newest_plan_mode_attachment() {
        let directory = std::env::temp_dir().join(format!(
            "gx-claude-plan-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let plan = directory.join("plan.md");
        std::fs::write(&plan, "# Plan\n\n1. One\n2. Two\n").unwrap();
        let transcript = directory.join("session.jsonl");
        std::fs::write(
            &transcript,
            format!(
                "{}\n{}\n{}\n",
                serde_json::json!({"type":"attachment","attachment":{"type":"plan_mode","planFilePath":"/nowhere/old.md"}}),
                serde_json::json!({"type":"attachment","attachment":{"type":"plan_mode","planFilePath":plan}}),
                serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"ExitPlanMode","input":{}}]}}),
            ),
        )
        .unwrap();
        let found = latest_plan_file_path(&transcript).unwrap();
        assert_eq!(Path::new(&found), plan);
        assert_eq!(
            read_plan(Path::new(&found)).unwrap(),
            "# Plan\n\n1. One\n2. Two"
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
