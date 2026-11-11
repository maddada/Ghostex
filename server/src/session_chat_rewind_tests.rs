use super::*;

#[test]
fn identical_prompts_move_when_the_picker_scrolls() {
    let before = screen_lines("Rewind\nRestore the code and/or conversation to the point before…\n↑ 2 more above\nRepeated prompt\nNo code changes\n❯ Repeated prompt\nNo code changes\n↓ 1 more below");
    let after = screen_lines("Rewind\nRestore the code and/or conversation to the point before…\n↑ 1 more above\nEarlier prompt\nNo code changes\n❯ Repeated prompt\nNo code changes\n↓ 2 more below");
    let before = highlighted_row(&before).unwrap();
    let after = highlighted_row(&after).unwrap();
    assert_eq!(before.text, after.text);
    assert_eq!(before.row, after.row);
    assert_ne!(before, after);
}

#[test]
fn identical_prompts_move_in_a_picker_that_fits_the_screen() {
    let before = screen_lines(
        "Rewind\nRepeated prompt\nNo code changes\n❯ Repeated prompt\nNo code changes\n(current)",
    );
    let after = screen_lines(
        "Rewind\n❯ Repeated prompt\nNo code changes\nRepeated prompt\nNo code changes\n(current)",
    );
    assert_ne!(highlighted_row(&before), highlighted_row(&after));
}

#[test]
fn a_stationary_highlight_does_not_move_when_relative_times_change() {
    let before = screen_lines("Rewind\n❯ Repeated prompt\nNo code changes (1m ago)");
    let after = screen_lines("Rewind\n❯ Repeated prompt\nNo code changes (2m ago)");
    assert_eq!(highlighted_row(&before), highlighted_row(&after));
}

#[tokio::test]
#[ignore = "Requires an explicitly disposable idle Claude zmx session and transcript"]
async fn live_claude_rewind_middle_then_oldest() {
    let name = std::env::var("GHOSTEX_CLAUDE_REWIND_TEST_ZMX").expect("disposable session");
    let path =
        PathBuf::from(std::env::var("GHOSTEX_CLAUDE_REWIND_TEST_TRANSCRIPT").expect("transcript"));
    let rows = read_claude_transcript_rows(&path).unwrap();
    let leaf = active_leaf_id(&path, &rows, std::fs::metadata(&path).unwrap().len());
    let prompts: Vec<_> = active_conversation(&rows, leaf.as_deref(), None)
        .into_iter()
        .filter(|row| row.prompt_text.is_some())
        .collect();
    assert!(
        prompts.len() >= 4,
        "seed four prompts, with the last two identical"
    );
    assert_eq!(
        prompts[prompts.len() - 1].prompt_text,
        prompts[prompts.len() - 2].prompt_text
    );
    let driver = RewindDriver {
        project_id: "claude-rewind-live-test",
        session_id: "disposable",
        zmx_name: &name,
        source: "claude-rewind-live-test",
        cancelled: &|| false,
    };
    for index in [1, 0] {
        let target = resolve_rewind_target(&path, &prompts[index].lineage.id, None, None).unwrap();
        let plan = RewindPlan {
            codex: None,
            claude_target: Some((path.clone(), target.clone())),
            target_first_line: target.first_line.clone(),
            presses: target.prompts_after + 1,
        };
        let started = std::time::Instant::now();
        driver.run(&plan).await.unwrap();
        assert!(resolve_rewind_target(&path, &target.message_id, None, None).is_err());
        assert_eq!(
            session_chat_pending_rewind(&path).unwrap().leaf_id,
            target.leaf_id
        );
        assert_eq!(
            composer_draft(&driver.capture().await.unwrap()).as_deref(),
            Some("")
        );
        eprintln!(
            "Verified Claude rewind with {} Up presses in {:?}",
            plan.presses,
            started.elapsed()
        );
    }
}
