//! A coordinator's board, beside its chat: its lines of work grouped by who has the ball, which the
//! Board pill opens. gxserver decides each thread's and item's group (server/src/coordinators/board.rs)
//! and sends the board inside `coordinatorThreads` (`board`); this adds the chat's own unanswered
//! questions to Needs you, folds and caps the groups, words every row and tag once for every
//! renderer, and counts the pill.
//!
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User, after the design pages (Q1236 side panel, Q1243 "Yes, build it"): one list grouped by
//! status beside the chat, groups in the order Later, Needs you, Blocked, Moving, Done ("shift
//! Later to be most left one"), and after the first build "please move ideas to the top above
//! later", so Ideas comes first, folded; tags say what a row is (its thread, question or
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
const GROUPS: [(&str, &str); 7] = [
    // The `ideas` group, shown as Backlog. User: "rename Ideas to Backlog in the board please".
    ("ideas", "Backlog"),
    ("later", "To do"),
    ("needs", "Needs you"),
    ("blocked", "Blocked"),
    ("moving", "Moving"),
    ("done", "Done today"),
    ("removed", "Removed"),
];

/// Groups folded until the reader opens them.
fn folded_by_default(group: &str) -> bool {
    matches!(group, "done" | "ideas" | "removed")
}

/// One chip on a row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardTag {
    /// `pr`, `issue`, `linear`, `link`, `branch`, `release`, or `question` (its Q tags).
    pub kind: String,
    pub label: String,
    /// How it is tinted: `question`, `good`, `bad`, `running`, `merged`, `closed`, `held` or ""
    /// (muted).
    pub tone: String,
    /// What a click on the chip does (a PR opens), or `null`.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub command: Value,
    /// The chip's full words for its tooltip (a PR's title, a release's whole name), "" for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
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
    /// What a click on the card itself does (a question jumps to its card), or `null`: a thread
    /// opens only from its agent button, a link only from its chip.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub command: Value,
    /// What the agent button sends to open the thread, or `null` for a row that is no thread.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub open_thread: Value,
    /// The thread's agent icon (`agent-icons/<icon>.svg`), "" for a row that is no thread.
    #[serde(default)]
    pub agent_icon: String,
    /// It waits on an approval, which takes the sidebar's red.
    pub needs_approval: bool,
    /// What the two-click × sends (`coordinatorBoardChange`, op `drop`), or `null` for a row that
    /// cannot be removed (a question, a row already under Removed).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub remove: Value,
    /// What Restore sends on a row under Removed, else `null`.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub restore: Value,
    /// The Markdown link a click on the card puts in the chat box, so the user can comment on it:
    /// `[title](board:B4)` for an item, the thread's session ref for a thread (the + mention's
    /// form); "" for a question, whose click jumps to its card instead.
    #[serde(default)]
    pub reference: String,
    /// The coordinator's own line for it, which the second line shows when it has no links.
    #[serde(default)]
    pub note: String,
    /// One-click answers for an item that asks the user something; the second line shows them in
    /// place of the links.
    #[serde(default)]
    pub answers: Vec<BoardAnswer>,
}

/// One answer on a card that asks the user something.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardAnswer {
    pub label: String,
    pub recommended: bool,
    /// The answer this client sent for the card.
    pub picked: bool,
    /// What a click sends (`coordinatorBoardAnswer`).
    pub command: Value,
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
    /// Every row the chat has for the group, whatever its fold: one column of the columns view.
    pub column_rows: Vec<BoardRow>,
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
        title: String::new(),
    }
}

/// When an ISO time was, in epoch milliseconds.
fn at_ms(at: &str) -> Option<i64> {
    crate::jstime::parse_iso_millis_utc(at).map(|at| at as i64)
}

/// The change `op` of a row gxserver sent: the item it names (`B7`), or its thread.
fn change_command(row: &Value, op: &str) -> Value {
    let mut command = json!({"type": "coordinatorBoardChange", "op": op});
    match text(row, "kind") {
        "item" => command["item"] = json!(text(row, "ref")),
        "thread" => {
            command["threadServerId"] = json!(text(row, "serverId"));
            command["threadProjectId"] = json!(text(row, "projectId"));
            command["threadSessionId"] = json!(text(row, "sessionId"));
        }
        _ => return Value::Null,
    }
    command
}

/// The release chip's short words: "Held \u{b7} <release>" while the release's name is short, its
/// first part when it names several ("next release + TestFlight"), else just "Held"; the chip's
/// tooltip has the whole name.
///
/// CDXC:Coordinators 2026-11-05 WHY:
/// User, of "Held \u{b7} next full release + Tes" cut off at a card's edge: "stuff cut off man". A
/// chip line must fit its card, so a long release name gives way and its words move to the tooltip.
fn release_chip_label(release: &str) -> String {
    const SHORT: usize = 16;
    let first = release
        .split(['+', ',', '(', ';'])
        .next()
        .unwrap_or_default()
        .trim()
        .replace("full ", "");
    if first.is_empty() {
        "Held".to_string()
    } else if first.chars().count() <= SHORT {
        format!("Held \u{b7} {first}")
    } else {
        "Held".to_string()
    }
}

/// A Markdown link for the chat box, its label escaped the way the + mention escapes one.
fn markdown_link(label: &str, destination: &str) -> String {
    let label: String = label
        .chars()
        .flat_map(|character| {
            if matches!(character, '\\' | '[' | ']') {
                vec!['\\', character]
            } else {
                vec![character]
            }
        })
        .collect();
    format!("[{label}]({destination})")
}

/// A row gxserver sent, worded.
fn board_row(row: &Value, now_ms: i64, picked: Option<&str>) -> BoardRow {
    let open = |url: &str| json!({"type": "openMarkdownLink", "href": url});
    let mut tags = Vec::new();
    let open_thread_command = json!({
        "type": "openCoordinatorThread",
        "serverId": row.get("serverId").cloned().unwrap_or(json!("")),
        "projectId": row["projectId"],
        "sessionId": row["sessionId"],
    });
    // A thread waiting on the user says what to do about it, first on its second line.
    //
    // CDXC:Coordinators 2026-11-05 DECISION:
    // User, of a Needs you card that said only "No links yet": "Wtf can i do with this card???????".
    // A Needs you card always says what the user can do: a thread waiting on them opens from
    // "Answer in thread" or "Approve in thread", an item that asks something shows its answers, and
    // an item's own note stands in for links it does not have.
    match text(row, "waiting") {
        "approval" => {
            let mut chip = tag("action", "Approve in thread", "bad");
            chip.command = open_thread_command.clone();
            tags.push(chip);
        }
        "question" => {
            let mut chip = tag("action", "Answer in thread", "question");
            chip.command = open_thread_command.clone();
            tags.push(chip);
        }
        _ => {}
    }
    let mut has_pr = false;
    for link in row["links"].as_array().into_iter().flatten() {
        let kind = text(link, "kind");
        let state = text(link, "state");
        let tone = match (kind, state, text(link, "checks")) {
            ("pr", "merged", _) => "merged",
            ("pr", "closed", _) | ("issue", "closed", _) => "closed",
            ("pr", "draft", _) => "",
            ("pr", _, "failing") => "bad",
            ("pr", _, "pending") => "running",
            ("pr", _, _) | ("issue", "open", _) => "good",
            _ => "",
        };
        has_pr |= kind == "pr";
        let mut chip = tag(kind, text(link, "label"), tone);
        if let Some(url) = link.get("url").and_then(Value::as_str) {
            chip.command = open(url);
            chip.title = url.to_string();
        }
        let title = text(link, "title");
        if !title.is_empty() {
            chip.title = title.to_string();
        }
        tags.push(chip);
    }
    // A PR stands for its branch; with none, the branch says where the work is.
    let branch = text(row, "branch");
    if !has_pr && !branch.is_empty() {
        tags.push(tag("branch", branch, ""));
    }
    if text(row, "reason") == "held" || !text(row, "release").is_empty() {
        let release = text(row, "release");
        let mut chip = tag("release", release_chip_label(release), "held");
        chip.title = match release {
            "" => "Held for release".to_string(),
            release => format!("Held for {release}"),
        };
        tags.push(chip);
    }
    let open_thread = match text(row, "kind") {
        "thread" => open_thread_command,
        _ => Value::Null,
    };
    let agent = text(row, "agent");
    let title = text(row, "title");
    let reference = match text(row, "kind") {
        "item" => markdown_link(title, &format!("board:{}", text(row, "ref"))),
        "thread" if !text(row, "globalRef").is_empty() => {
            markdown_link(title, text(row, "globalRef"))
        }
        _ => String::new(),
    };
    let answers = row["answers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|answer| {
            let label = text(answer, "label");
            BoardAnswer {
                label: label.to_string(),
                recommended: answer["recommended"] == true,
                picked: picked == Some(label),
                command: json!({
                    "type": "coordinatorBoardAnswer",
                    "key": text(row, "key"),
                    "label": label,
                    "text": format!("{}: {label}", text(row, "ref")),
                }),
            }
        })
        .collect();
    BoardRow {
        reference,
        note: text(row, "note").to_string(),
        answers,
        key: text(row, "key").to_string(),
        title: title.to_string(),
        detail: text(row, "detail").to_string(),
        age: at_ms(text(row, "at"))
            .map(|at| crate::transcript::message_time::age_label(now_ms, at))
            .unwrap_or_default(),
        tags,
        // A card opens nothing by itself: its links open their pages, and its agent button its
        // thread.
        command: Value::Null,
        agent_icon: if open_thread.is_null() {
            String::new()
        } else {
            crate::extras::welcome::welcome_agent_icon((!agent.is_empty()).then_some(agent), None)
                .unwrap_or_else(|| "claude".to_string())
        },
        open_thread,
        needs_approval: row["needsApproval"] == true,
        remove: if text(row, "group") == "removed" {
            Value::Null
        } else {
            change_command(row, "drop")
        },
        restore: if text(row, "group") == "removed" {
            change_command(row, "restore")
        } else {
            Value::Null
        },
    }
}

/// `coordinatorBoardChange {op, item | threadServerId, threadProjectId, threadSessionId}`: a row
/// moved to Removed (`drop`) or restored, asked of gxserver for this coordinator's board. The
/// board itself changes when the next frame brings it.
pub fn change(
    state: &mut ChatState,
    params: &serde_json::Map<String, Value>,
    request_id: u64,
) -> Option<crate::effect::Effect> {
    let op = params.get("op").and_then(Value::as_str)?;
    if !matches!(op, "drop" | "restore") {
        return None;
    }
    let mut body = json!({
        "projectId": state.identity.project_id,
        "sessionId": state.identity.session_id,
        "op": op,
    });
    for key in ["item", "threadServerId", "threadProjectId", "threadSessionId"] {
        if let Some(value) = params.get(key).filter(|value| value.is_string()) {
            body[key] = value.clone();
        }
    }
    state.extras.panels.board_change_requests.insert(request_id);
    Some(crate::effect::Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::CoordinatorBoard,
        params: Box::new(body),
    })
}

/// Takes the answer to [`change`]: nothing to do when it worked, a toast when gxserver refused.
/// `None` when `request_id` is not a board change.
pub fn settle_change(
    panels: &mut crate::state::PanelsState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> Option<Vec<crate::effect::Effect>> {
    if !panels.board_change_requests.remove(&request_id) {
        return None;
    }
    Some(match answer {
        Ok(_) => Vec::new(),
        Err(message) => vec![crate::effect::Effect::Toast {
            level: "error".to_string(),
            message: if message.is_empty() {
                "Couldn't change the board.".to_string()
            } else {
                message
            },
        }],
    })
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
                    [only] => vec![tag("question", only.clone(), "question")],
                    [first, .., last] => {
                        vec![tag("question", format!("{first}\u{2013}{last}"), "question")]
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
                        open_thread: Value::Null,
                        agent_icon: String::new(),
                        reference: String::new(),
                        note: String::new(),
                        answers: Vec::new(),
                        // A question leaves by being answered, or put off For later in Resources.
                        remove: Value::Null,
                        restore: Value::Null,
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
            .map(|row| board_row(row, now_ms, panels.board_answers.get(text(row, "key")).map(String::as_str)))
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
        // A group shows only while it holds something.
        //
        // CDXC:Coordinators 2026-11-05 DECISION:
        // User: "lets not show the empty statuses in the list (to make it less crowded and not have
        // bunch of Blocked 0 Moving 0 etc." A group with nothing in it is left out of every view and
        // comes back when something lands in it.
        if total == 0 {
            continue;
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
        let column_rows = rows.clone();
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
            column_rows,
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
        meta.push(format!("{later} to do"));
    }
    let pill_counts = [("needs", needs), ("blocked", blocked), ("moving", moving)]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(id, count)| (id.to_string(), count))
        .collect();
    let empty = out
        .iter()
        .filter(|group| !matches!(group.id.as_str(), "ideas" | "removed"))
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

/// The Board pill, before Workers. Its renderers draw `pillCounts` beside its label, so it has no
/// count of its own and no corner dot.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User, of the pill with its coloured counts and the strip's amber and pink corner dots: "pls don't
/// show the top right dots on the board". The counts already say what needs the user and what
/// moves, so the Board pill alone never takes the waiting or running dot; other pills keep theirs.
pub fn pill(board: &Value) -> Option<ChatPill> {
    if !board.is_object() {
        return None;
    }
    let meta = text(board, "meta").to_string();
    Some(ChatPill {
        id: BOARD_PILL.to_string(),
        label: "Board".to_string(),
        count: String::new(),
        waiting: String::new(),
        running: false,
        aria_label: if meta.is_empty() {
            "Board".to_string()
        } else {
            format!("Board, {meta}")
        },
        meta,
    })
}

/// `insertBoardReference {reference, text, start, end}`: a card's reference put in the chat box at
/// the caret (`text` is the box's text, `start`..`end` its selection, in UTF-16 units), so the user
/// can comment on it.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User: "we need to be able to click on a card to add a reference to it in the chat so i can
/// comment on it". A click on a card (not its chips, agent button or ×) inserts its Markdown link,
/// `[title](board:B4)` for an item or the thread's session ref, and the renderers focus the box.
pub fn insert_reference(params: &serde_json::Map<String, Value>) -> Vec<crate::effect::Effect> {
    let reference = params.get("reference").and_then(Value::as_str).unwrap_or_default();
    if reference.is_empty() {
        return Vec::new();
    }
    let text = params.get("text").and_then(Value::as_str).unwrap_or_default();
    let units = text.encode_utf16().count();
    let offset = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_u64)
            .map_or(units, |offset| (offset as usize).min(units))
    };
    // `insert_reference` spaces it off the words around it, as a mention is.
    let edit =
        crate::composer::references::insert_reference(text, reference, offset("start"), offset("end"));
    vec![crate::effect::Effect::SetComposerText {
        content: edit.text,
        caret: Some(edit.caret),
        from_history: false,
    }]
}

/// `coordinatorBoardAnswer {key, label, text}`: one of a card's answers, sent to the coordinator as
/// a message ("B4: Yes"), like a one-click answer, without touching the chat box.
pub fn answer(
    state: &mut ChatState,
    params: &serde_json::Map<String, Value>,
    context: &ChatContext,
    request_id: u64,
) -> Option<crate::effect::Effect> {
    let text = params.get("text").and_then(Value::as_str).filter(|text| !text.is_empty())?;
    let key = params.get("key").and_then(Value::as_str).unwrap_or_default();
    let label = params.get("label").and_then(Value::as_str).unwrap_or_default();
    state
        .extras
        .panels
        .board_answers
        .insert(key.to_string(), label.to_string());
    state.extras.panels.board_change_requests.insert(request_id);
    Some(crate::effect::Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::SendSessionChatMessage,
        params: Box::new(json!({
            "text": text,
            "sendRequestId": format!("board-{key}-{}", context.now_millis()),
        })),
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

    /// The groups come in the board's order, Done and Backlog folded; a group past `GROUP_ROWS`
    /// says how many more; the pill counts Needs you, Blocked and Moving.
    #[test]
    fn groups_fold_cap_and_count() {
        let moving: Vec<Value> = (0..7)
            .map(|index| row(&format!("G{index}"), "moving"))
            .collect();
        let mut red = row("Gred", "blocked");
        red["links"] = json!([{"kind": "pr", "label": "#12", "state": "open", "checks": "failing", "url": "https://github.com/o/r/pull/12"}]);
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
            ["later", "blocked", "moving", "done"],
            "in the board's order, empty groups left out"
        );
        let group = |id: &str| board.groups.iter().find(|group| group.id == id).unwrap();
        assert_eq!(group("later").rows[0].tags[0].label, "Held \u{b7} release");
        assert_eq!(group("later").rows[0].tags[0].title, "Held for full release");
        assert_eq!(
            release_chip_label("next full release + TestFlight"),
            "Held \u{b7} next release"
        );
        assert_eq!(release_chip_label("the one after the big refactor lands"), "Held");
        // The second line is the work's links, each opening its page.
        let pr = &group("blocked").rows[0].tags[0];
        assert_eq!((pr.kind.as_str(), pr.label.as_str(), pr.tone.as_str()), ("pr", "#12", "bad"));
        assert_eq!(pr.command["href"], "https://github.com/o/r/pull/12");
        assert_eq!(group("moving").rows.len(), GROUP_ROWS);
        assert_eq!(group("moving").more_label, "2 more");
        assert!(!group("done").open && group("done").rows.is_empty());
        // The columns view has every row, folded or capped in the list.
        assert_eq!(group("done").column_rows.len(), 1);
        assert_eq!(group("moving").column_rows.len(), 7);
        assert_eq!(group("done").count, 1);
        assert_eq!(
            board.pill_counts,
            vec![("blocked".to_string(), 1), ("moving".to_string(), 7)]
        );
        assert_eq!(board.meta, "1 blocked \u{b7} 7 moving");
        assert_eq!(
            group("moving").rows[0].open_thread["type"],
            "openCoordinatorThread"
        );
        // The card itself opens nothing; the agent button opens the thread.
        assert!(group("moving").rows[0].command.is_null());
        assert_eq!(group("moving").rows[0].agent_icon, "claude");
        // Every thread and item row can go to Removed; the change names the thread or the item.
        let remove = &group("moving").rows[0].remove;
        assert_eq!(remove["op"], "drop");
        assert_eq!(remove["threadSessionId"], "G0");
        assert_eq!(group("later").rows[0].remove["item"], "B1");
        assert_eq!(group("later").title, "To do");
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
        assert_eq!(GROUPS[0], ("ideas", "Backlog"), "Backlog comes first");
        toggle(&mut state.extras.panels.board_toggled, "done");
        toggle(&mut state.extras.panels.board_expanded, "moving");
        let board = coordinator_board(&state, &context()).unwrap();
        let group = |id: &str| board.groups.iter().find(|group| group.id == id).unwrap();
        assert!(group("done").open);
        assert_eq!(group("moving").rows.len(), 7);
        assert_eq!(group("moving").more_label, "Show fewer");
        assert_eq!(group("moving").not_shown, 2);
    }

    /// What a Needs you card offers: a waiting thread's "Answer in thread", an item's answers and
    /// note; and what a click on a card puts in the chat box.
    #[test]
    fn needs_you_cards_say_what_to_do() {
        let mut waiting = row("Gask", "needs");
        waiting["waiting"] = json!("question");
        waiting["globalRef"] = json!("S90:P:Gask");
        let item = json!({"key": "item:4", "kind": "item", "ref": "B4", "group": "needs",
            "title": "Delete branch [wip]", "note": "Its commits are on main",
            "answers": [{"label": "Yes", "recommended": true}, {"label": "No", "recommended": false}]});
        let mut state = state_with(json!({"groups": [{"id": "needs", "total": 2, "rows": [waiting, item]}]}));
        state.extras.panels.board_answers.insert("item:4".to_string(), "Yes".to_string());
        let board = coordinator_board(&state, &context()).unwrap();
        let rows = &board.groups[0].rows;
        assert_eq!(rows[0].tags[0].label, "Answer in thread");
        assert_eq!(rows[0].tags[0].command["type"], "openCoordinatorThread");
        assert_eq!(rows[0].reference, "[Gask](S90:P:Gask)");
        assert_eq!(rows[1].reference, "[Delete branch \\[wip\\]](board:B4)");
        assert_eq!(rows[1].note, "Its commits are on main");
        assert_eq!(rows[1].answers[1].command["text"], "B4: No");
        assert!(rows[1].answers[0].recommended && rows[1].answers[0].picked);

        let params = json!({"reference": "[Gask](S90:P:Gask)", "text": "look at ", "start": 8, "end": 8});
        let effects = insert_reference(params.as_object().unwrap());
        let [crate::effect::Effect::SetComposerText { content, .. }] = effects.as_slice() else {
            panic!("the reference goes in the chat box");
        };
        assert_eq!(content, "look at [Gask](S90:P:Gask) ");
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
