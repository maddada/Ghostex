//! Where the "+N previous tool calls" toggle sits among a run's rows, closed and open.

use ghostex_gx_chat_core::transcript::tool_rows::{tool_fold_toggle_before, tool_run_fold};

/// The row order a renderer draws: the rows before the toggle's slot, the toggle (`None`), then
/// the rest. `shown` is the rows in view.
fn order(fold: &serde_json::Value, open: bool) -> Vec<Option<usize>> {
    let shown: Vec<usize> = (0..fold["visible"].as_array().unwrap().len())
        .filter(|index| open || fold["visible"][*index] == true)
        .collect();
    let before = tool_fold_toggle_before(fold, open);
    let mut rows: Vec<Option<usize>> = shown
        .iter()
        .copied()
        .filter(|index| *index < before)
        .map(Some)
        .collect();
    rows.push(None);
    rows.extend(
        shown
            .iter()
            .copied()
            .filter(|index| *index >= before)
            .map(Some),
    );
    rows
}

#[test]
fn closed_toggle_sits_right_before_the_last_call() {
    let fold = tool_run_fold(&[false, false, false]);
    assert_eq!(order(&fold, false), vec![None, Some(2)]);
}

#[test]
fn open_toggle_is_first_and_every_call_follows_in_order() {
    let fold = tool_run_fold(&[false, false, false]);
    assert_eq!(order(&fold, true), vec![None, Some(0), Some(1), Some(2)]);
}

#[test]
fn single_hidden_call_opens_below_the_toggle() {
    let fold = tool_run_fold(&[false, false]);
    assert_eq!(order(&fold, false), vec![None, Some(1)]);
    assert_eq!(order(&fold, true), vec![None, Some(0), Some(1)]);
}

#[test]
fn an_answered_question_before_the_last_call_stays_above_the_closed_toggle_only() {
    let fold = tool_run_fold(&[false, true, false]);
    assert_eq!(order(&fold, false), vec![Some(1), None, Some(2)]);
    assert_eq!(order(&fold, true), vec![None, Some(0), Some(1), Some(2)]);
}

#[test]
fn a_fold_without_the_open_slot_still_opens_at_the_top() {
    let fold = serde_json::json!({ "toggleBefore": 4 });
    assert_eq!(tool_fold_toggle_before(&fold, true), 0);
    assert_eq!(tool_fold_toggle_before(&fold, false), 4);
}
