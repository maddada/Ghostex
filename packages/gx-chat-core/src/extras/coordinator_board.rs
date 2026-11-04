//! A coordinator's board, beside its chat: its lines of work grouped by who has the ball, which the
//! Board pill opens. gxserver decides each thread's and item's group (server/src/coordinators/board.rs)
//! and sends the board inside `coordinatorThreads` (`board`); this adds the chat's own unanswered
//! questions to Needs you, folds and caps the groups, words every row and tag once for every
//! renderer, and counts the pill.
//!
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User, after the design pages (Q1236 side panel, Q1243 "Yes, build it"): one list grouped by
//! status beside the chat, groups in the order Later, Needs you, Blocked, Moving, Done ("shift
//! Later to be most left one"), then Ideas folded; tags say what a row is (its thread, question or
//! item, its PR with a CI dot, its release, its computer, untested); and "keep this board light":
//! a few rows per group with "N more", Done for today only and folded. A row opens its thread, its
//! question card or its link. The user's words on the groups are in server/src/coordinators/board.rs.
//! SEE-ALSO: apps/desktop/src/app/native_chat/coordinator_board.rs (the side panel),
//! apps/mobile/app/src/chat/native/cards/CoordinatorBoard.tsx (the phone's sheet).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::document::ChatPill;
use crate::state::{ChatContext, ChatState};

/// The pill's id; its renderers open the board instead of a pill menu.
pub const BOARD_PILL: &str = "board";

/// Rows a group shows before its "N more".
pub const GROUP_ROWS: usize = 5;

/// Each group's id and title, in the board's order.
const GROUPS: [(&str, &str); 6] = [
    ("later", "Later"),
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    ("moving", "Moving"),
    ("done", "Done today"),
    ("ideas", "Ideas"),
];

/// Groups folded until the reader opens them.
fn folded_by_default(group: &str) -> bool {
    matches!(group, "done" | "ideas")
}

/// One chip on a row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardTag {
    /// `ref` (the thread, question or item id), `pr`, `reason`, `computer` or `untested`.
    pub kind: String,
    pub label: String,
    /// How it is tinted: `question`, `good`, `bad`, `running`, `held`, `warn` or "" (muted).
    pub tone: String,
    /// What a click on the chip does (a PR opens), or `null`.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub command: Value,
}

/// One line of work.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardRow {
    pub key: String,
    pub title: String,
    /// Its latest status in one line.
    pub detail: String,
    /// How long ago it last moved ("4m", "yesterday").
    pub age: String,
    pub tags: Vec<BoardTag>,
    /// What a click on the row does (open the thread, jump to the question, open the link), or
    /// `null` for an item with nothing to open.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub command: Value,
    /// It waits on an approval, which takes the sidebar's red.
    pub needs_approval: bool,
}

/// One group.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardGroup {
    /// `later`, `needs`, `blocked`, `moving`, `done` or `ideas`.
    pub id: String,
    pub title: String,
    pub count: usize,
    pub open: bool,
    pub rows: Vec<BoardRow>,
    /// "4 more" or "Show fewer" under the rows, "" for neither.
    pub more_label: String,
    /// Rows gxserver left out of the chat's copy, which `ghostex coordinator board` lists.
    pub not_shown: usize,
}

/// The `coordinatorBoard` document value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorBoard {
    pub groups: Vec<BoardGroup>,
    /// "2 need you · 1 blocked · 4 moving", the panel's header line.
    pub meta: String,
    /// The pill's coloured counts, in the order it shows them: needs, blocked, moving, each only
    /// when above zero.
    pub pill_counts: Vec<(String, usize)>,
    /// Nothing open and nothing done today.
    pub empty: bool,
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn tag(kind: &str, label: impl Into<String>, tone: &str) -> BoardTag {
    BoardTag {
        kind: kind.to_string(),
        label: label.into(),
        tone: tone.to_string(),
        command: Value::Null,
    }
}

/// When an ISO time was, in epoch milliseconds.
fn at_ms(at: &str) -> Option<i64> {
    crate::jstime::parse_iso_millis_utc(at).map(|at| at as i64)
}

/// A row gxserver sent, worded.
fn board_row(row: &Value, now_ms: i64) -> BoardRow {
    let mut tags = vec![tag("ref", text(row, "ref"), "")];
    if let Some(number) = row["pr"]["number"].as_u64() {
        let pr = &row["pr"];
        let (tone, words) = match (text(pr, "state"), text(pr, "checks")) {
            ("merged", _) => ("good", " merged"),
            ("closed", _) => ("", " closed"),
            (_, "failing") => ("bad", ""),
            (_, "pending") => ("running", ""),
            (_, "passing") => ("good", ""),
            _ => ("", ""),
        };
        let mut chip = tag("pr", format!("PR #{number}{words}"), tone);
        if let Some(url) = pr.get("url").and_then(Value::as_str) {
            chip.command = json!({"type": "openMarkdownLink", "href": url});
        }
        tags.push(chip);
    }
    match text(row, "reason") {
        "next" => tags.push(tag("reason", "Up next", "")),
        "held" => tags.push(tag(
            "reason",
            match text(row, "release") {
                "" => "Held for release".to_string(),
                release => format!("Held \u{b7} {release}"),
            },
            "held",
        )),
        "parked" => tags.push(tag("reason", "Parked", "")),
        _ => {}
    }
    let computer = text(row, "computer");
    if !computer.is_empty() {
        tags.push(tag("computer", computer, ""));
    }
    if row["untested"] == true {
        tags.push(tag("untested", "untested", "warn"));
    }
    let command = match text(row, "kind") {
        "thread" => json!({
            "type": "openCoordinatorThread",
            "serverId": row.get("serverId").cloned().unwrap_or(json!("")),
            "projectId": row["projectId"],
            "sessionId": row["sessionId"],
        }),
        _ => match text(row, "link") {
            "" => Value::Null,
            link => json!({"type": "openMarkdownLink", "href": link}),
        },
    };
    BoardRow {
        key: text(row, "key").to_string(),
        title: text(row, "title").to_string(),
        detail: text(row, "detail").to_string(),
        age: at_ms(text(row, "at"))
            .map(|at| crate::transcript::message_time::age_label(now_ms, at))
            .unwrap_or_default(),
        tags,
        command,
        needs_approval: row["needsApproval"] == true,
    }
}

/// The chat's own unanswered questions (forms and question pages not put off For later), as
/// Needs you rows, newest first.
fn question_rows(state: &ChatState, now_ms: i64) -> Vec<BoardRow> {
    let mut rows: Vec<(Option<i64>, BoardRow)> =
        crate::questions::open_questions::unanswered(state)
            .into_iter()
            .map(|asker| {
                let tags = match asker.tags.as_slice() {
                    [] => Vec::new(),
                    [only] => vec![tag("ref", only.clone(), "question")],
                    [first, .., last] => {
                        vec![tag("ref", format!("{first}\u{2013}{last}"), "question")]
                    }
                };
                let questions = asker.tags.len().max(1);
                let title = crate::questions::form_links::plain_words(&asker.title);
                (
                    asker.at,
                    BoardRow {
                        key: format!("question:{}", asker.id),
                        title: if title.is_empty() {
                            "Questions for you".to_string()
                        } else {
                            title
                        },
                        detail: if questions == 1 {
                            "1 question waiting".to_string()
                        } else {
                            format!("{questions} questions waiting")
                        },
                        age: asker
                            .at
                            .map(|at| crate::transcript::message_time::age_label(now_ms, at))
                            .unwrap_or_default(),
                        tags,
                        command: json!({"type": "jumpToQuestion", "id": asker.id}),
                        needs_approval: false,
                    },
                )
            })
            .collect();
    rows.sort_by(|left, right| right.0.cmp(&left.0));
    rows.into_iter().map(|(_, row)| row).collect()
}

/// The board, or `None` on a chat that is not a coordinator or whose gxserver sends no board.
pub fn coordinator_board(state: &ChatState, context: &ChatContext) -> Option<CoordinatorBoard> {
    let board = state.session.coordinator_threads.as_ref()?.get("board")?;
    let groups = board.get("groups")?.as_array()?;
    let now_ms = context.now_millis();
    let panels = &state.extras.panels;
    let mut out = Vec::new();
    for (id, title) in GROUPS {
        let sent = groups.iter().find(|group| text(group, "id") == id);
        let mut rows: Vec<BoardRow> = sent
            .and_then(|group| group.get("rows"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|row| board_row(row, now_ms))
            .collect();
        let mut total = sent
            .and_then(|group| group.get("total"))
            .and_then(Value::as_u64)
            .map_or(rows.len(), |total| total as usize);
        if id == "needs" {
            let questions = question_rows(state, now_ms);
            total += questions.len();
            rows.splice(0..0, questions);
        }
        let not_shown = total.saturating_sub(rows.len());
        let open = folded_by_default(id) == panels.board_toggled.contains(id);
        let expanded = panels.board_expanded.contains(id);
        let more_label = if rows.len() <= GROUP_ROWS {
            String::new()
        } else if expanded {
            "Show fewer".to_string()
        } else {
            format!("{} more", rows.len() - GROUP_ROWS)
        };
        if !expanded {
            rows.truncate(GROUP_ROWS);
        }
        out.push(BoardGroup {
            id: id.to_string(),
            title: title.to_string(),
            count: total,
            open,
            rows: if open { rows } else { Vec::new() },
            more_label: if open { more_label } else { String::new() },
            not_shown: if open && expanded { not_shown } else { 0 },
        });
    }
    let count = |id: &str| {
        out.iter()
            .find(|group| group.id == id)
            .map_or(0, |group| group.count)
    };
    let (needs, blocked, moving) = (count("needs"), count("blocked"), count("moving"));
    let mut meta = Vec::new();
    if needs > 0 {
        meta.push(format!(
            "{needs} {}",
            if needs == 1 { "needs you" } else { "need you" }
        ));
    }
    if blocked > 0 {
        meta.push(format!("{blocked} blocked"));
    }
    if moving > 0 {
        meta.push(format!("{moving} moving"));
    }
    let later = count("later");
    if meta.is_empty() && later > 0 {
        meta.push(format!("{later} later"));
    }
    let pill_counts = [("needs", needs), ("blocked", blocked), ("moving", moving)]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(id, count)| (id.to_string(), count))
        .collect();
    let empty = out
        .iter()
        .filter(|group| group.id != "ideas")
        .all(|group| group.count == 0);
    Some(CoordinatorBoard {
        groups: out,
        meta: meta.join(" \u{b7} "),
        pill_counts,
        empty,
    })
}

/// The document value, `null` when there is no board.
pub fn project(state: &ChatState, context: &ChatContext) -> Value {
    coordinator_board(state, context)
        .and_then(|board| serde_json::to_value(board).ok())
        .unwrap_or(Value::Null)
}

/// The Board pill: before Workers, its count what needs the user, its corner dot pink while
/// something does and amber while anything moves. Its renderers draw `pillCounts` beside it.
pub fn pill(board: &Value) -> Option<ChatPill> {
    if !board.is_object() {
        return None;
    }
    let count = |id: &str| {
        board["pillCounts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|pair| pair[0] == id)
            .and_then(|pair| pair[1].as_u64())
            .unwrap_or(0)
    };
    let meta = text(board, "meta").to_string();
    Some(ChatPill {
        id: BOARD_PILL.to_string(),
        label: "Board".to_string(),
        count: String::new(),
        waiting: if count("needs") > 0 { "question" } else { "" }.to_string(),
        running: count("moving") > 0,
        aria_label: if meta.is_empty() {
            "Board".to_string()
        } else {
            format!("Board, {meta}")
        },
        meta,
    })
}

/// `toggleCoordinatorBoardGroup {group}`: folds or unfolds a group; `coordinatorBoardMore
/// {group}`: shows all of a group's rows, or back to the first few.
pub fn toggle(set: &mut std::collections::BTreeSet<String>, group: &str) {
    if group.is_empty() {
        return;
    }
    if !set.remove(group) {
        set.insert(group.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(board: Value) -> ChatState {
        let mut state = ChatState::default();
        state.session.coordinator_threads =
            Some(json!({"threads": [], "total": 0, "board": board}));
        state
    }

    fn context() -> ChatContext {
        ChatContext {
            now_ms: 1_800_000_000_000.0,
            ..ChatContext::default()
        }
    }

    fn row(id: &str, group: &str) -> Value {
        json!({"key": format!("thread::P:{id}"), "kind": "thread", "ref": id, "group": group, "title": id, "detail": "d", "projectId": "P", "sessionId": id})
    }

    /// The groups come in the board's order, Done and Ideas folded; a group past `GROUP_ROWS`
    /// says how many more; the pill counts Needs you, Blocked and Moving.
    #[test]
    fn groups_fold_cap_and_count() {
        let moving: Vec<Value> = (0..7)
            .map(|index| row(&format!("G{index}"), "moving"))
            .collect();
        let mut red = row("Gred", "blocked");
        red["pr"] = json!({"number": 12, "state": "open", "checks": "failing", "url": "https://github.com/o/r/pull/12"});
        let state = state_with(json!({"groups": [
            {"id": "later", "total": 1, "rows": [{"key": "item:1", "kind": "item", "ref": "B1", "group": "later", "reason": "held", "release": "full release", "title": "Help text"}]},
            {"id": "blocked", "total": 1, "rows": [red]},
            {"id": "moving", "total": 7, "rows": moving},
            {"id": "done", "total": 1, "rows": [row("Gdone", "done")]},
        ]}));
        let board = coordinator_board(&state, &context()).unwrap();
        let ids: Vec<&str> = board.groups.iter().map(|group| group.id.as_str()).collect();
        assert_eq!(
            ids,
            ["later", "needs", "blocked", "moving", "done", "ideas"]
        );
        let group = |id: &str| board.groups.iter().find(|group| group.id == id).unwrap();
        assert_eq!(
            group("later").rows[0].tags[1].label,
            "Held \u{b7} full release"
        );
        assert_eq!(group("blocked").rows[0].tags[1].tone, "bad");
        assert_eq!(group("moving").rows.len(), GROUP_ROWS);
        assert_eq!(group("moving").more_label, "2 more");
        assert!(!group("done").open && group("done").rows.is_empty());
        assert_eq!(group("done").count, 1);
        assert_eq!(
            board.pill_counts,
            vec![("blocked".to_string(), 1), ("moving".to_string(), 7)]
        );
        assert_eq!(board.meta, "1 blocked \u{b7} 7 moving");
        assert_eq!(
            group("moving").rows[0].command["type"],
            "openCoordinatorThread"
        );
    }

    /// Opening a folded group and showing a group's every row are the reader's choices.
    #[test]
    fn the_reader_unfolds_and_expands() {
        let moving: Vec<Value> = (0..7)
            .map(|index| row(&format!("G{index}"), "moving"))
            .collect();
        let mut state = state_with(json!({"groups": [
            {"id": "moving", "total": 9, "rows": moving},
            {"id": "done", "total": 1, "rows": [row("Gdone", "done")]},
        ]}));
        toggle(&mut state.extras.panels.board_toggled, "done");
        toggle(&mut state.extras.panels.board_expanded, "moving");
        let board = coordinator_board(&state, &context()).unwrap();
        let group = |id: &str| board.groups.iter().find(|group| group.id == id).unwrap();
        assert!(group("done").open);
        assert_eq!(group("moving").rows.len(), 7);
        assert_eq!(group("moving").more_label, "Show fewer");
        assert_eq!(group("moving").not_shown, 2);
    }

    /// A chat that is no coordinator, or a gxserver that sends no board, has no board and no pill.
    #[test]
    fn no_board_no_pill() {
        let mut state = ChatState::default();
        assert!(coordinator_board(&state, &context()).is_none());
        state.session.coordinator_threads = Some(json!({"threads": [], "total": 0}));
        assert!(coordinator_board(&state, &context()).is_none());
        assert!(pill(&Value::Null).is_none());
    }
}
