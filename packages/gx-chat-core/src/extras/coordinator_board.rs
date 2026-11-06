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
    /// The answer given for the card, here or by any client.
    pub picked: bool,
    /// The card is answered: every answer is shown but none can be clicked again.
    pub locked: bool,
    /// What a click sends (`coordinatorBoardAnswer`), `null` once the card is answered.
    #[serde(default, skip_serializing_if = "Value::is_null")]
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
    /// The row a board pill in the chat asked to see, `{key, seq}`, or `null`: the renderers open
    /// the board and scroll to the row `key` names each time `seq` changes (`show_item`).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub focus: Value,
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
    // A thread not moving even after Ghostex nudged it: the red chip, which opens the thread
    // (CDXC:Coordinators 2026-11-05 in server/src/coordinators/stall.rs).
    if row["stuck"] == true {
        let mut chip = tag("action", "Not moving", "bad");
        chip.command = open_thread_command.clone();
        chip.title = "No progress for a while, even after Ghostex nudged it".to_string();
        tags.push(chip);
    }
    // What a thread that ended its turn still owes (server/src/coordinators/board.rs): a chip that
    // opens it, so a thread put to sleep mid-task never looks parked.
    let flag = text(row, "flag");
    if !flag.is_empty() && row["stuck"] != true {
        let mut chip = tag("action", flag, text(row, "flagTone"));
        chip.command = open_thread_command.clone();
        chip.title = match text(row, "unfinished") {
            "jobRunning" => {
                "It ended its turn waiting on a background job that still runs".to_string()
            }
            "waitsOnJob" | "queued" => {
                "Its background job ended and it has not read the result".to_string()
            }
            "carriesOn" => "Its last message says what it does next, but it stopped".to_string(),
            "cutOff" => {
                "Its turn was cut off before it finished (put to sleep, or its agent stopped)"
                    .to_string()
            }
            _ => String::new(),
        };
        tags.push(chip);
    }
    // A finished thread nothing is expected of, under Done for the day.
    if text(row, "reason") == "idle" {
        let mut chip = tag("state", "Idle", "");
        chip.title = "Finished its turn with nothing pending; not marked done".to_string();
        tags.push(chip);
    }
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
    // The answer recorded by gxserver wins over this client's own pick; either locks the card.
    let given = Some(text(row, "answered"))
        .filter(|answered| !answered.is_empty())
        .or(picked);
    let answers = row["answers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|answer| {
            let label = text(answer, "label");
            BoardAnswer {
                label: label.to_string(),
                recommended: answer["recommended"] == true,
                picked: given == Some(label),
                locked: given.is_some(),
                command: if given.is_some() {
                    Value::Null
                } else {
                    json!({
                        "type": "coordinatorBoardAnswer",
                        "key": text(row, "key"),
                        "item": text(row, "ref"),
                        "label": label,
                        "text": format!("{}: {label}", text(row, "ref")),
                    })
                },
            }
        })
        .collect();
    BoardRow {
        reference,
        // A finished thread with no links of its own says how its last report began, so a Done or
        // To do card's second line is still worth reading.
        //
        // CDXC:Coordinators 2026-11-05 WHY:
        // User, of Done cards that all read "No links yet": the line should be useful.
        //
        // CDXC:Coordinators 2026-11-06 WHY:
        // A card with no links read "No links yet" although the thread was fine (the user's
        // "Confusing notices audit" card). A Moving card falls back to its detail too, which is
        // the line it is on now, not its last report (server/src/coordinators/live.rs); a Needs
        // you card keeps its "Answer in thread" chip alone.
        note: match text(row, "note") {
            "" if text(row, "kind") == "thread"
                && matches!(text(row, "group"), "done" | "later" | "moving" | "blocked") =>
            {
                text(row, "detail").to_string()
            }
            note => note.to_string(),
        },
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
    for key in [
        "item",
        "threadServerId",
        "threadProjectId",
        "threadSessionId",
    ] {
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

/// A chip for each question this chat asked that an item names in its title or note (`Q12`),
/// tinted by where the question stands; a click jumps to it. It tells an item waiting on an answer
/// from a task for the user, and gxserver moves the item to Done once the question is settled
/// (server/src/coordinators/board.rs `settle_answered_items`).
fn question_chips(state: &ChatState, row: &Value) -> Vec<BoardTag> {
    use crate::transcript::open_questions::{tag_state, tag_words, TagState, QUESTION_LINK_PREFIX};
    let askers = &state.transcript_view.askers;
    tag_words(&format!("{}\n\n{}", text(row, "title"), text(row, "note")).replace('`', ""))
        .into_iter()
        .filter_map(|word| {
            let tone = match tag_state(askers, &word)? {
                TagState::Open => "question",
                TagState::Answered => "good",
                TagState::Withdrawn | TagState::Skipped => "",
            };
            let mut chip = tag("question", word.clone(), tone);
            chip.command = json!({"type": "openMarkdownLink", "href": format!("{QUESTION_LINK_PREFIX}{word}")});
            chip.title = format!("Go to {word}");
            Some(chip)
        })
        .collect()
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
                        vec![tag(
                            "question",
                            format!("{first}\u{2013}{last}"),
                            "question",
                        )]
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
    // How many of the Needs you rows are the chat's own open questions (`question_rows`).
    let mut asked = 0;
    for (id, title) in GROUPS {
        let sent = groups.iter().find(|group| text(group, "id") == id);
        let mut rows: Vec<BoardRow> = sent
            .and_then(|group| group.get("rows"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|row| {
                let mut out = board_row(
                    row,
                    now_ms,
                    panels
                        .board_answers
                        .get(text(row, "key"))
                        .map(String::as_str),
                );
                if text(row, "kind") == "item" {
                    let chips = question_chips(state, row);
                    out.tags.splice(0..0, chips);
                }
                out
            })
            .collect();
        let mut total = sent
            .and_then(|group| group.get("total"))
            .and_then(Value::as_u64)
            .map_or(rows.len(), |total| total as usize);
        if id == "needs" {
            let questions = question_rows(state, now_ms);
            asked = questions.len();
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
    // CDXC:Coordinators 2026-11-06 DECISION:
    // User, of the strip with the ? pill's pink dot and the Board pill's pink count: "if we show the
    // ? with pink dot no need to show it again on the board yea?" The pill's Needs you count leaves
    // out the chat's own open questions, which the ? pill already counts; the board still lists
    // them as rows, and its groups and header keep counting them.
    let pill_counts = [
        ("needs", needs.saturating_sub(asked)),
        ("blocked", blocked),
        ("moving", moving),
    ]
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
        focus: panels
            .board_focus
            .as_ref()
            .map_or(Value::Null, |(key, seq)| json!({"key": key, "seq": seq})),
    })
}

/// The document value, `null` when there is no board.
pub fn project(state: &ChatState, context: &ChatContext) -> Value {
    coordinator_board(state, context)
        .and_then(|board| serde_json::to_value(board).ok())
        .unwrap_or(Value::Null)
}

/// The Board pill, ahead of Resources and Workers (extras/document.rs). Its renderers draw `pillCounts` beside its label, so it has no
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
        // Needs you (pink), Blocked (red), Moving (amber), the order the board lists them.
        dots: board["pillCounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|pair| {
                let tone = match pair[0].as_str()? {
                    "needs" => "question",
                    "blocked" => "blocked",
                    _ => "working",
                };
                Some(crate::document::PillDot {
                    tone: tone.to_string(),
                    count: pair[1].as_u64()?.to_string(),
                })
            })
            .collect(),
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
    let reference = params
        .get("reference")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if reference.is_empty() {
        return Vec::new();
    }
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let units = text.encode_utf16().count();
    let offset = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_u64)
            .map_or(units, |offset| (offset as usize).min(units))
    };
    // `insert_reference` spaces it off the words around it, as a mention is.
    let edit = crate::composer::references::insert_reference(
        text,
        reference,
        offset("start"),
        offset("end"),
    );
    vec![crate::effect::Effect::SetComposerText {
        content: edit.text,
        caret: Some(edit.caret),
        from_history: false,
    }]
}

/// The `kind` of a board item's reference entry.
pub const BOARD_REFERENCE_KIND: &str = "board";

/// What a board pill says on hover.
pub const SHOW_ON_BOARD: &str = "Show on the board";

/// The item number of a board link's destination, `board:B4` (what a card's click puts in the chat
/// box, [`insert_reference`]).
pub fn board_item(href: &str) -> Option<u64> {
    let number = crate::transcript::jsstr::js_trim(href)
        .strip_prefix("board:")?
        .strip_prefix(['B', 'b'])?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok().filter(|item| *item > 0)
}

/// A Markdown link to a board item as a `board` reference: the board's icon and the link's text
/// (the item's title when a card put it there) in a pill, which shows the item on the board.
///
/// CDXC:Coordinators 2026-11-05 DECISION:
/// User, of a card click that left `[Release 10.47.0](board:B8)` as raw text in the chat box while
/// a thread's card left a pill: "why does this not show as a pill?". A board item's link is a pill
/// wherever a session mention is one: in the chat box, in the sent message and in the transcript,
/// with the board's icon, and a click on a sent one opens the board scrolled to that item.
pub fn board_reference(href: &str, label: &str) -> Option<Value> {
    let item = board_item(href)?;
    Some(json!({
        "href": href,
        "sourceLabel": label,
        "label": crate::transcript::session_mentions::pill_label(label),
        "title": SHOW_ON_BOARD,
        "kind": BOARD_REFERENCE_KIND,
        "item": format!("B{item}"),
    }))
}

/// A board pill was clicked: the board opens at item `item`. Its group unfolds and shows every
/// row, so the list has the card to scroll to; an item the chat's board does not have (gone, or a
/// chat with no board) is said in a toast instead.
pub fn show_item(state: &mut ChatState, item: u64) -> Vec<crate::effect::Effect> {
    let reference = format!("B{item}");
    let group = state
        .session
        .coordinator_threads
        .as_ref()
        .and_then(|threads| threads.get("board"))
        .and_then(|board| board.get("groups"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|group| {
            group["rows"]
                .as_array()?
                .iter()
                .find(|row| text(row, "kind") == "item" && text(row, "ref") == reference)
                .map(|row| (text(group, "id").to_string(), text(row, "key").to_string()))
        });
    let Some((group, key)) = group else {
        return vec![crate::effect::Effect::Toast {
            level: "info".to_string(),
            message: format!("{reference} is not on this chat's board."),
        }];
    };
    let panels = &mut state.extras.panels;
    if folded_by_default(&group) != panels.board_toggled.contains(&group) {
        // Folded: open it.
        toggle(&mut panels.board_toggled, &group);
    }
    panels.board_expanded.insert(group);
    let seq = panels.board_focus.as_ref().map_or(0, |(_, seq)| *seq) + 1;
    panels.board_focus = Some((key, seq));
    Vec::new()
}

/// `coordinatorBoardAnswer {key, item, label, text}`: one of a card's answers. It is recorded with
/// gxserver first (op `answer`, which refuses a second answer from any client), and only then sent
/// to the coordinator as a message ("B4: Yes"), without touching the chat box. A card already
/// answered here takes no second click.
///
/// CDXC:Coordinators 2026-11-05 WHY:
/// The coordinator received "B7: Yes" three times for one decision: three clicks a second and six
/// seconds apart, each its own send, while the busy coordinator queued them and the card showed no
/// sign the first had gone. Now the first click locks the card (the pick marked, the rest faded),
/// gxserver keeps the answer so a reload or another client cannot give a second, and the message's
/// send id is the item and answer, so the send ledger drops any repeat.
pub fn answer(
    state: &mut ChatState,
    params: &serde_json::Map<String, Value>,
    request_id: u64,
) -> Option<crate::effect::Effect> {
    let field = |name: &str| params.get(name).and_then(Value::as_str).unwrap_or_default();
    let (key, item, label, text) = (field("key"), field("item"), field("label"), field("text"));
    if text.is_empty() || item.is_empty() || state.extras.panels.board_answers.contains_key(key) {
        return None;
    }
    let panels = &mut state.extras.panels;
    panels
        .board_answers
        .insert(key.to_string(), label.to_string());
    panels.board_answer_requests.insert(
        request_id,
        (key.to_string(), label.to_string(), text.to_string()),
    );
    Some(crate::effect::Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::CoordinatorBoard,
        params: Box::new(json!({
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
            "op": "answer",
            "item": item,
            "answer": label,
        })),
    })
}

/// Takes gxserver's answer to [`answer`]'s record: once recorded, the message goes; refused because
/// the card was already answered, nothing goes and the card stays locked; refused otherwise, the
/// card unlocks and says why. `None` when `request_id` is not such a record.
pub fn settle_answer(
    state: &mut ChatState,
    request_id: u64,
    answer: Result<&Value, String>,
    next_request_id: u64,
) -> Option<Vec<crate::effect::Effect>> {
    let (key, label, text) = state
        .extras
        .panels
        .board_answer_requests
        .remove(&request_id)?;
    Some(match answer {
        Ok(_) => {
            state
                .extras
                .panels
                .board_change_requests
                .insert(next_request_id);
            vec![crate::effect::Effect::SendRpc {
                request_id: next_request_id,
                method: crate::wire::ChatRpcMethod::SendSessionChatMessage,
                params: Box::new(json!({
                    "text": text,
                    "sendRequestId": format!(
                        "board-answer-{}-{key}-{label}",
                        state.identity.session_id
                    ),
                })),
            }]
        }
        Err(message) if message.contains("already answered") => Vec::new(),
        Err(message) => {
            state.extras.panels.board_answers.remove(&key);
            vec![crate::effect::Effect::Toast {
                level: "error".to_string(),
                message: if message.is_empty() {
                    "Couldn't send that answer.".to_string()
                } else {
                    message
                },
            }]
        }
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
        assert_eq!(
            group("later").rows[0].tags[0].title,
            "Held for full release"
        );
        assert_eq!(
            release_chip_label("next full release + TestFlight"),
            "Held \u{b7} next release"
        );
        assert_eq!(
            release_chip_label("the one after the big refactor lands"),
            "Held"
        );
        // The second line is the work's links, each opening its page.
        let pr = &group("blocked").rows[0].tags[0];
        assert_eq!(
            (pr.kind.as_str(), pr.label.as_str(), pr.tone.as_str()),
            ("pr", "#12", "bad")
        );
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
        let mut state =
            state_with(json!({"groups": [{"id": "needs", "total": 2, "rows": [waiting, item]}]}));
        state
            .extras
            .panels
            .board_answers
            .insert("item:4".to_string(), "Yes".to_string());
        let board = coordinator_board(&state, &context()).unwrap();
        let rows = &board.groups[0].rows;
        assert_eq!(rows[0].tags[0].label, "Answer in thread");
        assert_eq!(rows[0].tags[0].command["type"], "openCoordinatorThread");
        assert_eq!(rows[0].reference, "[Gask](S90:P:Gask)");
        assert_eq!(rows[1].reference, "[Delete branch \\[wip\\]](board:B4)");
        assert_eq!(rows[1].note, "Its commits are on main");
        assert_eq!(
            rows[0].note, "",
            "a thread at work has no report line in its card"
        );
        // Answered here: Yes is marked, and no answer can be clicked again.
        assert!(rows[1].answers[0].recommended && rows[1].answers[0].picked);
        assert!(rows[1]
            .answers
            .iter()
            .all(|answer| answer.locked && answer.command.is_null()));

        let params =
            json!({"reference": "[Gask](S90:P:Gask)", "text": "look at ", "start": 8, "end": 8});
        let effects = insert_reference(params.as_object().unwrap());
        let [crate::effect::Effect::SetComposerText { content, .. }] = effects.as_slice() else {
            panic!("the reference goes in the chat box");
        };
        assert_eq!(content, "look at [Gask](S90:P:Gask) ");
    }

    /// The chat's own questions join Needs you only while the ? pill counts them (answered by an
    /// inline link or a form, or withdrawn, they leave both), and an item that names a question
    /// carries a chip for it, tinted by where it stands, that jumps to it.
    #[test]
    fn answered_questions_leave_needs_you_with_the_pill() {
        use crate::transcript::open_questions::{Asker, AskerKind};
        let asker = |id: &str, tag: &str, at: i64| Asker {
            id: id.to_string(),
            kind: AskerKind::Form,
            message_id: format!("m-{id}"),
            tags: vec![tag.to_string()],
            answered: false,
            obsolete: false,
            withdrawn: Vec::new(),
            keys: Vec::new(),
            skipped: Vec::new(),
            superseded: Vec::new(),
            inherited: false,
            said: Vec::new(),
            title: format!("About {tag}?"),
            at: Some(at),
        };
        let item = json!({"key": "item:1", "kind": "item", "ref": "B1", "group": "needs",
            "title": "Reformat My Passport (Q1)", "note": "Answer Q2 in chat"});
        let mut state =
            state_with(json!({"groups": [{"id": "needs", "total": 1, "rows": [item]}]}));
        let mut link = asker("link", "Q1", 1);
        let form = asker("form", "Q2", 2);
        let mut withdrawn = asker("gone", "Q3", 3);
        link.answered = true;
        withdrawn.obsolete = true;
        state.transcript_view.askers = vec![link, form, withdrawn];
        let pill = crate::questions::open_questions::project(&state, &context());
        let board = coordinator_board(&state, &context()).unwrap();
        let rows = &board.groups[0].rows;
        let questions: Vec<&str> = rows
            .iter()
            .filter(|row| row.key.starts_with("question:"))
            .map(|row| row.key.as_str())
            .collect();
        assert_eq!(questions, vec!["question:form"]);
        assert_eq!(pill.count as usize, questions.len());
        let chips: Vec<(&str, &str)> = rows[1]
            .tags
            .iter()
            .map(|chip| (chip.label.as_str(), chip.tone.as_str()))
            .collect();
        assert_eq!(chips, vec![("Q1", "good"), ("Q2", "question")]);
        assert_eq!(rows[1].tags[1].command["href"], "#gx-question=Q2");
    }

    /// One answer sends once: the first click records it with gxserver and, once recorded, sends
    /// the message with a send id fixed by the item and answer; any later click, the same answer or
    /// another, sends nothing.
    #[test]
    fn a_card_answer_sends_once() {
        let mut state = ChatState::default();
        state.identity.session_id = "Gcoord".to_string();
        let click = |label: &str| json!({"key": "item:7", "item": "B7", "label": label, "text": format!("B7: {label}")});
        let first = answer(&mut state, click("Yes").as_object().unwrap(), 1).expect("records");
        let crate::effect::Effect::SendRpc { params, .. } = &first else {
            panic!()
        };
        assert_eq!(
            (params["op"].as_str(), params["answer"].as_str()),
            (Some("answer"), Some("Yes"))
        );
        assert!(
            answer(&mut state, click("Yes").as_object().unwrap(), 2).is_none(),
            "a second click"
        );
        assert!(
            answer(&mut state, click("No").as_object().unwrap(), 3).is_none(),
            "another answer"
        );
        let sent = settle_answer(&mut state, 1, Ok(&json!({})), 4).unwrap();
        let [crate::effect::Effect::SendRpc { params, .. }] = sent.as_slice() else {
            panic!()
        };
        assert_eq!(params["text"], "B7: Yes");
        assert_eq!(params["sendRequestId"], "board-answer-Gcoord-item:7-Yes");
        // A record gxserver refuses because another client answered first sends nothing.
        let mut other = ChatState::default();
        answer(&mut other, click("No").as_object().unwrap(), 9).unwrap();
        let refused = settle_answer(
            &mut other,
            9,
            Err("B7 was already answered: Yes.".into()),
            10,
        );
        assert_eq!(refused, Some(Vec::new()));
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
    /// A card's `[title](board:B8)` is a `board` pill in the chat box and in a message, and a click
    /// on one opens the item's folded group (Done) in full with the board's focus on it, once per
    /// click; an item the board does not have is toasted.
    #[test]
    fn a_board_link_is_a_pill_that_shows_its_item() {
        assert_eq!(board_item("board:B8"), Some(8));
        assert_eq!(board_item(" board:b12 "), Some(12));
        assert_eq!(board_item("board:B"), None);
        assert_eq!(board_item("board:B0"), None);
        assert_eq!(board_item("board:8"), None);
        assert_eq!(board_item("board:B8x"), None);

        let reference =
            crate::transcript::markdown_links::markdown_link("board:B8", "Release 10.47.0")
                .unwrap();
        assert_eq!(reference["kind"], BOARD_REFERENCE_KIND);
        assert_eq!(reference["label"], "Release 10.47.0");
        assert_eq!(reference["title"], SHOW_ON_BOARD);

        let draft = "see [Release 10.47.0](board:B8) now";
        let pills = crate::composer::reference_pills::composer_references(draft);
        assert_eq!(pills.len(), 1);
        assert_eq!(
            pills[0].kind,
            crate::composer::reference_pills::ReferenceKind::Board
        );
        assert_eq!(pills[0].path, "board:B8");
        assert_eq!(pills[0].label, "Release 10.47.0");

        let mut done = row("Gdone", "done");
        done["key"] = json!("item:8");
        done["kind"] = json!("item");
        done["ref"] = json!("B8");
        let mut state = state_with(json!({"groups": [
            {"id": "done", "total": 1, "rows": [done]},
        ]}));
        assert!(!coordinator_board(&state, &context()).unwrap().groups[0].open);
        assert!(show_item(&mut state, 8).is_empty());
        let board = coordinator_board(&state, &context()).unwrap();
        assert!(board.groups[0].open);
        assert_eq!(board.focus, json!({"key": "item:8", "seq": 1}));
        // A second click scrolls again, and leaves the group open.
        show_item(&mut state, 8);
        let board = coordinator_board(&state, &context()).unwrap();
        assert!(board.groups[0].open);
        assert_eq!(board.focus["seq"], 2);

        let missing = show_item(&mut state, 99);
        assert!(
            matches!(&missing[..], [crate::effect::Effect::Toast { message, .. }] if message.contains("B99"))
        );
        assert_eq!(
            coordinator_board(&state, &context()).unwrap().focus["seq"],
            2
        );
    }
}
